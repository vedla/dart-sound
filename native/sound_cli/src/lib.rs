//! C ABI for the `sound` package's native playback.
//!
//! The Dart `FfiBackend` calls these functions. Each `sound_play_*` call
//! decodes the audio and spawns a playback thread, returning an opaque voice
//! id that can be polled, stopped, and freed. Concurrent voices are mixed by
//! the system audio server (PipeWire/PulseAudio/dmix on Linux).

mod wav;

#[cfg(target_os = "linux")]
mod alsa;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use wav::DecodedAudio;

// Voice state codes, mirrored by the Dart side.
const STATE_PLAYING: u8 = 0;
const STATE_COMPLETED: u8 = 1;
const STATE_STOPPED: u8 = 2;
const STATE_ERROR: u8 = 3;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_last_error(msg: impl Into<String>) {
    let c = CString::new(msg.into()).unwrap_or_default();
    LAST_ERROR.with(|e| *e.borrow_mut() = c);
}

struct Voice {
    stop: Arc<std::sync::atomic::AtomicBool>,
    state: Arc<AtomicU8>,
    error: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

/// Owns the audio backend and the table of live voices.
pub struct Player {
    voices: Mutex<HashMap<u64, Voice>>,
    next_id: AtomicU64,
    #[cfg(target_os = "linux")]
    alsa: Arc<alsa::Alsa>,
}

impl Player {
    fn new() -> Result<Player, String> {
        Ok(Player {
            voices: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            #[cfg(target_os = "linux")]
            alsa: Arc::new(alsa::Alsa::load()?),
        })
    }

    fn spawn(&self, audio: DecodedAudio) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let state = Arc::new(AtomicU8::new(STATE_PLAYING));
        let error = Arc::new(Mutex::new(None));

        let handle = self.start_thread(audio, stop.clone(), state.clone(), error.clone());

        self.voices.lock().unwrap().insert(
            id,
            Voice {
                stop,
                state,
                error,
                handle: Some(handle),
            },
        );
        id
    }

