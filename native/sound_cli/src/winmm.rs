//! Minimal waveOut binding via dlopen of winmm.dll.
//!
//! Same header-free pattern as ALSA/AAudio/CoreAudio: declare the symbols we
//! use and load them at runtime. winmm.dll ships with every Windows install.

use std::cell::UnsafeCell;

use libloading::Library;

use crate::PcmSink;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_MAPPER: u32 = 0xFFFF_FFFF;
const CALLBACK_NULL: u32 = 0;
const WHDR_DONE: u32 = 0x0000_0001;
const WHDR_PREPARED: u32 = 0x0000_0002;
const MMSYSERR_NOERROR: u32 = 0;

type HWaveOut = *mut std::ffi::c_void;

#[repr(C, packed(1))]
#[derive(Clone, Copy)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C)]
struct WaveHdr {
    data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    loops: u32,
    next: *mut WaveHdr,
    reserved: usize,
}

type FnWaveOutOpen = unsafe extern "system" fn(
    *mut HWaveOut,
    u32,
    *const WaveFormatEx,
    usize,
    usize,
    u32,
) -> u32;
type FnWaveOutClose = unsafe extern "system" fn(HWaveOut) -> u32;
type FnWaveOutReset = unsafe extern "system" fn(HWaveOut) -> u32;
type FnWaveOutPrepareHeader = unsafe extern "system" fn(HWaveOut, *mut WaveHdr, u32) -> u32;
type FnWaveOutUnprepareHeader = unsafe extern "system" fn(HWaveOut, *mut WaveHdr, u32) -> u32;
type FnWaveOutWrite = unsafe extern "system" fn(HWaveOut, *mut WaveHdr, u32) -> u32;

pub struct WinMM {
    _lib: Library,
    wave_out_open: FnWaveOutOpen,
    wave_out_close: FnWaveOutClose,
    wave_out_reset: FnWaveOutReset,
    wave_out_prepare_header: FnWaveOutPrepareHeader,
    wave_out_unprepare_header: FnWaveOutUnprepareHeader,
    wave_out_write: FnWaveOutWrite,
}

unsafe impl Send for WinMM {}
unsafe impl Sync for WinMM {}

impl WinMM {
    pub fn load() -> Result<WinMM, String> {
        unsafe {
            let lib = Library::new("winmm.dll")
                .map_err(|e| format!("failed to load winmm.dll: {e}"))?;
            macro_rules! sym {
                ($name:literal) => {
                    *lib.get($name).map_err(|e| {
                        format!("missing symbol {}: {e}", String::from_utf8_lossy($name))
                    })?
                };
            }
            Ok(WinMM {
                wave_out_open: sym!(b"waveOutOpen\0"),
                wave_out_close: sym!(b"waveOutClose\0"),
                wave_out_reset: sym!(b"waveOutReset\0"),
                wave_out_prepare_header: sym!(b"waveOutPrepareHeader\0"),
                wave_out_unprepare_header: sym!(b"waveOutUnprepareHeader\0"),
                wave_out_write: sym!(b"waveOutWrite\0"),
                _lib: lib,
            })
        }
    }
}

const NUM_BUFFERS: usize = 4;
const FRAMES_PER_BUFFER: usize = 2048;

struct BufferRing {
    hdrs: Vec<WaveHdr>,
    datas: Vec<Vec<u8>>,
    next: usize,
}

pub struct WinMMPlayback<'a> {
    wmm: &'a WinMM,
    handle: HWaveOut,
    ring: UnsafeCell<BufferRing>,
    channels: u32,
}

