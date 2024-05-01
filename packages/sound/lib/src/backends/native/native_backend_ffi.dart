import 'dart:async';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

import '../../exceptions.dart';
import '../../playback.dart';
import '../../sound_backend.dart';
import '../../sound_source.dart';

/// The default native backend on platforms with `dart:ffi`.
SoundBackend? createNativeBackend() => FfiBackend();

// --- C ABI signatures, mirroring native/sound_cli/src/lib.rs -------------

typedef _PlayerNewNative = Pointer<Void> Function();
typedef _PlayerFreeNative = Void Function(Pointer<Void>);
typedef _PlayerFree = void Function(Pointer<Void>);
typedef _PlayBytesNative = Uint64 Function(Pointer<Void>, Pointer<Uint8>, Size);
typedef _PlayBytes = int Function(Pointer<Void>, Pointer<Uint8>, int);
typedef _PlayFileNative = Uint64 Function(Pointer<Void>, Pointer<Utf8>);
typedef _PlayFile = int Function(Pointer<Void>, Pointer<Utf8>);
typedef _VoiceStateNative = Int32 Function(Pointer<Void>, Uint64);
typedef _VoiceState = int Function(Pointer<Void>, int);
typedef _VoiceOpNative = Int32 Function(Pointer<Void>, Uint64);
typedef _VoiceOp = int Function(Pointer<Void>, int);
typedef _VoiceErrorNative = Pointer<Utf8> Function(Pointer<Void>, Uint64);
typedef _VoiceError = Pointer<Utf8> Function(Pointer<Void>, int);
typedef _LastErrorNative = Pointer<Utf8> Function();
typedef _LastError = Pointer<Utf8> Function();

// Voice state codes mirrored from the native side.
const int _statePlaying = 0;
const int _stateCompleted = 1;
const int _stateStopped = 2;
const int _stateError = 3;

/// Locates and opens the `sound_cli` shared library.
class NativeLibrary {
  /// Explicit path override, highest priority. Set before first use.
  static String? overridePath;

  static DynamicLibrary? _opened;
  static bool _attempted = false;

  static String get _fileName => switch (Platform.operatingSystem) {
        'windows' => 'sound_cli.dll',
        'macos' || 'ios' => 'libsound_cli.dylib',
        _ => 'libsound_cli.so',
      };

  /// Opens the library, caching the result. Returns `null` if it cannot be
  /// found/loaded - never throws.
  static DynamicLibrary? open() {
    if (_attempted) return _opened;
    _attempted = true;
    for (final candidate in _candidates()) {
      try {
        _opened = DynamicLibrary.open(candidate);
        return _opened;
      } on Object {
        // Try the next candidate.
      }
    }
    return _opened;
  }

  static Iterable<String> _candidates() sync* {
    if (overridePath != null) yield overridePath!;
    final fromEnv = Platform.environment['SOUND_DART_LIB'];
    if (fromEnv != null && fromEnv.isNotEmpty) yield fromEnv;
    // Resolved via the OS loader path / Flutter app bundle.
    yield _fileName;
    // Developer builds of the in-repo crate, searched from the cwd upward.
    var dir = Directory.current.absolute;
    for (var i = 0; i < 6; i++) {
      for (final profile in ['release', 'debug']) {
        yield '${dir.path}/native/sound_cli/target/$profile/$_fileName';
      }
      final parent = dir.parent;
      if (parent.path == dir.path) break;
      dir = parent;
    }
  }
}

/// Bound entry points into the native library.
class _Bindings {
  _Bindings(DynamicLibrary lib)
      : playerNew = lib
            .lookupFunction<_PlayerNewNative, _PlayerNewNative>('sound_player_new'),
        playerFree = lib
            .lookupFunction<_PlayerFreeNative, _PlayerFree>('sound_player_free'),
        playBytes =
            lib.lookupFunction<_PlayBytesNative, _PlayBytes>('sound_play_wav_bytes'),
        playFile =
            lib.lookupFunction<_PlayFileNative, _PlayFile>('sound_play_wav_file'),
        voiceState =
            lib.lookupFunction<_VoiceStateNative, _VoiceState>('sound_voice_state'),
        stop = lib.lookupFunction<_VoiceOpNative, _VoiceOp>('sound_stop'),
        voiceFree =
            lib.lookupFunction<_VoiceOpNative, _VoiceOp>('sound_voice_free'),
        voiceError =
            lib.lookupFunction<_VoiceErrorNative, _VoiceError>('sound_voice_error'),
        lastError =
            lib.lookupFunction<_LastErrorNative, _LastError>('sound_last_error');

  final Pointer<Void> Function() playerNew;
  final _PlayerFree playerFree;
  final _PlayBytes playBytes;
  final _PlayFile playFile;
  final _VoiceState voiceState;
  final _VoiceOp stop;
  final _VoiceOp voiceFree;
  final _VoiceError voiceError;
  final _LastError lastError;
}

