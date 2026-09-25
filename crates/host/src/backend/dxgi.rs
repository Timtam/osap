//! DXGI Desktop Duplication: the second way of reading the screen on Windows.
//!
//! **Why it exists.** The standard path (`windows.rs`, `capture_screen` and `GetPixel`) reads
//! the screen DC, which is the image the compositor last composed. A game that presents
//! through a flip-model swap chain may skip that composition, and a module reading it may then
//! see a frozen or black picture — wrong data, not slow data. Desktop duplication reads the
//! image the compositor hands the display instead, which for one fullscreen game did contain
//! its frames: the standard path read it frozen and duplication live (TODO.md, step 0 and M4);
//! a game whose frames bypass composition altogether would be missed by both (M14). It is
//! also far cheaper per read (no wait for the next vsync), but speed alone is not why it was
//! built; see `docs/screen-frame-sharing-design.md`, step 7.
//!
//! **Who uses it.** Only a module whose manifest says `[screen] capture = "duplication"` (or
//! inherits that from its code runtime — `capture_source.rs`), and only while the
//! "read the screen through the graphics card" switch in Application settings is on. Every
//! other read in the process goes through GDI exactly as before, and until such a module
//! reads the screen or has a window trigger fire, this file does nothing at all: no thread, no
//! factory, no device, not a line in the log. The verified plug-in overlays never get here.
//!
//! **One owner thread.** Every Direct3D and DXGI object lives on the `dxgi-capture` thread.
//! Both the pump and the image worker read the screen, a process may hold one duplication
//! per output, the immediate context must not be used from two threads at once — and, the
//! decisive reason, a mutex cannot put a time limit on a call that is stuck inside a graphics
//! driver. Every captured key and hotkey waits for the pump — the low-level keyboard hook has a
//! thread of its own, but what it matches is dispatched there — so callers send a request and
//! wait with a deadline, and a driver that stops answering costs them a bounded wait, once, and
//! then nothing.
//!
//! **Pull, not push.** Each request calls `AcquireNextFrame(0)` once per output it touches.
//! A new frame is copied (GPU to GPU) into a kept "last frame"; `WAIT_TIMEOUT` means nothing
//! was composed since, so the kept frame IS the screen. There is no periodic capture — the
//! frame-sharing design rejected that for good reasons — and the kept frame is thrown away
//! on any error.
//!
//! **Mostly unmeasured.** Every constant below is an estimate from the design. The first
//! numbers from the reference machine (`dxgi_measure` in a test build: small reads in about
//! 1 ms, bytes identical to GDI, device creation 0.2 s and sometimes 4 s) are in TODO.md with
//! the measurements (M1-M19) that are to set them; the one set from the application under
//! load, a fullscreen game on an integrated GPU (6-7 ms per 1280x1024 read, device 32 ms), is
//! under M4.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::core::Interface;
use windows::Win32::Foundation::{E_ACCESSDENIED, HMODULE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_10_1,
    D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_FORMAT_B8G8R8A8_UNORM,
    DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_UNSPECIFIED,
    DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1, IDXGIOutput6,
    IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED,
    DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE, DXGI_ERROR_NOT_FOUND,
    DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_UNSUPPORTED, DXGI_ERROR_WAIT_TIMEOUT,
    DXGI_OUTDUPL_FRAME_INFO,
};

use super::CapturedImage;
use crate::logging;

// ---- Constants ------------------------------------------------------------------------------
//
// Estimates, every one of them. The step-3 gate of the design measures M1, M2, M3, M6, M9,
// M11 and M16 on the reference machine and sets these from the results.

/// The largest region a duplication read allocates for — the macOS backend's `MAX_POINTS`.
/// A Lua region of 60000x60000 would otherwise ask for about 14 GB and abort the process; the
/// GDI path is protected from that only by accident (its bitmap creation fails first).
const MAX_PIXELS: i64 = 40_000_000;
/// How long a pump read waits for an open duplication. The design's estimate for a small
/// read is well under 2 ms; this is the margin for a GPU busy with a game (M9) — and for the
/// reopen a read makes at once when it finds the duplication lost (a fullscreen switch),
/// which then waits up to [`FIRST_FRAME_WAIT`] for its first picture. A pump that gave up
/// sooner than that would call the engine late when it is doing exactly what it should.
const PUMP_READY_WAIT: Duration = Duration::from_millis(60);
const _: () = assert!(PUMP_READY_WAIT.as_millis() >= FIRST_FRAME_WAIT.as_millis() + 10);
/// How long the FIRST pump read of an opening waits — device creation and `DuplicateOutput`
/// together, estimated at 35-330 ms (M2). Only one pump read per opening waits at all, and
/// the budget below is the stricter rule: as the constants stand it caps this wait at 60 ms.
/// The opening measured on the reference machine (about 200 ms with a warm driver) is longer
/// than either, so there that first read gets the standard picture — which is what `prewarm`
/// is for. On the machine of M4 the device took 32 ms and the first picture came 4-16 ms
/// after the duplication opened; whether that first read got it is not recorded.
const PUMP_OPENING_WAIT: Duration = Duration::from_millis(150);
/// The image worker's wait. Shorter than the design's first 1000 ms because the worker is
/// shared by every VM, including the ones that read the standard way, and a batch stuck
/// behind a driver holds up all of them.
const WORKER_WAIT: Duration = Duration::from_millis(250);
/// How much of any 300 ms the pump may spend waiting for this thread. 300 ms was the keyboard
/// hook's budget (`LowLevelHooksTimeout`) while it shared the pump; it has a thread of its own
/// now, but every captured key and hotkey still waits for the pump, and that budget is shared
/// with everything else the pump does. Duplication may take a fifth of it and no more.
const PUMP_BUDGET: Duration = Duration::from_millis(60);
const PUMP_BUDGET_WINDOW: Duration = Duration::from_millis(300);
/// What an answered read costs anyway — about 1 ms measured for a small region. Only the part
/// of a pump wait beyond this is charged to the budget, which exists for SLOW answers: a module
/// reading quickly and often must not run it out and be sent to the 16 ms standard read the
/// budget was meant to spare it.
const PUMP_QUICK: Duration = Duration::from_millis(5);
/// With less budget left than this a pump read is not sent at all. A wait of a millisecond or
/// less can only time out, and a timed-out read is a request the engine then discards.
const PUMP_MIN_WAIT: Duration = Duration::from_millis(5);
/// How long a freshly opened duplication may take to deliver its first real picture.
const FIRST_FRAME_WAIT: Duration = Duration::from_millis(50);
/// A duplication unused this long is released: it holds one of the four duplication slots
/// Windows gives a session, and GPU memory the size of the screen twice over.
const IDLE_RELEASE: Duration = Duration::from_secs(30);
/// The Direct3D device outlives the duplications' idle release — it is what makes the next
/// opening cheap (M11) — but not the whole session: after this long without a read it goes
/// too, with its driver memory, and the thread sleeps until the next read.
const DEVICE_IDLE_RELEASE: Duration = Duration::from_secs(300);
/// How often the thread wakes while it holds something, to notice the switch going off and
/// a duplication going idle. It sleeps without a timeout when it holds nothing.
const WAKE: Duration = Duration::from_secs(1);
/// A read still outstanding after this long counts as a hang; three of them stop duplication
/// until the switch is turned off and on again.
const HANG: Duration = Duration::from_secs(2);
/// The same for an opening. Slow there is expected — the first device of a process loads the
/// graphics driver, measured at up to 4.3 s — so only an opening this long counts, and one
/// that never ends still does: a driver stuck in device creation.
const OPEN_HANG: Duration = Duration::from_secs(10);
const HANGS_TO_DISABLE: u32 = 3;
/// How long the application's exit waits for this thread before leaving it to the process.
const SHUTDOWN_WAIT: Duration = Duration::from_millis(500);

// ---- The pure part: which output holds which part of a region -------------------------------

/// One output's place on the desktop, in the physical pixels the per-monitor-aware manifest
/// makes every coordinate in this application. `right`/`bottom` are exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputGeom {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    /// Any rotation but none. The duplicated surface stays unrotated while the desktop
    /// rectangle is rotated, and this version does not turn one into the other.
    pub rotated: bool,
}

/// The part of a requested region that one output holds: where it is on that output's
/// surface, how big, and where it goes in the region's image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub output: usize,
    pub src_x: u32,
    pub src_y: u32,
    pub w: u32,
    pub h: u32,
    pub dst_x: u32,
    pub dst_y: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Empty when the region lies wholly off the desktop: every pixel stays (0,0,0,0), the
    /// bytes GDI produces there too (measured).
    Pieces(Vec<Piece>),
    /// The region touches this output, which is rotated.
    Rotated(usize),
}

/// Two outputs with the very same rectangle are one mirrored display ("Duplicate these
/// displays"): the same picture twice. Only the first is read.
fn mirrors_an_earlier(outs: &[OutputGeom], i: usize) -> bool {
    let o = outs[i];
    outs[..i]
        .iter()
        .any(|p| (p.left, p.top, p.right, p.bottom) == (o.left, o.top, o.right, o.bottom))
}

type Rect64 = (i64, i64, i64, i64);

fn intersect(a: Rect64, b: Rect64) -> Option<Rect64> {
    let r = (a.0.max(b.0), a.1.max(b.1), a.2.min(b.2), a.3.min(b.3));
    (r.0 < r.2 && r.1 < r.3).then_some(r)
}

fn area(r: Rect64) -> i64 {
    (r.2 - r.0) * (r.3 - r.1)
}

fn region64(x: i32, y: i32, w: i32, h: i32) -> Rect64 {
    let (x, y) = (x as i64, y as i64);
    (x, y, x + w.max(0) as i64, y + h.max(0) as i64)
}

fn geom64(o: &OutputGeom) -> Rect64 {
    (o.left as i64, o.top as i64, o.right as i64, o.bottom as i64)
}

/// Splits the region `(x, y, w, h)` into the pieces each output holds.
///
/// In i64 throughout: a region is whatever a module asked for, and `x + w` of two large i32s
/// overflows. A region across two outputs is stitched from two pieces by the same code that
/// reads one — general from the start, and unverified on a real multi-monitor machine (M10).
pub(crate) fn plan(x: i32, y: i32, w: i32, h: i32, outs: &[OutputGeom]) -> Plan {
    let mut pieces = Vec::new();
    if w <= 0 || h <= 0 {
        return Plan::Pieces(pieces);
    }
    let region = region64(x, y, w, h);
    for (i, o) in outs.iter().enumerate() {
        if mirrors_an_earlier(outs, i) {
            continue;
        }
        let Some(r) = intersect(region, geom64(o)) else { continue };
        if o.rotated {
            return Plan::Rotated(i);
        }
        pieces.push(Piece {
            output: i,
            src_x: (r.0 - o.left as i64) as u32,
            src_y: (r.1 - o.top as i64) as u32,
            w: (r.2 - r.0) as u32,
            h: (r.3 - r.1) as u32,
            dst_x: (r.0 - region.0) as u32,
            dst_y: (r.1 - region.1) as u32,
        });
    }
    Plan::Pieces(pieces)
}

/// Whether part of the region lies on a monitor Windows reports that no output DXGI reports
/// covers — a monitor plugged in since the outputs were listed, or one duplication cannot see.
///
/// Asked against the MONITOR list rather than the virtual screen's bounding box, which is what
/// the design first said: in an L-shaped arrangement of two monitors of different sizes the
/// bounding box has corners no monitor covers, and a region there is simply off the desktop —
/// GDI reads it as (0,0,0,0) too. Only a gap on a real monitor means the list is stale; read
/// as zeros it would be exactly the silent black this check exists to prevent.
pub(crate) fn uncovered_on_a_monitor(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    outs: &[OutputGeom],
    monitors: &[(i32, i32, i32, i32)],
) -> bool {
    if w <= 0 || h <= 0 {
        return false;
    }
    let region = region64(x, y, w, h);
    monitors.iter().any(|&(l, t, r, b)| {
        let Some(on_monitor) = intersect(region, (l as i64, t as i64, r as i64, b as i64)) else {
            return false;
        };
        let covered: i64 = (0..outs.len())
            .filter(|&i| !mirrors_an_earlier(outs, i))
            .filter_map(|i| intersect(on_monitor, geom64(&outs[i])))
            .map(area)
            .sum();
        covered < area(on_monitor)
    })
}

/// Whether the outputs DXGI listed are the monitors Windows reports, as sets of rectangles.
/// When they are not, the list is re-read — once per arrangement, so a display duplication
/// genuinely cannot see does not make every read enumerate the adapters again.
pub(crate) fn same_layout(outs: &[OutputGeom], monitors: &[(i32, i32, i32, i32)]) -> bool {
    let mut a: Vec<(i32, i32, i32, i32)> = (0..outs.len())
        .filter(|&i| !mirrors_an_earlier(outs, i))
        .map(|i| (outs[i].left, outs[i].top, outs[i].right, outs[i].bottom))
        .collect();
    let mut b = monitors.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    b.dedup();
    a == b
}

