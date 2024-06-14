# sound_flutter

Flutter integration for [`sound`](../sound) - cross-platform audio with **no
system dependencies** for your users to install.

This package builds the shared `sound_cli` Rust crate automatically (via
[cargokit](https://github.com/ManyMath/cargokit)) and bundles it with your app,
then re-exports the full `sound` API.

```dart
import 'package:sound_flutter/sound_flutter.dart';

await SoundFlutter.ensureInitialized();
final playback = await Sound.playFile('/path/to/chime.wav');
await playback.onComplete;
```

Building requires Rust (`rustup`); eventually precompiled binaries (a cargokit
feature) will remove even that for most users. See the repo

