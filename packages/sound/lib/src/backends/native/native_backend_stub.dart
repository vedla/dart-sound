import '../../sound_backend.dart';

/// Web/no-FFI build: there is no native backend.
SoundBackend? createNativeBackend() => null;
