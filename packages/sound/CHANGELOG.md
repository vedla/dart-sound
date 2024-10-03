# Changelog

## 0.0.1

- Initial scaffolding.
- Core API: `Sound`, `SoundSource`, `Playback`, pluggable `SoundBackend`s.
- FFI backend that plays WAV via the `sound_cli` Rust library; Linux
  playback over ALSA (PipeWire). Web-safe via conditional imports.
- WebAudio backend for the browser (selected automatically on web).
- Native decoding of WAV/MP3/OGG-Vorbis/FLAC via the pure-Rust `symphonia`
  (no system dependencies); the browser decodes natively on web.
- Per-voice volume (`setVolume`, `volume:`) and looping (`setLooping`, `loop:`).
- CLI example and a web example.
