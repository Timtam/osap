//! What the operating system does to the whole session — sleep, lock, the session switched away
//! from, a display change — and what the host does about it.
//!
//! **Why.** The application runs for days. Across a sleep, a lock or a new display arrangement
//! the parts of the host mostly recover on their own, but nothing re-checked what was cached
//! from before, and the log never said the machine had slept: a report of "the overlay stopped
//! working overnight" could only be placed by arithmetic on the rate of cached questions. Now
//! each such event is one `[system]` line, and the host acts on it:
//!
//! - **everything cached against the epoch is dropped** — the observation cache, pixels a
//!   module cached against the input epoch, `window.controls` rectangles from before a
//!   resolution change;
//! - after a wake, an unlock or the session coming back **the window in front is reported
//!   again**, as an activation ([`SystemEvent::reports_front`]): Windows does not always send a
//!   foreground event then, and an overlay that was active before the lock would otherwise wait
//!   for the next focus change to look again;
//! - after those and after a display, scale or work-area change or the displays waking **the
//!   focus is re-checked** ([`SystemEvent::front_may_have_changed`]), and after a display or
//!   scale change the `[env]` display lines are written again when they changed;
//! - after a wake or the session coming back the audio output is opened again at the next
//!   sound ([`SystemEvent::reopens_audio`]);
//! - on macOS a VoiceOver path parked after a refusal is tried again at once
//!   ([`SystemEvent::retries_screen_reader`]);
//! - the backend resets what it keeps for its own subsystems ([`SystemEvent::rechecks_capture`]:
//!   on Windows desktop duplication's back-off and hang count, see `backend/dxgi.rs` — for a
//!   display or scale change at most once in ten minutes ([`SystemEvent::may_storm`]); on macOS
//!   ScreenCaptureKit's next probe, see `backend/macos/backoff.rs`), and on macOS checks the
//!   event tap and reads the application in front again (`backend/macos/system.rs`). Each
//!   backend does its part **before** it hands the batch to the host, so the host's reactions
//!   — which capture and read the front at once — find it done.
//!
//! **The wall clock, not `Instant`.** How long a machine slept cannot be read off `Instant`: on
//! macOS it is `CLOCK_UPTIME_RAW`, which stops while the machine sleeps, so a night's sleep
//! measures as nothing. `SystemTime` keeps running. It can also be set back (a time-zone change
//! does not do that, a clock correction can): a start that lies in the future is dropped, and
//! the line then says nothing about how long.
//!
//! **When it was heard, not when the line is written.** Each event carries the time the backend
//! heard of it ([`Stamped::at`]), and the durations are measured between those. The notice that
//! the machine is about to sleep can be heard on time and reach the event loop only after the
//! wake — the loop may not turn again before the machine is down — and measured at delivery
//! both lines would be written together, the sleep lasting no time at all. A line written two
//! seconds or more after its event was heard says when it was heard. The notice can also be
//! *heard* only as the machine wakes (on macOS, the main thread was busy when it was posted);
//! then there is nothing to measure the sleep from, and the wake's line says so rather than
//! "asleep for 0 s".
//!
//! **A storm is bounded.** A display whose link keeps dropping — a TV or an AV receiver in
//! standby — sends a display change every few seconds, for hours. An event that repeats the one
//! queued just before it is folded into it, counted ([`push`]); a full queue drops its oldest,
//! never the newest, which is the state that holds now, and says so once; and the lines of the
//! display family (display, scale, work area, taskbar) go through [`crate::logging::Repeats`],
//! per kind: the first in full, then counted, with a line a minute at most ([`Since::plan`],
//! [`Since::due`]). Those counts run on the process's clock, which on macOS stops while the Mac
//! sleeps, so a wake writes what is still counted and forgets it: the first display change after
//! a wake is written in full, not counted against one from before the night. Sleep, lock and
//! session lines are always written: each is one person's or the machine's act, and each says
//! how long the state before it lasted.
//!
//! **Internal, not a host call.** No module is told about these events: nothing in the shipped
//! modules needs to be, and the host's own reactions above are what they need. If a module ever
//! does, the shape is a general subscription, not something made for one module.
//!
//! **Where they come from.** A backend pushes each one as it arrives ([`push`], from any thread)
//! and hands what was queued to the host from its `pump_pending` as one batch
//! (`HostEvents::on_system`), before the window events of the same drain. On Windows the
//! keyboard watch's thread hears them (`backend/hook_watch_thread.rs`), and the Windows message
//! codes are turned into events here ([`from_wts`], [`from_pbt`], [`from_setting`]) so the
//! mapping is tested without a window. On macOS the workspace's notifications, the distributed
//! lock notices and the display-reconfiguration callback do (`backend/macos/system.rs`).
//! Everything but the queue is pure, and `crates/macos-check` borrows the file.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::logging::{Repeats, Said};

/// One thing the operating system did to the whole session. Host-internal — no module is handed
/// one. Each platform sends the variants it can hear; the doc of each says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    /// The machine is about to sleep or hibernate. Windows `PBT_APMSUSPEND`, macOS
    /// `NSWorkspaceWillSleepNotification`.
    Sleep,
    /// The machine woke from sleep or hibernation. Windows `PBT_APMRESUMEAUTOMATIC`, macOS
    /// `NSWorkspaceDidWakeNotification`.
    Wake,
    /// The session was locked. Windows `WTS_SESSION_LOCK`, macOS `com.apple.screenIsLocked`.
    Lock,
    /// The session was unlocked. Windows `WTS_SESSION_UNLOCK`, macOS `com.apple.screenIsUnlocked`.
    Unlock,
    /// This session is at the console again (`remote` false: Windows `WTS_CONSOLE_CONNECT`,
    /// macOS `NSWorkspaceSessionDidBecomeActiveNotification` — fast user switching back) or was
    /// connected to a remote client (`remote` true: Windows `WTS_REMOTE_CONNECT` only).
    Connected { remote: bool },
    /// This session left the console (`remote` false: another user's session in front, or the
    /// console disconnected — Windows `WTS_CONSOLE_DISCONNECT`, macOS
    /// `NSWorkspaceSessionDidResignActiveNotification`) or its remote client (`remote` true:
    /// Windows `WTS_REMOTE_DISCONNECT` only).
    Disconnected { remote: bool },
    /// The displays went to sleep; the machine did not. macOS only
    /// (`NSWorkspaceScreensDidSleepNotification`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    DisplaysAsleep,
    /// The displays woke. macOS only (`NSWorkspaceScreensDidWakeNotification`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    DisplaysAwake,
    /// A display was added, removed, moved, mirrored, or changed its resolution or mode.
    /// Windows `WM_DISPLAYCHANGE`, macOS `CGDisplayRegisterReconfigurationCallback` (a mode's
    /// scale included).
    DisplaysChanged,
    /// A display's scale (DPI) changed. Windows only (`WM_DPICHANGED`,
    /// `SPI_SETLOGICALDPIOVERRIDE`); on macOS a scale change is a [`DisplaysChanged`](Self::DisplaysChanged).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Scale,
    /// The work area changed: the taskbar moved, grew, shrank or hid. Windows only.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    WorkArea,
    /// The taskbar was created again — Explorer restarted. Windows only.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    TaskbarCreated,
}

