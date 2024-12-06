# sound

Cross-platform sound playback for Dart and Flutter - with **no system
dependencies** for your users to install. The only build-time requirement
beyond Dart/Flutter is Rust, and eventually not even that (precompiled binaries
via cargokit).

This is a [melos](https://melos.invertase.dev) monorepo:

| Package | What it is |
|---|---|
| [`packages/sound`](packages/sound) | Pure Dart (no Flutter); works in CLI tools. Reaches native audio over `dart:ffi`, with a pluggable backend registry. |
| [`packages/sound_flutter`](packages/sound_flutter) | Flutter plugin. Builds the native library automatically (via cargokit) and re-exports the `sound` API. |
| [`native/sound_cli`](native/sound_cli) | Rust crate (the C ABI behind the FFI backend). Decodes WAV/MP3/OGG/FLAC and plays with no system deps - `dlopen`s `libasound.so.2` on Linux, `libaaudio.so` on Android. |

## Quick start (Dart CLI)

```dart
import 'package:sound/sound.dart';

final playback = await Sound.playFile('chime.wav', volume: 0.8);
await playback.onComplete;
```

Build the native library first: `cargo build --manifest-path native/sound_cli/Cargo.toml`.

## Status


looping, concurrent voices) and the Flutter plugin builds/bundles on Linux
desktop and Android; web compiles. Android/web runtime audio and macOS/Windows
are in progress.
