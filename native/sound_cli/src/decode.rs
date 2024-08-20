//! Pure-Rust audio decoding (via `symphonia`) into interleaved S16LE, which is
//! what the platform playback paths consume. Supports WAV, MP3, OGG/Vorbis and
//! FLAC with no system dependencies.

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded audio ready for playback.
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub channels: u32,
    pub rate: u32,
}

/// Decodes encoded audio [bytes] (format auto-detected) into interleaved S16LE.
///
/// [format_hint] is an optional file extension (e.g. `mp3`) that helps the
/// probe; decoding still works without it.
pub fn decode_bytes(bytes: &[u8], format_hint: Option<&str>) -> Result<DecodedAudio, String> {
    let source = Cursor::new(bytes.to_vec());
    let mss = MediaSourceStream::new(Box::new(source), Default::default());
    decode(mss, format_hint)
}

/// Decodes an audio file at [path] (format auto-detected) into interleaved S16LE.
pub fn decode_file(path: &str) -> Result<DecodedAudio, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let hint = path.rsplit('.').next();
    decode(mss, hint)
}

fn decode(mss: MediaSourceStream, format_hint: Option<&str>) -> Result<DecodedAudio, String> {
    let mut hint = Hint::new();
    if let Some(ext) = format_hint {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("unsupported or invalid audio: {e}"))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "no audio track found".to_string())?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("no decoder for track: {e}"))?;

    let mut samples: Vec<i16> = Vec::new();
    let mut channels = 0u32;
    let mut rate = 0u32;
    let mut sample_buf: Option<SampleBuffer<i16>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // The standard formats signal end-of-stream as an IO error; treat
            // that as a clean finish.
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(format!("read error: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                channels = spec.channels.count() as u32;
                rate = spec.rate;
                let buf = sample_buf
                    .get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
                buf.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buf.samples());
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(format!("decode error: {e}")),
        }
    }

    if channels == 0 || rate == 0 {
        return Err("decoded zero audio frames".to_string());
    }
    Ok(DecodedAudio {
        samples,
        channels,
        rate,
    })
}
