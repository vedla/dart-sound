import 'dart:typed_data';

import 'package:sound/sound.dart';
import 'package:test/test.dart';

/// A backend whose availability and priority are configurable, for exercising
/// selection logic.
class _ProbeBackend extends SoundBackend {
  _ProbeBackend(this.name, {this.available = true, this.priority = 0});

  @override
  final String name;
  final bool available;
  @override
  final int priority;

  int initializeCalls = 0;

  @override
  bool get isAvailable => available;

  @override
  Future<void> initialize() async => initializeCalls++;

  @override
  Future<Playback> load(
    SoundSource source, {
    double volume = 1.0,
    bool loop = false,
  }) async => SilentPlayback(Duration.zero);

  @override
  Future<void> dispose() async {}
}

void main() {
  tearDown(() => Sound.reset());

  group('backend selection', () {
    // Probe priorities are kept well above the real defaults (silent at -1000,
    // the FFI backend at 100) so these tests are deterministic regardless of
    // whether the native library happens to be built on the dev host.
    test('always registers the silent fallback', () {
      expect(Sound.backends.map((b) => b.name), contains('silent'));
      expect(Sound.backend.isAvailable, isTrue);
    });

    test('prefers the highest-priority available backend', () {
      Sound.registerBackend(_ProbeBackend('low', priority: 5000));
      Sound.registerBackend(_ProbeBackend('high', priority: 9000));
      expect(Sound.backend.name, 'high');
    });

    test('skips unavailable backends', () {
      Sound.registerBackend(
        _ProbeBackend('unavailable', priority: 9999, available: false),
      );
      Sound.registerBackend(_ProbeBackend('usable', priority: 9000));
      expect(Sound.backend.name, 'usable');
    });

    test('re-selects when a higher-priority backend is registered later', () {
      final first = Sound.backend.name;
      Sound.registerBackend(_ProbeBackend('better', priority: 9000));
      expect(Sound.backend.name, 'better');
      expect(first, isNot('better'));
    });

    test('makeActive pins a backend regardless of priority', () {
      Sound.registerBackend(_ProbeBackend('high', priority: 9000));
      final low = _ProbeBackend('low', priority: 1);
      Sound.registerBackend(low, makeActive: true);
      expect(Sound.backend.name, 'low');
    });

    test('useBackend pins by name and throws on unknown', () {
      Sound.registerBackend(_ProbeBackend('a', priority: 1));
      Sound.registerBackend(_ProbeBackend('b', priority: 2));
      Sound.useBackend('a');
      expect(Sound.backend.name, 'a');
      expect(
        () => Sound.useBackend('nope'),
        throwsA(isA<NoBackendAvailableException>()),
      );
    });
  });

  group('playback', () {
    test('play loads and starts via the active backend', () async {
      final silent = SilentBackend();
      Sound.registerBackend(silent, makeActive: true);
      final playback = await Sound.playBytes(
        Uint8List.fromList([1, 2, 3]),
        format: 'wav',
      );
      expect(silent.loaded, hasLength(1));
      expect(silent.loaded.single, isA<BytesSource>());
      await playback.onComplete;
      expect(playback.state, PlaybackState.completed);
    });

    test('initialize is called once per backend', () async {
      final probe = _ProbeBackend('probe', priority: 10);
      Sound.registerBackend(probe, makeActive: true);
      await Sound.playBytes(Uint8List(0));
      await Sound.playBytes(Uint8List(0));
      expect(probe.initializeCalls, 1);
    });
  });

  group('SilentPlayback', () {
    test('stop before completion marks stopped', () async {
      final pb = SilentPlayback(const Duration(seconds: 10));
      await pb.play();
      expect(pb.isPlaying, isTrue);
      await pb.stop();
      expect(pb.state, PlaybackState.stopped);
    });

    test('dispose completes onComplete and blocks further play', () async {
      final pb = SilentPlayback(const Duration(seconds: 10));
      await pb.dispose();
      await pb.onComplete; // should not hang
      await pb.play();
      expect(pb.state, PlaybackState.disposed);
    });
  });
}
