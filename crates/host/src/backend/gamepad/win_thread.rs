//! The Windows source: one thread named `gamepad`, XInput polled while somebody wants buttons
//! or axes, and a device-interface notification for controllers being plugged in.
//!
//! **Why polling, and why only while listened to.** `XInputGetState` reads the driver's current
//! state and needs no window and no focus, which is exactly what an observer of a game running
//! in front of us needs; but it has no events, so a press is only seen by asking. At 4 ms a
//! press is seen within 2 ms on average, well inside the 15 ms pump tick that dominates the
//! delay anyway. Asking costs a driver round trip per connected pad, so the thread asks only
//! while an enabled module listens for buttons or axes (or a `list()`/`state()` lease is
//! running), probes the empty slots only every 2 s — Microsoft's advice is not to poll empty
//! slots every frame — and otherwise parks with no timer at all, which costs nothing.
//!
//! **Why a high-resolution waitable timer, armed once per wait.** A 4 ms timeout in
//! `MsgWaitForMultipleObjects` waits for the next 15.6 ms system tick unless somebody raised
//! the timer resolution, and Windows 11 may ignore such a request from a process whose windows
//! are all hidden — ours, while the game is in front. `CREATE_WAITABLE_TIMER_HIGH_RESOLUTION`
//! asks for the period itself. It is armed as a one-shot before every wait rather than once as
//! a periodic timer: each arm is checked, a refused one drops the timer for the wait's own
//! timeout instead of leaving a poll that never comes, and the wait's timeout is set to the
//! next poll either way, so a timer that is armed but never fires still costs no more than the
//! 15.6 ms tick. Where the high-resolution kind is refused (Windows before 1803) an ordinary
//! timer is used and the log says the period is about 15.6 ms.
//!
//! **Why a device notification, and not Raw Input.** Plugging a controller in announces a HID
//! device interface — XInput pads included, which carry an `IG_` HID collection beside their
//! XInput driver — and `RegisterDeviceNotificationW` for `GUID_DEVINTERFACE_HID` delivers that
//! as `WM_DEVICECHANGE` to a message-only window, which is how SDL hears about joysticks. Raw
//! Input's `WM_INPUT_DEVICE_CHANGE` needs `RIDEV_INPUTSINK` to reach a window that is never in
//! front, and INPUTSINK also delivers every REPORT of every joystick and gamepad HID device — a
//! DualShock 4 sends about 250 a second — to a thread that in this phase has nothing to do
//! with them. The notification is registered once, at start, and costs nothing until a device
//! comes or goes; while parked it is ignored, and leaving the park reads every slot anyway.
//!
//! **Everything that crosses a boundary is caught.** A panic unwinding out of a window
//! procedure aborts the process, so the window procedure's body is inside `catch_unwind`, and
//! so is the thread's: a fault here must cost the gamepad, never the user's keyboard overlay.

use std::cell::Cell;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, FILETIME, HANDLE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    CancelWaitableTimer, CreateWaitableTimerExW, GetCurrentThread, GetCurrentThreadId, GetThreadTimes,
    SetThreadInformation, SetThreadPriority, SetWaitableTimer, ThreadPowerThrottling,
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, INFINITE, THREAD_POWER_THROTTLING_CURRENT_VERSION,
    THREAD_POWER_THROTTLING_EXECUTION_SPEED, THREAD_POWER_THROTTLING_STATE, THREAD_PRIORITY_ABOVE_NORMAL,
    TIMER_ALL_ACCESS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, MsgWaitForMultipleObjectsEx, PeekMessageW, PostMessageW,
    PostThreadMessageW, RegisterClassW, RegisterDeviceNotificationW, DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE,
    DBT_DEVTYP_DEVICEINTERFACE, DEVICE_NOTIFY_WINDOW_HANDLE, DEV_BROADCAST_DEVICEINTERFACE_W, HWND_MESSAGE, MSG,
    MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, WM_APP, WM_DEVICECHANGE, WM_NULL, WM_QUIT, WNDCLASSW,
};

