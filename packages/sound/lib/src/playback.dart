/// Lifecycle states of a [Playback].
enum PlaybackState {
  /// Loaded but not yet started.
  idle,

  /// Currently producing audio.
  playing,

  /// Stopped before reaching the end.
  stopped,

  /// Reached the end of the audio on its own.
  completed,

  /// Resources released; the handle can no longer be used.
  disposed,
}

/// A handle to a single loaded sound that can be controlled independently.
///
/// Backends return their own implementation from [SoundBackend.load]. Multiple
/// playbacks may exist at once so apps can layer sounds.
abstract class Playback {
  /// The current lifecycle state.
  PlaybackState get state;

  /// Whether audio is currently being produced.
  bool get isPlaying => state == PlaybackState.playing;

  /// Starts (or restarts) playback from the beginning.
  Future<void> play();

  /// Stops playback if it is running. Safe to call when already stopped.
  Future<void> stop();

  /// Completes when playback reaches the end on its own.
  ///
  /// Completes immediately if the sound has already finished, and never
  /// completes via this future if [stop] is called first (it resolves on
  /// natural completion only). Implementations may complete it on [dispose].
  Future<void> get onComplete;

  /// Releases native resources. The handle is unusable afterward.
  Future<void> dispose();
}
