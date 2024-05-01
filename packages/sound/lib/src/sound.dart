import 'dart:typed_data';

import 'backends/native/native_backend.dart';
import 'backends/silent_backend.dart';
import 'exceptions.dart';
import 'playback.dart';
import 'sound_backend.dart';
import 'sound_source.dart';

/// Entry point for playing audio.
///
/// `sound` keeps a registry of [SoundBackend]s and picks the best available one
/// for the current environment (highest [SoundBackend.priority] among those
/// whose [SoundBackend.isAvailable] is true). Apps can register extra backends
/// or pin a specific one.
///
/// ```dart
/// final playback = await Sound.playFile('chime.wav');
/// await playback.onComplete;
/// ```
class Sound {
  Sound._();

  static final List<SoundBackend> _backends = [];
  static final Set<SoundBackend> _initialized = Set.identity();
  static SoundBackend? _active;
  static bool _defaultsRegistered = false;

  /// Registers the backends that ship with `sound`.
  ///
  /// Backends from outside packages (e.g. sound_flutter) call
  /// [registerBackend] themselves; this only seeds the always-present floor.
  static void _ensureDefaults() {
    if (_defaultsRegistered) return;
    _defaultsRegistered = true;
    // Register the platform's native backend (FFI on native, none on web)
    // ahead of the silent fallback, which guarantees the API never crashes
    // for lack of a backend.
    final native = createNativeBackend();
    if (native != null) registerBackend(native);
    registerBackend(SilentBackend());
  }

  /// All registered backends, in registration order.
  static List<SoundBackend> get backends {
    _ensureDefaults();
    return List.unmodifiable(_backends);
  }

  /// Adds [backend] to the registry.
  ///
  /// Pass [makeActive] to select it immediately regardless of priority.
  /// Re-registering an instance is a no-op.
  static void registerBackend(SoundBackend backend, {bool makeActive = false}) {
    _ensureDefaults();
    if (!_backends.contains(backend)) _backends.add(backend);
    if (makeActive) {
      _active = backend;
    } else {
      // Force re-selection so a newly added, higher-priority backend wins.
      _active = null;
    }
  }

  /// The backend currently in use, selecting one on first access.
  ///
  /// Throws [NoBackendAvailableException] if nothing is available.
  static SoundBackend get backend {
    _ensureDefaults();
    return _active ??= _select();
  }

  /// Pins a specific registered backend by [name].
  ///
  /// Throws [NoBackendAvailableException] if no backend with that name is
  /// registered.
  static void useBackend(String name) {
    _ensureDefaults();
    final match = _backends.where((b) => b.name == name);
    if (match.isEmpty) {
      throw NoBackendAvailableException('No backend named "$name" is registered.');
    }
    _active = match.first;
  }

  static SoundBackend _select() {
    final available = _backends.where((b) => b.isAvailable).toList()
      ..sort((a, b) => b.priority.compareTo(a.priority));
    if (available.isEmpty) throw const NoBackendAvailableException();
    return available.first;
  }

  /// Loads [source] with the active backend, returning a controllable handle.
  static Future<Playback> load(SoundSource source) async {
    final b = backend;
    if (!_initialized.contains(b)) {
      await b.initialize();
      _initialized.add(b);
    }
    return b.load(source);
  }

  /// Loads and immediately starts [source].
  static Future<Playback> play(SoundSource source) async {
    final playback = await load(source);
    await playback.play();
    return playback;
  }

  /// Convenience for [play] with a [FileSource].
  static Future<Playback> playFile(String path) =>
      play(SoundSource.file(path));

  /// Convenience for [play] with a [BytesSource].
  static Future<Playback> playBytes(Uint8List bytes, {String? format}) =>
      play(SoundSource.bytes(bytes, format: format));

  /// Disposes the active backend and clears selection/registry.
  ///
  /// Primarily for tests; after this the defaults are re-seeded on next use.
  static Future<void> reset() async {
    for (final b in _initialized) {
      await b.dispose();
    }
    _initialized.clear();
    _backends.clear();
    _active = null;
    _defaultsRegistered = false;
  }
}