use super::xinput::{self, XInput, SLOTS};
use super::{Control, DeviceKey, Hub};

/// `GUID_DEVINTERFACE_HID`, written out rather than pulling in windows-sys's whole
/// `Win32_Devices_HumanInterfaceDevice` feature for one constant.
const GUID_DEVINTERFACE_HID: GUID = GUID::from_u128(0x4d1e55b2_f16f_11cf_88cb_001111000030);

/// The demand or the lease changed: decide again how hard to work.
const WM_APP_PAD_CONTROL: u32 = WM_APP + 0x47;
/// `list()`/`state()` want every pad read now.
const WM_APP_PAD_REFRESH: u32 = WM_APP + 0x48;

const POLL: Duration = Duration::from_millis(4);
const PROBE_PERIOD: Duration = Duration::from_secs(2);
/// A second look after an arrival notice. The HID interface is announced before the XInput
/// driver has finished with the pad, so the probe the notice triggers can come too early.
const ARRIVAL_RETRY: Duration = Duration::from_millis(500);

/// The thread that calls `start` — the main thread, which the waker posts to.
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);
/// The gamepad thread's message-only window, for the control messages. 0 until it exists.
static PAD_HWND: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    /// What the window procedure saw, for the loop to act on after dispatch. The procedure
    /// itself does nothing but set these: it runs inside `PeekMessageW`, and reading pads from
    /// there would nest one read inside another.
    static FLAGS: Cell<u8> = const { Cell::new(0) };
}
const F_ARRIVAL: u8 = 1;
const F_CONTROL: u8 = 2;
const F_REFRESH: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// Nothing listens: no timer, a wait with no timeout, arrival notices ignored.
    Parked,
    /// Somebody wants connections only, or no pad is connected: every slot every 2 s, plus
    /// on every arrival notice.
    Probe,
    /// A pad is connected and somebody wants its buttons or axes, or a lease runs: every 4 ms.
    Poll,
}

/// How hard to work, as a pure function so the rule is held by a test rather than read: it is
/// the same rule [`super::refresh_if_idle`] relies on when it decides whether `state()` has to
/// ask for a fresh read.
fn mode_for(xinput_loaded: bool, bits: u8, lease: bool, any_connected: bool) -> Mode {
    if !xinput_loaded {
        Mode::Parked
    } else if any_connected && super::reading_continuously(bits, lease) {
        Mode::Poll
    } else if bits != 0 || lease {
        Mode::Probe
    } else {
        Mode::Parked
    }
}

pub fn start(hub: &'static Hub) -> Result<(), String> {
    // SAFETY: plain thread-id query.
    MAIN_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
    hub.set_waker(|| {
        let tid = MAIN_THREAD.load(Ordering::SeqCst);
        if tid != 0 {
            // SAFETY: posting a no-op message to a thread; failure only means no wake, and the
            // 15 ms tick drains anyway.
            unsafe {
                PostThreadMessageW(tid, WM_NULL, 0, 0);
            }
        }
    });
    hub.set_control(move |what| {
        let hwnd = PAD_HWND.load(Ordering::SeqCst);
        if hwnd == 0 {
            // No thread to ask — not yet running, or gone. A refresh is answered here and now
            // with what the hub has, so a `list()` does not wait its 50 ms for nobody.
            if what == Control::Refresh {
                hub.refresh_done(hub.refresh_asked());
            }
            return;
        }
        let msg = match what {
            Control::Demand => WM_APP_PAD_CONTROL,
            Control::Refresh => WM_APP_PAD_REFRESH,
        };
        // SAFETY: posting to our own message-only window. A failure (the thread has ended and
        // taken the window with it) only means the message is not delivered.
        unsafe {
            PostMessageW(hwnd as HWND, msg, 0, 0);
        }
    });

    let (ready, answered) = mpsc::channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("gamepad".to_string())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(hub, ready)));
            // The window went with the thread; see the control closure above.
            PAD_HWND.store(0, Ordering::SeqCst);
            if let Err(p) = outcome {
                let text = p
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "no message".to_string());
                hub.set_status("thread", format!("stopped after a panic: {text}"));
                crate::logging::line("gamepad", &format!("the gamepad thread stopped after a panic: {text}"));
            }
        })
        .map_err(|e| format!("could not start the gamepad thread: {e}"))?;

    // Waited for, briefly, so the `list()` that usually follows the first call sees the pads
    // that are there. Not for longer: this is the pump thread, and an XInput slot that takes
    // long to answer is not worth a stalled keyboard.
    match answered.recv_timeout(Duration::from_millis(250)) {
        Ok(r) => r,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            crate::logging::line(
                "gamepad",
                "the first read of the XInput slots took longer than 250 ms; carrying on, and pads \
                 will be announced as they are found",
            );
            Ok(())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("the gamepad thread ended before it was ready".to_string())
        }
    }
}

