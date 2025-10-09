import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sound_flutter/sound_flutter.dart';

/// An [AssetBundle] that returns fixed bytes for any key, so asset loading can
/// be tested without a built asset bundle.
class _FakeBundle extends CachingAssetBundle {
  _FakeBundle(this._data);
  final Uint8List _data;

  @override
  Future<ByteData> load(String key) async => ByteData.sublistView(_data);
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  setUp(() => Sound.reset());
  tearDown(() => Sound.reset());

  test('loadAsset reads bundle bytes and dispatches to the backend', () async {
    final silent = SilentBackend();
    Sound.registerBackend(silent, makeActive: true);

    final wav = Uint8List.fromList(List<int>.generate(1644, (i) => i % 256));
    final playback = await SoundFlutter.loadAsset(
      'assets/test_tone.wav',
      bundle: _FakeBundle(wav),
    );

    expect(playback, isNotNull);
    expect(silent.loaded, hasLength(1));
    final source = silent.loaded.single;
    expect(source, isA<BytesSource>());
    expect((source as BytesSource).bytes.length, 1644);
    expect(source.format, 'wav');
  });

  test('ensureInitialized returns the active backend name', () async {
    Sound.registerBackend(SilentBackend(), makeActive: true);
    expect(await SoundFlutter.ensureInitialized(), 'silent');
  });
}
