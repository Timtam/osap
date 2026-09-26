//! Screen snapshots as a module holds them: `host.screen.snapshot`, `host.screen.snapshotAsync`,
//! the `Snapshot` handle, what it may cost, and the rules every call that reads one applies.
//!
//! **What a snapshot is for.** Several reads of ONE picture: the cursor of a game menu, the help
//! bubble beside it and the page title, all from the same moment and for the price of one
//! capture — where separate calls take separate pictures, and a picture that changed between
//! them gives an answer that was never on screen. A snapshot is taken because a module asked,
//! and read only where the module passes it (`{ snapshot = s }`); nothing is ever answered from
//! one implicitly. `docs/screen-frame-sharing-design.md` records why that line is where it is.
//!
//! **The handle.** VM-local userdata over one `Arc<Frame>` (`backend/frame.rs`). Its geometry,
//! `time`, `inputEpoch` and `via` stay readable after `release()`; only reading its pixels then
//! raises. A snapshot never leaves its VM: userdata cannot cross into another, and data exports
//! and settings refuse it.
//!
//! **What it may hold.** Luau's collector sees a handle as a few dozen bytes, not as the pixels
//! behind it, so a poll that never releases its snapshots would grow the process without the
//! collector ever hurrying. So every snapshot is charged against a budget — 128 MiB per module
//! VM and 512 MiB in the whole application — reserved before the capture from an estimate and
//! settled to the truth once the picture exists; over it, this VM's collector runs twice and the
//! call raises only if they still do not fit — for corners the module wrote; a window region,
//! whose size is the window's at run time, is answered `nil` and the reason instead, as its size
//! limit is (`reserve_for`). Every reservation also steps the collector by
//! about as much as it reserved, but never more than the heap holds (`gc_kbytes`), so an
//! unreleased poll is paced by the collector: a snapshot as large as the heap or larger costs
//! about one collection of it, a smaller one a part of one. The template budget in
//! `image_search.rs` works the same way; this one is its own counter, since a snapshot is a
//! picture a module holds for a moment and a template one it keeps.
//!
//! **The two geometry rules** (docs/api/screen.md, "Reading a snapshot"): the calls that read a
//! region — `profile`, the image searches, `save`, `saveMarked` — read the part of it inside the
//! snapshot (`clip`); the calls whose answer would change if the region were cut — `pixel`,
//! `pixels`, `crop`, `template{ capture }`, the cells calls — need it wholly inside (`inside`).
//! A miss is `nil` and one of three fixed reasons, never an error.
//!
//! **Taken off the event loop.** `snapshotAsync` hands the picture to the `screen-capture` thread
//! `host.ocr.read` photographs on (`ocr/service.rs`, `ocr/snap_queue.rs`): at once, at a set time
//! (`at`), or as a change wait (`change`, `ocr/change.rs`) that photographs round after round until
//! the watched part differs from a baseline. Its callback runs exactly once on the event loop —
//! `cb(snap, nil, info)` or `cb(nil, reason, info)` — unless the module is disabled, reloaded or
//! removed first, when it is dropped. A newer request with the same key answers the older one
//! `nil` and a reason; too many at once end the module's oldest or refuse the new one — never
//! both for one request — and never raise. Everything here that decides — the keys, the caps, the answers — works on [`SnapState`]
//! alone, so it is tested without the application around it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MetaMethod, MultiValue, RegistryKey, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::backend::frame::{self, Area, Frame, FrameVia};
use crate::backend::CapturedImage;
use crate::ocr::change::{ChangeSpec, Wait};
use crate::ocr::policy::{
    AT_MAX, MAX_WAIT_PIXELS, MAX_WATCH, SLOW_LOG_EVERY, SNAP_PER_OWNER, SNAP_TOTAL, SNAP_WAITS, SNAP_WAITS_PER_OWNER,
    WAIT_MAX, WAIT_MIN_PIXELS, WAIT_SETTLE, WAIT_TIMEOUT, WAIT_TOLERANCE,
};
use crate::ocr::sched::Owner;
use crate::ocr::snap_queue::{self, ChangeInfo, SnapDone, SnapId, SnapKind, SnapOutcome, SnapReq, SnapTicket};
use crate::ocr::types::{current_priority, enter_priority, Priority, Rect};
use crate::region::{self, ScreenRect};
use crate::region_lua::{self, PointArg};
use crate::{call_guarded, capture_source, describe_value, logging, one_value, with_reason, Shared};

/// What one module VM's snapshots may hold between them. A constant like the template budget,
/// not a setting: a snapshot of a whole 4K screen is 33 MB, so three of them held at once is a
/// module keeping more than it reads.
pub(crate) const SNAPSHOT_BUDGET_PER_VM: usize = 128 << 20;
/// What every module's snapshots together may hold.
pub(crate) const SNAPSHOT_BUDGET_PROCESS: usize = 512 << 20;
/// The most points one `host.screen.pixels` call reads.
pub(crate) const MAX_POINTS: usize = 4096;

/// How many times `w * h * 4` a snapshot is reserved at before it exists. On macOS a snapshot
/// keeps the backing-store image beside its point-sized pixels: four times as many pixels on a
/// Retina display, so five times in all. Settled to what the frame really holds once it does.
#[cfg(target_os = "macos")]
const ESTIMATE_FACTOR: usize = 5;
#[cfg(not(target_os = "macos"))]
const ESTIMATE_FACTOR: usize = 1;

/// The three reasons a read of a snapshot answers with when the region or point misses it.
/// Listed word for word in docs/api/screen.md ("Failure reasons"), which a test holds.
pub(crate) const NOT_INSIDE: &str = "the region is not inside the snapshot";
pub(crate) const POINT_NOT_INSIDE: &str = "the point is not inside the snapshot";
pub(crate) const NO_OVERLAP: &str = "the region does not overlap the snapshot";
/// What a call that is handed a released snapshot raises with, after its own name.
const RELEASED: &str = "the snapshot was released";

fn err(m: String) -> mlua::Error {
    mlua::Error::external(m)
}

fn rect_of(r: ScreenRect) -> Rect {
    Rect::new(r.x, r.y, r.w, r.h)
}

// ── The budget ─────────────────────────────────────────────────────────────────────────────

/// The running total for one Lua state, kept in its app data.
struct SnapBudget(Rc<Cell<usize>>);

/// This VM's snapshot budget, made the first time anything asks. Get-or-insert per LUA STATE,
/// not per `install_host_api` call — a code dependency's host is installed into the same state
/// once per dependency — and the app-data reference is dropped before this returns, so nothing
/// holds it while a collection runs finalisers. `image_search::budget`'s rule, for its reason.
fn vm_budget(lua: &Lua) -> Rc<Cell<usize>> {
    if let Ok(Some(b)) = lua.try_app_data_ref::<SnapBudget>() {
        return b.0.clone();
    }
    let b = Rc::new(Cell::new(0));
    let _ = lua.try_set_app_data(SnapBudget(b.clone()));
    b
}

/// Bytes taken from both budgets, given back when dropped — with the handle that holds them, or
/// straight away when the capture fails after reserving. Never touches Lua, so it may run inside
/// a collection.
pub(crate) struct Reservation {
    bytes: Cell<usize>,
    vm: Rc<Cell<usize>>,
    process: Rc<Cell<usize>>,
}

impl Reservation {
    /// A charge of nothing against this VM and the application, to be settled: for a picture
    /// that arrives without a reservation of its own, which the binding never makes.
    fn nothing(lua: &Lua, process: &Rc<Cell<usize>>) -> Reservation {
        Reservation { bytes: Cell::new(0), vm: vm_budget(lua), process: process.clone() }
    }

    /// Charges `actual` instead of what was reserved: the estimate becomes the truth once the
    /// picture exists. May leave a budget a little over its limit when the truth is larger than
    /// the estimate; the next reservation then collects first.
    fn settle(&self, actual: usize) {
        let was = self.bytes.replace(actual);
        for c in [&self.vm, &self.process] {
            c.set(c.get().saturating_sub(was).saturating_add(actual));
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let b = self.bytes.get();
        self.vm.set(self.vm.get().saturating_sub(b));
        self.process.set(self.process.get().saturating_sub(b));
    }
}

/// How far to step the collector for a reservation of `bytes` in a state using `used`: as much
/// as was reserved, but never more than the heap, and never past what the call takes. Luau
/// itself pays at most one heap's worth of debt per step call — asked for more than the heap
/// holds, it sets the threshold to 0, and the step ends after about one heap's work or at the
/// end of the cycle (`lua_gc(LUA_GCSTEP)`, lapi.cpp) — so a snapshot as large as the heap or
/// larger costs about one collection of it whatever this says; the clamp only keeps the value
/// honest and in range.
fn gc_kbytes(bytes: usize, used: usize) -> i32 {
    (bytes.min(used) / 1024).min(i32::MAX as usize) as i32
}

/// Reserves `bytes` against this VM's budget and the application's (`process`), collecting
/// this VM's garbage twice first if they do not fit — one full collection may only finish the
/// cycle already running, and a dropped handle gives its bytes back only once it is finalised —
/// and answering `Ok(Err(why))` if they still do not, `why` naming the limit and what to do,
/// without a function's name; [`reserve_for`] decides whether that raises. Then steps the
/// collector. Only this VM is collected: the application's limit can also be reached by other
/// modules' snapshots, which no call here can free, and the message says so.
fn try_reserve(lua: &Lua, process: &Rc<Cell<usize>>, bytes: usize) -> mlua::Result<Result<Reservation, String>> {
    let vm = vm_budget(lua);
    let fits = |vm: &Cell<usize>| {
        vm.get().saturating_add(bytes) <= SNAPSHOT_BUDGET_PER_VM
            && process.get().saturating_add(bytes) <= SNAPSHOT_BUDGET_PROCESS
    };
    if !fits(&vm) {
        lua.gc_collect()?;
        lua.gc_collect()?;
        if !fits(&vm) {
            let mib = |b: usize| b as f64 / (1 << 20) as f64;
            let over_vm = vm.get().saturating_add(bytes) > SNAPSHOT_BUDGET_PER_VM;
            return Ok(Err(if over_vm {
                format!(
                    "this module's snapshots would hold {:.1} MiB; the limit is {} MiB per module VM ({} MiB in the whole application). Release the ones you no longer need with s:release()",
                    mib(vm.get().saturating_add(bytes)),
                    SNAPSHOT_BUDGET_PER_VM >> 20,
                    SNAPSHOT_BUDGET_PROCESS >> 20
                )
            } else {
                format!(
                    "the application's snapshots would hold {:.1} MiB; the limit is {} MiB in the whole application ({} MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()",
                    mib(process.get().saturating_add(bytes)),
                    SNAPSHOT_BUDGET_PROCESS >> 20,
                    SNAPSHOT_BUDGET_PER_VM >> 20
                )
            }));
        }
    }
    vm.set(vm.get() + bytes);
    process.set(process.get() + bytes);
    let res = Reservation { bytes: Cell::new(bytes), vm, process: process.clone() };
    let kb = gc_kbytes(bytes, lua.used_memory());
    if kb > 0 {
        lua.gc_step_kbytes(kb)?;
    }
    Ok(Ok(res))
}

/// The reservation for a picture of a region given as `form`: corners the module wrote raise
/// when the budget has no room — a mistake in the call, like corners past the size limit — while
/// a window region, whose size is the window's at run time, is answered `Ok(Err(why))`, as the
/// size limit answers it: a poll of a window that was maximised on a large display never puts an
/// error dialog over it.
fn reserve_for(lua: &Lua, process: &Rc<Cell<usize>>, bytes: usize, fname: &str, form: &region::Region) -> mlua::Result<Result<Reservation, String>> {
    match (try_reserve(lua, process, bytes)?, form) {
        (Ok(res), _) => Ok(Ok(res)),
        (Err(why), region::Region::Window(..)) => Ok(Err(format!("{AT_WINDOW_SIZE}{why}"))),
        (Err(why), region::Region::Rect(_)) => Err(err(format!("{fname}: {why}"))),
    }
}

/// How every reason a window region's size at run time gives begins.
const AT_WINDOW_SIZE: &str = "at the window's current size ";

/// A charge that raises when it does not fit, as one for corners does: for the tests.
#[cfg(test)]
pub(crate) fn reserve(lua: &Lua, process: &Rc<Cell<usize>>, bytes: usize, fname: &str) -> mlua::Result<Reservation> {
    try_reserve(lua, process, bytes)?.map_err(|why| err(format!("{fname}: {why}")))
}

/// What a snapshot of `r` is reserved at before it exists (see [`ESTIMATE_FACTOR`]).
fn estimate(r: Rect) -> usize {
    (r.area().max(0) as usize).saturating_mul(4).saturating_mul(ESTIMATE_FACTOR)
}

// ── The handle ─────────────────────────────────────────────────────────────────────────────

/// A snapshot in a module's hands. See the file's comment.
pub(crate) struct SnapshotHandle {
    frame: RefCell<Option<Arc<Frame>>>,
    res: RefCell<Option<Reservation>>,
    rect: Rect,
    /// `host.now()` when the capture came back.
    time_ms: i64,
    /// `host.inputEpoch()` once the capture had come back (`Frame::input_epoch`).
    input_epoch: u64,
    via: FrameVia,
    /// The application's counter, for the reservations `crop` makes.
    process: Rc<Cell<usize>>,
}

impl SnapshotHandle {
    /// A handle over a picture taken on the event loop, where `host.inputEpoch()` was
    /// `input_epoch` from the call until the capture came back: no input is acted on meanwhile.
    fn new(mut frame: Frame, res: Reservation, input_epoch: u64, process: Rc<Cell<usize>>) -> SnapshotHandle {
        frame.input_epoch = input_epoch;
        SnapshotHandle::of(Arc::new(frame), res, process)
    }

    /// A handle over a picture, with the input epoch it carries, charged by `res` settled to
    /// what it holds. Two requests answered from one capture each hold the same pixels and are
    /// each charged for them.
    fn of(frame: Arc<Frame>, res: Reservation, process: Rc<Cell<usize>>) -> SnapshotHandle {
        res.settle(frame.bytes());
        SnapshotHandle {
            rect: frame.rect,
            time_ms: frame.taken.saturating_duration_since(crate::clock_origin()).as_millis() as i64,
            input_epoch: frame.input_epoch,
            via: frame.via,
            frame: RefCell::new(Some(frame)),
            res: RefCell::new(Some(res)),
            process,
        }
    }

    /// The picture, unless it was released.
    fn frame(&self) -> Option<Arc<Frame>> {
        self.frame.borrow().clone()
    }