struct Slot {
    connected: bool,
    packet: u32,
}

/// Poll timing, logged once a minute under trace: the number the TODO measurement wants, from
/// the machine it runs on rather than from the documentation.
struct Stats {
    since: Instant,
    last_poll: Option<Instant>,
    polls: u32,
    /// Intervals up to 4.5, 6, 10, 17 ms, and longer.
    buckets: [u32; 5],
    longest: Duration,
    cpu_at_start: u64,
}

struct Source {
    hub: &'static Hub,
    xinput: Option<XInput>,
    /// Null when none could be created, or after an arm was refused; polling then runs on the
    /// message wait's own timeout.
    timer: HANDLE,
    slots: [Slot; SLOTS],
    mode: Mode,
    throttling_refusal_logged: bool,
    next_probe: Instant,
    /// When the next poll is due. The timer is armed for it, and the wait's timeout ends at it
    /// too, so a poll is never later than the coarse tick even if the timer never fires.
    next_poll: Instant,
    stats: Stats,
}

fn run(hub: &'static Hub, ready: mpsc::Sender<Result<(), String>>) {
    // Emulators routinely take a whole core; at normal priority a 4 ms poll behind one of them
    // is a poll that happens when the scheduler gets round to it.
    // SAFETY: the pseudo-handle of this thread.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
    }

    let hwnd = create_window();
    if hwnd.is_null() {
        // SAFETY: plain error query.
        let err = unsafe { GetLastError() };
        let why = format!("could not create the gamepad thread's window (error {err})");
        hub.set_status("thread", format!("stopped: {why}"));
        // `ensure_started` logs the error it is sent — but it stops listening after 250 ms,
        // and a send after that goes nowhere. Then it is said here, or nobody ever says it.
        if let Err(mpsc::SendError(Err(why))) = ready.send(Err(why)) {
            crate::logging::line("gamepad", &format!("gamepad watcher failed after starting: {why}"));
        }
        return;
    }
    PAD_HWND.store(hwnd as isize, Ordering::SeqCst);
    register_arrival(hub, hwnd);

    let xinput = match XInput::load() {
        Ok(x) => {
            hub.set_status(
                "xinput",
                format!(
                    "{}, {}",
                    x.dll,
                    if x.guide() { "guide button through ordinal 100" } else { "no guide button (no ordinal 100)" }
                ),
            );
            Some(x)
        }
        Err(e) => {
            hub.set_status("xinput", format!("unavailable: {e}"));
            crate::logging::line("gamepad", &format!("XInput is unavailable, so no Xbox-type pad will be seen: {e}"));
            None
        }
    };

    let (timer, high_res) = create_timer();
    hub.set_status(
        "timer",
        if timer.is_null() {
            "none — polling on the message wait's timeout, about 15.6 ms".to_string()
        } else if high_res {
            "high resolution".to_string()
        } else {
            "ordinary — the poll period will be about 15.6 ms, not 4".to_string()
        },
    );
    if !high_res {
        crate::logging::line(
            "gamepad",
            "no high-resolution waitable timer on this Windows; polls will come about every 15.6 ms",
        );
    }
    hub.set_status("mode", "parked");

    let now = Instant::now();
    let mut src = Source {
        hub,
        xinput,
        timer,
        slots: [(); SLOTS].map(|_| Slot { connected: false, packet: 0 }),
        mode: Mode::Parked,
        throttling_refusal_logged: false,
        next_probe: now + PROBE_PERIOD,
        next_poll: now,
        stats: Stats { since: now, last_poll: None, polls: 0, buckets: [0; 5], longest: Duration::ZERO, cpu_at_start: 0 },
    };

    // The first enumeration, whatever the demand: it is what `ensure_started` waits for.
    src.read_all(now, false);
    let _ = ready.send(Ok(()));

    // SAFETY: MSG is plain data; zeroed is its documented initial state.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let now = Instant::now();
        let want = src.wanted(now);
        src.set_mode(want, now);

        let armed = src.mode == Mode::Poll && src.arm_poll(now);
        let handles = [src.timer];
        // SAFETY: at most one valid handle, and a count that says whether to look at it.
        let r = unsafe {
            MsgWaitForMultipleObjectsEx(
                if armed { 1 } else { 0 },
                if armed { handles.as_ptr() } else { std::ptr::null() },
                src.timeout(now),
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        // Sent messages (WM_DEVICECHANGE is one) are delivered inside PeekMessageW; posted
        // ones are dispatched here. Both only set flags.
        // SAFETY: a valid MSG out pointer; dispatching to our own window procedure.
        while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if msg.message == WM_QUIT {
                return;
            }
            unsafe {
                DispatchMessageW(&msg);
            }
        }

        let now = Instant::now();
        let flags = FLAGS.with(|f| f.replace(0));
        if flags & F_REFRESH != 0 {
            src.refresh(now);
        }
        if flags & F_ARRIVAL != 0 && src.mode != Mode::Parked {
            src.read_all(now, false);
            src.next_probe = now + ARRIVAL_RETRY;
        }
        // Due when the timer fired or the time has come, whichever is noticed first. Not "only
        // when the timer fired": a timer that stops firing would then stop the polling with
        // the status still saying "polling every 4 ms".
        if src.mode == Mode::Poll && ((armed && r == WAIT_OBJECT_0) || now >= src.next_poll) {
            src.poll(now);
        }
        if src.mode != Mode::Parked && now >= src.next_probe {
            src.probe(now);
        }
    }
}

