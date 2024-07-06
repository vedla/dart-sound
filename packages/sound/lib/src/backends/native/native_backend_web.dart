import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';

import 'package:web/web.dart' as web;

import '../../exceptions.dart';
import '../../playback.dart';
import '../../sound_backend.dart';
import '../../sound_source.dart';

/// On the web the default backend uses the WebAudio API.
SoundBackend? createNativeBackend() => WebAudioBackend();

/// Plays audio in the browser via WebAudio.
///
/// The browser decodes the container itself (WAV/MP3/OGG/…), so [BytesSource]
/// works for any format the browser supports. [FileSource] is not available on
/// the web - load file contents and pass them as bytes instead.
class WebAudioBackend extends SoundBackend {
  web.AudioContext? _context;

  @override
  String get name => 'webaudio';

  @override
  int get priority => 100;

  @override
  bool get isAvailable => true;

  @override
  Future<void> initialize() async {
    _context ??= web.AudioContext();
  }

  @override
  Future<Playback> load(SoundSource source) async {
    await initialize();
    final bytes = switch (source) {
      BytesSource(:final bytes) => bytes,
      FileSource() => throw const UnsupportedSourceException(
          'FileSource is not supported on the web; pass bytes instead.'),
    };
    // decodeAudioData detaches its input, so hand it a private copy.
    final copy = Uint8List.fromList(bytes);
    final buffer = await _context!.decodeAudioData(copy.buffer.toJS).toDart;
    return WebAudioPlayback(_context!, buffer);
  }

  @override
  Future<void> dispose() async {
    await _context?.close().toDart;
    _context = null;
  }
}

/// A single WebAudio voice backed by a decoded [web.AudioBuffer].
class WebAudioPlayback implements Playback {
  WebAudioPlayback(this._context, this._buffer);

  final web.AudioContext _context;
  final web.AudioBuffer _buffer;

  web.AudioBufferSourceNode? _source;
  PlaybackState _state = PlaybackState.idle;
  Completer<void>? _completer;

  @override
  PlaybackState get state => _state;

  @override
  bool get isPlaying => _state == PlaybackState.playing;

  @override
  Future<void> get onComplete => (_completer ??= Completer<void>()).future;

  @override
  Future<void> play() async {
    if (_state == PlaybackState.disposed) return;
    _stopSource();
    _completer = Completer<void>();

    final source = web.AudioBufferSourceNode(_context)..buffer = _buffer;
    source.connect(_context.destination);
    source.onended = (web.Event _) {
      if (_state == PlaybackState.playing) {
        _state = PlaybackState.completed;
        if (!(_completer?.isCompleted ?? true)) _completer!.complete();
      }
    }.toJS;
    source.start();
    _source = source;
    _state = PlaybackState.playing;
  }

  void _stopSource() {
    final source = _source;
    if (source != null) {
      source.onended = null;
      try {
        source.stop();
      } on Object {
        // Already stopped / never started.
      }
      _source = null;
    }
  }

  @override
  Future<void> stop() async {
    _stopSource();
    if (_state == PlaybackState.playing) _state = PlaybackState.stopped;
  }

  @override
  Future<void> dispose() async {
    _stopSource();
    _state = PlaybackState.disposed;
    if (!(_completer?.isCompleted ?? true)) _completer!.complete();
  }
}
