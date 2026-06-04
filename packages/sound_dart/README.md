# `sound_dart`

Cross-platform sound playback for Dart and Flutter with the goal of no system 
dependencies that need to be installed.  Pure Dart (works in CLI tools) with 
native audio over FFI.

For Flutter apps, see the `sound_flutter` package, which builds the native 
library for you via cargokit and should 'just work'.

The only build-time requirement beyond Dart/Flutter is Rust.  That may be 
worked-around with precompiled binaries (TODO).