impl Source {
    fn wanted(&self, now: Instant) -> Mode {
        mode_for(
            self.xinput.is_some(),
            self.hub.demand(),
            self.hub.lease_active(now),
            self.slots.iter().any(|s| s.connected),
        )
    }

    /// How long the message wait may sleep: until the next probe, the end of a lease or —
    /// polling — the next poll, whichever is first, so a lease that runs out parks the thread
    /// instead of leaving it probing forever. Parked, it sleeps until a message comes.
    fn timeout(&self, now: Instant) -> u32 {
        if self.mode == Mode::Parked {
            return INFINITE;
        }
        let mut until = self.next_probe;
        if let Some(l) = self.hub.lease_until() {
            if l > now {
                until = until.min(l);
            }
        }
        if self.mode == Mode::Poll {
            // With the timer too, as the backstop described at the top of this file.
            until = until.min(self.next_poll);
        }
        // Rounded up, so the wait does not return a hair early and spin once more.
        let ms = until.saturating_duration_since(now).as_micros().div_ceil(1000);
        ms.min(u32::MAX as u128 - 1) as u32
    }

    fn set_mode(&mut self, want: Mode, now: Instant) {
        if want == self.mode {
            return;
        }
        let from = self.mode;
        self.mode = want;
        match want {
            Mode::Parked | Mode::Probe => {
                self.cancel_timer();
                self.throttling(false);
            }
            Mode::Poll => {
                self.throttling(true);
                self.stats = Stats {
                    since: now,
                    last_poll: None,
                    polls: 0,
                    buckets: [0; 5],
                    longest: Duration::ZERO,
                    cpu_at_start: thread_cpu_100ns(),
                };
            }
        }
        // Leaving the park, or starting to poll: what the hub holds may be minutes old, and a
        // button held all that time must not be announced as pressed NOW. So the pads are read
        // afresh as a baseline — and a pad plugged in or out meanwhile is announced.
        if from == Mode::Parked || want == Mode::Poll {
            self.read_all(now, true);
            self.next_probe = now + PROBE_PERIOD;
            self.next_poll = now + POLL;
        }
        let words = match want {
            Mode::Parked => "parked".to_string(),
            Mode::Probe => format!("probing every {} s", PROBE_PERIOD.as_secs()),
            Mode::Poll => format!("polling every {} ms", POLL.as_millis()),
        };
        self.hub.set_status("mode", words.clone());
        crate::logging::trace("gamepad", || format!("{from:?} -> {want:?}: {words}"));
    }

