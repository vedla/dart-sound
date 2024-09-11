@TestOn('vm')
library;

import 'dart:math';
import 'dart:typed_data';

import 'package:sound/sound.dart';
import 'package:sound/src/backends/native/native_backend_ffi.dart';
import 'package:test/test.dart';

/// Builds a valid 16-bit mono PCM WAV containing a short sine tone.
Uint8List buildSineWav({
  int sampleRate = 8000,
  int milliseconds = 150,
  double freq = 440,
}) {
  final frames = sampleRate * milliseconds ~/ 1000;
  final dataBytes = frames * 2;
  final out = BytesData(44 + dataBytes);
  out.setAscii('RIFF');
  out.u32(36 + dataBytes);
  out.setAscii('WAVE');
  out.setAscii('fmt ');
  out.u32(16); // PCM fmt chunk size
  out.u16(1); // PCM
  out.u16(1); // mono
  out.u32(sampleRate);
  out.u32(sampleRate * 2); // byte rate
  out.u16(2); // block align
  out.u16(16); // bits per sample
  out.setAscii('data');
  out.u32(dataBytes);
  for (var i = 0; i < frames; i++) {
    final v = (0.3 * sin(2 * pi * freq * i / sampleRate) * 32767).round();
    out.i16(v);
  }
  return out.bytes;
}

/// Tiny little-endian byte writer.
class BytesData {
  BytesData(int size) : bytes = Uint8List(size), _view = ByteData(0) {
    _view = ByteData.sublistView(bytes);
  }
  final Uint8List bytes;
  ByteData _view;
  int _offset = 0;

  void setAscii(String s) {
    for (final c in s.codeUnits) {
      bytes[_offset++] = c;
    }
  }

  void u32(int v) {
    _view.setUint32(_offset, v, Endian.little);
    _offset += 4;
  }

  void u16(int v) {
    _view.setUint16(_offset, v, Endian.little);
    _offset += 2;
  }

  void i16(int v) {
    _view.setInt16(_offset, v, Endian.little);
    _offset += 2;
  }
}

void main() {
  final available = NativeLibrary.open() != null;

  group('FFI playback (end-to-end)', () {
    setUp(() => Sound.reset());
    tearDown(() => Sound.reset());

    test('plays a WAV to natural completion', () async {
      final backend = FfiBackend();
      expect(backend.isAvailable, isTrue);
      Sound.registerBackend(backend, makeActive: true);

      final playback = await Sound.playBytes(buildSineWav(), format: 'wav');
      expect(playback.isPlaying, isTrue);
      await playback.onComplete.timeout(const Duration(seconds: 5));
      expect(playback.state, PlaybackState.completed);
      await playback.dispose();
    });

    test('plays at reduced volume to completion', () async {
      Sound.registerBackend(FfiBackend(), makeActive: true);
      final playback = await Sound.playBytes(
        buildSineWav(milliseconds: 120),
        format: 'wav',
        volume: 0.25,
      );
      await playback.setVolume(0.5);
      await playback.onComplete.timeout(const Duration(seconds: 5));
      expect(playback.state, PlaybackState.completed);
      await playback.dispose();
    });

    test('stop ends playback early', () async {
      Sound.registerBackend(FfiBackend(), makeActive: true);
      final playback = await Sound.playBytes(
        buildSineWav(milliseconds: 2000),
        format: 'wav',
      );
      await Future<void>.delayed(const Duration(milliseconds: 100));
      await playback.stop();
      expect(playback.state, PlaybackState.stopped);
      await playback.dispose();
    });
  }, skip: available ? false : 'sound_cli library not built');
}
