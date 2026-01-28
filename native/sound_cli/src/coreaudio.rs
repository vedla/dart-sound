//! Minimal CoreAudio binding via dlopen of AudioToolbox.framework.
//!
//! Same header-free pattern as ALSA/AAudio: declare the AudioQueue symbols we
//! use and load them at runtime. AudioToolbox ships with every macOS and iOS
//! install, so consumers need no Xcode SDK linked at build time.
//!
//! AudioQueue is callback-driven; `PcmSink` is push-driven. We bridge them
//! with a bounded channel: `write` queues chunks, and the AudioQueue output
//! callback drains them into the buffer it is filling, padding with silence
//! on underrun.

use std::os::raw::{c_int, c_void};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Mutex;

use libloading::Library;

use crate::PcmSink;

const K_LINEAR_PCM: u32 = u32::from_be_bytes(*b"lpcm");
const K_PCM_FLAG_IS_SIGNED_INT: u32 = 0x4;
const K_PCM_FLAG_IS_PACKED: u32 = 0x8;

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioStreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioQueueBuffer {
    audio_data_bytes_capacity: u32,
    audio_data: *mut c_void,
    audio_data_byte_size: u32,
    user_data: *mut c_void,
    packet_description_capacity: u32,
    packet_descriptions: *mut c_void,
    packet_description_count: u32,
}

type AudioQueueRef = *mut c_void;
type AudioQueueBufferRef = *mut AudioQueueBuffer;

type FnNewOutput = unsafe extern "C" fn(
    *const AudioStreamBasicDescription,
    Option<unsafe extern "C" fn(*mut c_void, AudioQueueRef, AudioQueueBufferRef)>,
    *mut c_void,
    *mut c_void,
    *mut c_void,
    u32,
    *mut AudioQueueRef,
) -> c_int;
type FnAlloc = unsafe extern "C" fn(AudioQueueRef, u32, *mut AudioQueueBufferRef) -> c_int;
type FnEnqueue =
    unsafe extern "C" fn(AudioQueueRef, AudioQueueBufferRef, u32, *const c_void) -> c_int;
type FnStart = unsafe extern "C" fn(AudioQueueRef, *mut c_void) -> c_int;
type FnStop = unsafe extern "C" fn(AudioQueueRef, u8) -> c_int;
type FnDispose = unsafe extern "C" fn(AudioQueueRef, u8) -> c_int;
type FnFreeBuffer = unsafe extern "C" fn(AudioQueueRef, AudioQueueBufferRef) -> c_int;

pub struct CoreAudio {
    _lib: Library,
    new_output: FnNewOutput,
    alloc: FnAlloc,
    enqueue: FnEnqueue,
    start: FnStart,
    stop: FnStop,
    dispose: FnDispose,
    free_buffer: FnFreeBuffer,
}

unsafe impl Send for CoreAudio {}
unsafe impl Sync for CoreAudio {}

impl CoreAudio {
    pub fn load() -> Result<CoreAudio, String> {
        // macOS resolves the absolute framework path; iOS resolves the short
        // name from the dyld shared cache. Try both.
        let candidates = [
            "/System/Library/Frameworks/AudioToolbox.framework/AudioToolbox",
            "AudioToolbox.framework/AudioToolbox",
            "AudioToolbox",
        ];
        let mut last_err = String::new();
        let lib = candidates
            .iter()
            .find_map(|p| match unsafe { Library::new(p) } {
                Ok(l) => Some(l),
                Err(e) => {
                    last_err = format!("{p}: {e}");
                    None
                }
            })
            .ok_or_else(|| format!("failed to load AudioToolbox: {last_err}"))?;
        unsafe {
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get($name).map_err(|e| {
                        format!("missing symbol {}: {e}", String::from_utf8_lossy($name))
                    })?
                };
            }
            Ok(CoreAudio {
                new_output: sym!(b"AudioQueueNewOutput\0"),
                alloc: sym!(b"AudioQueueAllocateBuffer\0"),
                enqueue: sym!(b"AudioQueueEnqueueBuffer\0"),
                start: sym!(b"AudioQueueStart\0"),
                stop: sym!(b"AudioQueueStop\0"),
                dispose: sym!(b"AudioQueueDispose\0"),
                free_buffer: sym!(b"AudioQueueFreeBuffer\0"),
                _lib: lib,
            })
        }
    }
}

/// Passed as the AudioQueue user_data. Holds the consumer end of the chunk
/// channel and the re-enqueue entry point the callback needs.
struct Channel {
    rx: Mutex<Receiver<Vec<i16>>>,
    enqueue: FnEnqueue,
    // Tracks frames the callback has fed the device, so `drain` can wait for
    // exactly the amount the producer has pushed.
    frames_drained: std::sync::atomic::AtomicU64,
}

unsafe impl Send for Channel {}
unsafe impl Sync for Channel {}

pub struct CoreAudioPlayback<'a> {
    ca: &'a CoreAudio,
    queue: AudioQueueRef,
    tx: SyncSender<Vec<i16>>,
    channel: Box<Channel>,
    started: std::cell::Cell<bool>,
    channels: u32,
    frames_written: std::cell::Cell<u64>,
    buffers: Vec<AudioQueueBufferRef>,
}

const RING_BUFFERS: u32 = 4;
const FRAMES_PER_BUFFER: u32 = 2048;