    /// Every slot: connected ones updated (or, with `rebase`, taken as a baseline), empty
    /// ones asked whether a pad is there now.
    fn read_all(&mut self, now: Instant, rebase: bool) {
        for i in 0..SLOTS {
            self.read_slot(i, now, rebase);
        }
    }

    fn read_slot(&mut self, i: usize, now: Instant, rebase: bool) {
        let Some(x) = &self.xinput else { return };
        let key = DeviceKey::XInput(i as u8);
        match x.read(i) {
            Ok(s) => {
                let snap = xinput::snapshot(&s, x.guide());
                if !self.slots[i].connected {
                    if self.hub.connect(key, xinput::desc(i, x.guide(), x.ids(i)), &snap, now).is_some() {
                        self.slots[i] = Slot { connected: true, packet: s.packet };
                    }
                } else if rebase {
                    self.hub.rebaseline(&key, &snap, now);
                    self.slots[i].packet = s.packet;
                } else if s.packet != self.slots[i].packet {
                    // An unchanged packet number is XInput's own "nothing happened", and
                    // skipping it is most of what makes a 4 ms poll cheap.
                    self.hub.update(&key, &snap, now);
                    self.slots[i].packet = s.packet;
                }
            }
            Err(_) => {
                if self.slots[i].connected {
                    self.slots[i].connected = false;
                    self.hub.disconnect(&key, now);
                }
            }
        }
    }

    fn poll(&mut self, now: Instant) {
        for i in 0..SLOTS {
            if self.slots[i].connected {
                self.read_slot(i, now, false);
            }
        }
        self.next_poll = now + POLL;
        self.count_poll(now);
    }

    /// Polling, only the empty slots — the connected ones were read 4 ms ago. Probing,
    /// every slot, so a pad that went away is noticed too.
    fn probe(&mut self, now: Instant) {
        for i in 0..SLOTS {
            if self.mode == Mode::Probe || !self.slots[i].connected {
                self.read_slot(i, now, false);
            }
        }
        self.next_probe = now + PROBE_PERIOD;
    }

    fn refresh(&mut self, now: Instant) {
        // The request number is taken BEFORE reading, so a request that arrives while the
        // slots are being read is answered by the next refresh rather than by this one, which
        // may already have read its pad.
        let asked = self.hub.refresh_asked();
        self.read_all(now, false);
        self.hub.refresh_done(asked);
    }