/// Copies one piece of a BGRA surface into an RGBA image, row by row.
///
/// `src` starts at the piece's first pixel and advances `pitch` bytes per row — the mapped
/// row pitch, which is routinely wider than `w * 4`. The piece lands at `(dx, dy)` of an image
/// `img_w` pixels wide. Alpha is forced to 255: the compositor promises nothing about what it
/// leaves there, and on-screen GDI pixels were measured at 255, so both paths hand a module the
/// same bytes for the same picture.
pub(crate) fn blit_bgra_to_rgba(
    src: &[u8],
    pitch: usize,
    w: usize,
    h: usize,
    dst: &mut [u8],
    img_w: usize,
    dx: usize,
    dy: usize,
) {
    for row in 0..h {
        let s = &src[row * pitch..row * pitch + w * 4];
        let o = ((dy + row) * img_w + dx) * 4;
        let d = &mut dst[o..o + w * 4];
        for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
            dp[0] = sp[2];
            dp[1] = sp[1];
            dp[2] = sp[0];
            dp[3] = 255;
        }
    }
}

/// Whether a region is small enough to read this way. See [`MAX_PIXELS`].
pub(crate) fn fits(w: i32, h: i32) -> bool {
    w > 0 && h > 0 && (w as i64) * (h as i64) <= MAX_PIXELS
}

// ---- What the caller hears when duplication cannot answer -----------------------------------

/// Why a duplication read did not come back with a picture. The caller decides what that
/// means: read the standard way, or — for a module that declared `fallback = "none"` — fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fallback {
    /// The Application settings switch is off.
    SwitchedOff,
    /// Three reads hung; stopped until the switch is turned off and on again.
    Disabled,
    /// The capture thread stopped on an internal error (a panic); the same way out.
    Crashed,
    /// The capture thread could not be started.
    NoThread,
    /// The application is shutting down.
    Closing,
    /// Opening, and another pump read of this opening already waited.
    Opening,
    /// An earlier read has not come back; nothing is sent until it does.
    Overdue,
    /// The pump already spent its share of the last 300 ms waiting for duplication.
    Budget,
    TooLarge,
    /// `DXGI_ERROR_UNSUPPORTED` or `SESSION_DISCONNECTED`: Remote Desktop, or a two-GPU laptop
    /// running this application on the chip that does not drive the display.
    Unsupported(i32),
    /// `DXGI_ERROR_NOT_CURRENTLY_AVAILABLE`: four programs already duplicate this session.
    TooManyRecorders,
    /// `E_ACCESSDENIED`: the secure desktop (UAC, the lock screen).
    SecureDesktop,
    /// `DXGI_ERROR_ACCESS_LOST`: a mode change, a fullscreen switch, a desktop switch.
    AccessLost,
    /// The driver was reset or updated.
    DeviceRemoved,
    Rotated,
    /// The region touches a monitor no duplicated output covers.
    Topology,
    /// Opened, but no real picture arrived within [`FIRST_FRAME_WAIT`].
    NoFrameYet,
    NoOutput,
    /// The surface is not 8-bit BGRA.
    Format(i32),
    Failed(i32),
}

impl Fallback {
    /// The reason in words, for the log and for what a module under `fallback = "none"` is
    /// told: the tail of every read's reason, after "screen capture failed: desktop duplication
    /// could not answer — " (`windows.rs`, `unanswered`). Part of the API: docs/api/screen.md
    /// lists each one word for word under "Failure reasons", and a test in `windows.rs`
    /// (`every_reason_a_module_is_told_is_listed`) holds the two together: change the wording
    /// in both or not at all.
    pub(crate) fn describe(self) -> String {
        match self {
            Fallback::SwitchedOff => "it is switched off in Application settings".into(),
            Fallback::Disabled => "it stopped after three reads hung; switching it off and on \
                                   again in Application settings tries again"
                .into(),
            Fallback::Crashed => "its thread stopped on an internal error; switching it off and \
                                  on again in Application settings starts it afresh"
                .into(),
            Fallback::NoThread => "its thread could not be started".into(),
            Fallback::Closing => "the application is closing".into(),
            Fallback::Opening => "it is still opening".into(),
            Fallback::Overdue => "an earlier read has not come back yet".into(),
            Fallback::Budget => "this turn of the event loop already waited its share for it".into(),
            Fallback::TooLarge => "the region is larger than 40 million pixels".into(),
            Fallback::Unsupported(hr) => format!(
                "Windows does not support it here (0x{hr:08X}): over Remote Desktop, or on a laptop \
                 with two graphics chips where this application runs on the one that does not drive \
                 the display — Windows Settings, System, Display, Graphics, this application, \
                 Power saving"
            ),
            Fallback::TooManyRecorders => "four other programs already record the screen — a \
                                           screen recorder, a screen-sharing call, another reader"
                .into(),
            Fallback::SecureDesktop => "the secure desktop is showing (a UAC prompt or the lock screen)".into(),
            Fallback::AccessLost => "the picture was lost to a display mode change, a fullscreen \
                                     switch or a desktop switch"
                .into(),
            Fallback::DeviceRemoved => "the graphics driver was reset or updated".into(),
            Fallback::Rotated => "the region touches a rotated monitor, which this version reads \
                                  only the standard way"
                .into(),
            Fallback::Topology => "the region touches a monitor desktop duplication does not report".into(),
            Fallback::NoFrameYet => "no picture has arrived since it was opened".into(),
            Fallback::NoOutput => "no display output was found".into(),
            Fallback::Format(f) => format!("the display delivers pixel format {f}, which this version cannot read"),
            Fallback::Failed(hr) => format!("it failed with 0x{hr:08X}"),
        }
    }

    /// Reasons that are part of normal operation rather than news: a read that did not wait
    /// because another already did. They are counted and traced, not logged each time.
    fn is_quiet(self) -> bool {
        matches!(self, Fallback::Opening | Fallback::Budget | Fallback::Overdue)
    }
}

/// How long to leave duplication alone after a failure, or `None` to try again on the next
/// read. `streak` counts the lost-access failures in a row, so a fullscreen toggle costs a
/// quarter of a second and a lock screen that stays up is asked about every two.
pub(crate) fn backoff_for(why: Fallback, streak: u32) -> Option<Duration> {
    match why {
        Fallback::AccessLost | Fallback::SecureDesktop => {
            Some(Duration::from_millis(250u64 << streak.min(3)))
        }
        Fallback::TooManyRecorders | Fallback::Unsupported(_) | Fallback::Format(_) => {
            Some(Duration::from_secs(5))
        }
        Fallback::Failed(_) => Some(Duration::from_secs(1)),
        // Kept open, and asked again soon: the next read may bring the first picture. Short,
        // because a static screen can go on bringing none, and every such read waits.
        Fallback::NoFrameYet => Some(Duration::from_millis(250)),
        _ => None,
    }
}

/// Who is asking, which decides how long they may wait.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Caller {
    /// The event loop, which every captured key and hotkey waits for (the keyboard hook itself
    /// has a thread of its own).
    Pump,
    /// The image worker.
    Worker,
}

/// How much of the sliding budget window is left, given the waits already in it. Pure over
/// the ring so it can be tested; drops what has aged out.
pub(crate) fn budget_left(ring: &mut VecDeque<(Instant, Duration)>, now: Instant) -> Duration {
    while ring.front().is_some_and(|(at, _)| now.duration_since(*at) >= PUMP_BUDGET_WINDOW) {
        ring.pop_front();
    }
    let spent: Duration = ring.iter().map(|(_, d)| *d).sum();
    PUMP_BUDGET.saturating_sub(spent)
}

/// Records one pump wait in the budget ring, charged by [`charge`]. A wait that costs nothing
/// is not recorded at all, so a module reading quickly and often does not grow the ring.
pub(crate) fn record_wait(ring: &mut VecDeque<(Instant, Duration)>, at: Instant, spent: Duration, answered: bool) {
    let charged = charge(spent, answered);
    if !charged.is_zero() {
        ring.push_back((at, charged));
    }
}

/// What a pump wait costs the budget: the part beyond an ordinary answer, or all of it when
/// nothing came back in time. See [`PUMP_QUICK`].
pub(crate) fn charge(spent: Duration, answered: bool) -> Duration {
    if answered {
        spent.saturating_sub(PUMP_QUICK)
    } else {
        spent
    }
}

// ---- Shared state between the callers and one engine thread --------------------------------

const IDLE: u8 = 0;
const OPENING: u8 = 1;
const READY: u8 = 2;
const BACKOFF: u8 = 3;
/// Three reads hung. Nothing moves the state out of here: turning the switch off and on again
/// replaces the engine, and its gate with it.
const DISABLED: u8 = 4;
/// The engine thread panicked, or its inbox closed. Left the same way as DISABLED.
const CRASHED: u8 = 5;

/// [`Gate::open_waiter`] while a prewarm's opening is queued or running. No pump read waits
/// for it: it takes longer than any pump wait (about 200 ms warm, seconds cold).
const WARMING: u64 = u64::MAX;

/// What the callers and ONE engine thread share: the engine's state, and the caller rules
/// that read it.
///
/// One per engine thread rather than process-wide statics, for two reasons. A thread that is
/// abandoned — stuck inside a graphics driver when the switch is turned off and on again —
/// goes on writing to its own gate when it comes back, which nobody reads any more, instead of
/// the state of the engine that replaced it. And the rules can be tested: every method that
/// depends on the time takes it as a number instead of reading a clock.
pub(crate) struct Gate {
    state: AtomicU8,
    backoff_until_ms: AtomicU64,
    backoff_reason: Mutex<Option<Fallback>>,
    /// Set by a caller whose read missed its deadline; cleared when the engine is back.
    overdue: AtomicBool,
    /// True while the engine is creating a device or opening a duplication, and since when.
    /// Slow there is not a hang until [`OPEN_HANG`].
    opening: AtomicBool,
    opening_since_ms: AtomicU64,
    /// Who holds the one pump wait an opening allows: the seq of the pump read waiting for
    /// it, [`WARMING`], or 0. Given back by the engine when THAT request ends — answered,
    /// failed, or skipped because its caller had stopped waiting — so it cannot outlive the
    /// request that took it.
    open_waiter: AtomicU64,
    hangs: AtomicU32,
    /// The request the engine is working on (0 = none), and since when. The hang clock
    /// starts again after an opening.
    busy_seq: AtomicU64,
    busy_since_ms: AtomicU64,
    hang_counted: AtomicU64,
}

impl Gate {
    pub(crate) const fn new() -> Self {
        Gate {
            state: AtomicU8::new(IDLE),
            backoff_until_ms: AtomicU64::new(0),
            backoff_reason: Mutex::new(None),
            overdue: AtomicBool::new(false),
            opening: AtomicBool::new(false),
            opening_since_ms: AtomicU64::new(0),
            open_waiter: AtomicU64::new(0),
            hangs: AtomicU32::new(0),
            busy_seq: AtomicU64::new(0),
            busy_since_ms: AtomicU64::new(0),
            hang_counted: AtomicU64::new(0),
        }
    }

    fn state(&self) -> u8 {
        self.state.load(Ordering::SeqCst)
    }

