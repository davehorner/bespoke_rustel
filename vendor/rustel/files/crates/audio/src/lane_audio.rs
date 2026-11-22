//! Native shared-memory stereo stems (Windows and POSIX). Layout is mirrored by RustelAudio.cpp.
//! Creation, page commitment and destruction occur off the audio callback.
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Memory::{
            CreateFileMappingW, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile,
            PAGE_READWRITE, UnmapViewOfFile,
        },
        SystemInformation::GetTickCount64,
    },
};

pub const SLOTS: usize = 15; // Orbit 15 is reserved for unexported voices.
pub const CAPACITY: usize = 16_384;
const DATA: usize = 4096;
const BYTES: usize = DATA + SLOTS * CAPACITY * 2 * 8;
const MAGIC: u32 = 0x524c4131;

pub struct LaneAudioMap {
    name: String,
    #[cfg(windows)]
    handle: HANDLE,
    #[cfg(unix)]
    fd: libc::c_int,
    base: *mut u8,
    monitor_fades: [AtomicU32; SLOTS],
}
// SAFETY: mapping lifetime is Arc-owned; all concurrent accesses are aligned atomics.
// Only the admitted device callback writes samples and the producer never mutates them.
unsafe impl Send for LaneAudioMap {}
unsafe impl Sync for LaneAudioMap {}
impl LaneAudioMap {
    pub fn enabled() -> bool {
        std::env::var("RUSTEL_LANE_AUDIO").is_ok_and(|s| s == "1")
            && std::env::var("RUSTEL_MUSIC_LINK").is_ok_and(|s| s == "1")
    }
    pub fn from_environment() -> Option<std::sync::Arc<Self>> {
        if !Self::enabled() {
            return None;
        }
        match Self::create() {
            Ok(map) => Some(std::sync::Arc::new(map)),
            Err(error) => {
                eprintln!("lane audio export unavailable: {error}");
                None
            }
        }
    }
    pub fn create() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        #[cfg(windows)]
        let name = format!(
            "Local\\RustelAudio-{}-{}-{nonce}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        // macOS limits POSIX shm names to 31 bytes, including the initial slash.
        #[cfg(unix)]
        let name = format!(
            "/rla-{:x}-{:x}",
            std::process::id(),
            nonce as u64 ^ NEXT.fetch_add(1, Ordering::Relaxed)
        );
        #[cfg(windows)]
        let (handle, base) = {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let handle = unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    std::ptr::null(),
                    PAGE_READWRITE,
                    0,
                    BYTES as u32,
                    wide.as_ptr(),
                )
            };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, BYTES) };
            if view.Value.is_null() {
                let error = std::io::Error::last_os_error();
                unsafe {
                    CloseHandle(handle);
                }
                return Err(error);
            }
            (handle, view.Value.cast::<u8>())
        };
        #[cfg(unix)]
        let (fd, base) = {
            let c_name = std::ffi::CString::new(name.as_str()).expect("generated ASCII name");
            // Exclusive creation and mode 0600 keep streams private to this user.
            let fd = unsafe {
                libc::shm_open(
                    c_name.as_ptr(),
                    libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                    0o600,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let cleanup = || unsafe {
                libc::close(fd);
                libc::shm_unlink(c_name.as_ptr());
            };
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0
                || unsafe { libc::ftruncate(fd, BYTES as libc::off_t) } < 0
            {
                let error = std::io::Error::last_os_error();
                cleanup();
                return Err(error);
            }
            let base = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    BYTES,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            };
            if base == libc::MAP_FAILED {
                let error = std::io::Error::last_os_error();
                cleanup();
                return Err(error);
            }
            (fd, base.cast::<u8>())
        };
        let map = Self {
            name,
            #[cfg(windows)]
            handle,
            #[cfg(unix)]
            fd,
            base,
            monitor_fades: std::array::from_fn(|_| AtomicU32::new(0)),
        };
        // Touch every page and initialize the storage before publishing its name.
        for offset in (0..BYTES).step_by(8) {
            unsafe {
                map.base
                    .add(offset)
                    .cast::<AtomicU64>()
                    .write(AtomicU64::new(0));
            }
        }
        map.u32(4).store(1, Ordering::Relaxed);
        map.u32(12).store(SLOTS as u32, Ordering::Relaxed);
        map.u32(16).store(CAPACITY as u32, Ordering::Relaxed);
        map.u32(0).store(MAGIC, Ordering::Release);
        Ok(map)
    }
    fn u32(&self, offset: usize) -> &AtomicU32 {
        unsafe { &*self.base.add(offset).cast::<AtomicU32>() }
    }
    fn u64(&self, offset: usize) -> &AtomicU64 {
        unsafe { &*self.base.add(offset).cast::<AtomicU64>() }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Called by the single admitted callback after each render chunk. The taps
    /// include the whole effect chain, even when no new notes are scheduled.
    pub fn write(&self, stems: &crate::scalar::Stems, rate: u32, frames: usize, main: &mut [f32]) {
        let old_rate = self.u32(8).load(Ordering::Relaxed);
        if old_rate != rate {
            self.u32(8).store(rate, Ordering::Release);
            self.u64(32).fetch_add(1, Ordering::AcqRel);
        }
        let start = self.u64(24).load(Ordering::Relaxed);
        // Both native peers use the same OS monotonic clock.
        let now = monotonic_millis();
        for slot in 0..SLOTS {
            let samples = stems.orbit(slot, frames);
            let lease = self.u64(64 + slot * 64).load(Ordering::Acquire);
            let active = lease != 0 && now.saturating_sub(lease) < 500;
            let target = if active { 1.0f32 } else { 0.0 };
            let step = 1.0 / (rate as f32 * 0.01).max(1.0);
            let mut fade = f32::from_bits(self.monitor_fades[slot].load(Ordering::Relaxed));
            for frame in 0..frames {
                fade = (fade + (target - fade).clamp(-step, step)).clamp(0.0, 1.0);
                let absolute = start + frame as u64;
                for ch in 0..2 {
                    let sample = samples[frame * 2 + ch];
                    let tag = (absolute as u32 as u64) << 32;
                    let at = DATA
                        + ((slot * CAPACITY + (absolute as usize & (CAPACITY - 1))) * 2 + ch) * 8;
                    self.u64(at)
                        .store(tag | u64::from(sample.to_bits()), Ordering::Release);
                    if fade != 0.0 {
                        main[frame * 2 + ch] -= sample * fade;
                    }
                }
            }
            self.monitor_fades[slot].store(fade.to_bits(), Ordering::Relaxed);
        }
        self.u64(24).store(start + frames as u64, Ordering::Release);
    }
}
/// Shared clock domain for monitor leases; no allocation or locking.
pub fn monotonic_millis() -> u64 {
    #[cfg(windows)]
    {
        unsafe { GetTickCount64() }
    }
    #[cfg(unix)]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) } != 0 {
            return 0;
        }
        time.tv_sec as u64 * 1000 + time.tv_nsec as u64 / 1_000_000
    }
}
impl Drop for LaneAudioMap {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.base.cast(),
            });
            CloseHandle(self.handle);
        }
        #[cfg(unix)]
        unsafe {
            libc::munmap(self.base.cast(), BYTES);
            libc::close(self.fd);
            let name = std::ffi::CString::new(self.name.as_str()).expect("generated name");
            libc::shm_unlink(name.as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AudioBackend, DelayControls, Envelope, OnsetEvent, ScalarBackend};
    fn backend() -> ScalarBackend {
        let mut backend = ScalarBackend::new();
        backend.init(48000).unwrap();
        backend.capture_stems();
        backend
    }
    fn note(slot: u8, frequency: f32, duration: f32) -> OnsetEvent {
        let mut n = OnsetEvent::new(0, frequency, 0.1, duration);
        n.controls.orbit = slot;
        n.controls.envelope = Envelope {
            attack_secs: 0.001,
            decay_secs: 0.0,
            sustain: 1.0,
            release_secs: 0.001,
        };
        n
    }
    fn read(map: &LaneAudioMap, slot: usize, frame: u64, ch: usize) -> f32 {
        let at = DATA + ((slot * CAPACITY + (frame as usize & (CAPACITY - 1))) * 2 + ch) * 8;
        let value = map.u64(at).load(Ordering::Acquire);
        assert_eq!((value >> 32) as u32, frame as u32);
        f32::from_bits(value as u32)
    }
    #[test]
    fn stereo_stems_are_isolated_and_leases_only_mute_their_own_monitor() {
        let map = LaneAudioMap::create().unwrap();
        let mut backend = backend();
        let mut first = note(0, 220.0, 0.1);
        first.controls.pan = Some(-0.7);
        let mut second = note(1, 660.0, 0.1);
        second.controls.pan = Some(0.7);
        backend.note(first);
        backend.note(second);
        let mut main = [0.0; 256];
        backend.process_block(&mut main, 128);
        let taps = backend.stems().unwrap();
        let expected = main;
        map.write(taps, 48000, 128, &mut main);
        assert_eq!(main, expected);
        for i in 0..256 {
            assert_eq!(read(&map, 0, (i / 2) as u64, i % 2), taps.orbit(0, 128)[i]);
            assert_eq!(read(&map, 1, (i / 2) as u64, i % 2), taps.orbit(1, 128)[i]);
            assert_eq!(read(&map, 2, (i / 2) as u64, i % 2), 0.0);
        }
        assert_ne!(taps.orbit(0, 128), taps.orbit(1, 128));
        map.u64(64).store(monotonic_millis(), Ordering::Release);
        for _ in 0..4 {
            let mut warmup = expected;
            map.write(taps, 48000, 128, &mut warmup);
        }
        let mut monitored = expected;
        map.write(taps, 48000, 128, &mut monitored);
        for i in 0..256 {
            assert!((monitored[i] - taps.orbit(1, 128)[i]).abs() < 1e-6);
        }
        map.u64(64)
            .store(monotonic_millis().saturating_sub(1000), Ordering::Release);
        for _ in 0..4 {
            let mut warmup = expected;
            map.write(taps, 48000, 128, &mut warmup);
        }
        let mut restored = expected;
        map.write(taps, 48000, 128, &mut restored);
        assert_eq!(restored, expected);
    }
    #[test]
    fn delay_tail_survives_note_end_and_wrap_is_tagged_without_callback_allocation() {
        let map = LaneAudioMap::create().unwrap();
        let mut backend = backend();
        let mut n = note(0, 330.0, 0.008);
        n.controls.delay = Some(DelayControls {
            wet: 0.7,
            time_secs: 0.05,
            feedback: 0.5,
        });
        backend.note(n);
        let mut tail = 0.0f32;
        assert!(crate::tripwire::allocator_is_armed());
        for block in 0..150 {
            let mut main = [0.0; 256];
            backend.process_block(&mut main, 128);
            let taps = backend.stems().unwrap();
            let before = crate::tripwire::Violations::capture();
            crate::tripwire::audio_scope(|| map.write(taps, 48000, 128, &mut main));
            assert_eq!(
                crate::tripwire::Violations::capture().since(before),
                crate::tripwire::Violations::default()
            );
            if block > 20 {
                tail = tail.max(
                    taps.orbit(0, 128)
                        .iter()
                        .copied()
                        .map(f32::abs)
                        .fold(0.0, f32::max),
                );
            }
            assert!(taps.orbit(1, 128).iter().all(|s| *s == 0.0));
        }
        assert!(tail > 0.001, "delay outlives the 8 ms note");
        assert_eq!(map.u64(24).load(Ordering::Acquire), 19200);
        let tag = map.u64(DATA).load(Ordering::Acquire) >> 32;
        assert_eq!(
            tag, CAPACITY as u64,
            "an overwritten old frame can be detected"
        );
        assert!(LaneAudioMap::create().unwrap().name() != map.name());
    }
}