impl SystemEvent {
    /// What happened, for the `[system]` line and the lines that name their cause: a phrase that
    /// completes "the …".
    pub fn words(self) -> &'static str {
        match self {
            SystemEvent::Sleep => "machine is going to sleep",
            SystemEvent::Wake => "machine woke from sleep",
            SystemEvent::Lock => "screen was locked",
            SystemEvent::Unlock => "screen was unlocked",
            SystemEvent::Connected { remote: false } => "session is back at the console",
            SystemEvent::Connected { remote: true } => "session was connected to a remote client",
            SystemEvent::Disconnected { remote: false } => "session was switched away from the console",
            SystemEvent::Disconnected { remote: true } => "session was disconnected from its remote client",
            SystemEvent::DisplaysAsleep => "displays went to sleep",
            SystemEvent::DisplaysAwake => "displays woke",
            SystemEvent::DisplaysChanged => "display configuration changed",
            SystemEvent::Scale => "display scale changed",
            SystemEvent::WorkArea => "work area changed — the taskbar moved, grew, shrank or hid",
            SystemEvent::TaskbarCreated => "taskbar was created again — Explorer restarted",
        }
    }

    /// The event that started what this one ends — a wake ends a sleep — so the line can say
    /// how long it lasted. `None` for an event that ends nothing. A connection ends either kind
    /// of disconnection: the two are matched by kind, not by `remote` ([`same_kind`]).
    pub fn ends(self) -> Option<SystemEvent> {
        match self {
            SystemEvent::Wake => Some(SystemEvent::Sleep),
            SystemEvent::Unlock => Some(SystemEvent::Lock),
            SystemEvent::Connected { remote } => Some(SystemEvent::Disconnected { remote }),
            SystemEvent::DisplaysAwake => Some(SystemEvent::DisplaysAsleep),
            _ => None,
        }
    }

    /// What the machine or the session was in while the thing this event ends lasted.
    fn state_before(self) -> &'static str {
        match self {
            SystemEvent::Wake | SystemEvent::DisplaysAwake => "asleep",
            SystemEvent::Unlock => "locked",
            SystemEvent::Connected { .. } => "away",
            _ => "",
        }
    }

    /// Whether some later event ends this one.
    fn is_a_start(self) -> bool {
        matches!(
            self,
            SystemEvent::Sleep | SystemEvent::Lock | SystemEvent::Disconnected { .. } | SystemEvent::DisplaysAsleep
        )
    }

    /// Whether what is in front may have changed underneath the host — the moments after which
    /// the focus is re-checked even if the system reports no activation. The host's focus round
    /// ([`Plan::recheck`]); on macOS the backend also reads the application in front again.
    pub fn front_may_have_changed(self) -> bool {
        matches!(
            self,
            SystemEvent::Wake
                | SystemEvent::Unlock
                | SystemEvent::Connected { .. }
                | SystemEvent::DisplaysAwake
                | SystemEvent::DisplaysChanged
                | SystemEvent::Scale
                | SystemEvent::WorkArea
        )
    }

    /// Whether the window in front is reported again as an activation ([`Plan::report_front`]):
    /// the moments a person comes back to the machine, after which an overlay that was active
    /// before should look again at once.
    pub fn reports_front(self) -> bool {
        matches!(self, SystemEvent::Wake | SystemEvent::Unlock | SystemEvent::Connected { .. })
    }

    /// Whether the audio output is opened afresh at the next sound ([`Plan::reopen_audio`]): the
    /// device may have changed while the machine slept or the session was away.
    pub fn reopens_audio(self) -> bool {
        matches!(self, SystemEvent::Wake | SystemEvent::Connected { .. })
    }

    /// Whether the `[env]` display lines are written again, when they changed ([`Plan::env`]).
    pub fn rewrites_display_lines(self) -> bool {
        matches!(self, SystemEvent::DisplaysChanged | SystemEvent::Scale)
    }

    /// Whether a backend's capture paths look again after it: whatever made them back off or
    /// count hangs may be over, or may have been the event itself — the machine, the displays
    /// or the user coming back, the arrangement changing. Windows resets desktop duplication's
    /// back-off and hang count (`dxgi::system_changed`) — after a display or scale change
    /// ([`may_storm`](Self::may_storm)) at most once in ten minutes, so the hangs a flapping
    /// link brings can still add up to a stop; macOS brings a ScreenCaptureKit back-off's next
    /// probe forward (`capture::clear_sck_backoff`), once per step of the back-off.
    pub fn rechecks_capture(self) -> bool {
        matches!(
            self,
            SystemEvent::Wake
                | SystemEvent::Unlock
                | SystemEvent::Connected { .. }
                | SystemEvent::DisplaysAwake
                | SystemEvent::DisplaysChanged
                | SystemEvent::Scale
        )
    }

    /// Whether a screen reader that turned lines down is tried again at once after this rather
    /// than at its next look (the macOS VoiceOver path, `speech/vo_park.rs`): the moments after
    /// which the screen reader itself is likeliest to have been restarted.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub fn retries_screen_reader(self) -> bool {
        matches!(self, SystemEvent::Wake | SystemEvent::Unlock | SystemEvent::Connected { .. })
    }

    /// Whether it may keep coming: the display family, which hardware can send every few seconds
    /// for hours. The rest are acts of a person or of the machine. Its lines are counted when it
    /// does ([`Since::plan`]), and what a backend does about it is bounded (on Windows,
    /// `dxgi::system_changed`).
    pub fn may_storm(self) -> bool {
        matches!(
            self,
            SystemEvent::DisplaysChanged | SystemEvent::Scale | SystemEvent::WorkArea | SystemEvent::TaskbarCreated
        )
    }

    /// The key its lines are counted under, and what the summary calls it.
    fn kind(self) -> &'static str {
        match self {
            SystemEvent::Sleep => "going to sleep",
            SystemEvent::Wake => "wake",
            SystemEvent::Lock => "lock",
            SystemEvent::Unlock => "unlock",
            SystemEvent::Connected { .. } => "connection",
            SystemEvent::Disconnected { .. } => "disconnection",
            SystemEvent::DisplaysAsleep => "displays' sleep",
            SystemEvent::DisplaysAwake => "displays' wake",
            SystemEvent::DisplaysChanged => "display change",
            SystemEvent::Scale => "scale change",
            SystemEvent::WorkArea => "work-area change",
            SystemEvent::TaskbarCreated => "taskbar re-creation",
        }
    }

    /// What the host does about it, for the end of its line — derived from the rules above, so
    /// the line cannot claim what the host does not do.
    fn reactions(self) -> String {
        if self == SystemEvent::TaskbarCreated {
            return "; the tray icon is put back by the window toolkit itself (wxWidgets answers \
                    TaskbarCreated)"
                .to_string();
        }
        if !self.front_may_have_changed() {
            return String::new();
        }
        let mut s = String::from("; what was cached before is asked again");
        match (self.reports_front(), self.reopens_audio()) {
            (true, true) => s.push_str(
                ", the window in front is reported again and the focus re-checked, and the audio \
                 output is opened afresh for the next sound",
            ),
            (true, false) => s.push_str(", and the window in front is reported again and the focus re-checked"),
            _ => s.push_str(" and the focus re-checked"),
        }
        if self.rewrites_display_lines() {
            s.push_str("; the display lines follow when they changed");
        }
        s
    }
}

