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

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::io::Cursor;

    /// Writes a `frames`-long WAV (one interleaved value per sample) with the
    /// given spec, filling every sample with `value` (int) for easy assertions.
    fn const_wav(
        channels: u16,
        rate: u32,
        bits: u16,
        format: SampleFormat,
        frames: u32,
        value: i32,
    ) -> Vec<u8> {
        let spec = WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: bits,
            sample_format: format,
        };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = WavWriter::new(&mut buf, spec).unwrap();
            for _ in 0..frames * channels as u32 {
                match format {
                    SampleFormat::Int => w.write_sample(value).unwrap(),
                    SampleFormat::Float => w.write_sample(value as f32 / i16::MAX as f32).unwrap(),
                }
            }
            w.finalize().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn decodes_stereo_and_preserves_frame_count() {
        let wav = const_wav(2, 44_100, 16, SampleFormat::Int, 100, 1000);
        let audio = decode_bytes(&wav, Some("wav")).unwrap();
        assert_eq!(audio.channels, 2);
        assert_eq!(audio.rate, 44_100);
        assert_eq!(audio.samples.len(), 200); // 100 frames * 2 channels
        assert!(audio.samples.iter().all(|&s| s == 1000));
    }

    #[test]
    fn decodes_non_default_sample_rate() {
        let wav = const_wav(1, 48_000, 16, SampleFormat::Int, 64, 0);
        let audio = decode_bytes(&wav, Some("wav")).unwrap();
        assert_eq!(audio.rate, 48_000);
        assert_eq!(audio.channels, 1);
    }

    #[test]
    fn scales_8bit_up_to_16bit() {
        // 8-bit stores ~half-scale; decoding shifts it up into 16-bit range.
        let wav = const_wav(1, 8_000, 8, SampleFormat::Int, 16, 100);
        let audio = decode_bytes(&wav, Some("wav")).unwrap();
        // value 100 (8-bit) << 8 ~ 25600.
        assert!(audio
            .samples
            .iter()
            .all(|&s| (s as i32 - 25_600).abs() < 512));
    }

    #[test]
    fn scales_24bit_down_to_16bit() {
        // 24-bit value shifted down by 8 bits to fit 16-bit.
        let wav = const_wav(1, 44_100, 24, SampleFormat::Int, 16, 0x10_0000);
        let audio = decode_bytes(&wav, Some("wav")).unwrap();
        // 0x100000 >> 8 == 0x1000 == 4096.
        assert!(audio.samples.iter().all(|&s| (s as i32 - 4096).abs() < 4));
    }

    #[test]
    fn decodes_float_wav() {
        let wav = const_wav(1, 44_100, 32, SampleFormat::Float, 32, 16_000);
        let audio = decode_bytes(&wav, Some("wav")).unwrap();
        assert_eq!(audio.channels, 1);
        // ~16000/32767 scaled back to ~16000.
        assert!(audio.samples.iter().all(|&s| (s as i32 - 16_000).abs() < 8));
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode_bytes(&[0u8; 64], None).is_err());
    }

    /// Encodes 16-bit samples to FLAC (lossless) in memory.
    fn encode_flac(samples: &[i32], channels: usize, rate: usize) -> Vec<u8> {
        use flacenc::bitsink::ByteSink;
        use flacenc::component::BitRepr;
        use flacenc::error::Verify;
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .expect("flac config");
        let source = flacenc::source::MemSource::from_samples(samples, channels, 16, rate);
        let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .expect("flac encode");
        let mut sink = ByteSink::new();
        stream.write(&mut sink).expect("flac write");
        sink.as_slice().to_vec()
    }

    #[test]
    fn round_trips_flac_losslessly() {
        // A 440 Hz mono ramp encoded to FLAC and decoded back must match,
        // proving a real compressed codec path (not just WAV).
        let rate = 44_100usize;
        let input: Vec<i32> = (0..2_205)
            .map(|i| {
                (0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin()
                    * i16::MAX as f32) as i32
            })
            .collect();
        let flac = encode_flac(&input, 1, rate);

        let audio = decode_bytes(&flac, Some("flac")).unwrap();
        assert_eq!(audio.channels, 1);
        assert_eq!(audio.rate, 44_100);
        // Fixed-block encoding pads the final block, so the decoded stream is at
        // least as long as the input; the meaningful prefix must match exactly
        // (FLAC is lossless).
        assert!(audio.samples.len() >= input.len());
        for (got, want) in audio.samples.iter().zip(&input) {
            assert_eq!(*got as i32, *want);
        }
    }
}
