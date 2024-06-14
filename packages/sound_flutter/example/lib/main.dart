import 'dart:math';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:sound_flutter/sound_flutter.dart';

void main() {
  runApp(const SoundApp());
}

class SoundApp extends StatefulWidget {
  const SoundApp({super.key});

  @override
  State<SoundApp> createState() => _SoundAppState();
}

class _SoundAppState extends State<SoundApp> {
  String _backend = '...';
  String _status = 'idle';
  Playback? _playback;

  @override
  void initState() {
    super.initState();
    SoundFlutter.ensureInitialized().then((name) {
      if (mounted) setState(() => _backend = name);
    });
  }

  Future<void> _playTone(double freq) async {
    setState(() => _status = 'playing ${freq.toStringAsFixed(0)} Hz');
    final playback = await Sound.playBytes(_sineWav(freq: freq), format: 'wav');
    _playback = playback;
    await playback.onComplete;
    if (mounted && _playback == playback) {
      setState(() => _status = 'idle');
    }
  }

  Future<void> _stop() async {
    await _playback?.stop();
    if (mounted) setState(() => _status = 'stopped');
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        appBar: AppBar(title: const Text('sound_flutter example')),
        body: Center(
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text('Backend: $_backend'),
              Text('Status: $_status'),
              const SizedBox(height: 24),
              Wrap(
                spacing: 12,
                children: [
                  for (final f in [220.0, 440.0, 880.0])
                    ElevatedButton(
                      onPressed: () => _playTone(f),
                      child: Text('${f.toStringAsFixed(0)} Hz'),
                    ),
                  OutlinedButton(onPressed: _stop, child: const Text('Stop')),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Builds a 1-second 16-bit mono WAV tone in memory.
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