/// Whether `a` and `b` are the same kind of event, whatever their fields say: a remote
/// connection answers a console disconnection as well as a remote one.
fn same_kind(a: SystemEvent, b: SystemEvent) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

/// An event and when the backend heard it — on the clock of the day, not the process's: the
/// line for a sleep may be written after the wake (the event loop does not run while the
/// machine sleeps), and says how long the machine slept, which only the clock of the day can.
/// `count` is how many notifications in a row it stands for (see [`push`]); `at` is when the
/// first came — or, for an event that starts something (a sleep, a lock, a disconnection, the
/// displays' sleep), the last, since the later start is the one its end answers, as
/// [`Since`] has it for starts in separate batches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamped {
    pub event: SystemEvent,
    pub at: SystemTime,
    pub count: u32,
}

/// At most this many events wait for the event loop. A burst is a handful, and repeats of one
/// event in a row take one place; more than this means the loop is not running, and the OLDEST
/// are dropped — the last of a stream of changes is the state that holds now.
const QUEUE_MAX: usize = 64;

static QUEUE: Mutex<Vec<Stamped>> = Mutex::new(Vec::new());

/// Events dropped from a full queue since the last [`take`], for its one line.
static DROPPED: AtomicU32 = AtomicU32::new(0);

/// Queues `event` for the event loop, stamped now. From any thread; never blocks for longer
/// than a push.
pub fn push(event: SystemEvent) {
    let dropped = {
        let mut q = QUEUE.lock().unwrap_or_else(|p| p.into_inner());
        queue(&mut q, event, SystemTime::now())
    };
    if dropped {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// [`push`] into `q`: folded into the last event queued when it is the same one, else appended,
/// with the oldest dropped to make room. True when one was dropped. Pure, for the tests.
fn queue(q: &mut Vec<Stamped>, event: SystemEvent, at: SystemTime) -> bool {
    if let Some(last) = q.last_mut().filter(|l| l.event == event) {
        last.count = last.count.saturating_add(1);
        if event.is_a_start() {
            last.at = at;
        }
        return false;
    }
    let dropped = q.len() >= QUEUE_MAX;
    if dropped {
        q.remove(0);
    }
    q.push(Stamped { event, at, count: 1 });
    dropped
}

/// Takes what has been queued since the last call, for the backend to hand to the host as one
/// batch. Says once when the queue overflowed since.
pub fn take() -> Vec<Stamped> {
    let batch = {
        let mut q = QUEUE.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::take(&mut *q)
    };
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        crate::logging::line(
            "system",
            &format!(
                "{dropped} system event(s) dropped: more than {QUEUE_MAX} different ones arrived \
                 between two turns of the event loop, and the oldest went"
            ),
        );
    }
    batch
}

/// `WM_WTSSESSION_CHANGE`'s `wParam` (the `WTS_*` codes of WinUser.h) as an event.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn from_wts(code: u32) -> Option<SystemEvent> {
    Some(match code {
        1 => SystemEvent::Connected { remote: false },    // WTS_CONSOLE_CONNECT
        2 => SystemEvent::Disconnected { remote: false }, // WTS_CONSOLE_DISCONNECT
        3 => SystemEvent::Connected { remote: true },     // WTS_REMOTE_CONNECT
        4 => SystemEvent::Disconnected { remote: true },  // WTS_REMOTE_DISCONNECT
        7 => SystemEvent::Lock,                           // WTS_SESSION_LOCK
        8 => SystemEvent::Unlock,                         // WTS_SESSION_UNLOCK
        _ => return None,
    })
}