    /// Moves the state, except out of DISABLED or CRASHED: a request finishing late must not
    /// undo either.
    fn set_state(&self, s: u8) {
        let _ = self.state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
            (cur != DISABLED && cur != CRASHED).then_some(s)
        });
    }

    /// Why nothing is sent any more, if that is so.
    fn stopped(&self) -> Option<Fallback> {
        match self.state() {
            DISABLED => Some(Fallback::Disabled),
            CRASHED => Some(Fallback::Crashed),
            _ => None,
        }
    }

    fn back_off(&self, why: Fallback, for_: Duration, now: u64) {
        if let Ok(mut r) = self.backoff_reason.lock() {
            *r = Some(why);
        }
        self.backoff_until_ms.store(now + for_.as_millis() as u64, Ordering::SeqCst);
        self.set_state(BACKOFF);
    }

    fn in_backoff(&self, now: u64) -> Option<Fallback> {
        if now >= self.backoff_until_ms.load(Ordering::SeqCst) {
            return None;
        }
        Some(self.backoff_reason.lock().ok().and_then(|r| *r).unwrap_or(Fallback::AccessLost))
    }

    /// The caller rules, checked before anything is sent: a session that gave up, a back-off,
    /// a read that has not come back, the pump's budget, and the one pump wait an opening
    /// allows. `Ok` is how long to wait for the answer, and whether the budget rather than the
    /// rule for this state set that wait.
    pub(crate) fn admit(
        &self,
        caller: Caller,
        now: u64,
        budget_left: Duration,
        seq: u64,
    ) -> Result<(Duration, bool), Fallback> {
        if let Some(why) = self.stopped() {
            return Err(why);
        }
        if let Some(why) = self.in_backoff(now) {
            return Err(why);
        }
        if self.overdue.load(Ordering::SeqCst) {
            let busy = self.busy_seq.load(Ordering::SeqCst);
            if busy == 0 {
                // The late read came back between the engine clearing the flag and the caller
                // that missed it setting it; the engine is idle, so the episode is over.
                self.overdue.store(false, Ordering::SeqCst);
            } else {
                self.check_hang(busy, now);
                return Err(self.stopped().unwrap_or(Fallback::Overdue));
            }
        }
        let Caller::Pump = caller else { return Ok((WORKER_WAIT, false)) };
        if budget_left < PUMP_MIN_WAIT {
            return Err(Fallback::Budget);
        }
        let base = if self.state() == READY {
            PUMP_READY_WAIT
        } else if self.open_waiter.compare_exchange(0, seq, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return Err(Fallback::Opening);
        } else {
            PUMP_OPENING_WAIT
        };
        let wait = base.min(budget_left);
        Ok((wait, wait < base))
    }

    /// A caller's wait ran out. Only a wait that ran its full length says the engine is late
    /// and stops the reads after it; one the budget cut short says nothing about the engine.
    pub(crate) fn missed(&self, caller: Caller, wait: Duration, cut_short: bool) -> Fallback {
        let opening = self.opening.load(Ordering::SeqCst) || self.state() == OPENING;
        if cut_short {
            return if opening { Fallback::Opening } else { Fallback::Budget };
        }
        if !self.overdue.swap(true, Ordering::SeqCst) {
            logging::line(
                "capture",
                &format!(
                    "desktop duplication did not answer a {} read within {} ms{}; reads skip it until it does",
                    if caller == Caller::Pump { "pump" } else { "worker" },
                    wait.as_millis(),
                    if opening {
                        " — it is still opening, and the first opening in a process loads the graphics driver"
                    } else {
                        ""
                    }
                ),
            );
        }
        Fallback::Overdue
    }

    /// Takes the opening for a prewarm. False when there is nothing to warm or an opening is
    /// already on its way; then no Warm is sent, so a burst of window switches sends one.
    pub(crate) fn claim_warm(&self, now: u64) -> bool {
        if self.state() == READY
            || self.stopped().is_some()
            || self.in_backoff(now).is_some()
            || self.overdue.load(Ordering::SeqCst)
        {
            return false;
        }
        self.open_waiter.compare_exchange(0, WARMING, Ordering::SeqCst, Ordering::SeqCst).is_ok()
    }

    /// Gives back the opening wait `holder` took, if it still holds it.
    pub(crate) fn release_waiter(&self, holder: u64) {
        let _ = self.open_waiter.compare_exchange(holder, 0, Ordering::SeqCst, Ordering::SeqCst);
    }

    // -- The engine's side. --

    fn begin(&self, seq: u64, now: u64) {
        self.busy_since_ms.store(now, Ordering::SeqCst);
        self.busy_seq.store(seq, Ordering::SeqCst);
    }

    fn opening_started(&self, now: u64) {
        self.opening_since_ms.store(now, Ordering::SeqCst);
        self.opening.store(true, Ordering::SeqCst);
    }

    /// The hang clock starts again after the opening, on both sides of the channel.
    fn opening_ended(&self, now: u64) {
        self.busy_since_ms.store(now, Ordering::SeqCst);
        self.opening.store(false, Ordering::SeqCst);
    }

    /// Request `seq` is done: counted as a hang if it was one, its opening wait given back,
    /// and a caller that gave up on it told that the engine is back.
    fn end(&self, seq: u64, took: Duration, opening_spent: Duration) {
        if opening_spent >= OPEN_HANG {
            self.count_hang(seq, "opening", OPEN_HANG);
        } else if took.saturating_sub(opening_spent) >= HANG {
            self.count_hang(seq, "read", HANG);
        }
        // Before busy_seq goes back to zero: a caller that sees an idle engine with the flag
        // still set clears it itself, and would swallow this line.
        if self.overdue.swap(false, Ordering::SeqCst) {
            logging::line(
                "capture",
                &format!(
                    "desktop duplication answered again, {} ms after it was asked; reads use it again",
                    took.as_millis()
                ),
            );
        }
        self.release_waiter(seq);
        self.busy_seq.store(0, Ordering::SeqCst);
    }

    /// Request `seq` was dropped unread, its caller having stopped waiting.
    fn skipped(&self, seq: u64) {
        self.release_waiter(seq);
    }

    /// Counts request `busy` as a hang when it has been outstanding too long: [`HANG`] for a
    /// read, [`OPEN_HANG`] while it is opening.
    fn check_hang(&self, busy: u64, now: u64) {
        if self.opening.load(Ordering::SeqCst) {
            if now.saturating_sub(self.opening_since_ms.load(Ordering::SeqCst)) >= OPEN_HANG.as_millis() as u64 {
                self.count_hang(busy, "opening", OPEN_HANG);
            }
        } else if now.saturating_sub(self.busy_since_ms.load(Ordering::SeqCst)) >= HANG.as_millis() as u64 {
            self.count_hang(busy, "read", HANG);
        }
    }

    /// Counts a hang, once per request; the third stops duplication.
    fn count_hang(&self, seq: u64, what: &str, limit: Duration) {
        if seq == 0 || self.hang_counted.swap(seq, Ordering::SeqCst) == seq {
            return;
        }
        let n = self.hangs.fetch_add(1, Ordering::SeqCst) + 1;
        logging::line(
            "capture",
            &format!(
                "a desktop duplication {what} has taken more than {} s ({n} of {HANGS_TO_DISABLE})",
                limit.as_secs()
            ),
        );
        if n >= HANGS_TO_DISABLE {
            let _ = self.state.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |cur| {
                (cur != CRASHED).then_some(DISABLED)
            });
            logging::line(
                "capture",
                "desktop duplication stopped answering, so it is not used again this session: modules \
                 that allow it read the screen the standard way, and modules that declared fallback = \
                 \"none\" get no picture. Turning \"read the screen through the graphics card\" off and \
                 on again in Application settings tries it afresh.",
            );
        }
    }

    /// The engine thread is gone: it panicked, or its inbox closed. Nothing is sent to it
    /// again; turning the switch off and on again starts a new one.
    fn crashed(&self, why: &str) {
        let was = self.state.swap(CRASHED, Ordering::SeqCst);
        self.overdue.store(false, Ordering::SeqCst);
        self.open_waiter.store(0, Ordering::SeqCst);
        self.busy_seq.store(0, Ordering::SeqCst);
        if was != CRASHED {
            logging::line(
                "capture",
                &format!(
                    "desktop duplication's thread stopped ({why}), so it is not used again this \
                     session: modules that allow it read the screen the standard way, and modules \
                     that declared fallback = \"none\" get no picture. Turning \"read the screen \
                     through the graphics card\" off and on again in Application settings starts \
                     it afresh."
                ),
            );
        }
    }
}

static SEQ: AtomicU64 = AtomicU64::new(0);
static SHUT_DOWN: AtomicBool = AtomicBool::new(false);
/// Counters for the observation log line, read and reset by `take_counters`. Atomics rather
/// than fields of lib.rs's `Observations`, which is main-thread-only: the worker reads too.
static DUP_CAPTURES: AtomicU64 = AtomicU64::new(0);
static DUP_US: AtomicU64 = AtomicU64::new(0);
static DUP_FALLBACKS: AtomicU64 = AtomicU64::new(0);
/// The reason last logged by `note_fallback`, and which kinds of module met it (bit 1: one
/// that reads the standard way instead, bit 2: one that declared `fallback = "none"`).
static LAST_NOTED: Mutex<Option<(Fallback, u8)>> = Mutex::new(None);

/// The running engine: its inbox, its thread, its gate, and the generation of the settings
/// switch it was started under.
struct Running {
    tx: Sender<Req>,
    join: Option<std::thread::JoinHandle<()>>,
    gate: Arc<Gate>,
    generation: u32,
}
static ENGINE: Mutex<Option<Running>> = Mutex::new(None);

enum Req {
    Capture {
        seq: u64,
        rects: Vec<(i32, i32, i32, i32)>,
        deadline: Instant,
        reply: SyncSender<Result<Vec<CapturedImage>, Fallback>>,
    },
    /// Open the primary output's duplication and take a first picture; nobody waits for it.
    Warm,
    /// Reserved for the dirty-rectangle watch — "tell me when this rectangle is repainted" —
    /// that a read right after input needs (a fast read is not a fresh read: game plus
    /// compositor latency is one to three frames). Nothing sends it yet; the engine answers it
    /// by dropping the reply, which the asker hears as "no". Design step 9.
    #[allow(dead_code)]
    WaitChange {
        rect: (i32, i32, i32, i32),
        timeout: Duration,
        reply: SyncSender<bool>,
    },
    Shutdown {
        done: SyncSender<()>,
    },
}

fn clock() -> Instant {
    static BASE: OnceLock<Instant> = OnceLock::new();
    *BASE.get_or_init(Instant::now)
}

fn now_ms() -> u64 {
    clock().elapsed().as_millis() as u64
}

/// Never 0 (no request) and, in any session that ends, never [`WARMING`].
fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::SeqCst) + 1
}

/// The current engine's inbox and gate, starting its thread on first use — and replacing it
/// when the settings switch has been turned off and on again since it started.
///
/// The replacement is how "off and on again" gets another try whatever the old engine is
/// doing: stopped after three hangs, crashed, or still inside a graphics driver. The old
/// thread is not joined, since it may never come back; its inbox closes with the handle
/// dropped here, so it stops as soon as it is free, releasing what it held, and whatever it
/// writes on the way goes to its own gate, which nobody reads any more.
fn engine() -> Result<(Sender<Req>, Arc<Gate>), Fallback> {
    let generation = crate::appcfg::desktop_duplication_generation();
    let mut g = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    // Asked under the lock `shutdown` takes the handle under, so no thread starts after it.
    if SHUT_DOWN.load(Ordering::SeqCst) {
        return Err(Fallback::Closing);
    }
    if g.as_ref().is_some_and(|r| r.generation != generation) {
        if let Some(old) = g.take() {
            retire(old);
        }
    }
    if g.is_none() {
        let gate = Arc::new(Gate::new());
        let (tx, rx) = mpsc::channel();
        let theirs = gate.clone();
        let join = std::thread::Builder::new()
            .name("dxgi-capture".into())
            .spawn(move || run_guarded(rx, theirs))
            .map_err(|_| Fallback::NoThread)?;
        *g = Some(Running { tx, join: Some(join), gate, generation });
    }
    let r = g.as_ref().ok_or(Fallback::NoThread)?;
    Ok((r.tx.clone(), r.gate.clone()))
}

/// Lets go of an engine the switch has been turned off and on since. Says so when the person
/// turning it had a reason to: it had stopped, or was still stuck.
fn retire(old: Running) {
    let stuck = old.gate.busy_seq.load(Ordering::SeqCst) != 0;
    if old.gate.stopped().is_some() || old.gate.hangs.load(Ordering::SeqCst) > 0 || stuck {
        logging::line(
            "capture",
            &format!(
                "desktop duplication was switched off and on again, so it is tried afresh on a new thread{}",
                if stuck {
                    "; the old one is still inside the graphics driver and is left to finish on its own"
                } else {
                    ""
                }
            ),
        );
    }
    // Dropping `old` closes the thread's inbox and detaches its JoinHandle.
}

thread_local! {
    /// The pump's recent waits for this thread: (when, charge). Thread-local because only the
    /// pump has a budget, and it is one thread.
    static WAITED: RefCell<VecDeque<(Instant, Duration)>> = const { RefCell::new(VecDeque::new()) };
}

/// Reads `rects` through desktop duplication, one image per rectangle, each exactly the size
/// asked for — or the reason it could not.
///
/// The caller rules ([`Gate::admit`]) are checked on the caller's thread before anything is
/// sent. Only then is the engine asked, with a deadline; a pump read waits once per opening
/// and within its budget, and never for a thread that is already late.
pub(crate) fn capture(rects: &[(i32, i32, i32, i32)], caller: Caller) -> Result<Vec<CapturedImage>, Fallback> {
    if !crate::appcfg::desktop_duplication() {
        return Err(Fallback::SwitchedOff);
    }
    if rects.iter().any(|&(_, _, w, h)| !fits(w, h)) {
        return Err(Fallback::TooLarge);
    }
    let (tx, gate) = engine()?;
    let asked = Instant::now();
    let left = match caller {
        Caller::Pump => WAITED.with(|w| budget_left(&mut w.borrow_mut(), asked)),
        Caller::Worker => Duration::MAX,
    };
    let seq = next_seq();
    let (wait, cut_short) = gate.admit(caller, now_ms(), left, seq)?;
    let (reply, answer) = mpsc::sync_channel(1);
    let req = Req::Capture { seq, rects: rects.to_vec(), deadline: asked + wait, reply };
    if tx.send(req).is_err() {
        gate.release_waiter(seq);
        gate.crashed("its inbox is closed");
        return Err(Fallback::Crashed);
    }
    let got = answer.recv_timeout(wait);
    let spent = asked.elapsed();
    if caller == Caller::Pump {
        WAITED.with(|w| record_wait(&mut w.borrow_mut(), asked, spent, got.is_ok()));
    }
    // Every round trip, answered or not: what duplication cost the reads that asked it.
    DUP_US.fetch_add(spent.as_micros() as u64, Ordering::Relaxed);
    match got {
        Ok(Ok(images)) => {
            DUP_CAPTURES.fetch_add(1, Ordering::Relaxed);
            note_success();
            Ok(images)
        }
        Ok(Err(why)) => Err(why),
        // A dropped reply is the engine discarding a request whose deadline passed while it
        // queued behind a slow one — late all the same — or a thread that died serving it.
        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
            Err(gate.stopped().unwrap_or_else(|| gate.missed(caller, wait, cut_short)))
        }
    }
}

