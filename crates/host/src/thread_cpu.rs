//! The event loop's processor time, read from another thread: what the guard's watchdog
//! (`vm_guard.rs`) compares with a callback's budget of 2 s.
//!
//! A [`LoopThread`] is made on the thread to be measured and asked from any thread. Processor
//! time grows only while the thread really runs — waiting for the processor behind other
//! programs, or for an answer from another process, does not count — which is why it decides
//! before the wall clock does (way A's question 3).
//!
//! **Windows:** `GetThreadTimes` on a handle opened with `THREAD_QUERY_LIMITED_INFORMATION`,
//! kernel and user time together. The system counts it at its timer's tick, about 15.6 ms,
//! which is fine for a budget of seconds.
//!
//! **macOS:** `thread_info(THREAD_BASIC_INFO)` on the thread's Mach port, user and system time
//! together, in microseconds. The port comes from `pthread_mach_thread_np`, which adds no port
//! reference (the deprecated `mach_thread_self` would).
//!
//! **Elsewhere** there is no such clock here, and [`LoopThread::current`] answers `None`: the
//! guard then has the wall clock alone.
//!
//! Established crates first: `cpu-time` and `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` read only the
//! calling thread's time, and `sysinfo` gives no per-thread times on Windows. So it is the two OS
//! calls, through `windows-sys` and `libc`, which the build has already. Borrowed by
//! `crates/macos-check`, so it uses nothing else of the host.

use std::time::Duration;

/// The thread a `LoopThread` was made on — the event loop's — measured from any thread.
pub struct LoopThread {
    #[cfg(windows)]
    handle: isize,
    #[cfg(target_os = "macos")]
    port: u32,
}

// SAFETY: a thread handle opened for querying, or a Mach thread port, is a number the system
// resolves on every call; asking it from any thread is what both calls are for.
unsafe impl Send for LoopThread {}
unsafe impl Sync for LoopThread {}

impl LoopThread {
    /// On the thread to be measured. `None` where the platform has no such clock, or the system
    /// refused the handle.
    pub fn current() -> Option<LoopThread> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{GetCurrentThreadId, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
            // SAFETY: plain calls; a null handle is the failure, and is not kept.
            let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, GetCurrentThreadId()) };
            if handle.is_null() {
                return None;
            }
            Some(LoopThread { handle: handle as isize })
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: the calling thread's own pthread; the port stays valid while it lives, and
            // the event loop's thread lives as long as the application.
            let port = unsafe { libc::pthread_mach_thread_np(libc::pthread_self()) };
            if port == 0 {
                return None;
            }
            Some(LoopThread { port })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            None
        }
    }

    /// Its processor time so far, user and kernel; `None` if the system did not answer.
    pub fn cpu(&self) -> Option<Duration> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::FILETIME;
            use windows_sys::Win32::System::Threading::GetThreadTimes;
            let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            // SAFETY: four FILETIMEs of ours, and a handle that stays open until `drop`.
            let ok = unsafe { GetThreadTimes(self.handle as _, &mut created, &mut exited, &mut kernel, &mut user) };
            if ok == 0 {
                return None;
            }
            let ticks = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
            // 100-nanosecond units.
            Some(Duration::from_nanos((ticks(kernel) + ticks(user)).saturating_mul(100)))
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: zeroed plain data, filled in by the kernel; `count` says how much room it has.
            let mut info: libc::thread_basic_info = unsafe { std::mem::zeroed() };
            let mut count = libc::THREAD_BASIC_INFO_COUNT;
            let kr = unsafe {
                libc::thread_info(
                    self.port,
                    libc::THREAD_BASIC_INFO as libc::thread_flavor_t,
                    &mut info as *mut libc::thread_basic_info as libc::thread_info_t,
                    &mut count,
                )
            };
            if kr != libc::KERN_SUCCESS {
                return None;
            }
            let us = |t: libc::time_value_t| u64::from(t.seconds.max(0) as u32) * 1_000_000 + u64::from(t.microseconds.max(0) as u32);
            Some(Duration::from_micros(us(info.user_time) + us(info.system_time)))
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            None
        }
    }
}

#[cfg(windows)]
impl Drop for LoopThread {
    fn drop(&mut self) {
        // SAFETY: the handle `current` opened, closed once.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

#[cfg(all(test, any(windows, target_os = "macos")))]
mod tests {
    use super::LoopThread;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// The test thread's processor time, read from a helper thread: it grows while the test
    /// thread computes, and stands still while it sleeps. Spins until the helper has seen 150 ms,
    /// rather than for a fixed time, so a loaded machine only makes it slower; it fails only past
    /// a cap of 10 s.
    #[test]
    fn the_loop_threads_processor_time_is_read_from_another_thread() {
        let me = Arc::new(LoopThread::current().expect("this platform has the clock"));
        let start = me.cpu().expect("the system answers");
        let seen = Arc::new(AtomicBool::new(false));
        let (m, s) = (me.clone(), seen.clone());
        let watcher = std::thread::spawn(move || {
            let began = Instant::now();
            while began.elapsed() < Duration::from_secs(10) {
                if m.cpu().is_some_and(|c| c.saturating_sub(start) >= Duration::from_millis(150)) {
                    s.store(true, Ordering::SeqCst);
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        });
        let began = Instant::now();
        let mut x = 0u64;
        while !seen.load(Ordering::SeqCst) && began.elapsed() < Duration::from_secs(10) {
            x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
        }
        assert!(watcher.join().unwrap(), "150 ms of processor time not seen from the other thread in 10 s");

        // Asleep, it stands still: two reads 300 ms apart differ by under 100 ms.
        let m = me.clone();
        let reader = std::thread::spawn(move || {
            let a = m.cpu().unwrap();
            std::thread::sleep(Duration::from_millis(300));
            m.cpu().unwrap().saturating_sub(a)
        });
        std::thread::sleep(Duration::from_millis(400));
        let grew = reader.join().unwrap();
        assert!(grew < Duration::from_millis(100), "grew {grew:?} while the thread slept");
    }
}