    /// Lets go of the pixels and gives the bytes back. A read still in flight with it keeps its
    /// own reference until it is done; a second call does nothing.
    fn release(&self) {
        self.frame.borrow_mut().take();
        self.res.borrow_mut().take();
    }

    fn describe(&self) -> String {
        match self.frame.borrow().as_ref() {
            None => "Snapshot(released)".to_string(),
            Some(f) => format!(
                "Snapshot({}x{} at {},{}, {}, {:.1} MiB, {} ms old)",
                self.rect.w,
                self.rect.h,
                self.rect.x,
                self.rect.y,
                self.via.word(),
                f.bytes() as f64 / (1 << 20) as f64,
                f.taken.elapsed().as_millis()
            ),
        }
    }
}

impl UserData for SnapshotHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, s| Ok(s.rect.x));
        fields.add_field_method_get("y", |_, s| Ok(s.rect.y));
        fields.add_field_method_get("w", |_, s| Ok(s.rect.w));
        fields.add_field_method_get("h", |_, s| Ok(s.rect.h));
        fields.add_field_method_get("time", |_, s| Ok(s.time_ms));
        fields.add_field_method_get("inputEpoch", |_, s| Ok(s.input_epoch as i64));
        fields.add_field_method_get("via", |_, s| Ok(s.via.word()));
        fields.add_field_method_get("released", |_, s| Ok(s.frame.borrow().is_none()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("release", |_, s, ()| {
            s.release();
            Ok(())
        });
        methods.add_method("crop", |lua, s, region: Value| crop(lua, s, &region));
        methods.add_meta_method(MetaMethod::ToString, |_, s, ()| Ok(s.describe()));
    }
}

/// A handle over `frame`, charged by `res`, as the one value a call returns.
fn handle_value(lua: &Lua, frame: Frame, res: Reservation, input_epoch: u64, process: &Rc<Cell<usize>>) -> mlua::Result<MultiValue> {
    let h = SnapshotHandle::new(frame, res, input_epoch, process.clone());
    one_value(Value::UserData(lua.create_userdata(h)?))
}

/// `s:crop(region) -> (Snapshot?, string?)`: a snapshot of its own of a part of `s`, the pixels
/// copied and charged anew, from the same moment. The region is read strictly, as
/// `host.screen.snapshot`'s is, and must lie wholly inside `s`; the budget is [`reserve_for`]'s.
fn crop(lua: &Lua, s: &SnapshotHandle, v: &Value) -> mlua::Result<MultiValue> {
    const F: &str = "Snapshot:crop";
    let frame = s.frame().ok_or_else(|| err(format!("{F}: {RELEASED}")))?;
    let given = region_lua::read(v, "the region").map_err(|m| err(format!("{F}: {m}")))?;
    let r = match given.resolve() {
        Ok(r) => rect_of(r),
        Err(u) => return with_reason(lua, Value::Nil, u.to_string()),
    };
    if !frame.contains(r) {
        return with_reason(lua, Value::Nil, NOT_INSIDE.to_string());
    }
    let res = match reserve_for(lua, &s.process, estimate(r), F, &given)? {
        Ok(res) => res,
        Err(why) => return with_reason(lua, Value::Nil, why),
    };
    match frame.crop(r) {
        Some(f) => handle_value(lua, f, res, s.input_epoch, &s.process),
        None => with_reason(lua, Value::Nil, NOT_INSIDE.to_string()),
    }
}

// ── A snapshot where a call takes one ──────────────────────────────────────────────────────

/// `v` as a snapshot's picture: `None` for `nil`; raises, naming `what`, for anything that is
/// not a Snapshot — a Template handle included — and for one that was released.
pub(crate) fn from_value(v: &Value, fname: &str, what: &str) -> mlua::Result<Option<Arc<Frame>>> {
    match v {
        Value::Nil => Ok(None),
        Value::UserData(ud) => match ud.borrow::<SnapshotHandle>() {
            Ok(s) => s.frame().map(Some).ok_or_else(|| err(format!("{fname}: {what}: {RELEASED}"))),
            Err(_) => Err(err(format!("{fname}: {what} must be a Snapshot, got a userdata of another kind"))),
        },
        other => Err(err(format!("{fname}: {what} must be a Snapshot, got {}", describe_value(other)))),
    }
}

/// `opts.snapshot` of a call that reads its options loosely: every other key is left to the
/// call, as it always was.
pub(crate) fn from_opts(opts: Option<&Table>, fname: &str) -> mlua::Result<Option<Arc<Frame>>> {
    match opts {
        None => Ok(None),
        Some(t) => from_value(&t.get::<Value>("snapshot")?, fname, "opts.snapshot"),
    }
}

/// The options of `pixel` and `pixels`, strictly: `nil`, or `{ snapshot = s }` and nothing else.
pub(crate) fn strict_opts(v: &Value, fname: &str) -> mlua::Result<Option<Arc<Frame>>> {
    let t = match v {
        Value::Nil => return Ok(None),
        Value::Table(t) => t,
        other => return Err(err(format!("{fname}: opts must be a table {{ snapshot = s }}, got {}", describe_value(other)))),
    };
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair?;
        if !matches!(&k, Value::String(s) if s.as_bytes().as_ref() == b"snapshot") {
            return Err(err(format!(
                "{fname}: opts has {}; it takes snapshot",
                match &k {
                    Value::String(s) => format!("a key '{}'", s.to_string_lossy()),
                    other => format!("an entry at {}", describe_value(other)),
                }
            )));
        }
    }
    from_value(&t.get::<Value>("snapshot")?, fname, "opts.snapshot")
}

/// A region a call reads loosely, on a snapshot: the one reader with the snapshot's edges as the
/// defaults, so a region left out is the whole snapshot and a corner left out its edge. A
/// mistake raises as it does live; `Ok(Err(reason))` is a window region with no rectangle this
/// time. The rule (`clip` or `inside`) is the caller's to apply.
pub(crate) fn loose_region(v: &Value, fname: &str, what: &str, frame: &Frame) -> mlua::Result<Result<ScreenRect, String>> {
    let r = frame.rect;
    let given = region_lua::read_loose_within(v, what, (r.x, r.y, r.right(), r.bottom()))
        .map_err(|m| err(format!("{fname}: {m}")))?;
    Ok(given.resolve().map_err(|u| u.to_string()))
}

/// [`loose_region`] of `opts.region`.
pub(crate) fn opts_region_in(opts: Option<&Table>, fname: &str, frame: &Frame) -> mlua::Result<Result<ScreenRect, String>> {
    let v = match opts {
        Some(o) => o.get::<Value>("region")?,
        None => Value::Nil,
    };
    loose_region(&v, fname, "opts.region", frame)
}

/// The clip rule: the part of `r` inside the snapshot, or [`NO_OVERLAP`].
pub(crate) fn clip(frame: &Frame, r: ScreenRect) -> Result<Rect, String> {
    frame.clip(rect_of(r)).ok_or_else(|| NO_OVERLAP.to_string())
}

/// The inside rule: `r` when it lies wholly inside the snapshot, or [`NOT_INSIDE`].
pub(crate) fn inside(frame: &Frame, r: ScreenRect) -> Result<Rect, String> {
    let r = rect_of(r);
    if frame.contains(r) {
        Ok(r)
    } else {
        Err(NOT_INSIDE.to_string())
    }
}

// ── What a synchronous read reads ──────────────────────────────────────────────────────────

/// The picture a synchronous call reads: a capture made for it, or the part of a snapshot its
/// region covers. The matching and reduction after it are the same code either way.
pub(crate) enum Picture {
    /// Captured now: `img` is the region, whose top-left is `(x, y)` on screen.
    Live { img: CapturedImage, x: i32, y: i32 },
    /// A snapshot, read where `rect` lies in it; `rect` is inside it.
    Snap { frame: Arc<Frame>, rect: Rect },
}

impl Picture {
    pub(crate) fn image(&self) -> &CapturedImage {
        match self {
            Picture::Live { img, .. } => img,
            Picture::Snap { frame, .. } => &frame.img,
        }
    }

    /// Where the image's top-left pixel is on screen: what every position found in it is
    /// measured from.
    pub(crate) fn origin(&self) -> (i32, i32) {
        match self {
            Picture::Live { x, y, .. } => (*x, *y),
            Picture::Snap { frame, .. } => (frame.rect.x, frame.rect.y),
        }
    }

    /// The part of the image the region covers, in its own pixels: all of a live capture.
    pub(crate) fn area(&self) -> Area {
        match self {
            Picture::Live { img, .. } => Area::full(img.w, img.h),
            Picture::Snap { frame, rect } => frame.area(*rect).unwrap_or(Area { x: 0, y: 0, w: 0, h: 0 }),
        }
    }

    /// The area for the template matcher: `None` — the whole frame, exactly as a live search
    /// has always been made — for a live capture.
    pub(crate) fn search_area(&self) -> Option<crate::template::Rect> {
        match self {
            Picture::Live { .. } => None,
            Picture::Snap { .. } => {
                let a = self.area();
                Some(crate::template::Rect { x: a.x, y: a.y, w: a.w, h: a.h })
            }
        }
    }

    /// Whether a screen was touched for it: a live capture is counted in the observation line,
    /// a read of a snapshot is not.
    pub(crate) fn is_live(&self) -> bool {
        matches!(self, Picture::Live { .. })
    }
}

// ── host.screen.snapshot ───────────────────────────────────────────────────────────────────

/// `host.screen.snapshot`'s options: `{ region = … }`, strictly, and the region's rectangle with
/// the form it was given in — or `Ok(Err(reason))` for a window region with none this time, or
/// one so large at the window's current size that it is past [`frame::MAX_FRAME_PIXELS`];
/// corners that large raise.
fn read_opts(v: &Value, fname: &str) -> mlua::Result<Result<(Rect, region::Region), String>> {
    let Value::Table(t) = v else {
        return Err(err(format!("{fname}: opts must be a table {{ region = … }}, got {}", describe_value(v))));
    };
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair?;
        if !matches!(&k, Value::String(s) if s.as_bytes().as_ref() == b"region") {
            return Err(err(format!(
                "{fname}: opts has {}; it takes region",
                match &k {
                    Value::String(s) => format!("a key '{}'", s.to_string_lossy()),
                    other => format!("an entry at {}", describe_value(other)),
                }
            )));
        }
    }
    let given = region_lua::read(&t.get::<Value>("region")?, "opts.region").map_err(|m| err(format!("{fname}: {m}")))?;
    let r = match given.resolve() {
        Ok(r) => rect_of(r),
        Err(u) => return Ok(Err(u.to_string())),
    };
    if r.area() > frame::MAX_FRAME_PIXELS {
        let why = format!("the region is {}x{}, {} pixels; the limit is {}", r.w, r.h, r.area(), frame::MAX_FRAME_PIXELS);
        return match given {
            // Corners are the module's own: a mistake in the call.
            region::Region::Rect(_) => Err(err(format!("{fname}: {why}"))),
            // A window region's size is the window's, at run time: answered.
            region::Region::Window(..) => Ok(Err(format!("{AT_WINDOW_SIZE}{why}"))),
        };
    }
    Ok(Ok((r, given)))
}

/// `host.screen.snapshot(opts)` without the backend: reads `opts`, reserves the budget
/// ([`reserve_for`]: a window region's overrun is answered), takes the picture through `capture`
/// and answers the handle — or `nil` and why, giving the reservation back. The binding passes the
/// backend's `frame`; the tests a fake.
fn take(
    lua: &Lua,
    process: &Rc<Cell<usize>>,
    input_epoch: u64,
    opts: &Value,
    capture: impl FnOnce(Rect) -> Result<Frame, String>,
) -> mlua::Result<MultiValue> {
    const F: &str = "host.screen.snapshot";
    let (r, given) = match read_opts(opts, F)? {
        Ok(r) => r,
        Err(why) => return with_reason(lua, Value::Nil, why),
    };
    let res = match reserve_for(lua, process, estimate(r), F, &given)? {
        Ok(res) => res,
        Err(why) => return with_reason(lua, Value::Nil, why),
    };
    match capture(r) {
        Ok(frame) => handle_value(lua, frame, res, input_epoch, process),
        Err(why) => with_reason(lua, Value::Nil, why),
    }
}

/// `host.screen.snapshot(opts) -> (Snapshot?, string?)`: one capture on the event loop, through
/// the module's source, counted in the observation line like `profile`.
pub(crate) fn snapshot(lua: &Lua, shared: &Rc<Shared>) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, opts: Value| {
        take(lua, &sh.snap_bytes, sh.input_epoch.get(), &opts, |r| {
            // The source first, outside the timing, as every read does: in a module that reads
            // through desktop duplication the first read also compares the two sources, once.
            let src = capture_source::read_source(lua, &*sh.backend, r.tuple());
            let t0 = Instant::now();
            let got = sh.backend.frame(r, src);
            let mut obs = sh.observations();
            obs.pixels += 1;
            obs.pixel_us += t0.elapsed().as_micros();
            got
        })
    })
}

// ── host.screen.pixels ─────────────────────────────────────────────────────────────────────

/// `points` of `host.screen.pixels`: a list of 0 to [`MAX_POINTS`] points, each read strictly.
fn read_point_list(v: &Value, fname: &str) -> mlua::Result<Vec<PointArg>> {
    let Value::Table(t) = v else {
        return Err(err(format!("{fname}: points must be a list of points {{ {{ x, y }}, … }}, got {}", describe_value(v))));
    };
    let len = t.raw_len();
    if t.clone().pairs::<Value, Value>().count() != len {
        return Err(err(format!("{fname}: points must be a list (1, 2, 3, …) with nothing else in it")));
    }
    if len > MAX_POINTS {
        return Err(err(format!("{fname}: points has {len} entries; the limit is {MAX_POINTS}")));
    }
    (1..=len)
        .map(|i| {
            let p: Value = t.raw_get(i)?;
            region_lua::read_point_any(&p, &format!("points[{i}]")).map_err(|m| err(format!("{fname}: {m}")))
        })
        .collect()
}

/// `{ r, g, b, hex }`, the one shape a colour is returned in.
pub(crate) fn colour_table(lua: &Lua, (r, g, b): (u8, u8, u8)) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("r", r)?;
    t.set("g", g)?;
    t.set("b", b)?;
    t.set("hex", format!("#{r:02X}{g:02X}{b:02X}"))?;
    Ok(t)
}