impl<'a> WinMMPlayback<'a> {
    pub fn open(wmm: &'a WinMM, channels: u32, rate: u32) -> Result<Self, String> {
        let channels = channels.max(1);
        let block_align = channels as u16 * 2;
        let wfx = WaveFormatEx {
            format_tag: WAVE_FORMAT_PCM,
            channels: channels as u16,
            samples_per_sec: rate,
            avg_bytes_per_sec: rate * block_align as u32,
            block_align,
            bits_per_sample: 16,
            cb_size: 0,
        };
        let mut handle: HWaveOut = std::ptr::null_mut();
        unsafe {
            let rc = (wmm.wave_out_open)(
                &mut handle,
                WAVE_MAPPER,
                &wfx,
                0,
                0,
                CALLBACK_NULL,
            );
            if rc != MMSYSERR_NOERROR {
                return Err(format!("waveOutOpen failed: error {rc}"));
            }
        }

        let buf_bytes = FRAMES_PER_BUFFER * channels as usize * 2;
        let mut hdrs = Vec::with_capacity(NUM_BUFFERS);
        let mut datas = Vec::with_capacity(NUM_BUFFERS);
        for _ in 0..NUM_BUFFERS {
            let data = vec![0u8; buf_bytes];
            hdrs.push(WaveHdr {
                data: data.as_ptr() as *mut u8,
                buffer_length: buf_bytes as u32,
                bytes_recorded: 0,
                user: 0,
                flags: 0,
                loops: 0,
                next: std::ptr::null_mut(),
                reserved: 0,
            });
            datas.push(data);
        }

        Ok(WinMMPlayback {
            wmm,
            handle,
            ring: UnsafeCell::new(BufferRing {
                hdrs,
                datas,
                next: 0,
            }),
            channels,
        })
    }

    fn wait_for_hdr(hdr: &WaveHdr) {
        loop {
            let flags = hdr.flags;
            if flags & WHDR_PREPARED == 0 || flags & WHDR_DONE != 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

impl PcmSink for WinMMPlayback<'_> {
    fn write(&self, samples: &[i16]) -> Result<(), String> {
        let bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 2)
        };
        let buf_bytes = FRAMES_PER_BUFFER * self.channels as usize * 2;
        // Safety: PcmSink::write is only called from a single playback thread.
        let ring = unsafe { &mut *self.ring.get() };
        for chunk in bytes.chunks(buf_bytes) {
            let idx = ring.next % NUM_BUFFERS;
            Self::wait_for_hdr(&ring.hdrs[idx]);

            if ring.hdrs[idx].flags & WHDR_PREPARED != 0 {
                unsafe {
                    (self.wmm.wave_out_unprepare_header)(
                        self.handle,
                        &mut ring.hdrs[idx],
                        std::mem::size_of::<WaveHdr>() as u32,
                    );
                }
            }

            let len = chunk.len().min(ring.datas[idx].len());
            ring.datas[idx][..len].copy_from_slice(&chunk[..len]);
            ring.hdrs[idx].data = ring.datas[idx].as_ptr() as *mut u8;
            ring.hdrs[idx].buffer_length = len as u32;
            ring.hdrs[idx].flags = 0;

            unsafe {
                let rc = (self.wmm.wave_out_prepare_header)(
                    self.handle,
                    &mut ring.hdrs[idx],
                    std::mem::size_of::<WaveHdr>() as u32,
                );
                if rc != MMSYSERR_NOERROR {
                    return Err(format!("waveOutPrepareHeader: error {rc}"));
                }
                let rc = (self.wmm.wave_out_write)(
                    self.handle,
                    &mut ring.hdrs[idx],
                    std::mem::size_of::<WaveHdr>() as u32,
                );
                if rc != MMSYSERR_NOERROR {
                    return Err(format!("waveOutWrite: error {rc}"));
                }
            }
            ring.next += 1;
        }
        Ok(())
    }

    fn drain(&self) {
        let ring = unsafe { &*self.ring.get() };
        for hdr in &ring.hdrs {
            Self::wait_for_hdr(hdr);
        }
    }
}

impl Drop for WinMMPlayback<'_> {
    fn drop(&mut self) {
        let ring = self.ring.get_mut();
        unsafe {
            (self.wmm.wave_out_reset)(self.handle);
            for hdr in &mut ring.hdrs {
                if hdr.flags & WHDR_PREPARED != 0 {
                    (self.wmm.wave_out_unprepare_header)(
                        self.handle,
                        hdr,
                        std::mem::size_of::<WaveHdr>() as u32,
                    );
                }
            }
            (self.wmm.wave_out_close)(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_winmm() {
        WinMM::load().expect("winmm.dll loads on this host");
    }
}
