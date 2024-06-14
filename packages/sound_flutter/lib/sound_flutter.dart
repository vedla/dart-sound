/// Flutter integration for the `sound` package.
///
/// This package contributes the native `sound_cli` library to your Flutter
/// app (built automatically via cargokit) and re-exports the full `sound` API.
/// In most cases you only need the re-exported [Sound] entry point:
///
/// ```dart
/// import 'package:sound_flutter/sound_flutter.dart';
///
/// await SoundFlutter.ensureInitialized();
/// final playback = await Sound.playFile('/path/to/chime.wav');
/// ```
library;

import 'package:sound/sound.dart';

export 'package:sound/sound.dart';

/// Flutter-side conveniences over the `sound` registry.
abstract final class SoundFlutter {
  /// Ensures a playback backend is selected and ready.
  ///
  /// On native platforms `sound` already auto-registers the FFI backend that
  /// loads the bundled `sound_cli` library; this initializes it eagerly so
  /// the first [Sound.play] has no setup latency, and surfaces load errors
  /// early. Returns the name of the active backend (e.g. `ffi`, or `silent`
  /// when no audio device is available).
  static Future<String> ensureInitialized() async {
    final backend = Sound.backend;
    if (backend.isAvailable) {
      await backend.initialize();
    }
    return backend.name;
  }
}