/// Plays audio by calling the native `sound_cli` library over FFI.
class FfiBackend extends SoundBackend {
  _Bindings? _bindings;
  Pointer<Void> _player = nullptr;
  bool _available = false;
  bool _probed = false;

  @override
  String get name => 'ffi';

  @override
  int get priority => 100;

  @override
  bool get isAvailable {
    if (_probed) return _available;
    _probed = true;
    final lib = NativeLibrary.open();
    if (lib == null) return _available = false;
    try {
      _bindings = _Bindings(lib);
      _available = true;
    } on Object {
      _available = false;
    }
    return _available;
  }

  @override
  Future<void> initialize() async {
    if (!isAvailable) {
      throw const NoBackendAvailableException(
          'The sound_cli library could not be loaded.');
    }
    if (_player != nullptr) return;
    _player = _bindings!.playerNew();
    if (_player == nullptr) {
      throw PlaybackException('sound_player_new failed: ${_lastError()}');
    }
  }

  String _lastError() {
    final ptr = _bindings!.lastError();
    return ptr == nullptr ? 'unknown error' : ptr.toDartString();
  }

  @override
  Future<Playback> load(SoundSource source) async {
    if (_player == nullptr) await initialize();
    return FfiPlayback._(this, source);
  }

  @override
  Future<void> dispose() async {
    if (_player != nullptr) {
      _bindings!.playerFree(_player);
      _player = nullptr;
    }
  }
}

/// A single FFI-backed voice. Created idle; [play] starts a native voice and
/// polls its state to resolve [onComplete].
class FfiPlayback implements Playback {
  FfiPlayback._(this._backend, this._source);

  final FfiBackend _backend;
  final SoundSource _source;

  int _voiceId = 0;
  PlaybackState _state = PlaybackState.idle;
  Timer? _poll;
  Completer<void>? _completer;

  _Bindings get _b => _backend._bindings!;
  Pointer<Void> get _player => _backend._player;

  @override
  PlaybackState get state => _state;

  @override
  bool get isPlaying => _state == PlaybackState.playing;

  @override
  Future<void> get onComplete => (_completer ??= Completer<void>()).future;

  @override
  Future<void> play() async {
    if (_state == PlaybackState.disposed) return;
    _freeVoice();
    _completer = Completer<void>();
    _voiceId = _start();
    if (_voiceId == 0) {
      _state = PlaybackState.idle;
      throw PlaybackException('playback failed: ${_backend._lastError()}');
    }
    _state = PlaybackState.playing;
    _poll = Timer.periodic(const Duration(milliseconds: 50), (_) => _checkState());
  }

  int _start() {
    switch (_source) {
      case BytesSource(:final bytes):
        final buf = malloc<Uint8>(bytes.length);
        try {
          buf.asTypedList(bytes.length).setAll(0, bytes);
          // The native side decodes synchronously before returning, so the
          // buffer is safe to free immediately afterward.
          return _b.playBytes(_player, buf, bytes.length);
        } finally {
          malloc.free(buf);
        }
      case FileSource(:final path):
        final cPath = path.toNativeUtf8();
        try {
          return _b.playFile(_player, cPath);
        } finally {
          malloc.free(cPath);
        }
    }
  }

  void _checkState() {
    if (_voiceId == 0) return;
    final native = _b.voiceState(_player, _voiceId);
    switch (native) {
      case _statePlaying:
        return;
      case _stateCompleted:
        _finish(PlaybackState.completed, complete: true);
      case _stateStopped:
        _finish(PlaybackState.stopped, complete: false);
      case _stateError:
        final msg = _b.voiceError(_player, _voiceId);
        _finish(PlaybackState.stopped, complete: false);
        _completer?.completeError(
            PlaybackException(msg == nullptr ? 'playback error' : msg.toDartString()));
      default: // -1 unknown
        _finish(PlaybackState.stopped, complete: false);
    }
  }

  void _finish(PlaybackState state, {required bool complete}) {
    _poll?.cancel();
    _poll = null;
    _state = state;
    if (complete && _completer != null && !_completer!.isCompleted) {
      _completer!.complete();
    }
  }

  @override
  Future<void> stop() async {
    if (_voiceId != 0) {
      _b.stop(_player, _voiceId);
      // Reflect the request immediately; the poll will confirm.
      if (_state == PlaybackState.playing) _state = PlaybackState.stopped;
    }
    _poll?.cancel();
    _poll = null;
  }

  void _freeVoice() {
    _poll?.cancel();
    _poll = null;
    if (_voiceId != 0) {
      _b.voiceFree(_player, _voiceId);
      _voiceId = 0;
    }
  }

  @override
  Future<void> dispose() async {
    _freeVoice();
    _state = PlaybackState.disposed;
    if (_completer != null && !_completer!.isCompleted) _completer!.complete();
  }
}