/// Opens the primary output's duplication in the background — asked when a module that reads
/// through it has just come to the front, before its triggers run, so their first read does
/// not also pay for the opening. Nothing is sent when there is nothing to warm (switched off,
/// stopped, backing off, late, already open) or a warm-up is already on its way.
pub(crate) fn prewarm() {
    if !crate::appcfg::desktop_duplication() {
        return;
    }
    let Ok((tx, gate)) = engine() else { return };
    if !gate.claim_warm(now_ms()) {
        return;
    }
    if tx.send(Req::Warm).is_err() {
        gate.release_waiter(WARMING);
        gate.crashed("its inbox is closed");
    }
}

/// Whether a fallback is news for the log: a reason not logged since the last success, or one
/// already logged but now also met by a module with the other `fallback` — so at most two
/// lines per reason, however the reads of two such modules interleave.
fn newly_noted(last: &mut Option<(Fallback, u8)>, why: Fallback, or_standard: bool) -> bool {
    let kind = if or_standard { 1 } else { 2 };
    match last {
        Some((seen, kinds)) if *seen == why => {
            let fresh = *kinds & kind == 0;
            *kinds |= kind;
            fresh
        }
        _ => {
            *last = Some((why, kind));
            true
        }
    }
}

/// Records a read that duplication could not answer, and what became of it. Logged when the
/// reason is news — a fullscreen toggle would otherwise write a line per read — and traced
/// every time. The routine reasons (a read that did not wait because another already did)
/// are only counted.
pub(crate) fn note_fallback(why: Fallback, or_standard: bool) {
    DUP_FALLBACKS.fetch_add(1, Ordering::Relaxed);
    let what = if or_standard {
        "read the standard way instead"
    } else {
        "the read fails, as the module's fallback = \"none\" asks"
    };
    if why.is_quiet() {
        logging::trace("capture", || format!("desktop duplication skipped: {}; {what}", why.describe()));
        return;
    }
    let fresh = match LAST_NOTED.lock() {
        Ok(mut last) => newly_noted(&mut last, why, or_standard),
        Err(_) => true,
    };
    if fresh {
        logging::line(
            "capture",
            &format!("desktop duplication could not answer: {}; {what}", why.describe()),
        );
    } else {
        logging::trace("capture", || format!("desktop duplication could not answer: {}; {what}", why.describe()));
    }
}

/// A read answered: the next failure is news again, and the recovery is worth a line.
fn note_success() {
    let was = LAST_NOTED.lock().ok().and_then(|mut l| l.take());
    if let Some((why, _)) = was {
        logging::line(
            "capture",
            &format!("desktop duplication answers again (it last could not because {})", why.describe()),
        );
    }
}

/// (reads answered, microseconds spent in every round trip, reads it could not answer) since
/// the last call, reset on reading.
pub(crate) fn take_counters() -> (u64, u64, u64) {
    (
        DUP_CAPTURES.swap(0, Ordering::Relaxed),
        DUP_US.swap(0, Ordering::Relaxed),
        DUP_FALLBACKS.swap(0, Ordering::Relaxed),
    )
}

/// Below this many pixels a read says too little to compare — a pixel read is one — and the
/// first-read comparison looks at the foreground window's client area instead: for a module
/// written for one application, the window it was written for.
const COMPARE_MIN_PIXELS: i64 = 64 * 64;

/// What the first-read comparison of a module looks at, given the region it read and the
/// foreground window's client area: the region when it is big enough to say something, else
/// the window (the `true`), else the region anyway. `None` when neither can be read this way.
pub(crate) fn comparison_region(
    read: (i32, i32, i32, i32),
    foreground: Option<(i32, i32, i32, i32)>,
) -> Option<((i32, i32, i32, i32), bool)> {
    let telling = |(_, _, w, h): (i32, i32, i32, i32)| fits(w, h) && (w as i64) * (h as i64) >= COMPARE_MIN_PIXELS;
    if telling(read) {
        return Some((read, false));
    }
    if let Some(f) = foreground.filter(|&f| telling(f)) {
        return Some((f, true));
    }
    fits(read.2, read.3).then_some((read, false))
}

/// How two captures of the same region differ: the share of pixels whose RGB differs, the
/// largest channel difference, and each image's mean luminance (BT.601). `None` when they
/// are not the same size. Alpha is not compared — it is a mask, not colour.
pub(crate) fn compare(a: &CapturedImage, b: &CapturedImage) -> Option<(f64, u8, f64, f64)> {
    let n = (a.w as usize) * (a.h as usize);
    if a.w != b.w || a.h != b.h || n == 0 || a.rgba.len() < n * 4 || b.rgba.len() < n * 4 {
        return None;
    }
    let (mut differ, mut worst) = (0usize, 0u8);
    let (mut la, mut lb) = (0f64, 0f64);
    let lum = |p: &[u8]| (p[0] as f64 * 299.0 + p[1] as f64 * 587.0 + p[2] as f64 * 114.0) / 1000.0;
    for (pa, pb) in a.rgba.chunks_exact(4).zip(b.rgba.chunks_exact(4)).take(n) {
        let d = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap_or(0);
        if d > 0 {
            differ += 1;
            worst = worst.max(d);
        }
        la += lum(pa);
        lb += lum(pb);
    }
    Some((differ as f64 * 100.0 / n as f64, worst, la / n as f64, lb / n as f64))
}

/// Writes a module's first-read comparison: what its author reads after declaring the key and
/// reloading, to learn whether the declaration changes anything for their application. `who`
/// is the module, `what` says what was compared.
pub(crate) fn log_comparison(who: &str, what: &str, dup: &CapturedImage, standard: Option<&CapturedImage>) {
    let head = format!("[{who}] first desktop duplication read compared with the standard way, over {what}");
    let Some(standard) = standard else {
        logging::line("capture", &format!("{head}: the standard way could not read it"));
        return;
    };
    match compare(standard, dup) {
        Some((pct, _, ls, _)) if pct == 0.0 => logging::line(
            "capture",
            &format!("{head}: the two pictures are the same (mean luminance {ls:.1})"),
        ),
        Some((pct, worst, ls, ld)) => logging::line(
            "capture",
            &format!(
                "{head}: the two pictures DIFFER — {pct:.1} % of pixels, by up to {worst} per channel; mean luminance standard {ls:.1}, duplication {ld:.1} (read a few milliseconds apart, so anything moving differs as well)"
            ),
        ),
        None => logging::line("capture", &format!("{head}: the standard way returned a different size")),
    }
}

/// Stops the engine thread if it ever started. See `backend::shutdown_capture`.
pub(crate) fn shutdown() {
    let handle = {
        let mut g = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
        SHUT_DOWN.store(true, Ordering::SeqCst);
        g.take()
    };
    let Some(mut h) = handle else { return };
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    if h.tx.send(Req::Shutdown { done: done_tx }).is_err() {
        return; // the thread is already gone
    }
    match done_rx.recv_timeout(SHUTDOWN_WAIT) {
        Ok(()) => {
            if let Some(j) = h.join.take() {
                let _ = j.join();
            }
        }
        Err(_) => logging::line(
            "capture",
            "desktop duplication did not stop within half a second; leaving it to the process exit",
        ),
    }
}

// ---- The engine: everything below runs on the dxgi-capture thread only ----------------------

struct Adapter {
    adapter: IDXGIAdapter1,
    name: String,
    dev: Option<(ID3D11Device, ID3D11DeviceContext)>,
}

struct Output {
    name: String,
    geom: OutputGeom,
    adapter: usize,
    out: IDXGIOutput1,
    dup: Option<Dup>,
}

struct Dup {
    dup: IDXGIOutputDuplication,
    /// The last real picture, output-sized, on the GPU. `None` until the first one arrives.
    last: Option<ID3D11Texture2D>,
    /// Output-sized, CPU-readable. Every piece of one request is copied into it at its own
    /// position, then it is mapped ONCE: one GPU sync per output per request, not per region.
    staging: Option<ID3D11Texture2D>,
    size: (u32, u32),
    opened: Instant,
    used: Instant,
    /// Whether the time to the first picture has been logged — M16's number, wanted once per
    /// opening, however long it took.
    pictured: bool,
}

struct Engine {
    /// This engine's half of what it shares with the callers.
    gate: Arc<Gate>,
    factory: Option<IDXGIFactory1>,
    adapters: Vec<Adapter>,
    outputs: Vec<Output>,
    /// The monitor arrangement the outputs were last listed for, so a display that
    /// duplication cannot see does not make every read list them again.
    listed_for: Option<Vec<(i32, i32, i32, i32)>>,
    /// Lost-access failures in a row, for the back-off.
    lost_streak: u32,
    /// Time the current request spent opening, which does not count towards a hang.
    opening_spent: Duration,
    /// The last reason the engine logged, so an episode is one line and not one per read.
    last_logged: Option<Fallback>,
    /// When the last request that did any work came in: the device's idle clock.
    last_request: Instant,
}

impl Engine {
    fn new(gate: Arc<Gate>) -> Self {
        Engine {
            gate,
            factory: None,
            adapters: Vec::new(),
            outputs: Vec::new(),
            listed_for: None,
            lost_streak: 0,
            opening_spent: Duration::ZERO,
            last_logged: None,
            last_request: Instant::now(),
        }
    }
}

/// The thread's body, with a panic caught: without that, a panic would leave the callers
/// waiting for a request that never ends, which reads as "still opening" or "late" forever
/// and says nothing in the log.
fn run_guarded(rx: Receiver<Req>, gate: Arc<Gate>) {
    let theirs = gate.clone();
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || run(rx, theirs))).is_err() {
        gate.crashed("an internal error on its thread");
    }
}

fn run(rx: Receiver<Req>, gate: Arc<Gate>) {
    let mut eng = Engine::new(gate.clone());
    loop {
        let req = if eng.holds_anything() {
            match rx.recv_timeout(WAKE) {
                Ok(r) => Some(r),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(r) => Some(r),
                Err(_) => break,
            }
        };
        if eng.holds_anything() {
            if !crate::appcfg::desktop_duplication() {
                eng.release_all("it was switched off in Application settings");
            } else if gate.stopped().is_some() {
                // Stopped after three hangs: nothing will read through it this session, so
                // the device, its driver memory and the duplication slots go now.
                eng.release_all("it stopped answering");
            }
        }
        eng.release_idle();
        let Some(req) = req else { continue };
        match req {
            Req::Shutdown { done } => {
                eng.release_all("the application is closing");
                let _ = done.send(());
                return;
            }
            Req::Warm => {
                if crate::appcfg::desktop_duplication() && gate.stopped().is_none() {
                    // Marked busy like a read, so a read queued behind a slow opening sees a
                    // busy engine and does not wait for it.
                    eng.last_request = Instant::now();
                    busy(&mut eng, next_seq(), |e| e.warm());
                }
                gate.release_waiter(WARMING);
            }
            Req::WaitChange { .. } => {}
            Req::Capture { seq, rects, deadline, reply } => {
                if Instant::now() >= deadline {
                    logging::trace("capture", || {
                        "desktop duplication: a read whose caller had already stopped waiting was skipped".into()
                    });
                    gate.skipped(seq);
                    continue;
                }
                eng.last_request = Instant::now();
                let res = busy(&mut eng, seq, |e| e.serve(&rects));
                let _ = reply.send(res);
            }
        }
    }
}

/// Runs one piece of engine work under the bookkeeping the callers read: which request is
/// being worked on and since when, whether it took long enough to be a hang, and whether a
/// caller gave up on it and should hear that the engine is back.
fn busy<T>(eng: &mut Engine, seq: u64, work: impl FnOnce(&mut Engine) -> T) -> T {
    let gate = eng.gate.clone();
    gate.begin(seq, now_ms());
    let started = Instant::now();
    eng.opening_spent = Duration::ZERO;
    let out = work(eng);
    gate.end(seq, started.elapsed(), eng.opening_spent);
    out
}

/// The hard-to-read HRESULT, sorted into what it means for the read.
fn classify(e: &windows::core::Error) -> Fallback {
    let hr = e.code();
    if hr == DXGI_ERROR_ACCESS_LOST {
        Fallback::AccessLost
    } else if hr == E_ACCESSDENIED {
        Fallback::SecureDesktop
    } else if hr == DXGI_ERROR_NOT_CURRENTLY_AVAILABLE {
        Fallback::TooManyRecorders
    } else if hr == DXGI_ERROR_UNSUPPORTED || hr == DXGI_ERROR_SESSION_DISCONNECTED {
        Fallback::Unsupported(hr.0)
    } else if hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET {
        Fallback::DeviceRemoved
    } else {
        Fallback::Failed(hr.0)
    }
}

fn wide(s: &[u16]) -> String {
    let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// The monitors Windows reports, as (left, top, right, bottom).
fn gdi_monitors() -> Vec<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    unsafe extern "system" fn each(_: HMONITOR, _: HDC, r: *mut RECT, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the Vec passed below, alive for the whole enumeration, and `r` is
        // the monitor rectangle the system hands every callback.
        let v = unsafe { &mut *(data as *mut Vec<(i32, i32, i32, i32)>) };
        if let Some(r) = unsafe { r.as_ref() } {
            v.push((r.left, r.top, r.right, r.bottom));
        }
        1
    }
    let mut v: Vec<(i32, i32, i32, i32)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(each),
            &mut v as *mut Vec<(i32, i32, i32, i32)> as LPARAM,
        );
    }
    v
}

impl Engine {
    fn holds_anything(&self) -> bool {
        self.adapters.iter().any(|a| a.dev.is_some()) || self.outputs.iter().any(|o| o.dup.is_some())
    }