/// `host.screen.pixels(points, opts?)` without the backend: reads the arguments, then answers
/// the colours from the snapshot, or from `live` — the backend's `pixels` — when there is none.
fn read_pixels(
    lua: &Lua,
    points: &Value,
    opts: &Value,
    live: impl FnOnce(&[(i32, i32)]) -> Result<Vec<(u8, u8, u8)>, String>,
) -> mlua::Result<MultiValue> {
    const F: &str = "host.screen.pixels";
    let pts = read_point_list(points, F)?;
    let snap = strict_opts(opts, F)?;
    let mut at = Vec::with_capacity(pts.len());
    for p in &pts {
        match p.resolve() {
            Ok(xy) => at.push(xy),
            Err(u) => return with_reason(lua, Value::Nil, u.to_string()),
        }
    }
    let colours = if at.is_empty() {
        Vec::new()
    } else if let Some(frame) = snap {
        match at.iter().map(|&(x, y)| frame.sample(x, y)).collect::<Option<Vec<_>>>() {
            Some(c) => c,
            None => return with_reason(lua, Value::Nil, POINT_NOT_INSIDE.to_string()),
        }
    } else {
        match live(&at) {
            Ok(c) if c.len() == at.len() => c,
            Ok(_) => return with_reason(lua, Value::Nil, crate::backend::CAPTURE_FAILED.to_string()),
            Err(why) => return with_reason(lua, Value::Nil, why),
        }
    };
    let list = lua.create_table_with_capacity(colours.len(), 0)?;
    for (i, c) in colours.into_iter().enumerate() {
        list.raw_set(i + 1, colour_table(lua, c)?)?;
    }
    one_value(Value::Table(list))
}

/// `host.screen.pixels(points, opts?) -> ({ Colour }?, string?)`: every point from as few
/// captures as the backend can make of them, on the event loop, counted as one screen read in
/// the observation line — or from a snapshot, touching nothing.
pub(crate) fn pixels(lua: &Lua, shared: &Rc<Shared>) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (points, opts): (Value, Value)| {
        read_pixels(lua, &points, &opts, |at| {
            // The region the first-read comparison is offered: the points' box when one capture
            // of it is what the standard path takes, otherwise the first point, as `pixel`
            // offers — never a box that would have both sources capture a whole desktop.
            let b = match frame::bbox(at) {
                Some(r) if r.area() <= frame::POINTS_BOX_PIXELS => r.tuple(),
                _ => (at[0].0, at[0].1, 1, 1),
            };
            let src = capture_source::read_source(lua, &*sh.backend, b);
            let t0 = Instant::now();
            let got = sh.backend.pixels(at, src);
            let mut obs = sh.observations();
            obs.pixels += 1;
            obs.pixel_us += t0.elapsed().as_micros();
            got
        })
    })
}

// ── host.screen.snapshotAsync ──────────────────────────────────────────────────────────────

/// What a request is answered when a newer one with the same key replaced it.
pub(crate) const SUPERSEDED: &str = "a newer request with the same key replaced this one";
/// What a change wait is answered when a `watch` region misses its region at the window's size.
pub(crate) const WATCH_MISS: &str = "a watch region does not overlap the region";

/// The reasons the caps answer with, numbers included.
pub(crate) fn owner_waits_ended() -> String {
    format!("too many change waits for this module ({SNAP_WAITS_PER_OWNER}); the oldest was ended")
}
pub(crate) fn owner_requests_ended() -> String {
    format!("too many snapshot requests waiting for this module ({SNAP_PER_OWNER}); the oldest was ended")
}
pub(crate) fn all_waits_refused() -> String {
    format!("too many change waits in the whole application ({SNAP_WAITS}); this one was not started")
}
pub(crate) fn all_requests_refused() -> String {
    format!("too many snapshot requests waiting in the whole application ({SNAP_TOTAL}); this one was not taken")
}
/// What a request the thread answered `Cancelled` while the event loop still waited for it is
/// told — which no path of the host makes; kept so the callback still runs exactly once.
const CANCELLED: &str = "the request was cancelled";

/// `change` of `snapshotAsync`, once its arguments passed.
pub(crate) struct ChangeArgs {
    pub from: Option<Arc<Frame>>,
    /// Screen rectangles, each overlapping the region; empty for all of it.
    pub watch: Vec<Rect>,
    pub spec: ChangeSpec,
}

/// Every argument of `snapshotAsync` but the callback, checked.
pub(crate) struct AsyncArgs {
    /// The region's rectangle — or why there is none this time (a window region: its client area
    /// empty, past the size limit at the window's size; a `watch` of a window missing the region,
    /// or a window leaving fewer watched pixels than `minPixels`), which the callback is told on
    /// the next tick.
    pub rect: Result<Rect, String>,
    /// The region as it was given: a window region's budget overrun is answered (`reserve_for`).
    pub form: region::Region,
    pub key: Option<String>,
    /// `at`, in `host.now()` milliseconds.
    pub at_ms: Option<f64>,
    pub change: Option<ChangeArgs>,
}

const FA: &str = "host.screen.snapshotAsync";

/// The keys of a strict options table, raising for any other.
fn strict_keys(t: &Table, what: &str, allowed: &[&str], list: &str) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        match &k {
            Value::String(s) if allowed.contains(&s.to_str()?.as_ref()) => out.push((s.to_str()?.to_string(), v)),
            Value::String(s) => {
                return Err(err(format!("{FA}: {what} has a key '{}'; it takes {list}", s.to_string_lossy())))
            }
            other => return Err(err(format!("{FA}: {what} has an entry at {}; it takes {list}", describe_value(other)))),
        }
    }
    Ok(out)
}

/// A whole number from `lo` to `hi`, or a raise naming `what`, the range and what it got.
fn whole_in(v: &Value, what: &str, lo: i64, hi: i64, hi_is: &str) -> mlua::Result<i64> {
    let n = match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Number(n) => Some(*n),
        _ => None,
    };
    match n {
        Some(n) if n.is_finite() && n.fract() == 0.0 && n >= lo as f64 && n <= hi as f64 => Ok(n as i64),
        _ => Err(err(format!("{FA}: {what} must be a whole number from {lo} to {hi}{hi_is}, got {}", describe_value(v)))),
    }
}

/// A region's rectangle within `limit` pixels: corners past it raise (the module wrote them), a
/// window region past it at the window's current size is answered, as is one whose client area is
/// empty.
fn resolve_within(given: region::Region, what: &str, limit: i64) -> mlua::Result<Result<Rect, String>> {
    let r = match given.resolve() {
        Ok(r) => rect_of(r),
        Err(u) => return Ok(Err(u.to_string())),
    };
    if r.area() > limit {
        let why = format!("the region is {}x{}, {} pixels; the limit is {limit}", r.w, r.h, r.area());
        return match given {
            region::Region::Rect(_) => Err(err(format!("{FA}: {what}: {why}"))),
            region::Region::Window(..) => Ok(Err(format!("{AT_WINDOW_SIZE}{why}"))),
        };
    }
    Ok(Ok(r))
}

/// `change.watch`: a list of 1 to `MAX_WATCH` regions, each read strictly.
fn read_watch(v: &Value) -> mlua::Result<Vec<region::Region>> {
    const W: &str = "opts.change.watch";
    let Value::Table(t) = v else {
        return Err(err(format!("{FA}: {W} must be a list of regions {{ {{ x1, y1, x2, y2 }}, … }}, got {}", describe_value(v))));
    };
    let n = t.raw_len();
    if matches!(t.raw_get::<Value>(1)?, Value::Integer(_) | Value::Number(_)) || t.contains_key("x1")? || t.contains_key("window")? {
        return Err(err(format!(
            "{FA}: {W} is a list of regions, {{ {{ x1, y1, x2, y2 }}, … }}; this is one region — put it in braces: watch = {{ region }}"
        )));
    }
    if t.clone().pairs::<Value, Value>().count() != n {
        return Err(err(format!("{FA}: {W} must be a list (1, 2, 3, …) with nothing else in it")));
    }
    if n == 0 || n > MAX_WATCH {
        return Err(err(format!("{FA}: {W} holds {n} regions; it takes 1 to {MAX_WATCH}")));
    }
    (1..=n)
        .map(|i| {
            let item: Value = t.raw_get(i)?;
            region_lua::read(&item, &format!("{W}[{i}]")).map_err(|m| err(format!("{FA}: {m}")))
        })
        .collect()
}

/// `change`, strictly.
fn read_change(v: &Value) -> mlua::Result<(Option<Arc<Frame>>, Vec<region::Region>, ChangeSpec)> {
    const LIST: &str = "from, watch, timeout, tolerance, minPixels and settle";
    let Value::Table(t) = v else {
        return Err(err(format!("{FA}: opts.change must be a table {{ from?, watch?, timeout?, … }}, got {}", describe_value(v))));
    };
    let keys = strict_keys(t, "opts.change", &["from", "watch", "timeout", "tolerance", "minPixels", "settle"], LIST)?;
    let (mut from, mut watch) = (None, Vec::new());
    let mut spec = ChangeSpec { tolerance: WAIT_TOLERANCE, min_pixels: WAIT_MIN_PIXELS, settle: WAIT_SETTLE, timeout: WAIT_TIMEOUT };
    let mut settle: Option<Value> = None;
    let max_ms = WAIT_MAX.as_millis() as i64;
    for (k, v) in keys {
        match k.as_str() {
            "from" => from = from_value(&v, FA, "opts.change.from")?,
            "watch" => watch = read_watch(&v)?,
            "timeout" => spec.timeout = Duration::from_millis(whole_in(&v, "opts.change.timeout", 1, max_ms, " (ms)")? as u64),
            "tolerance" => spec.tolerance = whole_in(&v, "opts.change.tolerance", 0, 255, "")? as u8,
            "minPixels" => spec.min_pixels = whole_in(&v, "opts.change.minPixels", 1, MAX_WAIT_PIXELS, "")? as u32,
            _ => settle = Some(v),
        }
    }
    // After the timeout, which bounds it.
    if let Some(v) = settle {
        let most = spec.timeout.as_millis() as i64;
        spec.settle = Duration::from_millis(whole_in(&v, "opts.change.settle", 0, most, " (the timeout)")? as u64);
    }
    Ok((from, watch, spec))
}

/// Every argument of `host.screen.snapshotAsync(opts, cb)` but the callback, checked against
/// `host.now()`'s `now_ms`. A mistake raises; what a window does at run time is `rect: Err`.
pub(crate) fn parse_async(opts: &Value, now_ms: f64) -> mlua::Result<AsyncArgs> {
    const LIST: &str = "region, key, at and change";
    let Value::Table(t) = opts else {
        return Err(err(format!("{FA}: opts must be a table {{ region = …, key?, at?, change? }}, got {}", describe_value(opts))));
    };
    let keys = strict_keys(t, "opts", &["region", "key", "at", "change"], LIST)?;
    let (mut region_v, mut key, mut at_ms, mut change_v) = (Value::Nil, None, None, None);
    for (k, v) in keys {
        match k.as_str() {
            "region" => region_v = v,
            "key" => match v {
                Value::String(s) if !s.to_str()?.is_empty() => key = Some(s.to_str()?.to_string()),
                other => {
                    return Err(err(format!("{FA}: opts.key must be a non-empty string, got {}", describe_value(&other))))
                }
            },
            "at" => {
                let n = match v {
                    Value::Integer(i) => i as f64,
                    Value::Number(n) if n.is_finite() => n,
                    other => {
                        return Err(err(format!(
                            "{FA}: opts.at must be a time in host.now() milliseconds, got {}",
                            describe_value(&other)
                        )))
                    }
                };
                let most = now_ms + AT_MAX.as_millis() as f64;
                if n > most {
                    return Err(err(format!(
                        "{FA}: opts.at is {n}, more than {} ms after host.now() ({})",
                        AT_MAX.as_millis(),
                        now_ms.floor()
                    )));
                }
                at_ms = Some(n);
            }
            _ => change_v = Some(v),
        }
    }
    let given = region_lua::read(&region_v, "opts.region").map_err(|m| err(format!("{FA}: {m}")))?;
    let change = change_v.as_ref().map(read_change).transpose()?;
    let limit = if change.is_some() { MAX_WAIT_PIXELS } else { frame::MAX_FRAME_PIXELS };
    let region_is_corners = matches!(given, region::Region::Rect(_));
    let mut rect = resolve_within(given, "opts.region", limit)?;
    let change = match change {
        None => None,
        Some((from, watch_given, spec)) => {
            let mut watch = Vec::with_capacity(watch_given.len());
            let mut all_corners = region_is_corners;
            for (i, w) in watch_given.into_iter().enumerate() {
                let corners = matches!(w, region::Region::Rect(_));
                all_corners &= corners;
                let wr = match w.resolve() {
                    Ok(r) => rect_of(r),
                    Err(u) => {
                        if rect.is_ok() {
                            rect = Err(u.to_string());
                        }
                        continue;
                    }
                };
                if let Ok(r) = &rect {
                    match r.intersect(&wr) {
                        Some(_) => watch.push(wr),
                        // Both the module's own corners: a mistake in the call.
                        None if corners && region_is_corners => {
                            return Err(err(format!(
                                "{FA}: opts.change.watch[{}] {{ {}, {}, {}, {} }} does not overlap the region",
                                i + 1,
                                wr.x,
                                wr.y,
                                wr.right(),
                                wr.bottom()
                            )))
                        }
                        None => rect = Err(WATCH_MISS.to_string()),
                    }
                }
            }
            // A wait that watches fewer pixels than `minPixels` could never see a change: it
            // would run to its timeout every time.
            if let Ok(r) = &rect {
                let n = watched_pixels(*r, &watch);
                if n < spec.min_pixels as i64 {
                    let min = spec.min_pixels;
                    if all_corners {
                        return Err(err(format!(
                            "{FA}: opts.change.minPixels is {min}, more than the {n} pixel(s) the wait watches ({}), so it could never see a change",
                            if watch.is_empty() { "the whole region" } else { "opts.change.watch cut to the region, a shared pixel once" }
                        )));
                    }
                    rect = Err(too_few_watched(n, min));
                }
            }
            Some(ChangeArgs { from, watch, spec })
        }
    };
    Ok(AsyncArgs { rect, form: given, key, at_ms, change })
}

/// How many pixels a change wait over `region` watching `watch` (all of `region` when empty)
/// compares: each rectangle cut to the region, a pixel two of them share once — what
/// [`Wait::new`] watches.
fn watched_pixels(region: Rect, watch: &[Rect]) -> i64 {
    let cut: Vec<Rect> = if watch.is_empty() { vec![region] } else { watch.iter().filter_map(|w| region.intersect(w)).collect() };
    crate::ocr::change::disjoint(&cut).iter().map(|r| r.area()).sum()
}

