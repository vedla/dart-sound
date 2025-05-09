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

Building requires Rust (`rustup`). To remove even that for consumers, the repo
is set up for cargokit **precompiled binaries**:

- `native/sound_cli/cargokit.yaml` holds the public signing key and the
  release URL prefix.
- `.github/workflows/precompile.yml` builds and uploads signed binaries to
  `precompiled_<crate-hash>` releases on tag pushes. It needs a
  `CARGOKIT_PRIVATE_KEY` repository secret - the hex private key printed by
  `dart run build_tool gen-key` (keep it secret; never commit it).

Once a release exists, consuming builds download the matching signed binary
instead of invoking cargo; with none present, cargokit builds from source.