    fn geoms(&self) -> Vec<OutputGeom> {
        self.outputs.iter().map(|o| o.geom).collect()
    }

    fn release_all(&mut self, why: &str) {
        if self.holds_anything() {
            logging::line("capture", &format!("desktop duplication released everything it held: {why}"));
        }
        for o in &mut self.outputs {
            o.dup = None;
        }
        for a in &mut self.adapters {
            a.dev = None;
        }
        self.gate.set_state(IDLE);
    }

    /// Frees each duplication nobody has read through for [`IDLE_RELEASE`]: a slot of the four
    /// a session has, and two screen-sized textures. The device stays longer — it is what makes
    /// the next opening cheap (M11 measures by how much) — and goes after
    /// [`DEVICE_IDLE_RELEASE`], after which the thread holds nothing and sleeps until asked.
    fn release_idle(&mut self) {
        let mut released = false;
        for o in &mut self.outputs {
            if o.dup.as_ref().is_some_and(|d| d.used.elapsed() >= IDLE_RELEASE) {
                logging::line(
                    "capture",
                    &format!(
                        "desktop duplication of {} released after {} s without a read",
                        o.name,
                        IDLE_RELEASE.as_secs()
                    ),
                );
                o.dup = None;
                released = true;
            }
        }
        let any_open = self.outputs.iter().any(|o| o.dup.is_some());
        if released && !any_open {
            self.gate.set_state(IDLE);
        }
        if !any_open
            && self.adapters.iter().any(|a| a.dev.is_some())
            && self.last_request.elapsed() >= DEVICE_IDLE_RELEASE
        {
            for a in &mut self.adapters {
                a.dev = None;
            }
            logging::line(
                "capture",
                &format!(
                    "desktop duplication released its Direct3D device after {} minutes without a read",
                    DEVICE_IDLE_RELEASE.as_secs() / 60
                ),
            );
        }
    }

    /// Lists adapters and outputs from a new factory. Everything opened from the old list is
    /// dropped with it: an output of one factory must not be paired with a device of another.
    fn enumerate(&mut self) -> Result<(), Fallback> {
        self.outputs.clear();
        self.adapters.clear();
        self.factory = None;
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(|e| classify(&e))?;
        let mut described: Vec<String> = Vec::new();
        let mut i = 0u32;
        loop {
            let adapter = match unsafe { factory.EnumAdapters1(i) } {
                Ok(a) => a,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(classify(&e)),
            };
            i += 1;
            let name = unsafe { adapter.GetDesc1() }
                .map(|d| wide(&d.Description))
                .unwrap_or_else(|_| "an unnamed adapter".into());
            let ai = self.adapters.len();
            let mut j = 0u32;
            loop {
                let out = match unsafe { adapter.EnumOutputs(j) } {
                    Ok(o) => o,
                    Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                    Err(e) => return Err(classify(&e)),
                };
                j += 1;
                let Ok(desc) = (unsafe { out.GetDesc() }) else { continue };
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                let Ok(out1) = out.cast::<IDXGIOutput1>() else { continue };
                let r = desc.DesktopCoordinates;
                let rotated = desc.Rotation != DXGI_MODE_ROTATION_IDENTITY
                    && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED;
                let oname = wide(&desc.DeviceName);
                // Colour depth and HDR, from the newer interface where the system has it: an
                // HDR desktop is converted to 8 bits for this path, and its colours may then
                // differ from what GDI reads (M7).
                let colour = out
                    .cast::<IDXGIOutput6>()
                    .ok()
                    .and_then(|o6| unsafe { o6.GetDesc1() }.ok())
                    .map(|d| {
                        format!(
                            ", {} bits{}",
                            d.BitsPerColor,
                            if d.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 { ", HDR" } else { ", SDR" }
                        )
                    })
                    .unwrap_or_default();
                described.push(format!(
                    "{oname} at {},{} {}x{} on {name}{}{colour}",
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    if rotated { ", ROTATED (read the standard way)" } else { "" }
                ));
                self.outputs.push(Output {
                    name: oname,
                    geom: OutputGeom { left: r.left, top: r.top, right: r.right, bottom: r.bottom, rotated },
                    adapter: ai,
                    out: out1,
                    dup: None,
                });
            }
            self.adapters.push(Adapter { adapter, name, dev: None });
        }
        logging::line(
            "capture",
            &format!(
                "desktop duplication sees {} output(s): {}",
                self.outputs.len(),
                if described.is_empty() { "none".to_string() } else { described.join("; ") }
            ),
        );
        self.factory = Some(factory);
        Ok(())
    }

    /// Makes sure the output list describes the desktop as it is now, and returns the monitor
    /// list it was checked against. Re-lists on a factory that says it is stale (an adapter
    /// or output changed) and on a monitor arrangement not seen before — a hot-plugged
    /// monitor does not invalidate the primary's duplication, so nothing else would notice.
    fn ensure_topology(&mut self) -> Result<Vec<(i32, i32, i32, i32)>, Fallback> {
        let monitors = gdi_monitors();
        let stale = match &self.factory {
            None => true,
            Some(f) => !unsafe { f.IsCurrent() }.as_bool(),
        };
        let rearranged = !monitors.is_empty()
            && !same_layout(&self.geoms(), &monitors)
            && self.listed_for.as_ref() != Some(&monitors);
        if stale || rearranged {
            let again = self.factory.is_some();
            self.enumerate()?;
            self.listed_for = Some(monitors.clone());
            if again {
                logging::line("capture", "the display arrangement changed, so desktop duplication listed the outputs again");
            }
            if !monitors.is_empty() && !same_layout(&self.geoms(), &monitors) {
                logging::line(
                    "capture",
                    &format!(
                        "Windows reports monitors {monitors:?}, and desktop duplication does not see all of them; a read touching one it cannot see is read the standard way, or fails under fallback = \"none\""
                    ),
                );
            }
        }
        if self.outputs.is_empty() {
            return Err(Fallback::NoOutput);
        }
        Ok(monitors)
    }

    fn device(&mut self, ai: usize) -> Result<(ID3D11Device, ID3D11DeviceContext), Fallback> {
        if let Some(d) = &self.adapters[ai].dev {
            return Ok(d.clone());
        }
        let t = Instant::now();
        let levels: [D3D_FEATURE_LEVEL; 3] = [D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0];
        let mut dev: Option<ID3D11Device> = None;
        let mut ctx: Option<ID3D11DeviceContext> = None;
        // On the adapter that owns the output — required: a device on another adapter gets
        // DXGI_ERROR_UNSUPPORTED, which is exactly the two-GPU laptop failure.
        unsafe {
            D3D11CreateDevice(
                &self.adapters[ai].adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            )
        }
        .map_err(|e| classify(&e))?;
        let (Some(dev), Some(ctx)) = (dev, ctx) else {
            return Err(Fallback::Failed(0));
        };
        logging::line(
            "capture",
            &format!(
                "desktop duplication created a Direct3D device on {} in {} ms",
                self.adapters[ai].name,
                t.elapsed().as_millis()
            ),
        );
        self.adapters[ai].dev = Some((dev.clone(), ctx.clone()));
        Ok((dev, ctx))
    }

    fn open(&mut self, oi: usize) -> Result<(), Fallback> {
        if self.outputs[oi].dup.is_some() {
            return Ok(());
        }
        self.gate.set_state(OPENING);
        let began = Instant::now();
        self.gate.opening_started(now_ms());
        let opened = self.open_duplication(oi);
        self.gate.opening_ended(now_ms());
        self.opening_spent += began.elapsed();
        opened
    }

    fn open_duplication(&mut self, oi: usize) -> Result<(), Fallback> {
        let (dev, _) = self.device(self.outputs[oi].adapter)?;
        let t = Instant::now();
        let dup = unsafe { self.outputs[oi].out.DuplicateOutput(&dev) }.map_err(|e| classify(&e))?;
        let desc = unsafe { dup.GetDesc() };
        logging::line(
            "capture",
            &format!(
                "desktop duplication of {} opened in {} ms ({}x{}, the picture in {} memory)",
                self.outputs[oi].name,
                t.elapsed().as_millis(),
                desc.ModeDesc.Width,
                desc.ModeDesc.Height,
                if desc.DesktopImageInSystemMemory.as_bool() { "system" } else { "video" }
            ),
        );
        let now = Instant::now();
        self.outputs[oi].dup = Some(Dup {
            dup,
            last: None,
            staging: None,
            size: (desc.ModeDesc.Width, desc.ModeDesc.Height),
            opened: now,
            used: now,
            pictured: false,
        });
        Ok(())
    }

    /// What one failure does to the engine: which objects go, how long to wait, what to log.
    fn fail(&mut self, oi: usize, why: Fallback) -> Fallback {
        match why {
            Fallback::DeviceRemoved => {
                let ai = self.outputs[oi].adapter;
                self.adapters[ai].dev = None;
                for o in self.outputs.iter_mut().filter(|o| o.adapter == ai) {
                    o.dup = None;
                }
            }
            // Kept: the next read may bring the first picture.
            Fallback::NoFrameYet => {}
            _ => self.outputs[oi].dup = None,
        }
        if why == Fallback::AccessLost {
            // A mode change can also have changed the outputs themselves; a stale factory is
            // re-listed at the next read.
            if self.factory.as_ref().is_some_and(|f| !unsafe { f.IsCurrent() }.as_bool()) {
                self.factory = None;
            }
        }
        if matches!(why, Fallback::AccessLost | Fallback::SecureDesktop) {
            self.lost_streak += 1;
        }
        let wait = backoff_for(why, self.lost_streak.saturating_sub(1));
        if let Some(d) = wait {
            self.gate.back_off(why, d, now_ms());
        } else if !self.outputs.iter().any(|o| o.dup.is_some()) {
            self.gate.set_state(IDLE);
        }
        if self.last_logged != Some(why) {
            self.last_logged = Some(why);
            logging::line(
                "capture",
                &format!(
                    "desktop duplication of {}: {}{}",
                    self.outputs[oi].name,
                    why.describe(),
                    match wait {
                        Some(d) => format!("; not asked again for {} ms", d.as_millis()),
                        None => String::new(),
                    }
                ),
            );
        }
        why
    }

    /// Brings output `oi`'s kept picture up to date: at most one acquire when a picture is
    /// already kept, and up to [`FIRST_FRAME_WAIT`] of them when none has arrived yet.
    ///
    /// A picture is taken only from a frame whose `LastPresentTime` is not zero. A frame that
    /// carries a pointer move and nothing else says, in so many words, that the desktop image
    /// was not updated; seeding the kept picture from one on a static screen would serve
    /// whatever that texture held for as long as the screen stays still (M16).
    fn refresh(&mut self, oi: usize) -> Result<(), Fallback> {
        let mut reopened = false;
        loop {
            if let Err(why) = self.open(oi) {
                return Err(self.fail(oi, why));
            }
            match self.acquire(oi) {
                Ok(()) => break,
                // Lost once: reopen at once, which is what a fullscreen toggle needs. Lost
                // again straight after is an episode, and backs off.
                Err(Fallback::AccessLost) if !reopened => {
                    reopened = true;
                    self.outputs[oi].dup = None;
                }
                Err(why) => return Err(self.fail(oi, why)),
            }
        }
        if let Some(d) = self.outputs[oi].dup.as_mut() {
            d.used = Instant::now();
        }
        Ok(())
    }

    fn acquire(&mut self, oi: usize) -> Result<(), Fallback> {
        let ai = self.outputs[oi].adapter;
        let (dev, ctx) = self.adapters[ai].dev.clone().ok_or(Fallback::DeviceRemoved)?;
        let name = self.outputs[oi].name.clone();
        let geom = self.outputs[oi].geom;
        let d = self.outputs[oi].dup.as_mut().ok_or(Fallback::AccessLost)?;
        let first_by = Instant::now() + FIRST_FRAME_WAIT;
        let mut timeout_ms = 0u32;
        loop {
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut res: Option<IDXGIResource> = None;
            match unsafe { d.dup.AcquireNextFrame(timeout_ms, &mut info, &mut res) } {
                Ok(()) => {
                    let taken = if info.LastPresentTime != 0 {
                        res.as_ref().map(|r| take_picture(&dev, &ctx, d, r)).transpose()
                    } else {
                        Ok(None)
                    };
                    observe_frame(&info, geom);
                    drop(res);
                    // Released at once, whatever happened: a frame held between requests is
                    // a frame the compositor cannot move past, and acquiring again without
                    // releasing is DXGI_ERROR_INVALID_CALL.
                    let _ = unsafe { d.dup.ReleaseFrame() };
                    if taken?.is_some() && !d.pictured {
                        d.pictured = true;
                        logging::line(
                            "capture",
                            &format!(
                                "desktop duplication of {name}: first picture {} ms after opening",
                                d.opened.elapsed().as_millis()
                            ),
                        );
                    }
                }
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
                Err(e) => return Err(classify(&e)),
            }
            if d.last.is_some() {
                return Ok(());
            }
            let now = Instant::now();
            if now >= first_by {
                return Err(Fallback::NoFrameYet);
            }
            timeout_ms = ((first_by - now).as_millis() as u32).max(1);
        }
    }

