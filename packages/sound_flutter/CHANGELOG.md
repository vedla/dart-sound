## 0.0.1

- Flutter integration for `sound`: builds the shared `sound_cli` Rust crate
  via cargokit and bundles it. Re-exports the `sound` API and adds
  `SoundFlutter.ensureInitialized()`. Verified building/bundling/loading on
  Linux desktop.