    /// Arms the timer, once, for the next poll. False when there is no timer, when the poll is
    /// already due (the wait's zero timeout takes care of that), or when the arm was refused —
    /// which also drops the timer for good, logged once, so the loop polls on the wait's
    /// timeout from then on instead of waiting on a timer that will never fire.
    fn arm_poll(&mut self, now: Instant) -> bool {
        if self.timer.is_null() {
            return false;
        }
        let wait = self.next_poll.saturating_duration_since(now);
        if wait.is_zero() {
            return false;
        }
        match arm_once(self.timer, wait) {
            Ok(()) => true,
            Err(err) => {
                self.hub.set_status(
                    "timer",
                    format!("arming refused (error {err}) — polling on the message wait's timeout, about 15.6 ms"),
                );
                crate::logging::line(
                    "gamepad",
                    &format!(
                        "the poll timer could not be armed (error {err}); polling carries on at the \
                         message wait's resolution, about every 15.6 ms"
                    ),
                );
                // SAFETY: the handle this thread created; nothing else holds it.
                unsafe {
                    CloseHandle(self.timer);
                }
                self.timer = std::ptr::null_mut();
                false
            }
        }
    }

    fn cancel_timer(&mut self) {
        if !self.timer.is_null() {
            // SAFETY: a timer handle this thread created.
            unsafe {
                CancelWaitableTimer(self.timer);
            }
        }
    }

    /// Opts the thread out of EcoQoS while it polls, and back in when it stops.
    ///
    /// A process that does not state its execution-speed preference has one inferred, and
    /// ours — every window hidden, silent, the game in front — is what the heuristic slows
    /// down. The effect on the poll period is unmeasured; the setting is not.
    fn throttling(&mut self, opt_out: bool) {
        let state = THREAD_POWER_THROTTLING_STATE {
            Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: if opt_out { THREAD_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            StateMask: 0,
        };
        // SAFETY: a valid struct of the size passed, for this thread's pseudo-handle.
        let ok = unsafe {
            SetThreadInformation(
                GetCurrentThread(),
                ThreadPowerThrottling,
                &state as *const THREAD_POWER_THROTTLING_STATE as *const core::ffi::c_void,
                std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
            )
        } != 0;
        if !ok && !self.throttling_refusal_logged {
            self.throttling_refusal_logged = true;
            // SAFETY: plain error query.
            let err = unsafe { GetLastError() };
            crate::logging::line(
                "gamepad",
                &format!("the power-throttling opt-out was refused (error {err}); polling carries on without it"),
            );
        }
    }

    fn count_poll(&mut self, now: Instant) {
        let st = &mut self.stats;
        if let Some(last) = st.last_poll {
            let d = now.saturating_duration_since(last);
            let ms = d.as_secs_f64() * 1000.0;
            let bucket = if ms <= 4.5 {
                0
            } else if ms <= 6.0 {
                1
            } else if ms <= 10.0 {
                2
            } else if ms <= 17.0 {
                3
            } else {
                4
            };
            st.buckets[bucket] += 1;
            st.longest = st.longest.max(d);
        }
        st.last_poll = Some(now);
        st.polls += 1;
        if now.saturating_duration_since(st.since) >= Duration::from_secs(60) {
            let cpu_ms = thread_cpu_100ns().saturating_sub(st.cpu_at_start) as f64 / 10_000.0;
            let (b, polls, longest, secs) =
                (st.buckets, st.polls, st.longest, now.saturating_duration_since(st.since).as_secs());
            crate::logging::trace("gamepad", || {
                format!(
                    "last {secs} s of polling: {polls} polls; intervals <=4.5 ms {}, <=6 {}, <=10 {}, \
                     <=17 {}, longer {}; longest {:.1} ms; thread CPU {cpu_ms:.1} ms",
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    b[4],
                    longest.as_secs_f64() * 1000.0
                )
            });
            *st = Stats {
                since: now,
                last_poll: Some(now),
                polls: 0,
                buckets: [0; 5],
                longest: Duration::ZERO,
                cpu_at_start: thread_cpu_100ns(),
            };
        }
    }
}

/// This thread's kernel plus user time, in 100 ns units.
fn thread_cpu_100ns() -> u64 {
    let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut c, mut e, mut k, mut u) = (zero, zero, zero, zero);
    // SAFETY: valid out pointers, this thread's pseudo-handle.
    let ok = unsafe { GetThreadTimes(GetCurrentThread(), &mut c, &mut e, &mut k, &mut u) } != 0;
    if !ok {
        return 0;
    }
    let v = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    v(k) + v(u)
}

