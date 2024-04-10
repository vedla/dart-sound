//! Pure-Rust WAV decoding (via `hound`) into interleaved S16LE, which is what
//! our ALSA playback path consumes.

use std::io::{Cursor, Read};

use hound::{SampleFormat, WavReader};

/// Decoded audio ready for playback.
pub struct DecodedAudio {
    pub samples: Vec<i16>,
    pub channels: u32,
    pub rate: u32,
}

/// Decodes WAV [bytes] into interleaved S16LE samples.
pub fn decode_wav_bytes(bytes: &[u8]) -> Result<DecodedAudio, String> {
    decode(Cursor::new(bytes))
}

/// Decodes a WAV file at [path].
pub fn decode_wav_file(path: &str) -> Result<DecodedAudio, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {path}: {e}"))?;
    decode(std::io::BufReader::new(file))
}

fn decode<R: Read>(reader: R) -> Result<DecodedAudio, String> {
    let wav = WavReader::new(reader).map_err(|e| format!("not a valid WAV: {e}"))?;
    let spec = wav.spec();
    let channels = spec.channels as u32;
    let rate = spec.sample_rate;

    let samples: Vec<i16> = match spec.sample_format {
        SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            // Sign-extend everything to i32, then scale down to 16-bit.
            let shift = (bits as i32 - 16).max(0);
            let up = (16 - bits as i32).max(0);
            wav.into_samples::<i32>()
                .map(|s| {
                    s.map(|v| ((v >> shift) << up) as i16)
                        .map_err(|e| format!("decode: {e}"))
                })
                .collect::<Result<Vec<i16>, String>>()?
        }
        SampleFormat::Float => wav
            .into_samples::<f32>()
            .map(|s| {
                s.map(|v| (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                    .map_err(|e| format!("decode: {e}"))
            })
            .collect::<Result<Vec<i16>, String>>()?,
    };

    if channels == 0 {
        return Err("WAV reports zero channels".to_string());
    }
    Ok(DecodedAudio {
        samples,
        channels,
        rate,
    })
}