/// A suspend/resume notification's `PBT_*` event type as an event. `PBT_APMRESUMEAUTOMATIC` is
/// sent on every resume; `PBT_APMRESUMESUSPEND`, which follows it when a person woke the machine,
/// would be the same wake twice.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn from_pbt(pbt: u32) -> Option<SystemEvent> {
    match pbt {
        0x4 => Some(SystemEvent::Sleep), // PBT_APMSUSPEND
        0x12 => Some(SystemEvent::Wake), // PBT_APMRESUMEAUTOMATIC
        _ => None,
    }
}

/// `WM_SETTINGCHANGE`'s `wParam` as an event: the work area (`SPI_SETWORKAREA`) and the display
/// scale (`SPI_SETLOGICALDPIOVERRIDE`). Every other setting — and there are many, broadcast all
/// day (`Environment`, `intl`, `ImmersiveColorSet` arrive with a `wParam` of 0) — is nothing here.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn from_setting(spi: u32) -> Option<SystemEvent> {
    match spi {
        0x002F => Some(SystemEvent::WorkArea), // SPI_SETWORKAREA
        0x009F => Some(SystemEvent::Scale),    // SPI_SETLOGICALDPIOVERRIDE
        _ => None,
    }
}

/// What the host does about one batch of events.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The `[system]` lines: one per event (repeats of one event in a row are one line), apart
    /// from a display-family event that keeps coming, which is counted — see [`Since::plan`].
    pub lines: Vec<String>,
    /// Report the window in front again, as an activation.
    pub report_front: bool,
    /// Re-check the focus.
    pub recheck: bool,
    /// Write the `[env]` display lines again, when they changed.
    pub env: bool,
    /// Open the audio output again at the next sound.
    pub reopen_audio: bool,
}

/// How late a line has to be written before it says when its event was heard, and how short a
/// sleep measures as "heard together with the wake".
const LATE: Duration = Duration::from_secs(2);

/// When each event that starts something last happened — for the lines that say how long it
/// lasted — and the counts of display-family events that keep coming.
#[derive(Debug, Default)]
pub struct Since {
    began: Vec<(SystemEvent, SystemTime)>,
    repeats: Repeats,
}

impl Since {
    /// The plan for `batch`, in order, at `now` (the process's clock, for the counts) and `wall`
    /// (the clock of the day, for saying a line is late).
    ///
    /// A display, scale, work-area or taskbar event is written in full the first time, and when
    /// it comes again within ten minutes it is counted instead, with a line a minute at most —
    /// by the next one, or by [`due`](Self::due) when it has stopped. A wake writes what is
    /// still counted, just before its own line, and forgets it, so the next one is written in
    /// full. What the host does about it (the epochs, the focus round) is done every time all
    /// the same.
    pub fn plan(&mut self, batch: &[Stamped], now: Instant, wall: SystemTime) -> Plan {
        let mut plan = Plan::default();
        let mut i = 0;
        while i < batch.len() {
            let mut s = batch[i];
            // Repeats of one event in a row are one line: a scale change is broadcast as
            // several messages, and each would say the same. A start is stamped by the last of
            // them, as the queue stamps it ([`queue`]).
            let mut n = 0u32;
            let mut k = i;
            while k < batch.len() && batch[k].event == s.event {
                n = n.saturating_add(batch[k].count.max(1));
                if s.event.is_a_start() {
                    s.at = batch[k].at;
                }
                k += 1;
            }
            i = k;
            // The counts below run on the process's clock, which on macOS stops while the Mac
            // sleeps: across a wake a display change from before the night and one at the wake
            // would read as minutes apart, and the wake's own display change would only be
            // counted. So a wake writes what is still counted, and forgets it.
            if s.event == SystemEvent::Wake {
                for (kind, count, over) in self.repeats.take_all() {
                    plan.lines.push(before_sleep(&kind, count, over));
                }
            }
            let mut line = self.line(s, wall);
            if n > 1 {
                line.push_str(&format!(" ({n} notifications)"));
            }
            if s.event.may_storm() {
                match self.repeats.note(s.event.kind(), now) {
                    Said::Line => plan.lines.push(line),
                    Said::Counted => {}
                    Said::Summary { count, over } => plan.lines.push(summary(s.event.kind(), count, over)),
                }
            } else {
                plan.lines.push(line);
            }
            plan.report_front |= s.event.reports_front();
            plan.recheck |= s.event.front_may_have_changed();
            plan.env |= s.event.rewrites_display_lines();
            plan.reopen_audio |= s.event.reopens_audio();
        }
        plan
    }

    /// The summaries owed at `now` for display-family events that stopped coming before their
    /// minute was up, and forgets the kinds quiet for ten minutes, whose next one is written in
    /// full again. Asked from the event loop's tick.
    pub fn due(&mut self, now: Instant) -> Vec<String> {
        self.repeats.due(now).into_iter().map(|(kind, count, over)| summary(&kind, count, over)).collect()
    }

    /// The line for one event, heard at `s.at` and written at `wall`: what it ends and how long
    /// that lasted, when it was heard if that was a while ago, and what the host does about it.
    /// An event that starts something is remembered — a second one before the end replaces the
    /// first, since the later is the one the end answers.
    fn line(&mut self, s: Stamped, wall: SystemTime) -> String {
        let kind = s.event;
        let mut line = format!("the {}", kind.words());
        if let Some(start) = kind.ends() {
            let began = self.began.iter().position(|(k, _)| same_kind(*k, start)).map(|i| self.began.remove(i).1);
            match began.map(|t| s.at.duration_since(t)) {
                // Heard together with its start: the sleep notice was heard only as the machine
                // woke, and the time between the two says nothing about how long it slept.
                Some(Ok(lasted)) if kind == SystemEvent::Wake && lasted < LATE => line.push_str(
                    " (how long it slept is not known: the notice that it was going to sleep was \
                     heard only as it woke)",
                ),
                Some(Ok(lasted)) => line.push_str(&format!(" ({} for {})", kind.state_before(), human(lasted))),
                // The clock was set back in between: nothing is said about how long.
                Some(Err(_)) => {}
                None => line.push_str(&format!(
                    " (when it began was not heard, so how long it was {} is not known)",
                    kind.state_before()
                )),
            }
        } else if kind.is_a_start() {
            self.began.retain(|(k, _)| !same_kind(*k, kind));
            self.began.push((kind, s.at));
        }
        if let Ok(d) = wall.duration_since(s.at) {
            if d >= LATE {
                line.push_str(&format!(" — heard at {}, written {} later", secs(s.at), human(d)));
            }
        }
        line.push_str(&kind.reactions());
        line
    }
}

