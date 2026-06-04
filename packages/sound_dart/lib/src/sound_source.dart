import 'dart:typed_data';

/// Describes audio to be played, independent of how a backend loads it.
///
/// This is a sealed hierarchy so backends can exhaustively switch over the
/// concrete kinds they support and surface a clear error for the rest.
sealed class SoundSource {
  const SoundSource();

  /// Audio held in memory (e.g. a decoded asset or downloaded file).
  ///
  /// [format] is an optional hint such as `wav`, `mp3` or `ogg`. When omitted,
  /// a backend may sniff the container from the bytes.
  const factory SoundSource.bytes(Uint8List bytes, {String? format}) =
      BytesSource;

  /// Audio read from a file on disk by [path].
  const factory SoundSource.file(String path) = FileSource;
}

/// In-memory audio. See [SoundSource.bytes].
final class BytesSource extends SoundSource {
  const BytesSource(this.bytes, {this.format});

  final Uint8List bytes;

  /// Optional container/format hint (e.g. `wav`).
  final String? format;
}

/// A file on disk. See [SoundSource.file].
final class FileSource extends SoundSource {
  const FileSource(this.path);

  final String path;
}