/// What a change wait is answered when, at a window's size, it watches fewer than `minPixels`.
pub(crate) fn too_few_watched(n: i64, min: u32) -> String {
    format!("{AT_WINDOW_SIZE}the change wait watches {n} pixels, fewer than its minPixels {min}")
}

/// What a snapshot request is reserved at: the estimate — and for a change wait, which holds
/// its newest picture (the one it may answer) and two it only compares with, its baseline and
/// the one a still spell began with, both copies of the watched part's point-sized pixels
/// (`change.rs`, "What it holds") — two more pictures of the region's pixels without a backing
/// image: 3 x `w * h * 4` on Windows, 7 x on macOS.
fn reservation_for(r: Rect, change: bool) -> usize {
    let pixels = (r.area().max(0) as usize).saturating_mul(4);
    estimate(r).saturating_add(if change { pixels.saturating_mul(2) } else { 0 })
}

/// A request waiting for its answer. Main thread only.
pub(crate) struct PendingSnap {
    id: SnapId,
    lua: Lua,
    cb: RegistryKey,
    /// The identity the call was made under, named in an error report.
    scope: usize,
    owner: Owner,
    prio: Priority,
    key: Option<String>,
    /// Its charge against the budgets, handed to the snapshot when it comes.
    res: Option<Reservation>,
    change: bool,
    cancel: Arc<AtomicBool>,
    asked: Instant,
}

/// What a callback is called with.
pub(crate) enum Answer {
    Picture { frame: Arc<Frame>, frames: u32, change: Option<ChangeInfo>, waited: Duration },
    Failed { why: String, frames: u32, change: Option<ChangeInfo>, waited: Duration },
}

/// Everything `snapshotAsync` keeps on the event loop.
#[derive(Default)]
pub(crate) struct SnapState {
    pending: RefCell<HashMap<SnapId, PendingSnap>>,
    /// Answered without the thread — superseded, ended or refused by a cap, nothing to capture
    /// this time — on the next tick, never from inside the binding.
    ready: RefCell<Vec<(PendingSnap, String)>>,
    next_id: Cell<SnapId>,
    /// Per module VM and key, the newest request asked with it, while it has not been delivered:
    /// an older answer with the key that is delivered after a newer request was made — from a
    /// callback earlier in the same batch — is answered [`SUPERSEDED`] instead (`deliver`).
    newest_key: RefCell<HashMap<(Owner, String), SnapId>>,
    /// Per module: when crowding was last logged.
    logged: RefCell<HashMap<usize, Instant>>,
}

impl SnapState {
    /// Whether any request waits for its answer — a reason for the headless loop to run.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty() || !self.ready.borrow().is_empty()
    }

    /// Whether module `idx` has a request with the thread: the input barrier then asks the
    /// thread whether one of them holds its input.
    pub(crate) fn has_pending_for(&self, idx: usize) -> bool {
        self.pending.borrow().values().any(|p| p.owner.idx == idx)
    }

    fn next(&self) -> SnapId {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }

    /// Takes `id` off the thread's hands and answers it `why` on the next tick, giving its bytes
    /// back to the budget now: its answer carries no picture. False when it was not waiting.
    fn end(&self, id: SnapId, why: String) -> bool {
        let Some(mut p) = self.pending.borrow_mut().remove(&id) else { return false };
        p.cancel.store(true, Ordering::Release);
        p.res = None;
        self.ready.borrow_mut().push((p, why));
        true
    }

    /// `p` is the newest request of its module VM with its key.
    fn note_key(&self, p: &PendingSnap) {
        if let Some(k) = &p.key {
            self.newest_key.borrow_mut().insert((p.owner, k.clone()), p.id);
        }
    }

    /// Whether a newer request with `p`'s key was made after `p` — so `p`'s answer, not
    /// delivered yet, is superseded.
    fn replaced(&self, p: &PendingSnap) -> bool {
        p.key.as_ref().is_some_and(|k| self.newest_key.borrow().get(&(p.owner, k.clone())).is_some_and(|n| *n > p.id))
    }

    /// `p` was delivered or dropped: when it was the newest with its key, the key is forgotten.
    fn forget_key(&self, p: &PendingSnap) {
        if let Some(k) = &p.key {
            let mut keys = self.newest_key.borrow_mut();
            let at = (p.owner, k.clone());
            if keys.get(&at) == Some(&p.id) {
                keys.remove(&at);
            }
        }
    }

    /// Latest wins per key: `owner`'s requests with `key` are answered [`SUPERSEDED`] — also one
    /// whose picture was already taken. Keys are this call's own: a text read's never replaces a
    /// snapshot. How many were ended.
    fn supersede(&self, owner: Owner, key: &str) -> usize {
        let mut ids: Vec<SnapId> = self
            .pending
            .borrow()
            .values()
            .filter(|p| p.owner == owner && p.key.as_deref() == Some(key))
            .map(|p| p.id)
            .collect();
        ids.sort_unstable();
        ids.into_iter().filter(|id| self.end(*id, SUPERSEDED.to_string())).count()
    }

    /// Room for a new request of `owner`: the module's oldest change wait ends when it would
    /// have more than its share, and its oldest request of any kind likewise, so the newest
    /// proceeds — unless the whole application's limits, counted as they stand once those have
    /// ended, refuse it; then nothing of the module's is ended for a request that is not taken.
    /// The requests ended, and the reason the new one is refused.
    fn make_room(&self, owner: Owner, change: bool) -> (Vec<String>, Option<String>) {
        let (plan, refused) = {
            let pending = self.pending.borrow();
            // The module's requests, oldest first, and whether each is a change wait.
            let mut mine: Vec<(SnapId, bool)> =
                pending.values().filter(|p| p.owner.idx == owner.idx).map(|p| (p.id, p.change)).collect();
            mine.sort_unstable();
            let mut plan: Vec<(SnapId, bool, String)> = Vec::new();
            if change && mine.iter().filter(|m| m.1).count() >= SNAP_WAITS_PER_OWNER {
                if let Some(i) = mine.iter().position(|m| m.1) {
                    let (id, wait) = mine.remove(i);
                    plan.push((id, wait, owner_waits_ended()));
                }
            }
            if mine.len() >= SNAP_PER_OWNER {
                let (id, wait) = mine.remove(0);
                plan.push((id, wait, owner_requests_ended()));
            }
            let waits = pending.values().filter(|p| p.change).count() - plan.iter().filter(|p| p.1).count();
            let total = pending.len() - plan.len();
            let refused = if change && waits >= SNAP_WAITS {
                Some(all_waits_refused())
            } else if total >= SNAP_TOTAL {
                Some(all_requests_refused())
            } else {
                None
            };
            (plan, refused)
        };
        if refused.is_some() {
            return (Vec::new(), refused);
        }
        let ended = plan.into_iter().filter(|(id, _, why)| self.end(*id, why.clone())).map(|(_, _, why)| why).collect();
        (ended, None)
    }

    /// Every answer due now: those decided without the thread, then the thread's, each taken off
    /// the pending list — exactly once. An answer for a request no longer pending (ended, dropped)
    /// is let go, pixels and all.
    fn take_answers(&self, done: Vec<SnapDone>, now: Instant) -> Vec<(PendingSnap, Answer)> {
        let ready = std::mem::take(&mut *self.ready.borrow_mut());
        let mut out: Vec<(PendingSnap, Answer)> = ready
            .into_iter()
            .map(|(p, why)| {
                let change = p.change.then(ChangeInfo::default);
                let waited = now.saturating_duration_since(p.asked);
                (p, Answer::Failed { why, frames: 0, change, waited })
            })
            .collect();
        let mut pending = self.pending.borrow_mut();
        for d in done {
            let Some(p) = pending.remove(&d.id) else { continue };
            let waited = d.ended.saturating_duration_since(d.asked);
            let answer = match d.outcome {
                SnapOutcome::Picture { frame, frames, change } => {
                    let waited = frame.taken.saturating_duration_since(d.asked);
                    Answer::Picture { frame, frames, change, waited }
                }
                SnapOutcome::Failed { why, frames, change } => Answer::Failed { why, frames, change, waited },
                SnapOutcome::Cancelled => {
                    Answer::Failed { why: CANCELLED.to_string(), frames: 0, change: p.change.then(ChangeInfo::default), waited }
                }
            };
            out.push((p, answer));
        }
        out
    }

    /// Takes every request of module `idx` off the list and cancels it: disabled, reloaded,
    /// removed or rolled back, its callbacks are never called.
    fn drop_owner(&self, idx: usize) -> Vec<PendingSnap> {
        let mut gone: Vec<PendingSnap> = {
            let mut pending = self.pending.borrow_mut();
            let ids: Vec<SnapId> = pending.values().filter(|p| p.owner.idx == idx).map(|p| p.id).collect();
            ids.into_iter().filter_map(|id| pending.remove(&id)).collect()
        };
        {
            let mut ready = self.ready.borrow_mut();
            let (mine, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut *ready).into_iter().partition(|(p, _)| p.owner.idx == idx);
            *ready = keep;
            gone.extend(mine.into_iter().map(|(p, _)| p));
        }
        for p in &gone {
            p.cancel.store(true, Ordering::Release);
        }
        self.newest_key.borrow_mut().retain(|(o, _), _| o.idx != idx);
        gone
    }

    /// The module indices from `n` on that have anything here.
    fn owners_from(&self, n: usize) -> Vec<usize> {
        let mut owners: Vec<usize> = self.pending.borrow().values().map(|p| p.owner.idx).filter(|i| *i >= n).collect();
        owners.extend(self.ready.borrow().iter().map(|(p, _)| p.owner.idx).filter(|i| *i >= n));
        owners.sort_unstable();
        owners.dedup();
        owners
    }
}

/// Lets go of a request's callback.
fn release(p: PendingSnap) {
    let PendingSnap { lua, cb, .. } = p;
    let _ = lua.remove_registry_value(cb);
}

/// The `info` a callback gets.
fn info_table(lua: &Lua, frames: u32, change: Option<ChangeInfo>, waited: Duration) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("waited", waited.as_millis() as i64)?;
    t.set("frames", frames)?;
    if let Some(c) = change {
        t.set("changed", c.changed)?;
        t.set("settled", c.settled)?;
        t.set("rebased", c.rebased)?;
    }
    Ok(t)
}

/// Calls one request's callback: `cb(snap, nil, info)` with a handle charged by its reservation
/// settled to what the picture holds, or `cb(nil, reason, info)`.
fn call_back(p: &mut PendingSnap, answer: Answer, process: &Rc<Cell<usize>>) -> Result<(), String> {
    let lua = p.lua.clone();
    let f: Function = lua.registry_value(&p.cb).map_err(|e| e.to_string())?;
    let args = (|| -> mlua::Result<Vec<Value>> {
        Ok(match answer {
            Answer::Picture { frame, frames, change, waited } => {
                let res = p.res.take().unwrap_or_else(|| Reservation::nothing(&lua, process));
                let h = SnapshotHandle::of(frame, res, process.clone());
                vec![Value::UserData(lua.create_userdata(h)?), Value::Nil, Value::Table(info_table(&lua, frames, change, waited)?)]
            }
            Answer::Failed { why, frames, change, waited } => {
                p.res.take();
                vec![Value::Nil, Value::String(lua.create_string(why)?), Value::Table(info_table(&lua, frames, change, waited)?)]
            }
        })
    })()
    .map_err(|e| e.to_string())?;
    call_guarded(&f, MultiValue::from_vec(args))
}

/// Delivers `answers`: each to a VM that `alive` says is still the one that asked and enabled,
/// under the priority it was asked with; the others are dropped. An answer whose key a newer
/// request of its module VM has taken since — made by a callback earlier in this batch — is
/// answered [`SUPERSEDED`], its picture let go, as it would have been had the newer request come
/// before the thread answered. Every callback is let go afterwards. `report` hears of a callback
/// that raised.
fn deliver(
    st: &SnapState,
    answers: Vec<(PendingSnap, Answer)>,
    alive: &dyn Fn(Owner) -> bool,
    process: &Rc<Cell<usize>>,
    report: &dyn Fn(usize, &str),
) {
    for (mut p, answer) in answers {
        if alive(p.owner) {
            let answer = if st.replaced(&p) {
                p.res = None;
                let (Answer::Picture { change, waited, .. } | Answer::Failed { change, waited, .. }) = answer;
                Answer::Failed { why: SUPERSEDED.to_string(), frames: 0, change: change.map(|_| ChangeInfo::default()), waited }
            } else {
                answer
            };
            let _prio = enter_priority(p.prio);
            if let Err(e) = call_back(&mut p, answer, process) {
                report(p.scope, &e);
            }
        }
        st.forget_key(&p);
        release(p);
    }
}

impl Shared {
    /// `host.screen.snapshotAsync(opts, cb)`, called from the VM `lua` under the identity `scope`.
    pub(crate) fn snap_async(&self, lua: &Lua, scope: usize, opts: Value, cb: Value) -> mlua::Result<()> {
        let cb: Function = match cb {
            Value::Function(f) => f,
            other => {
                return Err(err(format!("{FA}: the callback (second argument) must be a function, got {}", describe_value(&other))))
            }
        };
        let now = Instant::now();
        let now_ms = now.saturating_duration_since(crate::clock_origin()).as_secs_f64() * 1000.0;
        let mut args = parse_async(&opts, now_ms)?;
        let owner = crate::ocr::lua::owner_of(lua, &self.vm_gens.borrow(), scope);
        let change = args.change.is_some();
        // The budget first: a call that raises for it changes nothing else. A window region
        // that does not fit is answered like one past the size limit.
        let res = match &args.rect {
            Ok(r) => match reserve_for(lua, &self.snap_bytes, reservation_for(*r, change), FA, &args.form)? {
                Ok(res) => Some(res),
                Err(why) => {
                    self.snap_note_crowding(owner.idx, &why);
                    args.rect = Err(why);
                    None
                }
            },
            Err(_) => None,
        };
        let st = &self.snap_state;
        let p = PendingSnap {
            id: st.next(),
            lua: lua.clone(),
            cb: lua.create_registry_value(cb)?,
            scope,
            owner,
            prio: current_priority(),
            key: args.key.clone(),
            res,
            change,
            cancel: Arc::new(AtomicBool::new(false)),
            asked: now,
        };
        // A newer request with the key is the newest whether or not it can be taken.
        let mut ended = args.key.as_deref().map_or(0, |k| st.supersede(owner, k));
        st.note_key(&p);
        let rect = match args.rect {
            Ok(r) => r,
            Err(why) => {
                st.ready.borrow_mut().push((p, why));
                if ended > 0 {
                    self.ocr.wake_snaps();
                }
                return Ok(());
            }
        };
        let (crowded, refused) = st.make_room(owner, change);
        ended += crowded.len();
        if let Some(why) = crowded.first().or(refused.as_ref()) {
            self.snap_note_crowding(owner.idx, why);
        }
        if ended > 0 {
            self.ocr.wake_snaps();
        }
        if let Some(why) = refused {
            // Refused: no picture comes, so its charge goes back now, not on the next tick.
            let mut p = p;
            p.res = None;
            st.ready.borrow_mut().push((p, why));
            return Ok(());
        }
        let source = capture_source::read_source(lua, &*self.backend, rect.tuple());
        let at = args.at_ms.map(|ms| instant_at(ms).max(now));
        let (kind, holds) = match args.change {
            Some(c) => {
                let start = at.unwrap_or(now);
                // Its first picture is its baseline when it has no `from`, or one it cannot
                // compare with: input waits for that picture, unless the module asked for it at
                // a later time anyway.
                let usable = c.from.as_ref().is_some_and(|f| snap_queue::from_usable(f, rect, &c.watch, source));
                let holds = snap_queue::holds_input(at.is_some(), true, usable);
                let wait = Box::new(Wait::new(rect, &c.watch, c.spec, c.from, start));
                (SnapKind::Change { wait, start }, holds)
            }
            None => (at.map_or(SnapKind::Plain, SnapKind::At), snap_queue::holds_input(at.is_some(), false, false)),
        };
        let req = SnapReq {
            ticket: SnapTicket { id: p.id, owner, prio: p.prio },
            region: rect,
            source,
            kind,
            asked: now,
            holds,
            cancel: p.cancel.clone(),
        };
        st.pending.borrow_mut().insert(p.id, p);
        self.ocr.submit_snap(req);
        Ok(())
    }