/// The line for a display-family event that came again `count` times over `over`.
fn summary(kind: &str, count: u64, over: Duration) -> String {
    format!(
        "the same {kind} came {count} more time(s) in the last {} s (written in full the first \
         time; counted after that, with a line like this at most once a minute, and each handled \
         as the first was)",
        over.as_secs()
    )
}

/// The line for a display-family event still counted when the machine woke: it came `count`
/// more times within `over` of its last line, all before the sleep.
fn before_sleep(kind: &str, count: u64, over: Duration) -> String {
    format!(
        "the same {kind} came {count} more time(s) within {} s of its last line, before the \
         machine slept (counted, as the ones after the first within ten minutes are; the first \
         after the wake is written in full again)",
        over.as_secs()
    )
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A duration as a person says it: `42 s`, `12 min 4 s`, `7 h 3 min`, `2 d 5 h`.
fn human(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min {} s", s / 60, s % 60),
        3600..=86_399 => format!("{} h {} min", s / 3600, (s % 3600) / 60),
        _ => format!("{} d {} h", s / 86_400, (s % 86_400) / 3600),
    }
}

#[cfg(test)]
mod tests {
    use super::SystemEvent::*;
    use super::*;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_790_000_000 + secs)
    }

    fn st(event: SystemEvent, secs: u64) -> Stamped {
        Stamped { event, at: at(secs), count: 1 }
    }

    /// The plan for `batch`, written the moment its last event was heard.
    fn plan(since: &mut Since, batch: &[Stamped]) -> Plan {
        let wall = batch.last().map(|s| s.at).unwrap_or(UNIX_EPOCH);
        since.plan(batch, Instant::now(), wall)
    }

    /// The one line for `kind`, heard at `t` and written as it happened.
    fn now(since: &mut Since, kind: SystemEvent, t: SystemTime) -> String {
        written(since, kind, t, t)
    }

    /// The one line for `kind`, heard at `t` and written at `wall`.
    fn written(since: &mut Since, kind: SystemEvent, t: SystemTime, wall: SystemTime) -> String {
        let mut p = since.plan(&[Stamped { event: kind, at: t, count: 1 }], Instant::now(), wall);
        assert_eq!(p.lines.len(), 1, "{:?}", p.lines);
        p.lines.remove(0)
    }

    const ALL: [SystemEvent; 14] = [
        Sleep,
        Wake,
        Lock,
        Unlock,
        Connected { remote: false },
        Connected { remote: true },
        Disconnected { remote: false },
        Disconnected { remote: true },
        DisplaysAsleep,
        DisplaysAwake,
        DisplaysChanged,
        Scale,
        WorkArea,
        TaskbarCreated,
    ];

    fn which(f: fn(SystemEvent) -> bool) -> Vec<SystemEvent> {
        ALL.into_iter().filter(|&k| f(k)).collect()
    }

    #[test]
    fn the_windows_codes_map_to_events_and_everything_else_to_nothing() {
        assert_eq!(from_wts(7), Some(Lock));
        assert_eq!(from_wts(8), Some(Unlock));
        assert_eq!(from_wts(1), Some(Connected { remote: false }));
        assert_eq!(from_wts(3), Some(Connected { remote: true }));
        assert_eq!(from_wts(2), Some(Disconnected { remote: false }));
        assert_eq!(from_wts(4), Some(Disconnected { remote: true }));
        // Logon, logoff, remote control, create, terminate: nothing for the host.
        for code in [5, 6, 9, 10, 11, 0, 99] {
            assert_eq!(from_wts(code), None, "{code}");
        }
        assert_eq!(from_pbt(0x4), Some(Sleep));
        assert_eq!(from_pbt(0x12), Some(Wake));
        // PBT_APMRESUMESUSPEND (the same resume again), PBT_APMPOWERSTATUSCHANGE, the setting one.
        for pbt in [0x7, 0xA, 0x8013] {
            assert_eq!(from_pbt(pbt), None, "{pbt:#x}");
        }
        assert_eq!(from_setting(0x2F), Some(WorkArea));
        assert_eq!(from_setting(0x9F), Some(Scale));
        // wParam 0 is how "Environment", "intl" and "ImmersiveColorSet" arrive.
        for spi in [0, 0x14, 0x44] {
            assert_eq!(from_setting(spi), None, "{spi:#x}");
        }
    }

    /// What the host and the backends do after each event, as a table: only the moments
    /// something comes BACK do anything, and each list is exactly the one its callers document.
    #[test]
    fn what_each_event_sets_off() {
        let back = [Connected { remote: false }, Connected { remote: true }];
        assert_eq!(
            which(SystemEvent::front_may_have_changed),
            [Wake, Unlock, back[0], back[1], DisplaysAwake, DisplaysChanged, Scale, WorkArea]
        );
        assert_eq!(which(SystemEvent::reports_front), [Wake, Unlock, back[0], back[1]]);
        assert_eq!(which(SystemEvent::reopens_audio), [Wake, back[0], back[1]]);
        assert_eq!(which(SystemEvent::rewrites_display_lines), [DisplaysChanged, Scale]);
        assert_eq!(
            which(SystemEvent::rechecks_capture),
            [Wake, Unlock, back[0], back[1], DisplaysAwake, DisplaysChanged, Scale]
        );
        assert_eq!(which(SystemEvent::retries_screen_reader), [Wake, Unlock, back[0], back[1]]);
        assert_eq!(which(SystemEvent::may_storm), [DisplaysChanged, Scale, WorkArea, TaskbarCreated]);
        let ends: Vec<(SystemEvent, SystemEvent)> = ALL.into_iter().filter_map(|k| k.ends().map(|s| (k, s))).collect();
        assert_eq!(
            ends,
            [
                (Wake, Sleep),
                (Unlock, Lock),
                (back[0], Disconnected { remote: false }),
                (back[1], Disconnected { remote: true }),
                (DisplaysAwake, DisplaysAsleep)
            ]
        );
    }

    #[test]
    fn every_end_has_a_start_and_a_state() {
        for end in ALL.into_iter().filter(|k| k.ends().is_some()) {
            let start = end.ends().expect("an end");
            assert!(start.is_a_start(), "{start:?}");
            assert!(!end.state_before().is_empty(), "{end:?}");
        }
        assert!(!DisplaysChanged.is_a_start());
        assert!(!Wake.is_a_start());
    }

    #[test]
    fn a_wake_says_how_long_the_machine_slept_and_asks_for_everything_again() {
        let mut since = Since::default();
        let p = plan(&mut since, &[st(Sleep, 0), st(Wake, 2 * 3600 + 13 * 60 + 5)]);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0], "the machine is going to sleep — heard at 1790000000, written 2 h 13 min later");
        assert!(p.lines[1].starts_with("the machine woke from sleep (asleep for 2 h 13 min); what was cached"), "{}", p.lines[1]);
        assert!(p.report_front && p.recheck && p.reopen_audio);
        assert!(!p.env);
        // A wake whose sleep was not heard says so rather than inventing a length.
        let p = plan(&mut since, &[st(Wake, 9000)]);
        assert!(p.lines[0].contains("when it began was not heard"), "{}", p.lines[0]);
    }

    #[test]
    fn a_wake_says_how_long_the_machine_slept() {
        let mut since = Since::default();
        assert_eq!(now(&mut since, Sleep, at(0)), "the machine is going to sleep");
        assert!(now(&mut since, Wake, at(7 * 3600 + 3 * 60 + 20)).starts_with("the machine woke from sleep (asleep for 7 h 3 min);"));
        // The sleep was answered; a second wake has nothing to measure, and says so.
        assert!(now(&mut since, Wake, at(8 * 3600))
            .starts_with("the machine woke from sleep (when it began was not heard, so how long it was asleep is not known);"));
    }

    /// The sleep notice was heard on time and reached the loop only after the wake: both lines
    /// are written together, the sleep is measured from when it was heard, and its line says it
    /// was written late.
    #[test]
    fn a_sleep_delivered_after_the_wake_is_measured_from_when_it_was_heard() {
        let mut since = Since::default();
        let woke = at(3 * 3600);
        let wrote = woke + Duration::from_millis(40);
        assert_eq!(
            written(&mut since, Sleep, at(0), wrote),
            "the machine is going to sleep — heard at 1790000000, written 3 h 0 min later"
        );
        assert!(written(&mut since, Wake, woke, wrote).starts_with("the machine woke from sleep (asleep for 3 h 0 min);"));
    }

    /// The sleep notice was only heard as the machine woke: the two were heard together, and the
    /// line does not claim a sleep of no time at all.
    #[test]
    fn a_sleep_heard_only_at_the_wake_is_not_measured() {
        let mut since = Since::default();
        let woke = at(3 * 3600);
        now(&mut since, Sleep, woke);
        let line = now(&mut since, Wake, woke + Duration::from_millis(30));
        assert!(
            line.starts_with(
                "the machine woke from sleep (how long it slept is not known: the notice that it was \
                 going to sleep was heard only as it woke);"
            ),
            "{line}"
        );
        // Other ends keep their short durations: a lock of one second is a lock of one second.
        now(&mut since, Lock, at(0));
        assert!(now(&mut since, Unlock, at(1)).starts_with("the screen was unlocked (locked for 1 s);"));
    }

    #[test]
    fn a_lock_and_unlock_report_the_front_again_without_reopening_the_audio() {
        let mut since = Since::default();
        let lock = plan(&mut since, &[st(Lock, 0)]);
        assert_eq!(lock, Plan { lines: lock.lines.clone(), ..Plan::default() }, "a lock only says so");
        let unlock = plan(&mut since, &[st(Unlock, 305)]);
        assert!(unlock.lines[0].starts_with("the screen was unlocked (locked for 5 min 5 s);"), "{}", unlock.lines[0]);
        assert!(unlock.report_front && unlock.recheck && !unlock.reopen_audio && !unlock.env);
    }

    #[test]
    fn each_end_answers_its_own_start() {
        let mut since = Since::default();
        now(&mut since, Lock, at(0));
        now(&mut since, Sleep, at(60));
        now(&mut since, DisplaysAsleep, at(61));
        assert!(now(&mut since, DisplaysAwake, at(3661)).starts_with("the displays woke (asleep for 1 h 0 min);"));
        assert!(now(&mut since, Wake, at(3662)).starts_with("the machine woke from sleep (asleep for 1 h 0 min);"));
        assert!(now(&mut since, Unlock, at(3700)).starts_with("the screen was unlocked (locked for 1 h 1 min);"));
        now(&mut since, Disconnected { remote: false }, at(4000));
        assert!(now(&mut since, Connected { remote: false }, at(4012))
            .starts_with("the session is back at the console (away for 12 s);"));
        // A remote client taking over a session switched away from answers it as well.
        now(&mut since, Disconnected { remote: false }, at(5000));
        assert!(now(&mut since, Connected { remote: true }, at(5090))
            .starts_with("the session was connected to a remote client (away for 1 min 30 s);"));
    }

    #[test]
    fn a_second_start_replaces_the_first() {
        let mut since = Since::default();
        now(&mut since, Lock, at(0));
        now(&mut since, Lock, at(600));
        assert!(now(&mut since, Unlock, at(630)).starts_with("the screen was unlocked (locked for 30 s);"));
    }

    #[test]
    fn a_clock_set_back_says_nothing_about_how_long() {
        let mut since = Since::default();
        now(&mut since, Sleep, at(1000));
        assert!(now(&mut since, Wake, at(10)).starts_with("the machine woke from sleep;"));
        // Nor about lateness: a delivery "before" it happened is just delivered.
        assert_eq!(written(&mut since, Lock, at(50), at(40)), "the screen was locked");
    }

    #[test]
    fn a_display_change_starts_and_ends_nothing() {
        let mut since = Since::default();
        assert!(now(&mut since, DisplaysChanged, at(0)).starts_with("the display configuration changed;"));
        assert!(since.began.is_empty());
        assert!(
            written(&mut since, Wake, at(0), at(1)).starts_with("the machine woke from sleep (when it began"),
            "a second late is not late"
        );
    }

    #[test]
    fn a_display_change_rewrites_the_display_lines_and_repeats_are_one_line() {
        let mut since = Since::default();
        let p = plan(&mut since, &[st(Scale, 0), st(Scale, 0), st(DisplaysChanged, 1), st(WorkArea, 1)]);
        assert_eq!(p.lines.len(), 3, "{:?}", p.lines);
        assert!(p.lines[0].ends_with("(2 notifications)"), "{}", p.lines[0]);
        assert!(p.lines[1].ends_with("; the display lines follow when they changed"), "{}", p.lines[1]);
        assert!(p.env && p.recheck && !p.report_front && !p.reopen_audio);
        let p = plan(&mut since, &[st(WorkArea, 2)]);
        assert!(p.recheck && !p.env);
    }

    /// A display whose link keeps dropping: the first change is written in full, the ones after
    /// it within ten minutes are counted, with a summary a minute at most — by the next one or
    /// by the tick — and each is still handled; a lock between them is always written.
    #[test]
    fn a_display_that_keeps_changing_is_written_once_and_then_counted() {
        let mut since = Since::default();
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        let first = since.plan(&[st(DisplaysChanged, 0)], s(0), at(0));
        assert_eq!(first.lines.len(), 1);
        assert!(first.lines[0].starts_with("the display configuration changed"), "{:?}", first.lines);
        for n in 1..20u64 {
            let p = since.plan(&[st(DisplaysChanged, n * 3)], s(n * 3), at(n * 3));
            assert!(p.lines.is_empty(), "{n}: {:?}", p.lines);
            assert!(p.env && p.recheck, "still handled");
        }
        // A lock between them is its own line.
        let lock = since.plan(&[st(Lock, 58)], s(58), at(58));
        assert_eq!(lock.lines.len(), 1);
        let later = since.plan(&[st(DisplaysChanged, 61)], s(61), at(61));
        assert_eq!(later.lines.len(), 1, "{:?}", later.lines);
        assert!(later.lines[0].starts_with("the same display change came 20 more time(s) in the last 61 s"), "{}", later.lines[0]);
        // It stops: the tick owes one more count, once the minute is up, and only once.
        since.plan(&[st(DisplaysChanged, 70)], s(70), at(70));
        assert!(since.due(s(100)).is_empty());
        let owed = since.due(s(122));
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert!(owed[0].starts_with("the same display change came 1 more time(s)"), "{}", owed[0]);
        assert!(since.due(s(200)).is_empty());
        // Quiet for ten minutes: the next one is news again.
        assert!(since.due(s(70 + 600)).is_empty());
        let news = since.plan(&[st(DisplaysChanged, 700)], s(700), at(700));
        assert!(news.lines[0].starts_with("the display configuration changed"), "{:?}", news.lines);
        // Lock and unlock twice within a minute: every line written, each with its length.
        let mut since = Since::default();
        for (i, secs) in [0u64, 20, 40].into_iter().enumerate() {
            let p = since.plan(&[st(Lock, secs), st(Unlock, secs + 5)], s(secs), at(secs + 5));
            assert_eq!(p.lines.len(), 2, "{i}: {:?}", p.lines);
            assert!(p.lines[1].contains("(locked for 5 s)"), "{}", p.lines[1]);
        }
    }

    /// A display that flaps once a second (the macOS reconfiguration callback, one event per
    /// change): the first change is written, the rest of the minute counted, the count written
    /// by the next change after the minute or, when none comes, by the tick; other events are
    /// written as they come in the middle of it.
    #[test]
    fn a_flapping_display_writes_a_line_a_minute_with_a_count() {
        let mut since = Since::default();
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        let flap = |since: &mut Since, n: u64| since.plan(&[st(DisplaysChanged, n)], s(n), at(n)).lines;
        assert!(since.due(s(0)).is_empty(), "nothing counted");
        assert_eq!(flap(&mut since, 0).len(), 1);
        for n in 1..60 {
            assert!(flap(&mut since, n).is_empty(), "{n} s in");
        }
        assert!(since.due(s(59)).is_empty(), "not before the minute");
        let line = flap(&mut since, 60);
        assert_eq!(line.len(), 1);
        assert!(line[0].starts_with("the same display change came 60 more time(s) in the last 60 s"), "{}", line[0]);
        // Other events are written as they come, in the middle of it.
        assert_eq!(now(&mut since, Lock, at(61)), "the screen was locked");
        assert!(flap(&mut since, 62).is_empty());
        assert!(since.due(s(100)).is_empty());
        let owed = since.due(s(120));
        assert_eq!(owed.len(), 1, "{owed:?}");
        assert!(owed[0].starts_with("the same display change came 1 more time(s) in the last 60 s"), "{}", owed[0]);
        assert!(since.due(s(300)).is_empty(), "written once");
        // The summary was a line: a change right after it is counted.
        assert!(flap(&mut since, 130).is_empty());
    }

    /// A display change before the night and one at the wake: on macOS the process's clock
    /// stopped while the Mac slept, so the two are seconds apart on it. The wake writes what was
    /// still counted, before its own line, and the display change at the wake is written in full
    /// — not counted, and not summed up as "in the last 61 s" for a span that holds the night.
    #[test]
    fn a_wake_writes_the_counts_from_before_the_sleep_and_starts_them_afresh() {
        let mut since = Since::default();
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        assert_eq!(since.plan(&[st(DisplaysChanged, 0)], s(0), at(0)).lines.len(), 1);
        assert!(since.plan(&[st(DisplaysChanged, 10)], s(10), at(10)).lines.is_empty());
        assert!(since.plan(&[st(DisplaysChanged, 20)], s(20), at(20)).lines.is_empty());
        // Asleep for eight hours by the wall clock; 25 s later by the process's.
        let night = 8 * 3600;
        let p = since.plan(&[st(Sleep, 30), st(Wake, night), st(DisplaysChanged, night + 2)], s(25), at(night + 2));
        assert_eq!(p.lines.len(), 4, "{:#?}", p.lines);
        assert!(p.lines[0].starts_with("the machine is going to sleep"), "{}", p.lines[0]);
        assert!(
            p.lines[1].starts_with("the same display change came 2 more time(s) within 20 s of its last line, before the machine slept"),
            "{}",
            p.lines[1]
        );
        assert!(p.lines[2].starts_with("the machine woke from sleep (asleep for 7 h 59 min)"), "{}", p.lines[2]);
        assert!(p.lines[3].starts_with("the display configuration changed;"), "{}", p.lines[3]);
        // Nothing is owed after it, and the next one within ten minutes is counted again.
        assert!(since.due(s(25 + 120)).is_empty());
        assert!(since.plan(&[st(DisplaysChanged, night + 5)], s(28), at(night + 5)).lines.is_empty());
        // A wake with nothing counted adds nothing.
        let mut since = Since::default();
        assert_eq!(plan(&mut since, &[st(Sleep, 0), st(Wake, 60)]).lines.len(), 2);
    }

    /// The queue folds an event into the one queued just before it when they are the same, and
    /// when it is full drops its oldest, never the newest, and says it did.
    #[test]
    fn the_queue_folds_repeats_and_drops_the_oldest_when_full() {
        let mut q = Vec::new();
        for n in 0..500 {
            assert!(!queue(&mut q, DisplaysChanged, at(n)));
        }
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].count, q[0].at), (500, at(0)), "one place, counted, stamped by the first");
        let p = plan(&mut Since::default(), &q);
        assert!(p.lines[0].ends_with("(500 notifications)"), "{}", p.lines[0]);
        // A start folded is stamped by the last: the later lock is the one the unlock answers,
        // as it is when the two come in separate batches (`a_second_start_replaces_the_first`).
        let mut q = Vec::new();
        queue(&mut q, Lock, at(0));
        queue(&mut q, Lock, at(600));
        assert_eq!((q.len(), q[0].count, q[0].at), (1, 2, at(600)));
        queue(&mut q, Unlock, at(630));
        let p = plan(&mut Since::default(), &q);
        assert!(p.lines[1].starts_with("the screen was unlocked (locked for 30 s);"), "{}", p.lines[1]);
        // The same when the batch itself holds the two in a row.
        let p = plan(&mut Since::default(), &[st(Lock, 0), st(Lock, 600), st(Unlock, 630)]);
        assert!(p.lines[1].starts_with("the screen was unlocked (locked for 30 s);"), "{}", p.lines[1]);
        // Alternating events cannot be folded: past the cap the oldest go, and the last — the
        // unlock — is kept.
        let mut q = Vec::new();
        let mut dropped = 0;
        for n in 0..(QUEUE_MAX as u64 + 10) {
            dropped += queue(&mut q, if n % 2 == 0 { DisplaysChanged } else { WorkArea }, at(n)) as usize;
        }
        dropped += queue(&mut q, Unlock, at(999)) as usize;
        assert_eq!(q.len(), QUEUE_MAX);
        assert_eq!(dropped, 11);
        assert_eq!(q.last().map(|s| s.event), Some(Unlock));
        assert_eq!(q[0].at, at(11), "the oldest were dropped");
    }

    #[test]
    fn the_taskbar_coming_back_is_only_said() {
        let p = plan(&mut Since::default(), &[st(TaskbarCreated, 0)]);
        assert_eq!(p.lines.len(), 1);
        assert!(p.lines[0].contains("wxWidgets"), "{}", p.lines[0]);
        assert!(!p.report_front && !p.recheck && !p.env && !p.reopen_audio);
    }

    #[test]
    fn which_events_recheck_the_capture_paths() {
        for e in [Wake, Unlock, Connected { remote: true }, DisplaysAwake, DisplaysChanged, Scale] {
            assert!(e.rechecks_capture(), "{e:?}");
        }
        for e in [Sleep, Lock, Disconnected { remote: false }, DisplaysAsleep, WorkArea, TaskbarCreated] {
            assert!(!e.rechecks_capture(), "{e:?}");
        }
    }

    /// The end of each line says what the host does, and only that: the rules above decide it.
    #[test]
    fn each_line_says_what_the_host_does_about_it() {
        for e in ALL {
            let r = e.reactions();
            assert_eq!(r.contains("focus re-checked"), e.front_may_have_changed(), "{e:?}: {r}");
            assert_eq!(r.contains("window in front is reported again"), e.reports_front(), "{e:?}: {r}");
            assert_eq!(r.contains("audio output is opened afresh"), e.reopens_audio(), "{e:?}: {r}");
            assert_eq!(r.contains("display lines follow"), e.rewrites_display_lines(), "{e:?}: {r}");
        }
    }

    #[test]
    fn durations_read_as_a_person_says_them() {
        assert_eq!(human(Duration::from_secs(0)), "0 s");
        assert_eq!(human(Duration::from_secs(45)), "45 s");
        assert_eq!(human(Duration::from_secs(59)), "59 s");
        assert_eq!(human(Duration::from_secs(724)), "12 min 4 s");
        assert_eq!(human(Duration::from_secs(725)), "12 min 5 s");
        assert_eq!(human(Duration::from_secs(3600)), "1 h 0 min");
        assert_eq!(human(Duration::from_secs(2 * 3600 + 13 * 60 + 59)), "2 h 13 min");
        assert_eq!(human(Duration::from_secs(2 * 86_400 + 5 * 3600 + 59)), "2 d 5 h");
    }

    #[test]
    fn the_queue_takes_what_was_pushed_once() {
        // The one process-wide queue: taken empty first, so what another test left is gone.
        let _ = take();
        push(Lock);
        push(Unlock);
        let got: Vec<SystemEvent> = take().into_iter().map(|s| s.event).collect();
        assert_eq!(got, vec![Lock, Unlock]);
        assert!(take().is_empty());
    }
}