/// Arms `timer` to fire once, `wait` from now. `Err` carries `GetLastError`.
fn arm_once(timer: HANDLE, wait: Duration) -> Result<(), u32> {
    // Relative, in 100 ns units: negative. At least one unit, because zero means "now" in
    // absolute time, which is long past.
    let due: i64 = -((wait.as_nanos() / 100).clamp(1, i64::MAX as u128) as i64);
    // SAFETY: a valid timer handle; no completion routine, no resume.
    let ok = unsafe { SetWaitableTimer(timer, &due, 0, None, std::ptr::null(), 0) } != 0;
    if ok {
        Ok(())
    } else {
        // SAFETY: plain error query.
        Err(unsafe { GetLastError() })
    }
}

/// Asks for `WM_DEVICECHANGE` whenever a HID device interface arrives or goes, for the whole
/// life of the thread; see the note at the top of this file. A refusal is logged and costs
/// only the promptness: the 2 s probe still finds a new pad.
fn register_arrival(hub: &Hub, hwnd: HWND) {
    let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
        dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
        dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
        dbcc_reserved: 0,
        dbcc_classguid: GUID_DEVINTERFACE_HID,
        dbcc_name: [0],
    };
    // SAFETY: our own window, a filter of the size it states; the handle is kept for the life
    // of the thread, and the window going away with the thread ends the registration.
    let h = unsafe {
        RegisterDeviceNotificationW(
            hwnd as HANDLE,
            &filter as *const DEV_BROADCAST_DEVICEINTERFACE_W as *const core::ffi::c_void,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
    };
    if h.is_null() {
        // SAFETY: plain error query.
        let err = unsafe { GetLastError() };
        hub.set_status("arrival", format!("refused (error {err}); a new pad waits for the 2 s probe"));
        crate::logging::line(
            "gamepad",
            &format!(
                "the device notification for controllers was refused (error {err}); a pad plugged \
                 in is noticed by the 2 s probe instead of at once"
            ),
        );
    } else {
        hub.set_status("arrival", "HID device notifications");
    }
}

/// A high-resolution waitable timer if the system has one, an ordinary one if not; null if
/// neither could be created, in which case the loop polls on its wait's timeout.
fn create_timer() -> (HANDLE, bool) {
    // SAFETY: no attributes, no name; the handle lives as long as the thread.
    unsafe {
        let h = CreateWaitableTimerExW(
            std::ptr::null(),
            std::ptr::null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS,
        );
        if !h.is_null() {
            return (h, true);
        }
        (CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 0, TIMER_ALL_ACCESS), false)
    }
}

fn create_window() -> HWND {
    // SAFETY: registering a class and creating a message-only window on this thread; the
    // class name outlives both calls.
    unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = "AutomationPlatformGamepad\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(pad_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hmod,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hmod,
            std::ptr::null(),
        )
    }
}

