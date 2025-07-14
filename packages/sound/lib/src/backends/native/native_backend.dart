import '../../sound_backend.dart';

// Selects the FFI implementation on native platforms and a no-op on the web,
// so importing `sound` never pulls in `dart:ffi` where it does not exist.
import 'native_backend_ffi.dart'
    if (dart.library.js_interop) 'native_backend_web.dart'
    as impl;

/// Returns the default native backend for this platform, or `null` when none
/// applies (e.g. the web, where playback comes from a different backend).
SoundBackend? createNativeBackend() => impl.createNativeBackend();