    /// One line per module per `SLOW_LOG_EVERY` when its snapshot requests are ended or refused
    /// by a limit, or a window region's is answered for the budget.
    fn snap_note_crowding(&self, idx: usize, why: &str) {
        let now = Instant::now();
        let mut logged = self.snap_state.logged.borrow_mut();
        if logged.get(&idx).is_some_and(|t| now.duration_since(*t) < SLOW_LOG_EVERY) {
            return;
        }
        logged.insert(idx, now);
        let id = self.ids.borrow().get(idx).cloned().unwrap_or_default();
        logging::line("snapshot", &format!("[{id}] {why} (said at most every 10 s)"));
    }

    /// Delivers what the capture thread answered and what was decided without it. Driven by
    /// both ticks, after the text reads' results.
    pub(crate) fn fire_snapshot_results(&self) {
        let done = self.ocr.drain_snaps();
        if done.is_empty() && self.snap_state.ready.borrow().is_empty() {
            return;
        }
        let answers = self.snap_state.take_answers(done, Instant::now());
        if answers.is_empty() {
            return;
        }
        for (p, a) in &answers {
            if let (true, Answer::Picture { change: Some(c), frames, waited, .. }) = (p.change, a) {
                let id = self.ids.borrow().get(p.owner.idx).cloned().unwrap_or_default();
                logging::trace("snapshot", || {
                    format!(
                        "[{id}] change wait: changed {}, settled {}, rebased {}, {frames} picture(s), {} ms",
                        c.changed,
                        c.settled,
                        c.rebased,
                        waited.as_millis()
                    )
                });
            }
        }
        // One epoch for the drain: every answer in it is a fresh look at the screen.
        self.bump_epoch();
        let alive = |o: Owner| crate::ocr::lua::deliverable(o, &self.vm_gens.borrow(), &self.enabled.borrow());
        let report = |scope: usize, e: &str| self.report_callback_error(scope, "screen.snapshotAsync", e);
        deliver(&self.snap_state, answers, &alive, &self.snap_bytes, &report);
    }

    /// Drops every snapshot request of module `idx`, never calling back: disabled, reloaded or
    /// removed. The capture thread stops photographing them at its next look.
    pub(crate) fn snap_drop_owner(&self, idx: usize) {
        let gone = self.snap_state.drop_owner(idx);
        if gone.is_empty() {
            return;
        }
        self.ocr.wake_snaps();
        for p in gone {
            release(p);
        }
    }

    /// `rollback_to(n)`'s share: every module from index `n` on.
    pub(crate) fn snap_drop_from(&self, n: usize) {
        for idx in self.snap_state.owners_from(n) {
            self.snap_drop_owner(idx);
        }
    }
}

/// The instant `ms` milliseconds after `host.now()`'s origin; before it, the origin.
fn instant_at(ms: f64) -> Instant {
    crate::clock_origin() + Duration::from_secs_f64(ms.max(0.0) / 1000.0)
}

/// `host.screen.snapshotAsync(opts, cb) -> nil`: see [`Shared::snap_async`].
pub(crate) fn snapshot_async(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (opts, cb): (Value, Value)| sh.snap_async(lua, idx, opts, cb))
}

/// A handle over `frame` in `lua`, for other modules' tests.
#[cfg(test)]
pub(crate) fn test_handle(lua: &Lua, frame: Frame) -> mlua::AnyUserData {
    let process = Rc::new(Cell::new(0));
    let res = reserve(lua, &process, frame.bytes(), "test").unwrap();
    lua.create_userdata(SnapshotHandle::new(frame, res, 0, process)).unwrap()
}