unsafe extern "system" fn pad_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Only flags are set here, and even that is inside catch_unwind: an unwind out of a window
    // procedure is an abort.
    let handled = std::panic::catch_unwind(|| {
        let set = |bit: u8| FLAGS.with(|f| f.set(f.get() | bit));
        match msg {
            WM_DEVICECHANGE => {
                let what = wparam as u32;
                if what == DBT_DEVICEARRIVAL || what == DBT_DEVICEREMOVECOMPLETE {
                    set(F_ARRIVAL);
                }
                // Answered by DefWindowProcW: the value matters only for the query events,
                // which this window never asks for, and the notices ignore it.
                false
            }
            WM_APP_PAD_CONTROL => {
                set(F_CONTROL);
                true
            }
            WM_APP_PAD_REFRESH => {
                set(F_REFRESH);
                true
            }
            _ => false,
        }
    });
    match handled {
        Ok(true) => 0,
        Ok(false) => DefWindowProcW(hwnd, msg, wparam, lparam),
        Err(_) => {
            crate::logging::line("gamepad", &format!("panic in the gamepad window procedure (message {msg:#x})"));
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::demand;
    use super::*;
    use windows_sys::Win32::Foundation::WAIT_TIMEOUT;

    #[test]
    fn the_mode_follows_what_is_wanted() {
        let (b, a, c) = (demand::BUTTONS, demand::AXES, demand::CONNECT);
        // (xinput loaded, demand, lease, a pad connected) -> mode
        for (x, bits, lease, any, want) in [
            (true, 0, false, true, Mode::Parked),
            (true, c, false, true, Mode::Probe),
            (true, c, false, false, Mode::Probe),
            (true, b, false, true, Mode::Poll),
            (true, a | c, false, true, Mode::Poll),
            (true, b, false, false, Mode::Probe),
            (true, 0, true, true, Mode::Poll),
            (true, 0, true, false, Mode::Probe),
            (false, b | a | c, true, true, Mode::Parked),
        ] {
            assert_eq!(mode_for(x, bits, lease, any), want, "xinput {x}, demand {bits:#05b}, lease {lease}, pad {any}");
            // And the rule `state()` relies on: whenever the source is NOT polling a connected
            // pad, `refresh_if_idle` must ask for a read.
            if x && any {
                assert_eq!(
                    mode_for(x, bits, lease, any) == Mode::Poll,
                    super::super::reading_continuously(bits, lease),
                    "demand {bits:#05b}, lease {lease}"
                );
            }
        }
    }

    /// The part of the 4 ms poll that needs no pad: the timer this thread creates, armed the
    /// way `arm_poll` arms it, has to actually fire inside the wait the loop makes. The review
    /// found that the poll depended on it firing and that nothing had ever run it. Lax on
    /// purpose — 25 fires in two seconds, where 4 ms apiece is 0.1 s and the coarse 15.6 ms
    /// tick would still be 0.4 s — so it catches "never fires", not a slow machine.
    #[test]
    fn the_poll_timer_fires_inside_the_message_wait() {
        let (timer, high_res) = create_timer();
        assert!(!timer.is_null(), "no waitable timer at all");
        let started = Instant::now();
        let mut fired = 0;
        let mut longest = Duration::ZERO;
        for _ in 0..25 {
            let t = Instant::now();
            arm_once(timer, POLL).expect("SetWaitableTimer refused");
            let handles = [timer];
            // SAFETY: one valid handle; no input is expected on a test thread, and the
            // 1000 ms timeout bounds the wait if the timer never fires.
            let r = unsafe { MsgWaitForMultipleObjectsEx(1, handles.as_ptr(), 1000, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
            assert_ne!(r, WAIT_TIMEOUT, "the armed timer did not fire within a second");
            if r == WAIT_OBJECT_0 {
                fired += 1;
                longest = longest.max(t.elapsed());
            }
        }
        let total = started.elapsed();
        // SAFETY: the handle created above.
        unsafe {
            CloseHandle(timer);
        }
        eprintln!(
            "poll timer: high resolution {high_res}; {fired} of 25 fired in {:.1} ms, {:.2} ms each on average, longest {:.2} ms",
            total.as_secs_f64() * 1000.0,
            total.as_secs_f64() * 1000.0 / 25.0,
            longest.as_secs_f64() * 1000.0
        );
        assert!(fired >= 20, "only {fired} of 25 waits ended on the timer");
        assert!(total < Duration::from_secs(2), "{total:?} for 25 polls");
    }
}