    /// Copies each piece of the request that output `oi` holds into its region's image.
    fn read_back(&mut self, oi: usize, plans: &[Vec<Piece>], out: &mut [CapturedImage]) -> Result<(), Fallback> {
        let ai = self.outputs[oi].adapter;
        let (_, ctx) = self.adapters[ai].dev.clone().ok_or(Fallback::DeviceRemoved)?;
        let d = self.outputs[oi].dup.as_ref().ok_or(Fallback::AccessLost)?;
        let (Some(last), Some(staging)) = (d.last.as_ref(), d.staging.as_ref()) else {
            return Err(Fallback::NoFrameYet);
        };
        let (tw, th) = d.size;
        let here: Vec<(usize, Piece)> = plans
            .iter()
            .enumerate()
            .flat_map(|(ri, ps)| ps.iter().filter(|p| p.output == oi).map(move |p| (ri, *p)))
            .collect();
        for (_, p) in &here {
            // The desktop rectangle and the surface disagree about the output's size: a mode
            // change the list has not caught up with. Read nothing rather than out of bounds.
            if p.src_x + p.w > tw || p.src_y + p.h > th {
                self.factory = None;
                return Err(Fallback::Topology);
            }
            let b = D3D11_BOX { left: p.src_x, top: p.src_y, front: 0, right: p.src_x + p.w, bottom: p.src_y + p.h, back: 1 };
            unsafe {
                ctx.CopySubresourceRegion(staging, 0, p.src_x, p.src_y, 0, last, 0, Some(&b as *const D3D11_BOX))
            };
        }
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut m)) }.map_err(|e| classify(&e))?;
        let pitch = m.RowPitch as usize;
        let base = m.pData as *const u8;
        if !base.is_null() && pitch >= tw as usize * 4 {
            for (ri, p) in here {
                let start = p.src_y as usize * pitch + p.src_x as usize * 4;
                let len = (p.h as usize - 1) * pitch + p.w as usize * 4;
                // SAFETY: the mapping covers `pitch * th` bytes, and the piece was checked to
                // lie inside the tw x th surface above, so [start, start + len) is inside it.
                let src = unsafe { std::slice::from_raw_parts(base.add(start), len) };
                let img = &mut out[ri];
                let img_w = img.w as usize;
                blit_bgra_to_rgba(src, pitch, p.w as usize, p.h as usize, &mut img.rgba, img_w, p.dst_x as usize, p.dst_y as usize);
            }
        }
        unsafe { ctx.Unmap(staging, 0) };
        if base.is_null() || pitch < tw as usize * 4 {
            return Err(Fallback::Failed(0));
        }
        Ok(())
    }

    fn serve(&mut self, rects: &[(i32, i32, i32, i32)]) -> Result<Vec<CapturedImage>, Fallback> {
        if rects.iter().any(|&(_, _, w, h)| !fits(w, h)) {
            return Err(Fallback::TooLarge);
        }
        let monitors = self.ensure_topology()?;
        let geoms = self.geoms();
        let mut plans: Vec<Vec<Piece>> = Vec::with_capacity(rects.len());
        for &(x, y, w, h) in rects {
            if uncovered_on_a_monitor(x, y, w, h, &geoms, &monitors) {
                return Err(Fallback::Topology);
            }
            match plan(x, y, w, h, &geoms) {
                Plan::Rotated(_) => return Err(Fallback::Rotated),
                Plan::Pieces(p) => plans.push(p),
            }
        }
        let mut involved: Vec<usize> = plans.iter().flatten().map(|p| p.output).collect();
        involved.sort_unstable();
        involved.dedup();
        for &oi in &involved {
            self.refresh(oi)?;
        }
        // Opaque black: whatever no output covers reads as the standard path reads it off the
        // desktop, (0,0,0,255), so a `save` or a template cut from it is the same bytes either way.
        let mut out: Vec<CapturedImage> = rects
            .iter()
            .map(|&(_, _, w, h)| CapturedImage {
                w: w as u32,
                h: h as u32,
                rgba: [0u8, 0, 0, 255].repeat(w as usize * h as usize),
            })
            .collect();
        for &oi in &involved {
            if let Err(why) = self.read_back(oi, &plans, &mut out) {
                return Err(self.fail(oi, why));
            }
        }
        // Only when a duplication was actually read. A request wholly off the desktop opens
        // nothing, and calling it open would give the next pump read the short wait of an
        // open duplication against a real opening.
        if !involved.is_empty() {
            self.answered();
        }
        Ok(out)
    }

    fn warm(&mut self) {
        if self.ensure_topology().is_err() {
            return;
        }
        let primary = self
            .outputs
            .iter()
            .position(|o| o.geom.left <= 0 && o.geom.top <= 0 && o.geom.right > 0 && o.geom.bottom > 0)
            .unwrap_or(0);
        if self.outputs[primary].geom.rotated {
            return;
        }
        if self.refresh(primary).is_ok() {
            self.answered();
        }
    }

    fn answered(&mut self) {
        self.gate.set_state(READY);
        self.lost_streak = 0;
        if let Some(was) = self.last_logged.take() {
            if was != Fallback::NoFrameYet {
                logging::line("capture", "desktop duplication is open again");
            }
        }
    }
}