/// A frame of `w` x `h` at `(x, y)` whose pixel at screen `(px, py)` is `(px, py, 50)`, for tests.
#[cfg(test)]
pub(crate) fn test_frame(x: i32, y: i32, w: i32, h: i32) -> Frame {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for py in y..y + h {
        for px in x..x + w {
            rgba.extend_from_slice(&[px as u8, py as u8, 50, 255]);
        }
    }
    Frame::from_image(Rect::new(x, y, w, h), CapturedImage { w: w as u32, h: h as u32, rgba }, Instant::now(), FrameVia::Gdi, None)
        .expect("a whole frame")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of `r` in one colour: cheap to make at the sizes the budget tests need.
    fn blank(r: Rect) -> Frame {
        let img = CapturedImage { w: r.w as u32, h: r.h as u32, rgba: vec![255; (r.w * r.h * 4) as usize] };
        Frame::from_image(r, img, Instant::now(), FrameVia::Gdi, None).expect("a whole frame")
    }

    /// `host.screen.snapshot` over a fake capture, charging `process`, with input epoch 7.
    fn snap_fn(lua: &Lua, process: &Rc<Cell<usize>>) -> Function {
        let p = process.clone();
        lua.create_function(move |lua, opts: Value| take(lua, &p, 7, &opts, |r| Ok(blank(r)))).unwrap()
    }

    fn err_text<T>(r: mlua::Result<T>) -> String {
        match r {
            Ok(_) => panic!("expected an error"),
            Err(e) => e.to_string(),
        }
    }

    /// A code dependency's host is installed into the same state once per dependency: two
    /// bindings made separately charge ONE counter.
    #[test]
    fn two_installs_in_one_state_share_one_budget() {
        let lua = Lua::new();
        assert!(Rc::ptr_eq(&vm_budget(&lua), &vm_budget(&lua)));
        let process = Rc::new(Cell::new(0));
        lua.globals().set("a", snap_fn(&lua, &process)).unwrap();
        lua.globals().set("b", snap_fn(&lua, &process)).unwrap();
        lua.load("keep1 = a({ region = { 0, 0, 10, 10 } }) keep2 = b({ region = { 0, 0, 20, 10 } })").exec().unwrap();
        assert_eq!(vm_budget(&lua).get(), 400 + 800);
        assert_eq!(process.get(), 1200);
        lua.load("keep1 = nil keep2 = nil").exec().unwrap();
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        assert_eq!((vm_budget(&lua).get(), process.get()), (0, 0), "both gave their bytes back");
    }

    #[test]
    fn release_refunds_and_later_use_raises() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        lua.load("s = snap({ region = { 0, 0, 10, 10 } })").exec().unwrap();
        assert_eq!(process.get(), 400);
        lua.load("s:release() s:release()").exec().unwrap();
        assert_eq!((vm_budget(&lua).get(), process.get()), (0, 0), "released at once, and a second release does nothing");
        let s: Value = lua.globals().get("s").unwrap();
        let e = err_text(from_value(&s, "host.screen.pixel", "opts.snapshot"));
        assert_eq!(e, "host.screen.pixel: opts.snapshot: the snapshot was released");
        let e = err_text(lua.load("return s:crop({ 0, 0, 2, 2 })").exec());
        assert!(e.contains("Snapshot:crop: the snapshot was released"), "{e}");
    }

    #[test]
    fn fields_stay_readable_after_release() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        let before: (i32, i32, i32, i32, i64, String, bool) = lua
            .load("s = snap({ region = { 5, 6, 15, 26 } }) return s.x, s.y, s.w, s.h, s.inputEpoch, s.via, s.released")
            .eval()
            .unwrap();
        assert_eq!(before, (5, 6, 10, 20, 7, "gdi".to_string(), false));
        let time: i64 = lua.load("return s.time").eval().unwrap();
        assert!(time >= 0);
        let after: (i32, i32, i32, i32, i64, i64, bool, String) = lua
            .load("s:release() return s.x, s.y, s.w, s.h, s.time, s.inputEpoch, s.released, tostring(s)")
            .eval()
            .unwrap();
        assert_eq!((after.0, after.1, after.2, after.3, after.4, after.5), (5, 6, 10, 20, time, 7));
        assert!(after.6);
        assert_eq!(after.7, "Snapshot(released)");
    }

    #[test]
    fn reservation_refunds_when_the_capture_fails() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        let opts: Value = lua.load("return { region = { 0, 0, 100, 100 } }").eval().unwrap();
        let got = take(&lua, &process, 0, &opts, |_| {
            assert_eq!(vm_budget(&lua).get(), 40_000 * ESTIMATE_FACTOR, "reserved before the capture");
            Err("screen capture failed".to_string())
        })
        .unwrap()
        .into_vec();
        assert!(got[0].is_nil());
        assert_eq!(got[1].as_string().unwrap().to_string_lossy(), "screen capture failed");
        assert_eq!((vm_budget(&lua).get(), process.get()), (0, 0));
    }

    #[test]
    fn over_budget_collects_twice_then_raises_naming_release() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        // 16 MiB each. Forty dropped at once are five times the budget, which only a collection
        // gives back.
        lua.load("for i = 1, 40 do local s = snap({ region = { 0, 0, 2048, 2048 } }) end").exec().unwrap();
        let e = err_text(lua.load("held = {} for i = 1, 40 do held[i] = snap({ region = { 0, 0, 2048, 2048 } }) end").exec());
        assert!(e.contains("host.screen.snapshot: this module's snapshots would hold 144.0 MiB; the limit is 128 MiB per module VM (512 MiB in the whole application)"), "{e}");
        assert!(e.contains("s:release()"), "{e}");
        let n: usize = lua.load("return #held").eval().unwrap();
        // Each is reserved at its estimate before it settles to 16 MiB: eight fit in 128 on
        // Windows, four on macOS, where the estimate is five times the pixels.
        let fit = (SNAPSHOT_BUDGET_PER_VM - (16 << 20) * ESTIMATE_FACTOR) / (16 << 20) + 1;
        assert_eq!(n, fit, "{fit} of 16 MiB fit in 128");
    }

    /// The application's limit is one counter across every VM: five VMs of 128 MiB would be 640.
    /// The charges are made directly, at 16 MiB each, so the estimate a platform reserves a
    /// snapshot at before it settles does not come into it.
    #[test]
    fn process_cap_spans_vms() {
        let process = Rc::new(Cell::new(0));
        let vms: Vec<Lua> = (0..5).map(|_| Lua::new()).collect();
        let mut held = Vec::new();
        let mut raised = None;
        'vms: for (i, lua) in vms.iter().enumerate() {
            for _ in 0..7 {
                match reserve(lua, &process, 16 << 20, "host.screen.snapshot") {
                    Ok(r) => held.push(r),
                    Err(e) => {
                        raised = Some((i, e.to_string()));
                        break 'vms;
                    }
                }
            }
        }
        let (i, e) = raised.expect("the fifth VM is refused");
        assert_eq!(i, 4, "four VMs of 112 MiB fit in 512");
        assert!(e.contains("the application's snapshots would hold 528.0 MiB; the limit is 512 MiB in the whole application"), "{e}");
        assert!(process.get() <= SNAPSHOT_BUDGET_PROCESS);
    }

    #[test]
    fn gc_kbytes_never_passes_the_heap() {
        assert_eq!(gc_kbytes(4 << 20, 100 << 20), 4096, "the reservation, when the heap is larger");
        assert_eq!(gc_kbytes(4 << 20, 1 << 20), 1024, "at most the heap");
        assert_eq!(gc_kbytes(100, 1 << 20), 0, "less than a kilobyte steps nothing");
        assert_eq!(gc_kbytes(usize::MAX, usize::MAX), i32::MAX, "never past what the call takes");
    }

    /// Reloading a module drops its VM: the snapshots it still held give their bytes back to the
    /// application's budget as the state is closed, without a release or a collection.
    #[test]
    fn dropping_the_vm_refunds_the_application_budget() {
        let process = Rc::new(Cell::new(0));
        let lua = Lua::new();
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        lua.load("keep1 = snap({ region = { 0, 0, 100, 100 } }) keep2 = snap({ region = { 0, 0, 50, 10 } })").exec().unwrap();
        assert_eq!(process.get(), 40_000 + 2_000);
        drop(lua);
        assert_eq!(process.get(), 0, "the closed state's snapshots are refunded");
    }

    /// S9: a poll that takes a 2 MiB snapshot ten times a second and never releases one stays
    /// under the budget without raising, paced by the collector steps — in a bare VM, and in one
    /// whose heap already holds about 20 MB, with 2 MiB and 8 MiB snapshots. Says what each call
    /// cost apart from its (fake) capture: the reservation, the collector's step and the handle,
    /// which is what a real snapshot adds to its capture.
    #[test]
    fn unreleased_10hz_poll_stays_under_the_cap() {
        for (ballast_mb, (w, h)) in [(0usize, (1024, 512)), (20, (1024, 512)), (20, (2048, 1024))] {
            let lua = Lua::new();
            let process = Rc::new(Cell::new(0));
            let captured = Rc::new(Cell::new(std::time::Duration::ZERO));
            let (p, c) = (process.clone(), captured.clone());
            let snap = lua
                .create_function(move |lua, opts: Value| {
                    take(lua, &p, 7, &opts, |r| {
                        let t = Instant::now();
                        let f = blank(r);
                        c.set(c.get() + t.elapsed());
                        Ok(f)
                    })
                })
                .unwrap();
            lua.globals().set("snap", snap).unwrap();
            if ballast_mb > 0 {
                // Strings of 1 KiB, as a module's tables of names and states would be: live data
                // the collector must traverse on every cycle.
                lua.load(format!(
                    "ballast = {{}} for i = 1, {} do ballast[i] = string.rep(string.char(65 + i % 26), 1000) .. i end",
                    ballast_mb * 1024
                ))
                .exec()
                .unwrap();
            }
            let heap = lua.used_memory();
            let poll = lua.load(format!("last = snap({{ region = {{ 0, 0, {w}, {h} }} }})")).into_function().unwrap();
            let (mut most, mut worst, mut other) = (0usize, std::time::Duration::ZERO, std::time::Duration::ZERO);
            let started = Instant::now();
            for _ in 0..300 {
                let before = captured.get();
                let t = Instant::now();
                poll.call::<()>(()).expect("a poll that never releases must not raise");
                let rest = t.elapsed().saturating_sub(captured.get() - before);
                other += rest;
                worst = worst.max(rest);
                most = most.max(vm_budget(&lua).get());
            }
            eprintln!(
                "S9: heap {:.1} MB, 300 unreleased {} MiB snapshots in {} ms; at most {:.1} MiB held; apart from the capture {:.2} ms a call on average, {:.2} ms at most",
                heap as f64 / 1e6,
                w * h * 4 >> 20,
                started.elapsed().as_millis(),
                most as f64 / (1 << 20) as f64,
                other.as_secs_f64() * 1000.0 / 300.0,
                worst.as_secs_f64() * 1000.0
            );
            assert!(most <= SNAPSHOT_BUDGET_PER_VM);
        }
    }

    #[test]
    fn crop_is_charged_and_inside_only() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        let (w, h, x, y, r): (i32, i32, i32, i32, i64) = lua
            .load("s = snap({ region = { 100, 50, 140, 80 } }) c = s:crop({ 110, 60, 120, 65 }) return c.w, c.h, c.x, c.y, c.time - s.time")
            .eval()
            .unwrap();
        assert_eq!((w, h, x, y, r), (10, 5, 110, 60, 0), "the same moment");
        assert_eq!(process.get(), 40 * 30 * 4 + 10 * 5 * 4, "charged anew");
        let got: (Value, String) = lua.load("return s:crop({ 130, 60, 150, 65 })").eval().unwrap();
        assert!(got.0.is_nil());
        assert_eq!(got.1, NOT_INSIDE);
        let win: (Value, String) = lua
            .load("return s:crop({ window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } })")
            .eval()
            .unwrap();
        assert_eq!(win.1, "the window's client area is empty (0x0)");
        let e = err_text(lua.load("return s:crop({ 110, 60 })").exec());
        assert!(e.contains("Snapshot:crop: the region.x2 is missing"), "{e}");
        let e = err_text(lua.load("return s:crop()").exec());
        assert!(e.contains("Snapshot:crop: the region must be a table"), "{e}");
    }

    #[test]
    fn tostring_forms() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        let t: String = lua.load("s = snap({ region = { 96, 120, 736, 600 } }) return tostring(s)").eval().unwrap();
        assert!(t.starts_with("Snapshot(640x480 at 96,120, gdi, 1.2 MiB, ") && t.ends_with(" ms old)"), "{t}");
        let t: String = lua.load("s:release() return tostring(s)").eval().unwrap();
        assert_eq!(t, "Snapshot(released)");
    }

    #[test]
    fn options_and_regions_raise_or_answer_by_form() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        for (src, want) in [
            ("return snap()", "host.screen.snapshot: opts must be a table { region = … }, got nothing"),
            ("return snap({})", "host.screen.snapshot: opts.region must be a table"),
            ("return snap({ region = { 0, 0, 10, 10 }, key = 'x' })", "host.screen.snapshot: opts has a key 'key'; it takes region"),
            ("return snap({ region = { 0, 0, 10.5, 10 } })", "opts.region.x2 must be a whole number, got 10.5"),
            ("return snap({ region = { 10, 0, 10, 10 } })", "is empty or turned around"),
            ("return snap({ region = { 0, 0, 10000, 5000 } })", "host.screen.snapshot: the region is 10000x5000, 50000000 pixels; the limit is 40000000"),
        ] {
            let e = err_text(lua.load(src).exec());
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
        let (v, why): (Value, String) = lua
            .load("return snap({ region = { window = { client = { x = 0, y = 0, w = 10000, h = 5000 } }, fraction = { 0, 0, 1, 1 } } })")
            .eval()
            .unwrap();
        assert!(v.is_nil());
        assert_eq!(why, "at the window's current size the region is 10000x5000, 50000000 pixels; the limit is 40000000");
        let (v, why): (Value, String) = lua
            .load("return snap({ region = { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } } })")
            .eval()
            .unwrap();
        assert!(v.is_nil());
        assert_eq!(why, "the window's client area is empty (0x0)");
        assert_eq!(process.get(), 0, "nothing reserved for what was not taken");
        // The window form, resolved at the call.
        let (x, y, w, h): (i32, i32, i32, i32) = lua
            .load("local s = snap({ region = { window = { client = { x = 100, y = 50, w = 200, h = 100 } }, fraction = { 0.5, 0, 1, 0.5 } } }) return s.x, s.y, s.w, s.h")
            .eval()
            .unwrap();
        assert_eq!((x, y, w, h), (200, 50, 100, 50));
    }

    /// The loosely read calls' defaults on a snapshot are its edges, and each call class's rule.
    #[test]
    fn loose_defaults_are_the_snapshots_edges() {
        let lua = Lua::new();
        let f = test_frame(100, 50, 640, 480);
        let v = |src: &str| -> Value { lua.load(src).eval().unwrap() };
        let rect = |x, y, w, h| ScreenRect { x, y, w, h };
        assert_eq!(loose_region(&Value::Nil, "f", "opts.region", &f).unwrap(), Ok(rect(100, 50, 640, 480)));
        assert_eq!(loose_region(&v("return { x1 = 300 }"), "f", "opts.region", &f).unwrap(), Ok(rect(300, 50, 440, 480)));
        let e = err_text(loose_region(&v("return {}"), "host.screen.profile", "opts.region", &f));
        assert!(e.starts_with("host.screen.profile: opts.region is neither"), "{e}");
    }

    #[test]
    fn clip_rule_per_call_class() {
        let f = test_frame(100, 50, 640, 480);
        let r = |x, y, w, h| ScreenRect { x, y, w, h };
        assert_eq!(clip(&f, r(0, 0, 200, 100)), Ok(Rect::new(100, 50, 100, 50)), "the part inside");
        assert_eq!(clip(&f, r(200, 100, 10, 10)), Ok(Rect::new(200, 100, 10, 10)));
        assert_eq!(clip(&f, r(0, 0, 50, 50)), Err(NO_OVERLAP.to_string()));
        assert_eq!(clip(&f, r(200, 100, 0, 0)), Err(NO_OVERLAP.to_string()), "an empty region overlaps nothing");
    }

    #[test]
    fn inside_rule_per_call_class() {
        let f = test_frame(100, 50, 640, 480);
        let r = |x, y, w, h| ScreenRect { x, y, w, h };
        assert_eq!(inside(&f, r(100, 50, 640, 480)), Ok(Rect::new(100, 50, 640, 480)));
        assert_eq!(inside(&f, r(0, 0, 200, 100)), Err(NOT_INSIDE.to_string()), "not cut, refused");
        assert_eq!(inside(&f, r(739, 529, 1, 1)), Ok(Rect::new(739, 529, 1, 1)));
        assert_eq!(inside(&f, r(739, 529, 2, 1)), Err(NOT_INSIDE.to_string()));
    }

    #[test]
    fn a_template_handle_or_released_snapshot_raises() {
        let lua = Lua::new();
        let other = lua.create_any_userdata(5i32).unwrap();
        let e = err_text(from_value(&Value::UserData(other), "host.screen.profile", "opts.snapshot"));
        assert_eq!(e, "host.screen.profile: opts.snapshot must be a Snapshot, got a userdata of another kind");
        let e = err_text(from_value(&Value::Boolean(true), "host.screen.profile", "opts.snapshot"));
        assert_eq!(e, "host.screen.profile: opts.snapshot must be a Snapshot, got a boolean");
        assert!(from_value(&Value::Nil, "f", "opts.snapshot").unwrap().is_none());
        let h = test_handle(&lua, test_frame(0, 0, 4, 4));
        assert!(from_value(&Value::UserData(h), "f", "opts.snapshot").unwrap().is_some());
        // pixel's and pixels' options are strict: `snapshot` and nothing else.
        let t: Value = lua.load("return { snap = 1 }").eval().unwrap();
        assert_eq!(err_text(strict_opts(&t, "host.screen.pixel")), "host.screen.pixel: opts has a key 'snap'; it takes snapshot");
        assert_eq!(
            err_text(strict_opts(&Value::Integer(3), "host.screen.pixel")),
            "host.screen.pixel: opts must be a table { snapshot = s }, got 3"
        );
        assert!(strict_opts(&lua.load("return {}").eval::<Value>().unwrap(), "f").unwrap().is_none());
    }

    fn pixels_of(lua: &Lua, points: &str, opts: &str) -> mlua::Result<Vec<Value>> {
        let p: Value = lua.load(points).eval()?;
        let o: Value = lua.load(opts).eval()?;
        read_pixels(lua, &p, &o, |at| Ok(at.iter().map(|&(x, y)| (x as u8, y as u8, 9)).collect())).map(|m| m.into_vec())
    }

    #[test]
    fn points_strict_forms_and_limits() {
        let lua = Lua::new();
        let got = pixels_of(&lua, "return { { 1, 2 }, { x = 3, y = 4 }, { window = { client = { x = 10, y = 20, w = 100, h = 100 } }, fraction = { 0.5, 0.5 } } }", "return nil").unwrap();
        assert_eq!(got.len(), 1, "one value when it looked");
        let Value::Table(list) = &got[0] else { panic!("a list") };
        let hex: Vec<String> = list.sequence_values::<Table>().map(|t| t.unwrap().get("hex").unwrap()).collect();
        assert_eq!(hex, ["#010209", "#030409", "#3C4609"], "in the order asked, the window point resolved");
        let empty = pixels_of(&lua, "return {}", "return nil").unwrap();
        let Value::Table(e) = &empty[0] else { panic!("a list") };
        assert_eq!(e.raw_len(), 0, "{{}} reads nothing");
        let min = pixels_of(&lua, "return { { 1, 2 }, { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0 } } }", "return nil").unwrap();
        assert!(min[0].is_nil());
        assert_eq!(min[1].as_string().unwrap().to_string_lossy(), "the window's client area is empty (0x0)");
        for (points, want) in [
            ("return 5", "host.screen.pixels: points must be a list of points { { x, y }, … }, got 5"),
            ("return { { 1, 2 }, n = 1 }", "points must be a list (1, 2, 3, …) with nothing else in it"),
            ("return { { 1, 2 }, { 1.5, 2 } }", "host.screen.pixels: points[2].x must be a whole number"),
            ("local t = {} for i = 1, 4097 do t[i] = { 0, 0 } end return t", "points has 4097 entries; the limit is 4096"),
        ] {
            let e = err_text(pixels_of(&lua, points, "return nil"));
            assert!(e.contains(want), "{points}:\n  got  {e}\n  want {want}");
        }
        let e = err_text(pixels_of(&lua, "return { { 1, 2 } }", "return { region = 1 }"));
        assert!(e.contains("host.screen.pixels: opts has a key 'region'; it takes snapshot"), "{e}");
        let max = pixels_of(&lua, "local t = {} for i = 1, 4096 do t[i] = { i, 0 } end return t", "return nil").unwrap();
        let Value::Table(m) = &max[0] else { panic!("a list") };
        assert_eq!(m.raw_len(), 4096);
    }

    #[test]
    fn pixels_from_a_snapshot_touch_nothing_and_need_every_point_inside() {
        let lua = Lua::new();
        lua.globals().set("s", test_handle(&lua, test_frame(10, 20, 30, 40))).unwrap();
        let read = |points: &str| -> Vec<Value> {
            let p: Value = lua.load(points).eval().unwrap();
            let o: Value = lua.load("return { snapshot = s }").eval().unwrap();
            read_pixels(&lua, &p, &o, |_| panic!("the screen was read")).unwrap().into_vec()
        };
        let got = read("return { { 10, 20 }, { 39, 59 } }");
        let Value::Table(list) = &got[0] else { panic!("a list") };
        let first: Table = list.raw_get(1).unwrap();
        assert_eq!((first.get::<u8>("r").unwrap(), first.get::<u8>("g").unwrap(), first.get::<u8>("b").unwrap()), (10, 20, 50));
        let miss = read("return { { 10, 20 }, { 40, 59 } }");
        assert!(miss[0].is_nil());
        assert_eq!(miss[1].as_string().unwrap().to_string_lossy(), POINT_NOT_INSIDE);
    }

    /// A live read of `live` that fails is `nil` and its reason, and one that answers for too
    /// few points is a failed capture, never a short list.
    #[test]
    fn a_failed_live_read_is_nil_and_why() {
        let lua = Lua::new();
        let p: Value = lua.load("return { { 1, 2 }, { 3, 4 } }").eval().unwrap();
        let got = read_pixels(&lua, &p, &Value::Nil, |_| Err("screen capture failed: desktop duplication could not answer — it is still opening".into()))
            .unwrap()
            .into_vec();
        assert!(got[0].is_nil());
        assert!(got[1].as_string().unwrap().to_string_lossy().ends_with("it is still opening"));
        let short = read_pixels(&lua, &p, &Value::Nil, |_| Ok(vec![(0, 0, 0)])).unwrap().into_vec();
        assert_eq!(short[1].as_string().unwrap().to_string_lossy(), "screen capture failed");
    }

    /// Every reason a snapshot call answers with is listed in the reference, word for word.
    #[test]
    fn the_snapshot_reasons_are_listed() {
        const DOC: &str = include_str!("../../../docs/api/screen.md");
        let fixed = [
            NOT_INSIDE,
            POINT_NOT_INSIDE,
            NO_OVERLAP,
            SUPERSEDED,
            WATCH_MISS,
            crate::ocr::change::NO_PICTURE,
            crate::ocr::snap_queue::ROUND_FAILED,
            crate::ocr::service::CAPTURE_PANICKED,
            CANCELLED,
        ];
        let counted = [owner_waits_ended(), owner_requests_ended(), all_waits_refused(), all_requests_refused()];
        for why in fixed.iter().map(|s| s.to_string()).chain(counted) {
            assert!(DOC.contains(&format!("`{why}`")), "docs/api/screen.md does not list `{why}`");
        }
        assert!(DOC.contains("`at the window's current size the region is …x…, … pixels; the limit is 40000000`"));
        assert!(DOC.contains("`at the window's current size the region is …x…, … pixels; the limit is 8294400`"));
        assert!(DOC.contains("`at the window's current size the change wait watches … pixels, fewer than its minPixels …`"));
        assert_eq!(too_few_watched(7, 9), "at the window's current size the change wait watches 7 pixels, fewer than its minPixels 9");
        assert!(DOC.contains("`at the window's current size this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`"));
        assert!(DOC.contains("`at the window's current size the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()`"));
    }

    // ── snapshotAsync ──────────────────────────────────────────────────────────────────────

    const A: Owner = Owner { idx: 1, gen: 5 };

    fn args(lua: &Lua, opts: &str) -> mlua::Result<AsyncArgs> {
        let v: Value = lua.load(opts).eval()?;
        parse_async(&v, 1000.0)
    }

    fn raises(lua: &Lua, opts: &str, want: &str) {
        match args(lua, opts) {
            Ok(_) => panic!("{opts} was accepted"),
            Err(e) => assert!(e.to_string().contains(want), "{opts}:\n  got  {e}\n  want {want}"),
        }
    }

    #[test]
    fn option_ranges_raise() {
        let lua = Lua::new();
        let ok = args(
            &lua,
            "return { region = { 0, 0, 10, 10 }, key = 'k', at = 999, change = { timeout = 2000, settle = 2000, tolerance = 0, minPixels = 1, watch = { { 2, 2, 4, 4 } } } }",
        )
        .unwrap();
        assert_eq!(ok.rect, Ok(Rect::new(0, 0, 10, 10)));
        assert_eq!((ok.key.as_deref(), ok.at_ms), (Some("k"), Some(999.0)));
        let c = ok.change.unwrap();
        assert_eq!(c.spec, ChangeSpec { tolerance: 0, min_pixels: 1, settle: Duration::from_secs(2), timeout: Duration::from_secs(2) });
        assert_eq!(c.watch, vec![Rect::new(2, 2, 2, 2)]);
        let d = args(&lua, "return { region = { 0, 0, 10, 10 }, change = {} }").unwrap().change.unwrap();
        assert_eq!(d.spec, ChangeSpec { tolerance: 16, min_pixels: 4, settle: Duration::ZERO, timeout: Duration::from_millis(500) }, "the defaults");
        assert!(d.watch.is_empty() && d.from.is_none());
        for (opts, want) in [
            ("return nil", "host.screen.snapshotAsync: opts must be a table { region = …, key?, at?, change? }, got nothing"),
            ("return { region = { 0, 0, 1, 1 }, keys = 'k' }", "opts has a key 'keys'; it takes region, key, at and change"),
            ("return {}", "host.screen.snapshotAsync: opts.region"),
            ("return { region = { 0, 0, 1, 1 }, key = '' }", "opts.key must be a non-empty string, got the string \"\""),
            ("return { region = { 0, 0, 1, 1 }, at = 3000.5 }", "opts.at is 3000.5, more than 2000 ms after host.now() (1000)"),
            ("return { region = { 0, 0, 1, 1 }, at = 'soon' }", "opts.at must be a time in host.now() milliseconds, got the string"),
            ("return { region = { 0, 0, 1, 1 }, at = 0/0 }", "opts.at must be a time in host.now() milliseconds"),
            ("return { region = { 0, 0, 1, 1 }, change = 5 }", "opts.change must be a table"),
            ("return { region = { 0, 0, 1, 1 }, change = { frm = 1 } }", "opts.change has a key 'frm'; it takes from, watch, timeout, tolerance, minPixels and settle"),
            ("return { region = { 0, 0, 1, 1 }, change = { timeout = 0 } }", "opts.change.timeout must be a whole number from 1 to 2000 (ms), got 0"),
            ("return { region = { 0, 0, 1, 1 }, change = { timeout = 2001 } }", "from 1 to 2000 (ms), got 2001"),
            ("return { region = { 0, 0, 1, 1 }, change = { timeout = 1.5 } }", "got 1.5"),
            ("return { region = { 0, 0, 1, 1 }, change = { tolerance = 256 } }", "opts.change.tolerance must be a whole number from 0 to 255, got 256"),
            ("return { region = { 0, 0, 1, 1 }, change = { minPixels = 0 } }", "opts.change.minPixels must be a whole number from 1 to 8294400, got 0"),
            ("return { region = { 0, 0, 1, 1 }, change = { settle = 600 } }", "opts.change.settle must be a whole number from 0 to 500 (the timeout), got 600"),
            ("return { region = { 0, 0, 1, 1 }, change = { timeout = 100, settle = 101 } }", "from 0 to 100 (the timeout), got 101"),
            ("return { region = { 0, 0, 1, 1 }, change = { from = 5 } }", "host.screen.snapshotAsync: opts.change.from must be a Snapshot, got 5"),
            ("return { region = { 0, 0, 1, 1 }, change = { watch = {} } }", "opts.change.watch holds 0 regions; it takes 1 to 16"),
            ("return { region = { 0, 0, 1, 1 }, change = { watch = { 1, 2, 3, 4 } } }", "this is one region — put it in braces"),
            ("return { region = { 0, 0, 1, 1 }, change = { watch = 'column' } }", "opts.change.watch must be a list of regions"),
            (
                "local w = {} for i = 1, 17 do w[i] = { 0, 0, 1, 1 } end return { region = { 0, 0, 1, 1 }, change = { watch = w } }",
                "opts.change.watch holds 17 regions; it takes 1 to 16",
            ),
            ("return { region = { 0, 0, 1, 1 }, change = { watch = { { 0, 0, 1 } } } }", "opts.change.watch[1]"),
        ] {
            raises(&lua, opts, want);
        }
        // A time in the past is not a mistake: the picture is taken at once.
        assert_eq!(args(&lua, "return { region = { 0, 0, 1, 1 }, at = -50 }").unwrap().at_ms, Some(-50.0));
    }

    /// A `watch` region that misses the region: the module's own corners on both sides are a
    /// mistake and raise; where a window decides it, the callback is told.
    #[test]
    fn watch_corners_without_overlap_raise_window_answered() {
        let lua = Lua::new();
        raises(
            &lua,
            "return { region = { 0, 0, 100, 100 }, change = { watch = { { 50, 50, 60, 60 }, { 200, 200, 300, 300 } } } }",
            "opts.change.watch[2] { 200, 200, 300, 300 } does not overlap the region",
        );
        let win = "{ window = { client = { x = 0, y = 0, w = 100, h = 100 } }, fraction = { 0, 0, 1, 1 } }";
        let a = args(&lua, &format!("return {{ region = {win}, change = {{ watch = {{ {{ 200, 200, 300, 300 }} }} }} }}")).unwrap();
        assert_eq!(a.rect, Err(WATCH_MISS.to_string()));
        let a = args(&lua, &format!("return {{ region = {{ 150, 150, 400, 400 }}, change = {{ watch = {{ {win} }} }} }}")).unwrap();
        assert_eq!(a.rect, Err(WATCH_MISS.to_string()), "a window watch beside corners is the window's doing");
        let empty = "{ window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } }";
        let a = args(&lua, &format!("return {{ region = {{ 0, 0, 100, 100 }}, change = {{ watch = {{ {empty} }} }} }}")).unwrap();
        assert_eq!(a.rect, Err("the window's client area is empty (0x0)".to_string()));
        let a = args(&lua, "return { region = { 0, 0, 100, 100 }, change = { watch = { { 90, 90, 120, 120 } } } }").unwrap();
        assert_eq!(a.change.unwrap().watch, vec![Rect::new(90, 90, 30, 30)], "cut to the region by the wait itself");
    }

    #[test]
    fn region_limits_raise_or_answer_by_form() {
        let lua = Lua::new();
        raises(&lua, "return { region = { 0, 0, 10000, 5000 } }", "opts.region: the region is 10000x5000, 50000000 pixels; the limit is 40000000");
        raises(&lua, "return { region = { 0, 0, 4000, 3000 }, change = {} }", "the region is 4000x3000, 12000000 pixels; the limit is 8294400");
        assert_eq!(args(&lua, "return { region = { 0, 0, 4000, 3000 } }").unwrap().rect, Ok(Rect::new(0, 0, 4000, 3000)));
        let a = args(&lua, "return { region = { window = { client = { x = 0, y = 0, w = 4000, h = 3000 } }, fraction = { 0, 0, 1, 1 } }, change = {} }")
            .unwrap();
        assert_eq!(a.rect, Err("at the window's current size the region is 4000x3000, 12000000 pixels; the limit is 8294400".to_string()));
        assert_eq!(reservation_for(Rect::new(0, 0, 100, 50), false), 100 * 50 * 4 * ESTIMATE_FACTOR);
        assert_eq!(reservation_for(Rect::new(0, 0, 100, 50), true), (ESTIMATE_FACTOR + 2) * 100 * 50 * 4, "3x on Windows, 7x on macOS");
    }

    /// A request waiting in `st` for `owner`, whose callback records what it is called with in
    /// the global `got`.
    fn add(st: &SnapState, lua: &Lua, owner: Owner, key: Option<&str>, change: bool) -> SnapId {
        let cb: Function = lua
            .load("got = got or {} return function(snap, why, info) table.insert(got, { snap = snap, why = why, info = info }) end")
            .eval()
            .unwrap();
        let id = st.next();
        st.pending.borrow_mut().insert(
            id,
            PendingSnap {
                id,
                lua: lua.clone(),
                cb: lua.create_registry_value(cb).unwrap(),
                scope: owner.idx,
                owner,
                prio: Priority::Background,
                key: key.map(str::to_string),
                res: None,
                change,
                cancel: Arc::new(AtomicBool::new(false)),
                asked: Instant::now(),
            },
        );
        id
    }

    fn flag(st: &SnapState, id: SnapId) -> bool {
        let pending = st.pending.borrow();
        let ready = st.ready.borrow();
        let p = pending.get(&id).or_else(|| ready.iter().map(|(p, _)| p).find(|p| p.id == id)).expect("known");
        p.cancel.load(Ordering::Acquire)
    }

    fn got(lua: &Lua) -> Table {
        lua.globals().get::<Option<Table>>("got").unwrap().unwrap_or_else(|| lua.create_table().unwrap())
    }

    fn deliver_all(st: &SnapState, done: Vec<SnapDone>, process: &Rc<Cell<usize>>) {
        let answers = st.take_answers(done, Instant::now());
        deliver(st, answers, &|_| true, process, &|_, e| panic!("a callback raised: {e}"));
    }

    fn done(id: SnapId, outcome: SnapOutcome) -> SnapDone {
        SnapDone { id, outcome, asked: Instant::now(), ended: Instant::now() }
    }

    #[test]
    fn callback_gets_snap_why_info() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let a = add(&st, &lua, A, None, false);
        let b = add(&st, &lua, A, None, true);
        let mut f = test_frame(10, 20, 30, 40);
        f.input_epoch = 3; // as the capture thread stamps it
        let frame = Arc::new(f);
        let rebased = ChangeInfo { changed: false, settled: false, rebased: true };
        deliver_all(
            &st,
            vec![
                done(a, SnapOutcome::Picture { frame: frame.clone(), frames: 1, change: None }),
                done(b, SnapOutcome::Failed { why: "screen capture failed".into(), frames: 3, change: Some(rebased) }),
            ],
            &process,
        );
        lua.globals().set("g", got(&lua)).unwrap();
        let ok: bool = lua
            .load(
                r#"return #g == 2
                  and g[1].snap.w == 30 and g[1].snap.x == 10 and g[1].snap.inputEpoch == 3 and g[1].why == nil
                  and g[1].info.frames == 1 and g[1].info.waited >= 0 and g[1].info.changed == nil
                  and g[2].snap == nil and g[2].why == "screen capture failed" and g[2].info.frames == 3
                  and g[2].info.changed == false and g[2].info.settled == false and g[2].info.rebased == true"#,
            )
            .eval()
            .unwrap();
        assert!(ok);
        assert_eq!(process.get(), frame.bytes(), "the handle is charged what it holds");
        assert!(!st.has_pending());
        deliver_all(&st, vec![done(a, SnapOutcome::Picture { frame, frames: 1, change: None })], &process);
        assert_eq!(got(&lua).raw_len(), 2, "exactly once: an answer for a request already answered is let go");
    }

    #[test]
    fn a_wait_reserves_3x_then_settles() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let r = Rect::new(0, 0, 100, 50);
        let id = add(&st, &lua, A, None, true);
        let res = reserve(&lua, &process, reservation_for(r, true), "t").unwrap();
        st.pending.borrow_mut().get_mut(&id).unwrap().res = Some(res);
        assert_eq!(process.get(), (ESTIMATE_FACTOR + 2) * 20_000);
        let frame = Arc::new(test_frame(0, 0, 100, 50));
        let change = Some(ChangeInfo { changed: true, settled: true, rebased: false });
        deliver_all(&st, vec![done(id, SnapOutcome::Picture { frame, frames: 7, change })], &process);
        assert_eq!(process.get(), 20_000, "settled to what the picture holds");
        assert_eq!(vm_budget(&lua).get(), 20_000);
        // A wait that ends without a picture gives its whole reservation back.
        let id = add(&st, &lua, A, None, true);
        st.pending.borrow_mut().get_mut(&id).unwrap().res = Some(reserve(&lua, &process, reservation_for(r, true), "t").unwrap());
        deliver_all(&st, vec![done(id, SnapOutcome::Failed { why: "no".into(), frames: 0, change })], &process);
        assert_eq!(process.get(), 20_000);
    }

    #[test]
    fn delivery_dropped_for_disabled_owner_and_stale_gen() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let gens: HashMap<usize, u64> = [(1, 5), (2, 9)].into_iter().collect();
        let enabled = [true, true, false];
        let alive = |o: Owner| crate::ocr::lua::deliverable(o, &gens, &enabled);
        let live = add(&st, &lua, A, None, false);
        let stale = add(&st, &lua, Owner { idx: 1, gen: 4 }, None, false);
        let off = add(&st, &lua, Owner { idx: 2, gen: 9 }, None, false);
        for id in [live, stale, off] {
            st.pending.borrow_mut().get_mut(&id).unwrap().res = Some(reserve(&lua, &process, 400, "t").unwrap());
        }
        let pic = |id| done(id, SnapOutcome::Picture { frame: Arc::new(test_frame(0, 0, 10, 10)), frames: 1, change: None });
        let answers = st.take_answers(vec![pic(live), pic(stale), pic(off)], Instant::now());
        deliver(&st, answers, &alive, &process, &|_, e| panic!("{e}"));
        assert_eq!(got(&lua).raw_len(), 1, "only the enabled VM that asked is called");
        assert_eq!(process.get(), 400, "the dropped ones gave their bytes back");
    }

    /// A newer request with the same key, from the same VM, answers the older one on the next
    /// tick — even one whose picture was already taken — and nothing else.
    #[test]
    fn key_supersedes_with_a_callback_on_the_next_tick() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let old = add(&st, &lua, A, Some("press"), true);
        let other_key = add(&st, &lua, A, Some("bubble"), true);
        let other_vm = add(&st, &lua, Owner { idx: 1, gen: 6 }, Some("press"), true);
        assert_eq!(st.supersede(A, "press"), 1);
        assert!(flag(&st, old), "the thread is told to stop photographing it");
        assert!(!flag(&st, other_key) && !flag(&st, other_vm));
        assert_eq!(got(&lua).raw_len(), 0, "not from inside the call");
        let late = done(old, SnapOutcome::Picture { frame: Arc::new(test_frame(0, 0, 4, 4)), frames: 2, change: None });
        deliver_all(&st, vec![late], &process);
        lua.globals().set("g", got(&lua)).unwrap();
        let ok: bool = lua
            .load(r#"return #g == 1 and g[1].snap == nil and g[1].why == "a newer request with the same key replaced this one" and g[1].info.changed == false"#)
            .eval()
            .unwrap();
        assert!(ok, "answered once, superseded — the picture that came late is let go");
        assert_eq!(process.get(), 0);
    }

    /// A request a newer one with its key ended, or a limit ended, gives its bytes back at once —
    /// before its answer is delivered — so a burst of presses holds at most the newest request's
    /// charge and the one being made.
    #[test]
    fn an_ended_request_gives_its_bytes_back_at_once() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let r = Rect::new(0, 0, 100, 50);
        let old = add(&st, &lua, A, Some("press"), true);
        st.pending.borrow_mut().get_mut(&old).unwrap().res = Some(reserve(&lua, &process, reservation_for(r, true), "t").unwrap());
        let charged = process.get();
        assert!(charged > 0);
        assert_eq!(st.supersede(A, "press"), 1);
        assert_eq!((process.get(), vm_budget(&lua).get()), (0, 0), "given back before any delivery");
        deliver_all(&st, Vec::new(), &process);
        assert_eq!(got(&lua).raw_len(), 1, "and still answered once");
        // The same for the oldest request a module's limit ends.
        let waits: Vec<SnapId> = (0..SNAP_WAITS_PER_OWNER).map(|_| add(&st, &lua, A, None, true)).collect();
        st.pending.borrow_mut().get_mut(&waits[0]).unwrap().res = Some(reserve(&lua, &process, 4000, "t").unwrap());
        assert_eq!(process.get(), 4000);
        st.make_room(A, true);
        assert_eq!(process.get(), 0);
    }

    /// A newer request with a key, made by a callback earlier in the same delivery batch,
    /// replaces an older answer with that key still waiting in the batch.
    #[test]
    fn a_newer_key_made_in_the_same_batch_replaces_the_older_answer() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let x = add(&st, &lua, A, None, false);
        let w1 = add(&st, &lua, A, Some("k"), true);
        st.note_key(&st.pending.borrow()[&w1]);
        let other_vm = add(&st, &lua, Owner { idx: 1, gen: 6 }, Some("k"), false);
        st.note_key(&st.pending.borrow()[&other_vm]);
        let pic = |id| done(id, SnapOutcome::Picture { frame: Arc::new(test_frame(0, 0, 4, 4)), frames: 2, change: None });
        let answers = st.take_answers(vec![pic(x), pic(w1), pic(other_vm)], Instant::now());
        // X's callback asks again with the key: a request of W1's module VM, newer than W1.
        let newer = add(&st, &lua, A, Some("k"), true);
        st.note_key(&st.pending.borrow()[&newer]);
        deliver(&st, answers, &|_| true, &process, &|_, e| panic!("{e}"));
        lua.globals().set("g", got(&lua)).unwrap();
        let ok: bool = lua
            .load(r#"return #g == 3 and g[1].snap ~= nil and g[2].snap == nil and g[2].why == "a newer request with the same key replaced this one" and g[3].snap ~= nil"#)
            .eval()
            .unwrap();
        assert!(ok, "W1 superseded, another VM's key untouched");
        assert_eq!(st.newest_key.borrow().len(), 1, "only the waiting newest is remembered");
        assert!(st.newest_key.borrow().values().all(|id| *id == newer));
        st.drop_owner(A.idx).into_iter().for_each(release);
        assert!(st.newest_key.borrow().is_empty(), "a dropped module's keys are forgotten");
    }

    #[test]
    fn owner_caps_end_the_oldest() {
        let lua = Lua::new();
        let st = SnapState::default();
        let waits: Vec<SnapId> = (0..SNAP_WAITS_PER_OWNER).map(|_| add(&st, &lua, A, None, true)).collect();
        let (ended, refused) = st.make_room(A, true);
        assert_eq!((ended, refused), (vec![owner_waits_ended()], None));
        assert!(flag(&st, waits[0]) && !flag(&st, waits[1]), "the oldest wait");
        let (ended, _) = st.make_room(A, false);
        assert!(ended.is_empty(), "a plain request does not count against the waits");
        let more = SNAP_PER_OWNER - (SNAP_WAITS_PER_OWNER - 1);
        for _ in 0..more {
            add(&st, &lua, A, None, false);
        }
        let (ended, refused) = st.make_room(A, false);
        assert_eq!((ended, refused), (vec![owner_requests_ended()], None));
        assert!(flag(&st, waits[1]), "the oldest request of any kind");
        let (ended, _) = st.make_room(Owner { idx: 2, gen: 1 }, true);
        assert!(ended.is_empty(), "another module's share is its own");
    }

    #[test]
    fn process_busy_answers_without_raising() {
        let lua = Lua::new();
        let st = SnapState::default();
        for i in 0..SNAP_WAITS {
            add(&st, &lua, Owner { idx: 10 + i / 2, gen: 1 }, None, true);
        }
        let newcomer = Owner { idx: 99, gen: 1 };
        assert_eq!(st.make_room(newcomer, true), (Vec::new(), Some(all_waits_refused())));
        assert_eq!(st.make_room(newcomer, false), (Vec::new(), None), "a plain one still fits");
        for i in 0..(SNAP_TOTAL - SNAP_WAITS) {
            add(&st, &lua, Owner { idx: 30 + i / 8, gen: 1 }, None, false);
        }
        assert_eq!(st.make_room(newcomer, false), (Vec::new(), Some(all_requests_refused())));
    }

    /// A request the application's limit refuses ends nothing of its module's: counted with
    /// the slots the module's own ends would free, and when it is refused all the same, the
    /// module keeps its oldest.
    #[test]
    fn a_refused_request_ends_nothing_of_its_module() {
        let lua = Lua::new();
        let st = SnapState::default();
        // The module holds its 16, none a wait; the other modules every wait slot.
        let mine: Vec<SnapId> = (0..SNAP_PER_OWNER).map(|_| add(&st, &lua, A, None, false)).collect();
        for i in 0..SNAP_WAITS {
            add(&st, &lua, Owner { idx: 10 + i / 2, gen: 1 }, None, true);
        }
        assert_eq!(st.make_room(A, true), (Vec::new(), Some(all_waits_refused())), "no wait slot comes free");
        assert!(mine.iter().all(|id| !flag(&st, *id)), "and nothing of the module's was ended for it");
        // A plain one takes the slot its module's oldest frees.
        assert_eq!(st.make_room(A, false), (vec![owner_requests_ended()], None));
        assert!(flag(&st, mine[0]));
        // The application full: the module's own end would free one slot, which is enough.
        let st = SnapState::default();
        let mine: Vec<SnapId> = (0..SNAP_PER_OWNER).map(|_| add(&st, &lua, A, None, false)).collect();
        for i in 0..(SNAP_TOTAL - SNAP_PER_OWNER) {
            add(&st, &lua, Owner { idx: 30 + i / 8, gen: 1 }, None, false);
        }
        assert_eq!(st.make_room(A, false), (vec![owner_requests_ended()], None), "its own end makes the room");
        assert!(flag(&st, mine[0]));
        // Its oldest wait ended frees a wait slot, counted before refusing.
        let st = SnapState::default();
        let waits: Vec<SnapId> = (0..SNAP_WAITS_PER_OWNER).map(|_| add(&st, &lua, A, None, true)).collect();
        for i in 0..(SNAP_WAITS - SNAP_WAITS_PER_OWNER) {
            add(&st, &lua, Owner { idx: 10 + i / 2, gen: 1 }, None, true);
        }
        assert_eq!(st.make_room(A, true), (vec![owner_waits_ended()], None));
        assert!(flag(&st, waits[0]));
    }

    /// The budget for a window region, whose size is the window's at run time, is answered like
    /// the size limit; the same region given as corners raises. `snapshot` and `crop` alike.
    #[test]
    fn a_window_region_past_the_budget_is_answered_corners_raise() {
        let lua = Lua::new();
        let process = Rc::new(Cell::new(0));
        lua.globals().set("snap", snap_fn(&lua, &process)).unwrap();
        lua.load("s = snap({ region = { 0, 0, 100, 100 } })").exec().unwrap();
        // Everything but 1000 bytes of the module's budget is taken.
        let _hold = reserve(&lua, &process, SNAPSHOT_BUDGET_PER_VM - vm_budget(&lua).get() - 1000, "t").unwrap();
        let before = process.get();
        let win = "{ window = { client = { x = 0, y = 0, w = 100, h = 100 } }, fraction = { 0, 0, 1, 1 } }";
        let (v, why): (Value, String) = lua.load(format!("return snap({{ region = {win} }})")).eval().unwrap();
        assert!(v.is_nil());
        assert!(why.starts_with("at the window's current size this module's snapshots would hold "), "{why}");
        assert!(why.ends_with("Release the ones you no longer need with s:release()"), "{why}");
        let (v, why): (Value, String) = lua.load(format!("return s:crop({win})")).eval().unwrap();
        assert!(v.is_nil());
        assert!(why.starts_with("at the window's current size this module's snapshots would hold "), "{why}");
        assert_eq!(process.get(), before, "nothing charged for what was answered");
        let e = err_text(lua.load("return snap({ region = { 0, 0, 100, 100 } })").exec());
        assert!(e.contains("host.screen.snapshot: this module's snapshots would hold "), "{e}");
        let e = err_text(lua.load("return s:crop({ 0, 0, 100, 100 })").exec());
        assert!(e.contains("Snapshot:crop: this module's snapshots would hold "), "{e}");
        // snapshotAsync keeps the form it was given for the same decision.
        assert!(matches!(args(&lua, &format!("return {{ region = {win} }}")).unwrap().form, region::Region::Window(..)));
        assert!(matches!(args(&lua, "return { region = { 0, 0, 10, 10 } }").unwrap().form, region::Region::Rect(_)));
    }

    /// A change wait that watches fewer pixels than `minPixels` could never see a change: its
    /// own corners raise; where a window decides it, the callback is told.
    #[test]
    fn min_pixels_past_the_watched_pixels_raises_or_is_answered() {
        let lua = Lua::new();
        raises(
            &lua,
            "return { region = { 0, 0, 100, 100 }, change = { watch = { { 10, 10, 11, 11 } } } }",
            "host.screen.snapshotAsync: opts.change.minPixels is 4, more than the 1 pixel(s) the wait watches (opts.change.watch cut to the region, a shared pixel once), so it could never see a change",
        );
        raises(
            &lua,
            "return { region = { 0, 0, 2, 1 }, change = { minPixels = 3 } }",
            "opts.change.minPixels is 3, more than the 2 pixel(s) the wait watches (the whole region)",
        );
        // Cut to the region ({ 98, 0, 100, 1 } and { 99, 0, 100, 1 }), a shared pixel once: 2.
        raises(&lua, "return { region = { 0, 0, 100, 1 }, change = { watch = { { 98, 0, 100, 1 }, { 99, 0, 101, 1 } } } }", "more than the 2 pixel(s)");
        assert!(args(&lua, "return { region = { 0, 0, 100, 100 }, change = { minPixels = 1, watch = { { 10, 10, 11, 11 } } } }").is_ok());
        assert!(args(&lua, "return { region = { 0, 0, 2, 2 }, change = {} }").is_ok(), "four pixels, four needed");
        let win = "{ window = { client = { x = 0, y = 0, w = 2, h = 1 } }, fraction = { 0, 0, 1, 1 } }";
        let a = args(&lua, &format!("return {{ region = {win}, change = {{}} }}")).unwrap();
        assert_eq!(a.rect, Err(too_few_watched(2, 4)));
        let a = args(&lua, &format!("return {{ region = {{ 0, 0, 100, 100 }}, change = {{ watch = {{ {win} }} }} }}")).unwrap();
        assert_eq!(a.rect, Err(too_few_watched(2, 4)), "a window's watch beside corners");
    }

    #[test]
    fn dropping_a_module_cancels_its_requests_and_answers_none() {
        let lua = Lua::new();
        let st = SnapState::default();
        let process = Rc::new(Cell::new(0));
        let mine = add(&st, &lua, A, Some("k"), false);
        let mine2 = add(&st, &lua, A, Some("k"), false);
        let theirs = add(&st, &lua, Owner { idx: 4, gen: 1 }, None, false);
        st.supersede(A, "k"); // both of A's now answered on the next tick
        let _ = mine2;
        let fresh = add(&st, &lua, A, None, false);
        let flags: Vec<Arc<AtomicBool>> = [fresh].iter().map(|id| st.pending.borrow()[id].cancel.clone()).collect();
        assert_eq!(st.owners_from(2), vec![4]);
        let gone = st.drop_owner(A.idx);
        assert_eq!(gone.len(), 3, "the waiting one and the two answers not delivered yet");
        assert!(flags.iter().all(|f| f.load(Ordering::Acquire)));
        assert!(!st.has_pending_for(A.idx) && st.has_pending_for(4));
        for p in gone {
            release(p);
        }
        deliver_all(&st, vec![done(mine, SnapOutcome::Cancelled)], &process);
        assert_eq!(got(&lua).raw_len(), 0, "nothing of A's is called");
        assert!(st.has_pending(), "B's still waits");
        let _ = theirs;
    }
}