impl<'a> CoreAudioPlayback<'a> {
    pub fn open(ca: &'a CoreAudio, channels: u32, rate: u32) -> Result<Self, String> {
        let channels = channels.max(1);
        let bytes_per_frame = channels * 2;
        let asbd = AudioStreamBasicDescription {
            sample_rate: rate as f64,
            format_id: K_LINEAR_PCM,
            format_flags: K_PCM_FLAG_IS_SIGNED_INT | K_PCM_FLAG_IS_PACKED,
            bytes_per_packet: bytes_per_frame,
            frames_per_packet: 1,
            bytes_per_frame,
            channels_per_frame: channels,
            bits_per_channel: 16,
            reserved: 0,
        };
        let (tx, rx) = sync_channel::<Vec<i16>>(RING_BUFFERS as usize * 2);
        let channel = Box::new(Channel {
            rx: Mutex::new(rx),
            enqueue: ca.enqueue,
            frames_drained: std::sync::atomic::AtomicU64::new(0),
        });
        let user_data = &*channel as *const Channel as *mut c_void;
        unsafe {
            let mut queue: AudioQueueRef = std::ptr::null_mut();
            let rc = (ca.new_output)(
                &asbd,
                Some(output_cb),
                user_data,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
                &mut queue,
            );
            if rc != 0 {
                return Err(format!("AudioQueueNewOutput: {rc}"));
            }
            let buffer_bytes = FRAMES_PER_BUFFER * bytes_per_frame;
            let mut buffers = Vec::with_capacity(RING_BUFFERS as usize);
            for _ in 0..RING_BUFFERS {
                let mut buf: AudioQueueBufferRef = std::ptr::null_mut();
                let rc = (ca.alloc)(queue, buffer_bytes, &mut buf);
                if rc != 0 {
                    (ca.dispose)(queue, 1);
                    return Err(format!("AudioQueueAllocateBuffer: {rc}"));
                }
                // Prime each buffer with silence so the callback has work to
                // do as soon as Start fires.
                std::ptr::write_bytes((*buf).audio_data as *mut u8, 0, buffer_bytes as usize);
                (*buf).audio_data_byte_size = buffer_bytes;
                (ca.enqueue)(queue, buf, 0, std::ptr::null());
                buffers.push(buf);
            }
            Ok(CoreAudioPlayback {
                ca,
                queue,
                tx,
                channel,
                started: std::cell::Cell::new(false),
                channels,
                frames_written: std::cell::Cell::new(0),
                buffers,
            })
        }
    }

    fn ensure_started(&self) -> Result<(), String> {
        if self.started.get() {
            return Ok(());
        }
        unsafe {
            let rc = (self.ca.start)(self.queue, std::ptr::null_mut());
            if rc != 0 {
                return Err(format!("AudioQueueStart: {rc}"));
            }
        }
        self.started.set(true);
        Ok(())
    }
}

/// Called by AudioQueue on its own thread when a buffer is ready to be filled.
unsafe extern "C" fn output_cb(
    user_data: *mut c_void,
    queue: AudioQueueRef,
    buf: AudioQueueBufferRef,
) {
    let channel = &*(user_data as *const Channel);
    let cap = (*buf).audio_data_bytes_capacity as usize;
    let dst = (*buf).audio_data as *mut u8;
    let mut written = 0usize;
    loop {
        if written >= cap {
            break;
        }
        let chunk = {
            let rx = channel.rx.lock().unwrap();
            rx.try_recv()
        };
        match chunk {
            Ok(samples) => {
                let bytes = samples.len() * 2;
                let n = bytes.min(cap - written);
                std::ptr::copy_nonoverlapping(samples.as_ptr() as *const u8, dst.add(written), n);
                written += n;
            }
            Err(_) => break,
        }
    }
    if written < cap {
        std::ptr::write_bytes(dst.add(written), 0, cap - written);
    }
    (*buf).audio_data_byte_size = cap as u32;
    channel
        .frames_drained
        .fetch_add(written as u64 / 2, std::sync::atomic::Ordering::Relaxed);
    // Re-arm: hand the same buffer back so the device gets the next slice.
    (channel.enqueue)(queue, buf, 0, std::ptr::null());
}

impl PcmSink for CoreAudioPlayback<'_> {
    fn write(&self, samples: &[i16]) -> Result<(), String> {
        self.ensure_started()?;
        let frames_per_send = FRAMES_PER_BUFFER as usize;
        let stride = frames_per_send * self.channels as usize;
        for chunk in samples.chunks(stride) {
            self.tx
                .send(chunk.to_vec())
                .map_err(|e| format!("audio channel closed: {e}"))?;
        }
        self.frames_written
            .set(self.frames_written.get() + samples.len() as u64 / self.channels as u64);
        Ok(())
    }

    fn drain(&self) {
        // Wait until the callback has consumed every sample we queued; that
        // means the device has it. Then sleep one full ring's worth of audio
        // so the speaker actually plays it out before we stop the queue.
        let target_samples = self.frames_written.get() * self.channels as u64;
        for _ in 0..4000 {
            let drained = self
                .channel
                .frames_drained
                .load(std::sync::atomic::Ordering::Relaxed);
            if drained >= target_samples {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let buffered_ms = (RING_BUFFERS * FRAMES_PER_BUFFER) as u64 * 1000 / 44_100;
        std::thread::sleep(std::time::Duration::from_millis(buffered_ms.min(500)));
        unsafe {
            // immediate=true so the next dispose does not have to wait.
            (self.ca.stop)(self.queue, 1);
        }
    }
}

impl Drop for CoreAudioPlayback<'_> {
    fn drop(&mut self) {
        unsafe {
            for buf in self.buffers.drain(..) {
                (self.ca.free_buffer)(self.queue, buf);
            }
            (self.ca.dispose)(self.queue, 1);
        }
        // Keep Channel alive until after the queue is gone so the callback
        // cannot deref a freed pointer mid-stop.
        let _ = &self.channel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_audiotoolbox() {
        CoreAudio::load().expect("AudioToolbox loads on this host");
    }
}