/// Copies a newly acquired frame into the kept picture, creating the two textures the first
/// time and whenever the surface's size changes.
fn take_picture(
    dev: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    d: &mut Dup,
    res: &IDXGIResource,
) -> Result<(), Fallback> {
    let tex: ID3D11Texture2D = res.cast().map_err(|e| classify(&e))?;
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { tex.GetDesc(&mut desc) };
    // DuplicateOutput (DXGI 1.2) documents its image as always 8-bit BGRA — HDR and 10-bit
    // desktops are converted — so anything else is a surprise worth refusing loudly.
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB {
        return Err(Fallback::Format(desc.Format.0));
    }
    if d.last.is_none() || d.size != (desc.Width, desc.Height) {
        let mut kept = D3D11_TEXTURE2D_DESC {
            Width: desc.Width,
            Height: desc.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: 0,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut last: Option<ID3D11Texture2D> = None;
        unsafe { dev.CreateTexture2D(&kept, None, Some(&mut last)) }.map_err(|e| classify(&e))?;
        kept.Usage = D3D11_USAGE_STAGING;
        kept.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut staging: Option<ID3D11Texture2D> = None;
        unsafe { dev.CreateTexture2D(&kept, None, Some(&mut staging)) }.map_err(|e| classify(&e))?;
        d.last = last;
        d.staging = staging;
        d.size = (desc.Width, desc.Height);
    }
    let last = d.last.as_ref().ok_or(Fallback::Failed(0))?;
    unsafe { ctx.CopyResource(last, &tex) };
    Ok(())
}

/// Whether the pointer at `(x, y)` is on this output (`right`/`bottom` exclusive).
pub(crate) fn pointer_on(geom: OutputGeom, x: i32, y: i32) -> bool {
    x >= geom.left && x < geom.right && y >= geom.top && y < geom.bottom
}

/// Two things a frame can say once that are worth knowing for the rest of the session.
/// `geom` is the rectangle of the output the frame came from.
fn observe_frame(info: &DXGI_OUTDUPL_FRAME_INFO, geom: OutputGeom) {
    static PROTECTED: AtomicBool = AtomicBool::new(false);
    static POINTER: AtomicBool = AtomicBool::new(false);
    if info.ProtectedContentMaskedOut.as_bool() && !PROTECTED.swap(true, Ordering::Relaxed) {
        logging::line(
            "capture",
            "desktop duplication: Windows blanked protected content (DRM video) out of the picture",
        );
    }
    // The pointer is usually NOT part of the picture — duplication reports it separately — but
    // a software pointer (an enlarged or coloured one, for instance) is drawn into the image
    // itself, and then a template or a colour read under it can see it. When Windows shows a
    // pointer that duplication does not report as a separate one, that is the likely case (M17).
    // Only asked while the pointer is on THIS output: each output's duplication reports the
    // pointer as not visible whenever it is on another one.
    if info.LastMouseUpdateTime != 0 && !POINTER.load(Ordering::Relaxed) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetCursorInfo, CURSORINFO, CURSOR_SHOWING};
        let mut ci: CURSORINFO = unsafe { std::mem::zeroed() };
        ci.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if unsafe { GetCursorInfo(&mut ci) } != 0 && pointer_on(geom, ci.ptScreenPos.x, ci.ptScreenPos.y) {
            let shown = ci.flags & CURSOR_SHOWING != 0;
            if shown != info.PointerPosition.Visible.as_bool() && !POINTER.swap(true, Ordering::Relaxed) {
                logging::line(
                    "capture",
                    &format!(
                        "desktop duplication: Windows says the pointer is {}, duplication says it is {} — {}",
                        if shown { "shown" } else { "hidden" },
                        if info.PointerPosition.Visible.as_bool() { "visible" } else { "not visible" },
                        if shown {
                            "the pointer is probably drawn into the picture (a software pointer), so reads under it can see it"
                        } else {
                            "worth a look if reads under the pointer look wrong"
                        }
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(left: i32, top: i32, w: i32, h: i32) -> OutputGeom {
        OutputGeom { left, top, right: left + w, bottom: top + h, rotated: false }
    }

    #[test]
    fn a_region_inside_one_output_is_one_piece() {
        let outs = [out(0, 0, 1920, 1080)];
        assert_eq!(
            plan(100, 200, 55, 27, &outs),
            Plan::Pieces(vec![Piece { output: 0, src_x: 100, src_y: 200, w: 55, h: 27, dst_x: 0, dst_y: 0 }])
        );
    }

    #[test]
    fn a_region_across_two_outputs_is_stitched_from_two_pieces() {
        let outs = [out(0, 0, 1920, 1080), out(1920, 0, 1280, 1024)];
        let Plan::Pieces(p) = plan(1900, 10, 40, 20, &outs) else { panic!("not rotated") };
        assert_eq!(
            p,
            vec![
                Piece { output: 0, src_x: 1900, src_y: 10, w: 20, h: 20, dst_x: 0, dst_y: 0 },
                Piece { output: 1, src_x: 0, src_y: 10, w: 20, h: 20, dst_x: 20, dst_y: 0 },
            ]
        );
    }

    #[test]
    fn a_monitor_at_negative_coordinates_maps_to_its_own_surface() {
        // A secondary monitor to the LEFT of the primary has negative desktop coordinates; its
        // surface still starts at (0, 0).
        let outs = [out(0, 0, 1920, 1080), out(-1280, -200, 1280, 1024)];
        assert_eq!(
            plan(-100, -150, 10, 10, &outs),
            Plan::Pieces(vec![Piece { output: 1, src_x: 1180, src_y: 50, w: 10, h: 10, dst_x: 0, dst_y: 0 }])
        );
    }

    #[test]
    fn what_no_output_covers_is_left_out_of_the_pieces() {
        let outs = [out(0, 0, 1920, 1080)];
        // Partly off the right edge: only the on-screen part is a piece, placed at the start.
        let Plan::Pieces(p) = plan(1910, 1070, 20, 20, &outs) else { panic!() };
        assert_eq!(p, vec![Piece { output: 0, src_x: 1910, src_y: 1070, w: 10, h: 10, dst_x: 0, dst_y: 0 }]);
        // Partly off the top-left: the piece lands inside the image, not at its origin.
        let Plan::Pieces(p) = plan(-5, -3, 10, 10, &outs) else { panic!() };
        assert_eq!(p, vec![Piece { output: 0, src_x: 0, src_y: 0, w: 5, h: 7, dst_x: 5, dst_y: 3 }]);
        // Wholly off the desktop: no pieces, so every pixel stays (0,0,0,0).
        assert_eq!(plan(5000, 5000, 10, 10, &outs), Plan::Pieces(vec![]));
    }

    #[test]
    fn a_rotated_output_is_refused_only_when_touched() {
        let mut portrait = out(1920, 0, 1080, 1920);
        portrait.rotated = true;
        let outs = [out(0, 0, 1920, 1080), portrait];
        assert_eq!(plan(1910, 0, 20, 10, &outs), Plan::Rotated(1));
        assert!(matches!(plan(0, 0, 20, 10, &outs), Plan::Pieces(p) if p.len() == 1));
    }

    #[test]
    fn a_zero_or_negative_size_plans_nothing() {
        let outs = [out(0, 0, 1920, 1080)];
        assert_eq!(plan(10, 10, 0, 10, &outs), Plan::Pieces(vec![]));
        assert_eq!(plan(10, 10, 10, -1, &outs), Plan::Pieces(vec![]));
    }

    #[test]
    fn a_region_whose_edge_overflows_i32_is_planned_without_wrapping() {
        let outs = [out(0, 0, 1920, 1080)];
        // x + w overflows i32; in i32 arithmetic it would wrap negative and plan garbage.
        let Plan::Pieces(p) = plan(i32::MAX - 5, 0, i32::MAX, 10, &outs) else { panic!() };
        assert!(p.is_empty());
        let Plan::Pieces(p) = plan(-10, -10, i32::MAX, i32::MAX, &outs) else { panic!() };
        assert_eq!(p, vec![Piece { output: 0, src_x: 0, src_y: 0, w: 1920, h: 1080, dst_x: 10, dst_y: 10 }]);
    }

    #[test]
    fn a_mirrored_display_is_read_once() {
        let outs = [out(0, 0, 1920, 1080), out(0, 0, 1920, 1080)];
        let Plan::Pieces(p) = plan(0, 0, 10, 10, &outs) else { panic!() };
        assert_eq!(p.len(), 1);
        assert!(same_layout(&outs, &[(0, 0, 1920, 1080)]));
    }

    #[test]
    fn a_gap_on_a_real_monitor_is_told_from_a_corner_of_an_l_shape() {
        // Two monitors of different heights side by side: the virtual screen's bounding box has
        // a corner below the smaller one that no monitor covers. That corner is off the desktop,
        // not a stale list.
        let outs = [out(0, 0, 1920, 1080), out(1920, 0, 1280, 720)];
        let monitors = [(0, 0, 1920, 1080), (1920, 0, 3200, 720)];
        assert!(!uncovered_on_a_monitor(2000, 800, 50, 50, &outs, &monitors));
        assert!(same_layout(&outs, &monitors));
        // A monitor plugged in since the outputs were listed: a region on it is a real gap.
        let monitors_now = [(0, 0, 1920, 1080), (1920, 0, 3200, 720), (-1280, 0, 0, 1024)];
        assert!(uncovered_on_a_monitor(-100, 10, 50, 50, &outs, &monitors_now));
        assert!(!uncovered_on_a_monitor(10, 10, 50, 50, &outs, &monitors_now));
        assert!(!same_layout(&outs, &monitors_now));
    }

    #[test]
    fn the_blit_swaps_channels_forces_alpha_and_honours_pitch_and_offset() {
        // A 2x2 BGRA piece in a surface whose rows are 12 bytes (one pixel of padding each),
        // blitted into a 3x3 RGBA image at (1, 1).
        let src: Vec<u8> = vec![
            1, 2, 3, 0, 4, 5, 6, 7, 99, 99, 99, 99, //
            10, 20, 30, 40, 50, 60, 70, 80, 99, 99, 99, 99,
        ];
        let mut dst = vec![0u8; 3 * 3 * 4];
        blit_bgra_to_rgba(&src, 12, 2, 2, &mut dst, 3, 1, 1);
        let px = |x: usize, y: usize| &dst[(y * 3 + x) * 4..(y * 3 + x) * 4 + 4];
        assert_eq!(px(1, 1), &[3, 2, 1, 255]);
        assert_eq!(px(2, 1), &[6, 5, 4, 255]);
        assert_eq!(px(1, 2), &[30, 20, 10, 255]);
        assert_eq!(px(2, 2), &[70, 60, 50, 255]);
        // Nothing outside the piece was touched: that is where "off the desktop" stays zero.
        assert_eq!(px(0, 0), &[0, 0, 0, 0]);
        assert_eq!(px(0, 2), &[0, 0, 0, 0]);
    }

    #[test]
    fn a_region_too_large_to_allocate_is_refused_before_anything_else() {
        assert!(fits(1920, 1080));
        assert!(fits(7680, 4320)); // 8K, 33 million pixels
        assert!(!fits(60000, 60000));
        assert!(!fits(0, 10));
        assert!(!fits(10, -1));
    }

    #[test]
    fn the_back_off_grows_for_lost_access_and_is_flat_for_the_rest() {
        let ms = |d: Option<Duration>| d.map(|d| d.as_millis());
        assert_eq!(ms(backoff_for(Fallback::AccessLost, 0)), Some(250));
        assert_eq!(ms(backoff_for(Fallback::AccessLost, 1)), Some(500));
        assert_eq!(ms(backoff_for(Fallback::SecureDesktop, 2)), Some(1000));
        assert_eq!(ms(backoff_for(Fallback::SecureDesktop, 9)), Some(2000));
        assert_eq!(ms(backoff_for(Fallback::TooManyRecorders, 5)), Some(5000));
        assert_eq!(ms(backoff_for(Fallback::Unsupported(0), 0)), Some(5000));
        // Recreated on the next read, not waited out: the driver is already back by then.
        assert_eq!(backoff_for(Fallback::DeviceRemoved, 0), None);
        // A caller-side reason never backs the engine off.
        assert_eq!(backoff_for(Fallback::Budget, 0), None);
        assert_eq!(backoff_for(Fallback::Overdue, 0), None);
    }

    #[test]
    fn the_pump_budget_is_a_sliding_window() {
        let t0 = Instant::now();
        let mut ring = VecDeque::new();
        assert_eq!(budget_left(&mut ring, t0), PUMP_BUDGET);
        ring.push_back((t0, Duration::from_millis(40)));
        assert_eq!(budget_left(&mut ring, t0 + Duration::from_millis(10)), Duration::from_millis(20));
        ring.push_back((t0 + Duration::from_millis(20), Duration::from_millis(30)));
        assert_eq!(budget_left(&mut ring, t0 + Duration::from_millis(50)), Duration::ZERO);
        // The first wait ages out of the window; only the second still counts.
        assert_eq!(budget_left(&mut ring, t0 + Duration::from_millis(305)), Duration::from_millis(30));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn quick_answers_do_not_spend_the_pump_budget() {
        // A module reading through duplication in a loop: two hundred answers of 1 ms each in
        // one window. Charged in full they would run the budget out after sixty, and every read
        // after that would go to the 16 ms standard path the budget exists to spare.
        let t0 = Instant::now();
        let mut ring = VecDeque::new();
        for i in 0..200u64 {
            record_wait(&mut ring, t0 + Duration::from_micros(i * 1500), Duration::from_millis(1), true);
        }
        assert_eq!(budget_left(&mut ring, t0 + Duration::from_millis(299)), PUMP_BUDGET);
        assert!(ring.is_empty(), "nothing charged, nothing kept");
        // What is slow beyond an ordinary answer is charged; a wait that ran out, all of it.
        assert_eq!(charge(Duration::from_millis(12), true), Duration::from_millis(7));
        assert_eq!(charge(Duration::from_millis(12), false), Duration::from_millis(12));
    }

    #[test]
    fn a_pump_read_is_not_sent_with_a_sliver_of_budget_and_a_budget_cut_wait_blames_nobody() {
        let g = Gate::new();
        g.set_state(READY);
        // Too little left to wait for anything: not sent at all, rather than sent with a
        // deadline of a fraction of a millisecond that can only run out.
        assert_eq!(g.admit(Caller::Pump, 0, Duration::from_micros(300), 1), Err(Fallback::Budget));
        assert_eq!(g.admit(Caller::Pump, 0, Duration::from_millis(4), 2), Err(Fallback::Budget));
        // A wait the budget shortened, and that ran out: the budget's answer, not a late
        // engine. Nothing stops the next read, and nothing is logged.
        let (wait, cut) = g.admit(Caller::Pump, 0, Duration::from_millis(20), 3).unwrap();
        assert_eq!((wait, cut), (Duration::from_millis(20), true));
        assert_eq!(g.missed(Caller::Pump, wait, cut), Fallback::Budget);
        assert!(!g.overdue.load(Ordering::SeqCst));
        // A full wait that ran out does say the engine is late.
        let (wait, cut) = g.admit(Caller::Pump, 0, PUMP_BUDGET, 4).unwrap();
        assert_eq!((wait, cut), (PUMP_READY_WAIT, false));
        assert_eq!(g.missed(Caller::Pump, wait, cut), Fallback::Overdue);
        assert!(g.overdue.load(Ordering::SeqCst));
    }

    #[test]
    fn a_pump_read_dropped_unread_does_not_shut_the_pump_out_for_good() {
        // The sequence the review found. The first pump read of an opening takes the one wait
        // an opening allows, its wait runs out before the engine even picks the request up,
        // and the engine drops it unread. Before, nothing gave that wait back, and every pump
        // read after it heard "still opening" for the rest of the session.
        for cut_short in [false, true] {
            let g = Gate::new();
            let (wait, _) = g.admit(Caller::Pump, 1_000, PUMP_BUDGET, 1).unwrap();
            let why = g.missed(Caller::Pump, wait, cut_short);
            assert!(matches!(why, Fallback::Overdue | Fallback::Budget), "{why:?}");
            g.skipped(1);
            let next = g.admit(Caller::Pump, 1_100, PUMP_BUDGET, 2);
            assert!(next.is_ok(), "cut short: {cut_short}: {next:?}");
        }
    }

    #[test]
    fn an_opening_allows_one_pump_wait_until_the_request_that_took_it_ends() {
        let g = Gate::new();
        let (wait, _) = g.admit(Caller::Pump, 0, PUMP_BUDGET, 1).unwrap();
        assert_eq!(wait, PUMP_OPENING_WAIT.min(PUMP_BUDGET));
        g.begin(1, 0);
        g.set_state(OPENING);
        g.opening_started(0);
        // Another pump read of the same opening does not wait at all...
        assert_eq!(g.admit(Caller::Pump, 10, PUMP_BUDGET, 2), Err(Fallback::Opening));
        // ...while the worker, which has its own wait and blocks no hook, is still sent.
        assert_eq!(g.admit(Caller::Worker, 10, Duration::MAX, 3), Ok((WORKER_WAIT, false)));
        // A worker read ending does not give back the pump's wait: only its own request does.
        g.release_waiter(3);
        assert_eq!(g.admit(Caller::Pump, 20, PUMP_BUDGET, 4), Err(Fallback::Opening));
        g.opening_ended(200);
        g.set_state(READY);
        g.end(1, Duration::from_millis(210), Duration::from_millis(200));
        assert_eq!(g.admit(Caller::Pump, 220, PUMP_BUDGET, 5), Ok((PUMP_READY_WAIT, false)));
    }

    #[test]
    fn a_prewarm_takes_the_opening_so_no_pump_read_waits_behind_it() {
        let g = Gate::new();
        assert!(g.claim_warm(0));
        assert!(!g.claim_warm(1), "one warm-up at a time, however many windows come forward");
        // The first detection read of the trigger that caused the warm-up: no wait, the
        // standard picture (or none) this once.
        assert_eq!(g.admit(Caller::Pump, 2, PUMP_BUDGET, 1), Err(Fallback::Opening));
        // The engine runs the warm-up under a seq of its own and then gives WARMING back.
        g.begin(7, 3);
        g.set_state(READY);
        g.end(7, Duration::from_millis(200), Duration::from_millis(195));
        g.release_waiter(WARMING);
        assert_eq!(g.admit(Caller::Pump, 250, PUMP_BUDGET, 2), Ok((PUMP_READY_WAIT, false)));
        assert!(!g.claim_warm(251), "nothing to warm while it is open");
    }

    #[test]
    fn an_opening_that_never_ends_is_counted_as_a_hang_and_a_cold_one_is_not() {
        let g = Gate::new();
        let (wait, cut) = g.admit(Caller::Worker, 0, Duration::MAX, 1).unwrap();
        g.begin(1, 0);
        g.set_state(OPENING);
        g.opening_started(0);
        assert_eq!(g.missed(Caller::Worker, wait, cut), Fallback::Overdue);
        // Five seconds into an opening is a cold driver (measured up to 4.3 s), not a hang.
        assert_eq!(g.admit(Caller::Pump, 5_000, PUMP_BUDGET, 2), Err(Fallback::Overdue));
        assert_eq!(g.hangs.load(Ordering::SeqCst), 0);
        // Past OPEN_HANG it is one, counted once however often it is asked about.
        assert_eq!(g.admit(Caller::Pump, 10_500, PUMP_BUDGET, 3), Err(Fallback::Overdue));
        assert_eq!(g.admit(Caller::Pump, 12_000, PUMP_BUDGET, 4), Err(Fallback::Overdue));
        assert_eq!(g.hangs.load(Ordering::SeqCst), 1);
        // An opening that does end, but only after OPEN_HANG, counts on the engine's side.
        g.begin(9, 0);
        g.end(9, Duration::from_secs(12), Duration::from_secs(11));
        assert_eq!(g.hangs.load(Ordering::SeqCst), 2);
        // A read slow only because of its opening is not a hang.
        g.begin(10, 0);
        g.end(10, Duration::from_millis(4_000), Duration::from_millis(3_000));
        assert_eq!(g.hangs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn three_hangs_stop_it_and_a_dead_thread_has_its_own_reason() {
        let g = Gate::new();
        for seq in 1..=3 {
            g.begin(seq, 0);
            g.end(seq, HANG, Duration::ZERO);
        }
        assert_eq!(g.admit(Caller::Worker, 0, Duration::MAX, 9), Err(Fallback::Disabled));
        // A request that finishes late does not undo it.
        g.set_state(READY);
        assert_eq!(g.admit(Caller::Pump, 0, PUMP_BUDGET, 10), Err(Fallback::Disabled));
        assert!(!g.claim_warm(0));

        let g = Gate::new();
        assert!(g.claim_warm(0));
        g.crashed("a test");
        assert_eq!(g.admit(Caller::Pump, 0, PUMP_BUDGET, 1), Err(Fallback::Crashed));
        assert_ne!(Fallback::Crashed.describe(), Fallback::Disabled.describe());
    }

    #[test]
    fn a_reason_is_logged_once_per_kind_of_module_however_their_reads_interleave() {
        // Two modules, one reading the standard way instead and one with fallback = "none",
        // alternating reads through an outage: two lines, not one per read.
        let mut last = None;
        let lines: Vec<bool> = [true, false, true, false, true]
            .into_iter()
            .map(|or_standard| newly_noted(&mut last, Fallback::SecureDesktop, or_standard))
            .collect();
        assert_eq!(lines, [true, true, false, false, false]);
        // A different reason is news again.
        assert!(newly_noted(&mut last, Fallback::AccessLost, true));
    }

    #[test]
    fn the_first_read_comparison_looks_at_the_window_when_the_read_is_too_small_to_say_anything() {
        let window = (100, 50, 800, 600);
        // A pixel read compares the foreground window.
        assert_eq!(comparison_region((412, 318, 1, 1), Some(window)), Some((window, true)));
        // A region big enough to say something is compared as read.
        assert_eq!(comparison_region((0, 0, 200, 100), Some(window)), Some(((0, 0, 200, 100), false)));
        // No usable window: the small region anyway, and nothing at all for one that cannot
        // be read this way.
        assert_eq!(comparison_region((5, 5, 1, 1), None), Some(((5, 5, 1, 1), false)));
        assert_eq!(comparison_region((5, 5, 1, 1), Some((0, 0, 0, 0))), Some(((5, 5, 1, 1), false)));
        assert_eq!(comparison_region((0, 0, 0, 5), None), None);
    }

    #[test]
    fn turning_the_switch_off_and_on_again_starts_a_fresh_engine() {
        // The way out of a stopped, crashed or stuck engine: a new thread with a new gate,
        // rather than a reset of state the old thread may still be writing. Starting an engine
        // creates no device and opens nothing; its thread waits for a request.
        let (_, before) = engine().unwrap();
        crate::appcfg::set("desktop_duplication", false);
        crate::appcfg::set("desktop_duplication", true);
        let (_, after) = engine().unwrap();
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(after.state(), IDLE);
    }

    #[test]
    fn the_pointer_is_compared_only_on_its_own_output() {
        let right = out(1920, 0, 1280, 1024);
        assert!(pointer_on(right, 1920, 0));
        assert!(!pointer_on(right, 1919, 10), "on the monitor to its left");
        assert!(!pointer_on(right, 3200, 10), "the right edge is exclusive");
    }

    #[test]
    fn the_comparison_counts_colour_and_ignores_alpha() {
        let a = CapturedImage { w: 2, h: 1, rgba: vec![10, 20, 30, 255, 0, 0, 0, 255] };
        let same_but_alpha = CapturedImage { w: 2, h: 1, rgba: vec![10, 20, 30, 0, 0, 0, 0, 7] };
        let (pct, worst, _, _) = compare(&a, &same_but_alpha).unwrap();
        assert_eq!((pct, worst), (0.0, 0));
        let other = CapturedImage { w: 2, h: 1, rgba: vec![10, 20, 30, 255, 0, 9, 0, 255] };
        let (pct, worst, _, _) = compare(&a, &other).unwrap();
        assert_eq!((pct, worst), (50.0, 9));
        let smaller = CapturedImage { w: 1, h: 1, rgba: vec![0; 4] };
        assert!(compare(&a, &smaller).is_none());
    }

    #[test]
    fn every_reason_reads_as_a_sentence() {
        for why in [
            Fallback::SwitchedOff,
            Fallback::Disabled,
            Fallback::Crashed,
            Fallback::NoThread,
            Fallback::Closing,
            Fallback::Unsupported(0x887A0004u32 as i32),
            Fallback::TooManyRecorders,
            Fallback::Format(10),
            Fallback::Failed(-1),
        ] {
            let s = why.describe();
            assert!(!s.is_empty() && !s.ends_with('.'), "{why:?}: {s}");
        }
        // The one a laptop owner needs names the remedy, not only the error.
        assert!(Fallback::Unsupported(0).describe().contains("Power saving"));
    }

    // ---- Live, on a real desktop: `cargo test -p host dxgi_live -- --ignored --nocapture` ----

    /// The test binary has no manifest, so it is not DPI-aware unless it says so; the
    /// application is (per-monitor v2), and both paths must be compared in that space.
    fn dpi_aware() {
        use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        });
    }

    const COLOURS: [(u8, u8, u8); 4] = [(220, 30, 40), (30, 200, 60), (40, 60, 230), (128, 128, 128)];

    /// A topmost popup painted in four known colours, one per quadrant.
    struct TestWindow(windows_sys::Win32::Foundation::HWND);

    impl TestWindow {
        fn show(x: i32, y: i32, w: i32, h: i32) -> Self {
            use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
            use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
            use windows_sys::Win32::Graphics::Gdi::{
                BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, UpdateWindow, PAINTSTRUCT,
            };
            use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, GetClientRect, RegisterClassW, ShowWindow, SW_SHOWNOACTIVATE,
                WM_PAINT, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            };
            unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
                if msg == WM_PAINT {
                    unsafe {
                        let mut ps: PAINTSTRUCT = std::mem::zeroed();
                        let dc = BeginPaint(hwnd, &mut ps);
                        let mut rc: RECT = std::mem::zeroed();
                        GetClientRect(hwnd, &mut rc);
                        let (hw, hh) = (rc.right / 2, rc.bottom / 2);
                        for (i, (r, g, b)) in COLOURS.iter().enumerate() {
                            let (qx, qy) = ((i as i32 % 2) * hw, (i as i32 / 2) * hh);
                            let q = RECT { left: qx, top: qy, right: qx + hw, bottom: qy + hh };
                            let brush = CreateSolidBrush(*r as u32 | (*g as u32) << 8 | (*b as u32) << 16);
                            FillRect(dc, &q, brush);
                            DeleteObject(brush);
                        }
                        EndPaint(hwnd, &ps);
                    }
                    return 0;
                }
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
            let class: Vec<u16> = "dxgi-live-test\0".encode_utf16().collect();
            unsafe {
                let hinst = GetModuleHandleW(std::ptr::null());
                let mut wc: WNDCLASSW = std::mem::zeroed();
                wc.lpfnWndProc = Some(proc_);
                wc.hInstance = hinst;
                wc.lpszClassName = class.as_ptr();
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_POPUP,
                    x,
                    y,
                    w,
                    h,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    hinst,
                    std::ptr::null(),
                );
                assert!(!hwnd.is_null(), "the test window could not be created");
                // No fade-in: DWM's show animation races the first capture otherwise.
                let off: i32 = 1;
                DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_TRANSITIONS_FORCEDISABLED as u32,
                    &off as *const i32 as *const core::ffi::c_void,
                    4,
                );
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                UpdateWindow(hwnd);
                TestWindow(hwnd)
            }
        }

        fn settle(&self, for_: Duration) {
            use windows_sys::Win32::Graphics::Dwm::DwmFlush;
            use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE};
            let until = Instant::now() + for_;
            while Instant::now() < until {
                unsafe {
                    let mut m: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&m);
                        DispatchMessageW(&m);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            unsafe { DwmFlush() };
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(self.0) };
        }
    }

    /// A read that waits out the engine's own opening and lateness, as a test may and a pump
    /// must not: Overdue and Opening mean "ask again shortly", and the time until the answer
    /// came is itself worth printing (M2).
    fn capture_patiently(rects: &[(i32, i32, i32, i32)]) -> (Result<Vec<CapturedImage>, Fallback>, Duration) {
        let t = Instant::now();
        loop {
            let r = capture(rects, Caller::Worker);
            match r {
                Err(Fallback::Overdue) | Err(Fallback::Opening) | Err(Fallback::NoFrameYet)
                    if t.elapsed() < Duration::from_secs(5) =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => return (r, t.elapsed()),
            }
        }
    }

    /// The engine logs what it did — the outputs it found, how long the device and the
    /// duplication took to open, the first picture — and a test binary has a log like the
    /// application's, next to it. Printed so the numbers reach a CI log.
    fn print_engine_log(since: u64) {
        let Ok(text) = std::fs::read_to_string(crate::logging::log_path()) else { return };
        for line in text.lines() {
            let stamp: u64 = line.split(' ').next().and_then(|s| s.parse().ok()).unwrap_or(0);
            if stamp >= since && line.contains("[capture]") {
                println!("DXGI LOG: {line}");
            }
        }
    }

    fn unix_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// The colour at the centre of each quadrant of a (w x h) capture.
    fn quadrant_colours(img: &CapturedImage) -> Vec<(u8, u8, u8)> {
        let (w, h) = (img.w as usize, img.h as usize);
        (0..4)
            .map(|i| {
                let (x, y) = ((i % 2) * w / 2 + w / 4, (i / 2) * h / 2 + h / 4);
                let o = (y * w + x) * 4;
                (img.rgba[o], img.rgba[o + 1], img.rgba[o + 2])
            })
            .collect()
    }

    /// Informational on purpose, for now: whether GDI even works in the CI runner's session is
    /// itself an open question (M19), and whether duplication works on its Basic Display
    /// Adapter is another (M12). So anything the environment decides prints a `::warning::` and
    /// passes; only what would be OUR fault — a picture of the wrong size — fails.
    #[test]
    #[ignore = "needs a real desktop session: cargo test -p host dxgi_live -- --ignored --nocapture"]
    fn dxgi_live() {
        dpi_aware();
        crate::logging::init();
        let since = unix_now();
        crate::appcfg::set("desktop_duplication", true);
        let (x, y, w, h) = (200, 200, 240, 120);
        let win = TestWindow::show(x, y, w, h);
        live_attempts(&win, x, y, w, h);
        print_engine_log(since);
    }

    fn live_attempts(win: &TestWindow, x: i32, y: i32, w: i32, h: i32) {
        for attempt in 1..=3 {
            win.settle(Duration::from_millis(200 * attempt));
            let gdi = super::super::windows::capture_screen(x, y, w, h);
            let gdi_sees = gdi.as_ref().map(quadrant_colours);
            let gdi_ok = gdi_sees.as_deref() == Some(&COLOURS[..]);
            println!("DXGI LIVE: attempt {attempt}, GDI sees {gdi_sees:?}");
            let (dup, took) = capture_patiently(&[(x, y, w, h)]);
            println!("DXGI LIVE: duplication answered after {} ms", took.as_millis());
            match &dup {
                Ok(v) => {
                    assert_eq!(v.len(), 1, "one image per region");
                    assert_eq!((v[0].w, v[0].h), (w as u32, h as u32), "exactly the size asked for");
                    assert_eq!(v[0].rgba.len(), (w * h * 4) as usize);
                    let dup_sees = quadrant_colours(&v[0]);
                    println!("DXGI LIVE: duplication sees {dup_sees:?}");
                    if let Some(g) = &gdi {
                        match compare(g, &v[0]) {
                            Some((pct, worst, ls, ld)) => println!(
                                "DXGI LIVE: {pct:.2} % of pixels differ from GDI, by up to {worst}; mean luminance GDI {ls:.1}, duplication {ld:.1}"
                            ),
                            None => println!("::warning::DXGI LIVE: GDI and duplication returned different sizes"),
                        }
                    }
                    if gdi_ok && dup_sees == COLOURS {
                        println!("DXGI LIVE: both paths see the four colours");
                        return;
                    }
                }
                Err(why) => println!("DXGI LIVE: duplication could not answer: {}", why.describe()),
            }
            if attempt == 3 {
                if !gdi_ok {
                    println!("::warning::DXGI LIVE: GDI does not see the test window's colours in this session (M19)");
                }
                match dup {
                    Ok(_) => println!("::warning::DXGI LIVE: duplication answered but does not see the test window's colours"),
                    Err(why) => println!("::warning::DXGI LIVE: duplication unavailable here: {} (M12)", why.describe()),
                }
            }
        }
    }

    /// The design's M1/M2/M3 numbers, printed: what opening costs, what a read costs at the
    /// sizes the project reads, through each path, and whether the two paths agree byte for
    /// byte. Run on the reference machine; the constants at the top of this file come from it.
    #[test]
    #[ignore = "a measurement, not a test: cargo test -p host dxgi_measure -- --ignored --nocapture"]
    fn dxgi_measure() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        dpi_aware();
        crate::logging::init();
        let since = unix_now();
        crate::appcfg::set("desktop_duplication", true);
        let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        let (first, took) = capture_patiently(&[(0, 0, 1, 1)]);
        println!(
            "DXGI MEASURE: first read (device, duplication, first picture): {} ms — {}",
            took.as_millis(),
            match &first {
                Ok(_) => "answered".to_string(),
                Err(why) => why.describe(),
            }
        );
        let median = |mut v: Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[v.len() / 2]
        };
        for (w, h) in [(1, 1), (55, 27), (633, 418), (sw, sh)] {
            let (mut g, mut d) = (Vec::new(), Vec::new());
            let (mut last_g, mut last_d) = (None, None);
            for _ in 0..15 {
                let t = Instant::now();
                last_g = super::super::windows::capture_screen(0, 0, w, h);
                g.push(t.elapsed().as_secs_f64() * 1000.0);
                let t = Instant::now();
                last_d = capture(&[(0, 0, w, h)], Caller::Worker).ok().and_then(|mut v| v.pop());
                d.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let agree = match (&last_g, &last_d) {
                (Some(a), Some(b)) => match compare(a, b) {
                    Some((pct, worst, _, _)) => format!("{pct:.2} % of pixels differ (by up to {worst})"),
                    None => "different sizes".into(),
                },
                _ => "one path failed".into(),
            };
            println!(
                "DXGI MEASURE: {w}x{h}: GDI median {:.2} ms, duplication median {:.2} ms; {agree}",
                median(g),
                median(d)
            );
        }
        let (n, us, fb) = take_counters();
        println!("DXGI MEASURE: {n} duplication read(s), {:.1} ms in total, {fb} unanswered", us as f64 / 1000.0);
        print_engine_log(since);
    }
}
