/// Base class for all errors thrown by `sound`.
class SoundException implements Exception {
  const SoundException(this.message);

  final String message;

  @override
  String toString() => 'SoundException: $message';
}

/// Thrown when no registered backend can run in the current environment.
class NoBackendAvailableException extends SoundException {
  const NoBackendAvailableException([
    super.message =
        'No sound backend is available in this environment. '
        'Register one with Sound.registerBackend, or use sound_flutter.',
  ]);
}

/// Thrown when a backend is asked to play a [SoundSource] kind or format it
/// does not support.
class UnsupportedSourceException extends SoundException {
  const UnsupportedSourceException(super.message);
}

/// Thrown when the native playback layer reports a failure.
class PlaybackException extends SoundException {
  const PlaybackException(super.message);
}
