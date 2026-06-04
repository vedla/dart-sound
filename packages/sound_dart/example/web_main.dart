// A minimal web entry point demonstrating `sound_dart` in the browser, where
// the WebAudio backend is selected automatically. Compile with:
//   dart compile js packages/sound_dart/example/web_main.dart -o /tmp/out.js
// and host alongside an HTML page with a button that calls `playTone` (browsers
// require a user gesture before audio can start).
import 'dart:js_interop';
import 'dart:math';
import 'dart:typed_data';

import 'package:sound_dart/sound_dart.dart';
import 'package:web/web.dart' as web;

void main() {
  final button = web.HTMLButtonElement()..textContent = 'Play 440 Hz';
  button.onclick = (web.Event _) {
    playTone();
  }.toJS;
  web.document.body?.append(button);
}

Future<void> playTone() async {
  final playback = await Sound.playBytes(_sineWav(), format: 'wav');
  await playback.onComplete;
  await playback.dispose();
}

Uint8List _sineWav({int sampleRate = 44100, double freq = 440}) {
  final frames = sampleRate;
  final dataBytes = frames * 2;
  final bytes = Uint8List(44 + dataBytes);
  final view = ByteData.sublistView(bytes);
  var o = 0;
  void ascii(String s) {
    for (final c in s.codeUnits) {
      bytes[o++] = c;
    }
  }

  void u32(int v) {
    view.setUint32(o, v, Endian.little);
    o += 4;
  }

  void u16(int v) {
    view.setUint16(o, v, Endian.little);
    o += 2;
  }

  ascii('RIFF');
  u32(36 + dataBytes);
  ascii('WAVE');
  ascii('fmt ');
  u32(16);
  u16(1);
  u16(1);
  u32(sampleRate);
  u32(sampleRate * 2);
  u16(2);
  u16(16);
  ascii('data');
  u32(dataBytes);
  for (var i = 0; i < frames; i++) {
    final v = (0.3 * sin(2 * pi * freq * i / sampleRate) * 32767).round();
    view.setInt16(o, v, Endian.little);
    o += 2;
  }
  return bytes;
}
