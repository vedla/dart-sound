# Changelog

## 0.0.1

- Initial scaffolding.
- Core API: `Sound`, `SoundSource`, `Playback`, pluggable `SoundBackend`s.
- FFI backend that plays WAV via the `sound_cli` Rust library; Linux
  playback over ALSA (PipeWire). Web-safe via conditional imports.
- WebAudio backend for the browser (selected automatically on web).
- CLI example and a web example.
