import 'dart:async';

import '../playback.dart';
import '../sound_backend.dart';
import '../sound_source.dart';

/// A backend that produces no audio.
///
/// It is the lowest-priority fallback so that `sound` never crashes in
/// environments without an audio device (CI, headless servers). It is also a
/// convenient test double: every loaded source is recorded in [loaded].
class SilentBackend extends SoundBackend {
  SilentBackend({this.simulatedDuration = Duration.zero});

  /// How long a [SilentPlayback] pretends to play before completing.
  final Duration simulatedDuration;

  /// Sources passed to [load], in order. Useful in tests.
  final List<SoundSource> loaded = [];

  @override
  String get name => 'silent';

  @override
  bool get isAvailable => true;

  @override
  int get priority => -1000;

  @override
  Future<void> initialize() async {}

  @override
  Future<Playback> load(
    SoundSource source, {
    double volume = 1.0,
    bool loop = false,
  }) async {
    loaded.add(source);
    return SilentPlayback(simulatedDuration);
  }

  @override
  Future<void> dispose() async {}
}

/// The [Playback] returned by [SilentBackend].
class SilentPlayback implements Playback {
  SilentPlayback(this._duration);

  final Duration _duration;
  final Completer<void> _completer = Completer<void>();
  Timer? _timer;
  PlaybackState _state = PlaybackState.idle;

  @override
  PlaybackState get state => _state;

  @override
  bool get isPlaying => _state == PlaybackState.playing;

  @override
  Future<void> get onComplete => _completer.future;

  @override
  Future<void> play() async {
    if (_state == PlaybackState.disposed) return;
    _timer?.cancel();
    _state = PlaybackState.playing;
    if (_duration == Duration.zero) {
      _finish();
    } else {
      _timer = Timer(_duration, _finish);
    }
  }

  void _finish() {
    if (_state != PlaybackState.playing) return;
    _state = PlaybackState.completed;
    if (!_completer.isCompleted) _completer.complete();
  }

  @override
  Future<void> stop() async {
    if (_state == PlaybackState.disposed) return;
    _timer?.cancel();
    if (_state == PlaybackState.playing) _state = PlaybackState.stopped;
  }

  @override
  Future<void> setVolume(double volume) async {}

  @override
  Future<void> setLooping(bool looping) async {}

  @override
  Future<void> pause() async {
    if (_state == PlaybackState.playing) {
      _timer?.cancel();
      _state = PlaybackState.paused;
    }
  }

  @override
  Future<void> resume() async {
    if (_state == PlaybackState.paused) {
      _state = PlaybackState.playing;
      if (_duration == Duration.zero) {
        _finish();
      } else {
        _timer = Timer(_duration, _finish);
      }
    }
  }

  @override
  Future<void> seek(Duration position) async {}

  @override
  Duration get position => Duration.zero;

  @override
  Duration? get duration => _duration == Duration.zero ? null : _duration;

  @override
  Future<void> dispose() async {
    _timer?.cancel();
    _state = PlaybackState.disposed;
    if (!_completer.isCompleted) _completer.complete();
  }
}