    #[cfg(target_os = "linux")]
    fn start_thread(
        &self,
        audio: DecodedAudio,
        stop: Arc<std::sync::atomic::AtomicBool>,
        state: Arc<AtomicU8>,
        error: Arc<Mutex<Option<String>>>,
    ) -> JoinHandle<()> {
        let alsa = self.alsa.clone();
        std::thread::spawn(move || {
            play_alsa(&alsa, audio, &stop, &state, &error);
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn start_thread(
        &self,
        _audio: DecodedAudio,
        _stop: Arc<std::sync::atomic::AtomicBool>,
        state: Arc<AtomicU8>,
        error: Arc<Mutex<Option<String>>>,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || {
            *error.lock().unwrap() =
                Some("native playback is not implemented on this platform yet".into());
            state.store(STATE_ERROR, Ordering::SeqCst);
        })
    }
}

#[cfg(target_os = "linux")]
fn play_alsa(
    alsa: &alsa::Alsa,
    audio: DecodedAudio,
    stop: &std::sync::atomic::AtomicBool,
    state: &AtomicU8,
    error: &Mutex<Option<String>>,
) {
    let pcm = match alsa::PcmPlayback::open(alsa, audio.channels, audio.rate) {
        Ok(p) => p,
        Err(e) => {
            *error.lock().unwrap() = Some(e);
            state.store(STATE_ERROR, Ordering::SeqCst);
            return;
        }
    };

    // ~2048 frames per write keeps stop latency under ~50 ms at 44.1 kHz.
    let chunk = audio.channels as usize * 2048;
    let mut i = 0;
    while i < audio.samples.len() {
        if stop.load(Ordering::SeqCst) {
            state.store(STATE_STOPPED, Ordering::SeqCst);
            return;
        }
        let end = (i + chunk).min(audio.samples.len());
        if let Err(e) = pcm.write(&audio.samples[i..end]) {
            *error.lock().unwrap() = Some(e);
            state.store(STATE_ERROR, Ordering::SeqCst);
            return;
        }
        i = end;
    }
    pcm.drain();
    let final_state = if stop.load(Ordering::SeqCst) {
        STATE_STOPPED
    } else {
        STATE_COMPLETED
    };
    state.store(final_state, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Creates a player. Returns null on failure; see [sound_last_error].
#[no_mangle]
pub extern "C" fn sound_player_new() -> *mut Player {
    match Player::new() {
        Ok(p) => Box::into_raw(Box::new(p)),
        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

/// Frees a player created by [sound_player_new], stopping all voices.
#[no_mangle]
pub extern "C" fn sound_player_free(player: *mut Player) {
    if player.is_null() {
        return;
    }
    let player = unsafe { Box::from_raw(player) };
    let mut voices = player.voices.lock().unwrap();
    for (_, mut voice) in voices.drain() {
        voice.stop.store(true, Ordering::SeqCst);
        if let Some(h) = voice.handle.take() {
            let _ = h.join();
        }
    }
}

fn with_player<'a>(player: *mut Player) -> Option<&'a Player> {
    if player.is_null() {
        set_last_error("null player");
        None
    } else {
        Some(unsafe { &*player })
    }
}

/// Decodes WAV bytes and starts playback. Returns a voice id, or 0 on error.
///
/// # Safety
/// `data` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn sound_play_wav_bytes(
    player: *mut Player,
    data: *const u8,
    len: usize,
) -> u64 {
    let Some(player) = with_player(player) else {
        return 0;
    };
    if data.is_null() {
        set_last_error("null data");
        return 0;
    }
    let bytes = std::slice::from_raw_parts(data, len);
    match wav::decode_wav_bytes(bytes) {
        Ok(audio) => player.spawn(audio),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

/// Decodes a WAV file and starts playback. Returns a voice id, or 0 on error.
///
/// # Safety
/// `path` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn sound_play_wav_file(player: *mut Player, path: *const c_char) -> u64 {
    let Some(player) = with_player(player) else {
        return 0;
    };
    if path.is_null() {
        set_last_error("null path");
        return 0;
    }
    let path = match CStr::from_ptr(path).to_str() {
        Ok(p) => p,
        Err(_) => {
            set_last_error("path is not valid UTF-8");
            return 0;
        }
    };
    match wav::decode_wav_file(path) {
        Ok(audio) => player.spawn(audio),
        Err(e) => {
            set_last_error(e);
            0
        }
    }
}

/// Returns the voice state (0 playing, 1 completed, 2 stopped, 3 error), or -1
/// if the id is unknown.
#[no_mangle]
pub extern "C" fn sound_voice_state(player: *mut Player, id: u64) -> c_int {
    let Some(player) = with_player(player) else {
        return -1;
    };
    let voices = player.voices.lock().unwrap();
    match voices.get(&id) {
        Some(v) => v.state.load(Ordering::SeqCst) as c_int,
        None => -1,
    }
}

/// Signals a voice to stop. Returns 0 on success, -1 if the id is unknown.
#[no_mangle]
pub extern "C" fn sound_stop(player: *mut Player, id: u64) -> c_int {
    let Some(player) = with_player(player) else {
        return -1;
    };
    let voices = player.voices.lock().unwrap();
    match voices.get(&id) {
        Some(v) => {
            v.stop.store(true, Ordering::SeqCst);
            0
        }
        None => -1,
    }
}

/// Joins a voice's thread and removes it. Returns 0 on success, -1 if unknown.
#[no_mangle]
pub extern "C" fn sound_voice_free(player: *mut Player, id: u64) -> c_int {
    let Some(player) = with_player(player) else {
        return -1;
    };
    let voice = player.voices.lock().unwrap().remove(&id);
    match voice {
        Some(mut v) => {
            v.stop.store(true, Ordering::SeqCst);
            if let Some(h) = v.handle.take() {
                let _ = h.join();
            }
            0
        }
        None => -1,
    }
}

/// Copies a voice's error message into the thread-local error buffer and
/// returns it, or returns an empty string if there is none.
#[no_mangle]
pub extern "C" fn sound_voice_error(player: *mut Player, id: u64) -> *const c_char {
    if let Some(player) = with_player(player) {
        let voices = player.voices.lock().unwrap();
        if let Some(v) = voices.get(&id) {
            if let Some(msg) = v.error.lock().unwrap().clone() {
                set_last_error(msg);
            } else {
                set_last_error("");
            }
        }
    }
    sound_last_error()
}

/// Returns the most recent error message on the calling thread.
#[no_mangle]
pub extern "C" fn sound_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};
    use std::io::Cursor;

    fn sine_wav() -> Vec<u8> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = WavWriter::new(&mut buf, spec).unwrap();
            for i in 0..4_410 {
                let t = i as f32 / 44_100.0;
                let v =
                    (0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() * i16::MAX as f32) as i16;
                w.write_sample(v).unwrap();
            }
            w.finalize().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn decodes_wav_bytes() {
        let bytes = sine_wav();
        let audio = wav::decode_wav_bytes(&bytes).unwrap();
        assert_eq!(audio.channels, 1);
        assert_eq!(audio.rate, 44_100);
        assert_eq!(audio.samples.len(), 4_410);
    }
}
