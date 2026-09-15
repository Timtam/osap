//! Screen reading: the size of the display, one pixel, and a region as an image.
//!
//! The unit rule lives here. A capture is asked for in points and must come back as
//! **exactly** that many pixels — `w * h * 4` bytes of tightly packed top-down RGBA, alpha
//! forced opaque — because the image matcher indexes the buffer arithmetically and the host
//! adds a match offset straight back onto the requested origin. On a Retina display the
//! backing store has four times as many pixels as that, so the image is downsampled here
//! and nowhere else.
//!
//! The downsample is not a resampling pass over the returned bytes: the destination bitmap
//! is created at exactly the requested size in points and the captured image is drawn into
//! it. That makes `cap.w == w`, `cap.h == h` and `rgba.len() == w * h * 4` structural
//! properties of the buffer rather than something to check and hope for, and it costs one
//! blit that a Retina machine would have had to pay anyway. The scale factor never appears
//! in the arithmetic, which is exactly why it cannot be got wrong; where it is needed (the
//! OCR path wants the sharp image) it is derived from the image the window server actually
//! returned rather than asked for and cached, because a laptop docked to an external
//! display changes it mid-session.
//!
//! The failure mode this file spends the most code on is not a failure at all as far as
//! macOS is concerned: without Screen Recording permission a capture succeeds and returns a
//! picture of the desktop wallpaper with every other application's window removed. Nothing
//! errors, nothing is empty, and the only symptom is that image search and OCR stop
//! matching. A blind tester would report "it does not find anything" forever. So the
//! permission is preflighted the first time anything is captured and again when several
//! captures in a row come back a single flat colour, and either way the log says so in
//! words the user can act on — once, not per call.

use core::ffi::{c_void, CStr};
use core::ptr::NonNull;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::mpsc::channel;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use objc2::runtime::AnyClass;
use objc2::sel;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::SCScreenshotManager;
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDirectDisplayID, CGDisplayBounds, CGError,
    CGGetDisplaysWithPoint, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGGetActiveDisplayList, CGInterpolationQuality, CGMainDisplayID, CGPreflightScreenCaptureAccess,
    CGWindowImageOption, CGWindowListOption,
};

// WHERE A CAPTURE COMES FROM, in order: ScreenCaptureKit, then the two older functions —
// and the older two are never LINKED, only looked up by name when they are needed.
//
// `CGWindowListCreateImage` and `CGDisplayCreateImageForRect` are marked obsoleted in the macOS
// 15 SDK; C and Objective-C code no longer compiles against them. This file used them anyway
// for as long as they ran, because they are synchronous and ScreenCaptureKit is not. What
// ended that is not their behaviour but their existence: a symbol this binary imports and the
// running macOS no longer exports does not fail a capture, it stops the application from
// launching at all, before a line of the log is written — which, on a remote Mac with a blind
// tester, is an hour with nothing in it. So they are resolved with `dlsym` at the first
// capture that falls back to them, and a macOS without them simply answers "absent".
//
// `+[SCScreenshotManager captureImageInRect:completionHandler:]` (macOS 15.2) is the direct
// replacement: a rectangle in points, in the same display-agnostic space the window-list
// function took, and a `CGImage` back. It answers through a completion handler, and this file
// turns that back into a straight answer by waiting on a channel with a deadline. Whether the
// handler can arrive while the waiting thread is the main thread is not documented anywhere;
// the CI job's capture probe asks exactly that on a Mac, and a capture that does not answer in
// time switches ScreenCaptureKit off for the session rather than stalling every capture after
// it.

use crate::backend::CapturedImage;

/// How much of the screen a `pixel()` read grabs, in points, and where the asked-for point
/// sits inside it.
///
/// Sized from the one module that makes this method hurt: Melodyne's tool bar is read with
/// up to eight `pixel()` calls per keystroke (`modules/melodyne/src/main.luau`, `activeTool`
/// + `activeVariant`), and every one of those points lies within x 84..200, y 59..72 of the
/// window's client origin. A tile that reaches 96 points left and 32 points up from the
/// first of them covers all eight, so a keystroke costs one round trip to the window server
/// instead of eight. Kontakt's two probes are far apart and gain nothing, which is fine —
/// they cost exactly what they cost today.
const TILE_W: i32 = 256;
const TILE_H: i32 = 96;
const TILE_BACK_X: i32 = 96;
const TILE_BACK_Y: i32 = 32;

/// How long a tile may answer for.
///
/// Short enough that no tile survives from one 15 ms pump tick to the next, which is the
/// blunt half of the argument. The sharper half: the callers that must not be answered from
/// a cache are the ones that have just driven input and want to see the result — the host
/// bumps its input epoch for exactly that reason. But a click is a `CGEvent` posted to
/// another process, which then has to notice it and repaint, and that is at minimum one
/// compositor frame away. A capture taken 5 ms after a click shows the same pre-click screen
/// the cache does, so the cache adds no staleness the caller was not already going to get.
/// Anything on the order of a frame would not be true, which is why this is milliseconds.
const TILE_TTL: Duration = Duration::from_millis(5);

/// Flat captures in a row before the permission is questioned. More than one because a
/// legitimately uniform region does occur — a plugin's flat panel background — and fewer
/// than this would cry wolf at it.
const FLAT_RUN_ALARM: u32 = 4;

/// Refuse a request bigger than this many points in total rather than trying to allocate
/// four bytes for each of them. 40 M points is well past any real display and still only
/// 160 MB, so nothing legitimate is refused and a nonsense request cannot abort the process
/// on a failed allocation.
const MAX_POINTS: i64 = 40_000_000;

static PERMISSION_PREFLIGHTED: AtomicBool = AtomicBool::new(false);
static PERMISSION_WARNED: AtomicBool = AtomicBool::new(false);
static FLAT_WARNED: AtomicBool = AtomicBool::new(false);
static FLAT_RUN: AtomicU32 = AtomicU32::new(0);
static FIRST_CAPTURE_REPORTED: AtomicBool = AtomicBool::new(false);
static SLOW_CAPTURE_REPORTED: AtomicBool = AtomicBool::new(false);
/// The scale of the first capture OCR reads, said once. See `capture_backing`.
static BACKING_REPORTED: AtomicBool = AtomicBool::new(false);
static BACKING_TRIM_REPORTED: AtomicBool = AtomicBool::new(false);
/// Said once: a capture reached past the edge of the desktop and was placed rather than
/// stretched. Worth knowing, because it usually means a module's region arithmetic is off.
static CLIPPED_REPORTED: AtomicBool = AtomicBool::new(false);
static FALLBACK_REPORTED: AtomicBool = AtomicBool::new(false);
static FAILURE_REPORTED: AtomicBool = AtomicBool::new(false);
/// Which capture paths exist on this macOS, said once, at the first capture.
static PATHS_REPORTED: AtomicBool = AtomicBool::new(false);
/// Which path first delivered an image, one bit per path, so each is named once.
static PATH_USED: AtomicU32 = AtomicU32::new(0);
static SCK_ERROR_REPORTED: AtomicBool = AtomicBool::new(false);
/// ScreenCaptureKit did not answer within `SCK_TIMEOUT` once, and is not asked again this
/// session: a capture that stalls is paid once, not on every keystroke that reads the screen.
static SCK_DISABLED: AtomicBool = AtomicBool::new(false);
static NO_CAPTURE_REPORTED: AtomicBool = AtomicBool::new(false);

/// How long a ScreenCaptureKit capture may take before it is given up on.
///
/// A capture that returns normally costs tens of milliseconds; this bound is for the one that
/// never returns, and it is the whole cost of finding that out — paid once, because the first
/// timeout switches ScreenCaptureKit off for the session. A pump stalled this long can get the
/// event tap switched off by the system; tap.rs re-enables it on the notice the system sends,
/// so that too costs one line in the log rather than a dead overlay.
const SCK_TIMEOUT: Duration = Duration::from_millis(1500);
static OVERSIZE_REPORTED: AtomicBool = AtomicBool::new(false);

/// Last reported primary-display size, so a change is a log line and a repeat is not.
static LAST_SCREEN_W: AtomicI32 = AtomicI32::new(0);
static LAST_SCREEN_H: AtomicI32 = AtomicI32::new(0);

/// Primary display, in points.
///
/// `CGDisplayBounds` is already in points with the primary display's top-left at (0,0),
/// which is the platform's coordinate space, so nothing is converted here. Deliberately not
/// `CGDisplayPixelsWide`: despite the name that one also reports points, and relying on a
/// function whose name says the opposite of what it does is how the units get lost.
///
/// Only the default region for a Lua caller that omitted one — never a clipping bound. The
/// other three methods here work outside it, on secondary displays and at negative
/// coordinates, because the host uses both.
pub fn screen_size() -> (i32, i32) {
    let bounds = CGDisplayBounds(CGMainDisplayID());
    let (w, h) = (bounds.size.width as i32, bounds.size.height as i32);

    // A changed primary display size is worth a line rather than a trace: it is what happens
    // when the tester docks the laptop, and it moves every coordinate a module ever measured.
    // Both swaps run before the test, deliberately — short-circuiting past the second would
    // leave it holding the old height and report the change twice.
    let was_w = LAST_SCREEN_W.swap(w, Ordering::Relaxed);
    let was_h = LAST_SCREEN_H.swap(h, Ordering::Relaxed);
    if was_w != w || was_h != h {
        crate::logging::line("macos", &format!("primary display is {w}x{h} points"));
    }
    if w <= 0 || h <= 0 {
        crate::logging::trace("macos", || {
            format!("CGDisplayBounds gave a degenerate primary display: {bounds:?}")
        });
    }
    (w, h)
}

/// A tile of the screen, and when it was taken.
struct Tile {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    rgba: Vec<u8>,
    taken: Instant,
}

thread_local! {
    /// The last region a `pixel()` read grabbed. Thread-local rather than shared: the only
    /// caller is the pump thread, and a lock in the pump to serve a thread that is not
    /// contending for it would be pure cost.
    static TILE: RefCell<Option<Tile>> = const { RefCell::new(None) };
}

/// What is composited at that point right now.
///
/// The same caveat the Windows implementation records applies unchanged, because it is a
/// property of compositing rather than of an API: right after a focus change this still
/// reads the window that used to be there. Callers re-ask; nothing here tries to hide it.
///
/// This is the expensive method on every platform and there is no cheap version of it — a
/// point on the screen can only be read by capturing the screen. Windows pays a measured
/// fixed ~16.7 ms per read, one compositor frame, regardless of size. The macOS cost is a
/// synchronous round trip to the window server and is **unmeasured**; the first capture of a
/// session is timed into the log so the tester's log answers it. What this can do is ask
/// fewer times: a read grabs a tile rather than a point and the next few reads inside it are
/// free, which turns Melodyne's eight probes per keystroke into one round trip.
///
/// No failure value exists — `(0,0,0)` is a real black pixel — so a failure is logged rather
/// than signalled.
pub fn pixel(x: i32, y: i32) -> (u8, u8, u8) {
    if let Some(px) = TILE.with(|cell| {
        let tile = cell.borrow();
        let t = tile.as_ref()?;
        if t.taken.elapsed() > TILE_TTL
            || x < t.x
            || y < t.y
            || x >= t.x + t.w
            || y >= t.y + t.h
        {
            return None;
        }
        sample(&t.rgba, t.w, x - t.x, y - t.y)
    }) {
        crate::logging::trace("macos", || {
            format!("pixel({x},{y}) = {},{},{} from the held tile", px.0, px.1, px.2)
        });
        return px;
    }

    let (tx, ty, tw, th) = tile_rect(x, y);
    if let Some(rgba) = capture_rgba(tx, ty, tw, th) {
        if let Some(px) = sample(&rgba, tw, x - tx, y - ty) {
            TILE.with(|cell| {
                *cell.borrow_mut() = Some(Tile {
                    x: tx,
                    y: ty,
                    w: tw,
                    h: th,
                    rgba,
                    taken: Instant::now(),
                });
            });
            crate::logging::trace("macos", || {
                format!(
                    "pixel({x},{y}) = {},{},{} from a fresh {tw}x{th} tile at {tx},{ty}",
                    px.0, px.1, px.2
                )
            });
            return px;
        }
    }
    // The tile is gone either way — a stale one must not answer for a point it never covered.
    TILE.with(|cell| *cell.borrow_mut() = None);

    // A tile can fail where a single point does not: it reaches past the edge of the display
    // and the window server clips it, which `grab` refuses because a clipped image drawn into
    // an unclipped destination is a silent geometry lie. One point cannot be clipped.
    if let Some(rgba) = capture_rgba(x, y, 1, 1) {
        if let Some(px) = sample(&rgba, 1, 0, 0) {
            crate::logging::trace("macos", || {
                format!("pixel({x},{y}) = {},{},{} from a single-point read", px.0, px.1, px.2)
            });
            return px;
        }
    }

    if !FAILURE_REPORTED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "could not read the screen at {x},{y} — pixel reads will report black until this \
                 clears. Check the Screen Recording permission; this is logged once."
            ),
        );
    }
    crate::logging::trace("macos", || format!("pixel({x},{y}) failed; reporting black"));
    (0, 0, 0)
}

/// A region as an image. A bare `fn` on purpose: the image worker calls it from another
/// thread, without the backend.
///
/// Nothing in the path below needs a run loop or any state of ours on the calling thread:
/// ScreenCaptureKit answers on a queue of its own and this thread waits on a channel for it,
/// the older functions are synchronous window-server calls, `CGImage` is `Send + Sync` in
/// these bindings, and the bitmap context is created, used and dropped inside one call. The
/// tile cache that `pixel()` keeps is thread-local, so a worker-thread capture neither reads
/// nor disturbs it.
pub fn capture_region(x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
    if w <= 0 || h <= 0 {
        crate::logging::trace("macos", || {
            format!("capture({x},{y},{w},{h}) refused: a region must have both dimensions")
        });
        return None;
    }
    if w as i64 * h as i64 > MAX_POINTS {
        if !OVERSIZE_REPORTED.swap(true, Ordering::Relaxed) {
            crate::logging::line(
                "macos",
                &format!("capture({x},{y},{w},{h}) refused: {} points is past anything a display \
                          holds, and allocating for it would take the process down", w as i64 * h as i64),
            );
        }
        return None;
    }

    let rgba = capture_rgba(x, y, w, h)?;

    // Only here, and not in the pixel path: a region asked for by name is one a module means
    // to search or read, and a flat one is a symptom. A pixel probe's tile is very often
    // deliberately flat — Kontakt's panel toggles read a plain background — so watching those
    // would raise the alarm on the thing working correctly.
    watch_for_blank(&rgba, w, h);

    Some(CapturedImage {
        w: w as u32,
        h: h as u32,
        rgba,
    })
}

/// The same capture at the resolution the display actually has, plus the scale that came
/// back, for the one caller that wants the sharp image: OCR.
///
/// Downsampling to points first would throw away exactly the detail that makes a 67x13 point
/// read-out legible, so Vision is handed the backing-store image and the word boxes it
/// reports are divided by this scale on the way out. The scale is measured from the returned
/// image rather than read from the display, so it is right even where a mode change has just
/// happened and cannot disagree with the image it describes.
#[allow(dead_code)] // Used by `ocr`; see docs/macos-port.md, "Units".
pub fn capture_backing(x: i32, y: i32, w: i32, h: i32) -> Option<(CFRetained<CGImage>, f64)> {
    if w <= 0 || h <= 0 {
        return None;
    }
    // Refused when the region's top or left edge is off the desktop, trimmed when only its
    // right or bottom edge is. The caller maps recognised text back to screen coordinates by
    // adding this region's origin to a box measured within the returned image, so an image
    // that STARTS somewhere else would put every word out by the part that was cut — a
    // plausible wrong coordinate, which is worse than none. Cutting the far edges moves
    // nothing: every word keeps its true offset from the same origin, and the only words lost
    // are ones that were not on the screen to be read.
    //
    // The second half is for laptops. A MacBook's desktop is 900-odd points tall against the
    // mini's 1080, so a window placed the way it was on a desktop monitor can hang off the
    // bottom, and a whole-window read — the probe's, or Kontakt's menu read reaching 460 points
    // below FILE — came back empty with the text all there above the edge.
    let Some((vx, vy, vw, vh)) = on_screen_part(x, y, w, h) else {
        crate::logging::line(
            "macos",
            &format!("not reading text in {w}x{h} at {x},{y}: it is not on the desktop"),
        );
        return None;
    };
    if (vx, vy) != (x, y) {
        crate::logging::line(
            "macos",
            &format!(
                "not reading text in {w}x{h} at {x},{y}: its top or left edge is off the desktop"
            ),
        );
        return None;
    }
    if (vw, vh) != (w, h) && !BACKING_TRIM_REPORTED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "reading text in the {vw}x{vh} part of a {w}x{h} region at {x},{y}; the rest hangs \
                 off the right or bottom edge of the desktop. Logged once."
            ),
        );
    }
    // The image is owned outright, so it outlives the pool; see `capture_rgba`.
    let captured = objc2::rc::autoreleasepool(|_| grab(x, y, vw, vh, true));
    // THE Retina measurement, at line level. The "first screen capture" line belongs to the
    // pixel path, whose scale depends on which capture function served it and need not be the
    // display's; this is the image OCR actually reads, and its scale is the one every word box
    // is divided by. Before this line it was written only under trace, so a Retina session
    // would have had to be run twice to learn what it measured.
    if let Some((image, scale)) = &captured {
        if !BACKING_REPORTED.swap(true, Ordering::Relaxed) {
            crate::logging::line(
                "macos",
                &format!(
                    "first capture for reading text: {vw}x{vh} points came back as {}x{} px — \
                     {scale:.2}x, the factor every word box is divided by",
                    CGImage::width(Some(image)),
                    CGImage::height(Some(image))
                ),
            );
        }
    }
    captured
}

/// One byte triple out of a tightly packed RGBA buffer.
fn sample(rgba: &[u8], stride_px: i32, dx: i32, dy: i32) -> Option<(u8, u8, u8)> {
    if dx < 0 || dy < 0 || stride_px <= 0 {
        return None;
    }
    let i = (dy as usize).checked_mul(stride_px as usize)?.checked_add(dx as usize)?.checked_mul(4)?;
    let px = rgba.get(i..i + 3)?;
    Some((px[0], px[1], px[2]))
}

/// Where to put the tile a `pixel()` read grabs.
///
/// Clamped to the display the point is on, because a rect that reaches past the edge comes
/// back clipped and is then refused; clamping keeps the fast path fast for a plugin window
/// sitting against the edge of the screen instead of making every read there cost two round
/// trips. A point on no display at all gets a single point, which will fail, which is the
/// honest answer.
fn tile_rect(x: i32, y: i32) -> (i32, i32, i32, i32) {
    let Some((_, left, top, right, bottom)) = display_at(x, y) else {
        crate::logging::trace("macos", || {
            format!("no display contains {x},{y}; reading the single point")
        });
        return (x, y, 1, 1);
    };

    let tx = (x - TILE_BACK_X).max(left);
    let ty = (y - TILE_BACK_Y).max(top);
    let tw = TILE_W.min(right - tx);
    let th = TILE_H.min(bottom - ty);
    if tw < 1 || th < 1 || x < tx || y < ty || x >= tx + tw || y >= ty + th {
        // A display smaller than the tile, or a layout the arithmetic above did not survive.
        // One point always works and is never clipped, so it is the safe answer.
        return (x, y, 1, 1);
    }
    (tx, ty, tw, th)
}

/// The rectangle that all displays together cover, in points.
///
/// A bounding box, so it can include a gap between two displays of different heights — it is
/// used to trim a request, never to promise that everything inside it is visible. `None`
/// when no display could be enumerated at all, which is not a state worth guessing around.
fn desktop_bounds() -> Option<(i32, i32, i32, i32)> {
    let mut ids: [CGDirectDisplayID; 16] = [0; 16];
    let mut found: u32 = 0;
    // SAFETY: both out parameters point at live storage of at least the declared capacity.
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut found) };
    if err != CGError::Success || found == 0 {
        return None;
    }
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for id in ids.iter().take(found as usize) {
        let b = CGDisplayBounds(*id);
        let (l, t) = (b.origin.x as i32, b.origin.y as i32);
        let (r, bo) = (l + b.size.width as i32, t + b.size.height as i32);
        bounds = Some(match bounds {
            None => (l, t, r, bo),
            Some((cl, ct, cr, cb)) => (cl.min(l), ct.min(t), cr.max(r), cb.max(bo)),
        });
    }
    bounds
}

/// The part of a requested region that is actually on a display, as (x, y, w, h).
///
/// Returns `None` when none of it is. Returns the request unchanged when all of it is, which
/// is the overwhelmingly common case and costs one display enumeration.
///
/// This exists because a capture of a region that hangs over the edge of the desktop comes
/// back SMALLER than it was asked for, and nothing in the returned image says by how much or
/// from which side. Working it out afterwards from the ratio of the sizes cannot distinguish
/// a clipped capture from a scaled one, so the trimming has to happen before the call, where
/// the numbers are still known.
fn on_screen_part(x: i32, y: i32, w: i32, h: i32) -> Option<(i32, i32, i32, i32)> {
    let (dl, dt, dr, db) = desktop_bounds()?;
    let (l, t) = (x.max(dl), y.max(dt));
    let (r, b) = ((x + w).min(dr), (y + h).min(db));
    if r <= l || b <= t {
        return None;
    }
    Some((l, t, r - l, b - t))
}

/// The display a point sits on and its bounds in points, right and bottom exclusive.
fn display_at(x: i32, y: i32) -> Option<(CGDirectDisplayID, i32, i32, i32, i32)> {
    let mut ids: [CGDirectDisplayID; 8] = [0; 8];
    let mut found: u32 = 0;
    // SAFETY: both out parameters point at live storage of at least the declared capacity,
    // and `ids.len()` is what is declared.
    let err = unsafe {
        CGGetDisplaysWithPoint(
            CGPoint::new(x as f64, y as f64),
            ids.len() as u32,
            ids.as_mut_ptr(),
            &mut found,
        )
    };
    if err != CGError::Success || found == 0 {
        return None;
    }
    // Mirrored displays all contain the point; any of them shows the same picture.
    let id = ids[0];
    let b = CGDisplayBounds(id);
    Some((
        id,
        b.origin.x as i32,
        b.origin.y as i32,
        (b.origin.x + b.size.width) as i32,
        (b.origin.y + b.size.height) as i32,
    ))
}

/// A region of the screen as exactly `w * h` pixels of tightly packed top-down RGBA.
fn capture_rgba(x: i32, y: i32, w: i32, h: i32) -> Option<Vec<u8>> {
    if !PERMISSION_PREFLIGHTED.swap(true, Ordering::Relaxed) {
        check_screen_recording("first capture of the session");
    }

    let started = Instant::now();
    // Around the whole round trip, because the image worker's thread has no pool of its own.
    // Core Graphics is a C API and the objects here are all owned outright, but it uses
    // Objective-C underneath and anything it autoreleases on a thread without a pool leaks for
    // the life of the process — and this is the one function called several times a second
    // from a thread we did not create.
    // Trimmed to the desktop BEFORE the capture, not judged afterwards. A region that hangs
    // over the edge — a plugin window docked against the bottom of the screen is the ordinary
    // case — comes back as a smaller image, and drawing that into the full-size destination
    // would stretch it: every template match and every reported coordinate inside it
    // displaced by a few per cent, silently, with the module clicking near the control
    // instead of on it. The part that exists is drawn at its true position and true size;
    // the rest stays the black that a Windows capture of off-screen area also produces.
    let (vx, vy, vw, vh) = on_screen_part(x, y, w, h)?;
    let (rgba, scale) = objc2::rc::autoreleasepool(|_| {
        let (image, scale) = grab(vx, vy, vw, vh, false)?;
        let rgba = image_to_rgba(&image, w, h, vx - x, vy - y, vw, vh)?;
        Some((rgba, scale))
    })?;
    if (vx, vy, vw, vh) != (x, y, w, h) && !CLIPPED_REPORTED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "a {w}x{h} capture at {x},{y} reaches past the edge of the desktop; the \
                 {vw}x{vh} part that exists was placed at its true offset and the rest \
                 is black"
            ),
        );
    }
    let elapsed = started.elapsed();

    // What a capture costs on this platform is one of the numbers docs/macos-port.md lists as
    // unmeasured, and the tester's log is the only instrument that will ever measure it.
    let first = !FIRST_CAPTURE_REPORTED.swap(true, Ordering::Relaxed);
    if first {
        crate::logging::line(
            "macos",
            &format!(
                "first screen capture: {w}x{h} points at {x},{y} came back at {scale:.2}x and took \
                 {} ms",
                elapsed.as_millis()
            ),
        );
    }
    // Not for the first capture, which carries its own time in the line above and pays what
    // ScreenCaptureKit costs to start: the CI Mac measured 61 ms for it and 11 ms for the next,
    // and a warning spent on start-up is one that cannot fire for a capture that is slow for real.
    if !first && elapsed.as_millis() >= 50 && !SLOW_CAPTURE_REPORTED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "a {w}x{h} point capture took {} ms. Everything runs on one thread here, and past \
                 roughly 300 ms in a single pump iteration macOS disables the event tap outright. \
                 Logged once; turn on AUTOMATION_PLATFORM_TRACE for every capture.",
                elapsed.as_millis()
            ),
        );
    }
    crate::logging::trace("macos", || {
        format!(
            "capture {w}x{h} points at {x},{y}: {scale:.2}x, {} ms, {} bytes",
            elapsed.as_millis(),
            rgba.len()
        )
    });
    Some(rgba)
}

/// One capture, as the system hands it over, with the scale it came back at.
///
/// ScreenCaptureKit first, where this macOS has its rectangle capture; then the window-list
/// function and the display function, where this macOS still has them. See the note at the top
/// of this file for why the older two are looked up rather than linked.
///
/// `best` asks the older functions for the backing-store resolution rather than one pixel per
/// point. ScreenCaptureKit's rectangle capture takes no such option; whatever resolution it
/// returns, the scale is MEASURED from the image, so the callers behave identically — the
/// point-sized destination in `image_to_rgba` downsamples it, and OCR divides by it.
///
/// A returned image whose two axes imply different scales has been clipped — the rect
/// reached past the edge of the desktop — and is refused rather than stretched, because a
/// stretched capture is a coordinate lie of exactly the kind this file exists to prevent,
/// and the host handles `None` as "no match" everywhere.
fn grab(x: i32, y: i32, w: i32, h: i32, best: bool) -> Option<(CFRetained<CGImage>, f64)> {
    let rect = CGRect::new(
        CGPoint::new(x as f64, y as f64),
        CGSize::new(w as f64, h as f64),
    );
    report_capture_paths();

    if sck_rect_capture_available() && !SCK_DISABLED.load(Ordering::Relaxed) {
        match sck_capture(rect) {
            SckOutcome::Image(image) => {
                if let Some(scale) = uniform_scale(&image, w, h) {
                    note_path_used(PATH_SCK, "ScreenCaptureKit captureImageInRect");
                    return Some((image, scale));
                }
                crate::logging::trace("macos", || {
                    format!(
                        "ScreenCaptureKit capture of {w}x{h} at {x},{y} came back {}x{} — clipped, \
                         not a whole region; trying the older functions",
                        CGImage::width(Some(&image)),
                        CGImage::height(Some(&image))
                    )
                });
            }
            SckOutcome::Error(why) => {
                if !SCK_ERROR_REPORTED.swap(true, Ordering::Relaxed) {
                    crate::logging::line(
                        "macos",
                        &format!(
                            "ScreenCaptureKit refused a {w}x{h} capture at {x},{y}: {why} — trying \
                             the older capture functions, where this macOS still has them. \
                             Logged once; if the reason is the Screen Recording permission, \
                             every capture after this one is refused the same way."
                        ),
                    );
                }
            }
            SckOutcome::TimedOut => {
                SCK_DISABLED.store(true, Ordering::Relaxed);
                crate::logging::line(
                    "macos",
                    &format!(
                        "ScreenCaptureKit did not answer a {w}x{h} capture at {x},{y} within {} ms \
                         — its answer may need the very thread that is waiting for it. It is not \
                         asked again this session; captures go to the older functions, where \
                         this macOS still has them.",
                        SCK_TIMEOUT.as_millis()
                    ),
                );
            }
        }
    }

    let resolution = if best {
        CGWindowImageOption::BestResolution
    } else {
        CGWindowImageOption::NominalResolution
    };
    if let Some(create) = window_list_create_image() {
        // SAFETY: the signature is the documented C one — CGRect by value, three uint32_t
        // options — and the result follows the Create rule, so it is ours to release.
        let raw = unsafe {
            create(rect, CGWindowListOption::OptionOnScreenOnly.0, 0, resolution.0)
        };
        if let Some(p) = NonNull::new(raw) {
            // SAFETY: a +1 reference from a Create function, owned from here on.
            let image = unsafe { CFRetained::from_raw(p) };
            if let Some(scale) = uniform_scale(&image, w, h) {
                note_path_used(PATH_WINDOW_LIST, "CGWindowListCreateImage (looked up at run time)");
                return Some((image, scale));
            }
            crate::logging::trace("macos", || {
                format!(
                    "window-list capture of {w}x{h} at {x},{y} came back {}x{} — clipped, not a \
                     whole region; trying the display path",
                    CGImage::width(Some(&image)),
                    CGImage::height(Some(&image))
                )
            });
        }
    }

    // Last. Its rect is display-local, so the origin of the display the region starts on is
    // subtracted first; on the primary display that subtraction is zero, which is the common
    // case and the one that stays right even if this convention is the other way round.
    let Some(create) = display_create_image_for_rect() else {
        if !sck_rect_capture_available() && window_list_create_image().is_none()
            && !NO_CAPTURE_REPORTED.swap(true, Ordering::Relaxed)
        {
            crate::logging::line(
                "macos",
                "no screen capture is possible on this macOS: it has neither ScreenCaptureKit's \
                 rectangle capture nor either of the older capture functions. Image search and \
                 OCR will find nothing, for the whole session.",
            );
        }
        return None;
    };
    let (display, left, top, _, _) = display_at(x, y)?;
    let local = CGRect::new(
        CGPoint::new((x - left) as f64, (y - top) as f64),
        CGSize::new(w as f64, h as f64),
    );
    // SAFETY: as above — the documented C signature, and a Create-rule result.
    let raw = unsafe { create(display, local) };
    let image = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
    let scale = uniform_scale(&image, w, h)?;
    if !FALLBACK_REPORTED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            "captures are coming from CGDisplayCreateImageForRect, the last of the three paths. \
             On a secondary display that path depends on the rect being display-local — if \
             coordinates are wrong on the second monitor only, this line is why.",
        );
    }
    note_path_used(PATH_DISPLAY, "CGDisplayCreateImageForRect (looked up at run time)");
    Some((image, scale))
}

const PATH_SCK: u32 = 1;
const PATH_WINDOW_LIST: u32 = 2;
const PATH_DISPLAY: u32 = 4;

/// Names the path that delivered an image, the first time each one does.
fn note_path_used(bit: u32, name: &str) {
    if PATH_USED.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
        crate::logging::line("macos", &format!("screen captures are coming from {name}"));
    }
}

/// Which of the three capture paths this macOS has, said once. The line a remote session needs
/// before any other capture line means anything: on a macOS that has dropped the older
/// functions it is the difference between "capture is broken" and "capture was never possible".
fn report_capture_paths() {
    if PATHS_REPORTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let has = |b: bool| if b { "present" } else { "absent" };
    crate::logging::line(
        "macos",
        &format!(
            "screen capture paths on this macOS: ScreenCaptureKit captureImageInRect {}, \
             CGWindowListCreateImage {}, CGDisplayCreateImageForRect {}",
            has(sck_rect_capture_available()),
            has(window_list_create_image().is_some()),
            has(display_create_image_for_rect().is_some()),
        ),
    );
}

/// Whether `+[SCScreenshotManager captureImageInRect:completionHandler:]` exists here.
///
/// Asked of the runtime, never assumed. The class arrived in macOS 14 and this method in 15.2
/// (Apple's SDK diff for it), the oldest Mac this has been tested on runs 12.7.6, and on this
/// objc2 a selector that does not exist aborts the process rather than returning nothing. So
/// the class is looked up by NAME first — `SCScreenshotManager::class()` would panic on a macOS
/// without it — and the selector is asked of its metaclass, the way avspeech.rs guards Personal
/// Voice.
fn sck_rect_capture_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        AnyClass::get(c"SCScreenshotManager")
            .is_some_and(|c| c.metaclass().responds_to(sel!(captureImageInRect:completionHandler:)))
    })
}

enum SckOutcome {
    Image(CFRetained<CGImage>),
    Error(String),
    TimedOut,
}

/// One rectangle through ScreenCaptureKit, turned back into a straight answer.
fn sck_capture(rect: CGRect) -> SckOutcome {
    let (tx, rx) = channel::<Result<CFRetained<CGImage>, String>>();
    // Called once, on a queue of the framework's. The image belongs to the framework for the
    // duration of the call, so it is retained before it crosses to the waiting thread; if that
    // thread has already given up, the send fails and the retained image is released with it.
    let handler = block2::RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let outcome = match NonNull::new(image) {
            // SAFETY: a live image for the duration of this call (Get rule).
            Some(p) => Ok(unsafe { CFRetained::retain(p) }),
            None => Err(
                // SAFETY: nil or a live error for the duration of this call.
                unsafe { error.as_ref() }
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| "no image and no error was returned".to_string()),
            ),
        };
        let _ = tx.send(outcome);
    });
    // SAFETY: availability was checked by `sck_rect_capture_available` before this is called;
    // the block outlives the call because the framework copies it.
    unsafe { SCScreenshotManager::captureImageInRect_completionHandler(rect, Some(&*handler)) };
    match rx.recv_timeout(SCK_TIMEOUT) {
        Ok(Ok(image)) => SckOutcome::Image(image),
        Ok(Err(why)) => SckOutcome::Error(why),
        Err(_) => SckOutcome::TimedOut,
    }
}

type WindowListCreateImageFn = unsafe extern "C" fn(CGRect, u32, u32, u32) -> *mut CGImage;
type DisplayCreateImageForRectFn = unsafe extern "C" fn(u32, CGRect) -> *mut CGImage;

/// A system function found by name at run time, or `None` where this macOS no longer has it.
///
/// Core Graphics is loaded anyway — everything else in this file comes from it — so the
/// default search order finds its exports without opening anything.
fn lookup(name: &CStr) -> Option<usize> {
    // SAFETY: RTLD_DEFAULT with a NUL-terminated name; dlsym only reads.
    let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) } as *mut c_void;
    (!p.is_null()).then_some(p as usize)
}

fn window_list_create_image() -> Option<WindowListCreateImageFn> {
    static F: OnceLock<Option<usize>> = OnceLock::new();
    // SAFETY: the address of `CGWindowListCreateImage`, whose C signature is the type above.
    F.get_or_init(|| lookup(c"CGWindowListCreateImage"))
        .map(|p| unsafe { core::mem::transmute::<usize, WindowListCreateImageFn>(p) })
}

fn display_create_image_for_rect() -> Option<DisplayCreateImageForRectFn> {
    static F: OnceLock<Option<usize>> = OnceLock::new();
    // SAFETY: the address of `CGDisplayCreateImageForRect`, whose C signature is the type above.
    F.get_or_init(|| lookup(c"CGDisplayCreateImageForRect"))
        .map(|p| unsafe { core::mem::transmute::<usize, DisplayCreateImageForRectFn>(p) })
}

/// The scale an image came back at, or `None` if its two axes disagree about what that scale
/// is, which means it is not the region that was asked for.
fn uniform_scale(image: &CGImage, w: i32, h: i32) -> Option<f64> {
    let (iw, ih) = (
        CGImage::width(Some(image)) as f64,
        CGImage::height(Some(image)) as f64,
    );
    if iw < 1.0 || ih < 1.0 {
        return None;
    }
    let (sx, sy) = (iw / w as f64, ih / h as f64);
    if !(0.2..=8.0).contains(&sx) || !(0.2..=8.0).contains(&sy) {
        return None;
    }
    // An absolute floor as well as a relative tolerance: on a one-point read the rounding is
    // the whole of the number, and 2% of it would refuse a perfectly good pixel.
    if (sx - sy).abs() > 0.25_f64.max(0.02 * sx.max(sy)) {
        return None;
    }
    Some((sx + sy) / 2.0)
}

/// Draw a captured image into a buffer whose layout we chose, at exactly `dw * dh` pixels.
///
/// The layout is stated to Core Graphics rather than read off the image: a screen `CGImage`
/// on Apple silicon reports premultiplied-first with little-endian byte order, which means
/// the bytes in memory are BGRA, and that combination is a well-known trap to get wrong by
/// hand. Asking a bitmap context for `PremultipliedLast | Order32Big` makes RGBA a
/// compile-time constant instead of a runtime negotiation, tightly packed with no stride
/// padding, and it survives a future macOS changing what the capture format is.
///
/// Drawing into a destination of the requested point size is also where the Retina
/// downsample happens, and why there is no separate resampling step to get wrong.
fn image_to_rgba(
    image: &CGImage,
    dw: i32,
    dh: i32,
    at_x: i32,
    at_y: i32,
    fill_w: i32,
    fill_h: i32,
) -> Option<Vec<u8>> {
    let (dw, dh) = (dw as usize, dh as usize);
    let bytes_per_row = dw.checked_mul(4)?;
    let mut buf = vec![0u8; bytes_per_row.checked_mul(dh)?];
    let space = CGColorSpace::new_device_rgb()?;
    let bitmap_info: u32 = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;

    // SAFETY: `buf` is `bytes_per_row * dh` bytes, which is what the context is told, and it
    // outlives the context — the context is dropped below before anything reads the buffer.
    let ctx = unsafe {
        CGBitmapContextCreate(
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            dw,
            dh,
            8,
            bytes_per_row,
            Some(&space),
            bitmap_info,
        )
    }?;

    // Stated rather than left to the context's default, so that two machines running the same
    // build downsample a Retina capture the same way. Low is a box-average rather than a
    // nearest-neighbour pick: a probe point on a 2x rendering then reads the colour of that
    // point rather than whichever of its four backing pixels happened to be first, which is
    // what a module measuring against a 1x reference asked for.
    CGContext::set_interpolation_quality(Some(&ctx), CGInterpolationQuality::Low);
    // The destination rectangle is the part of the region that was captured, at its own size
    // and offset — not the whole destination. Drawing into the whole destination is what
    // would turn a clipped capture into a scaled one.
    //
    // Core Graphics draws with the origin at the BOTTOM left of the context, which is the one
    // place in this backend where that matters: a band missing from the top of the region has
    // to be left blank at the top of the buffer, and the buffer's rows run top-down.
    let bottom_gap = dh as f64 - (at_y as f64 + fill_h as f64);
    CGContext::draw_image(
        Some(&ctx),
        CGRect::new(
            CGPoint::new(at_x as f64, bottom_gap),
            CGSize::new(fill_w as f64, fill_h as f64),
        ),
        Some(image),
    );
    CGContext::flush(Some(&ctx));
    drop(ctx);

    // Opaque, always. The matcher ignores the capture's alpha, but `host.screen.save` writes
    // it straight into a PNG, and a fully transparent calibration screenshot is one the blind
    // author's sighted helper cannot see anything in.
    for px in buf.chunks_exact_mut(4) {
        px[3] = 255;
    }
    Some(buf)
}

/// Notice captures that come back a single flat colour, and question the permission when
/// several do in a row.
///
/// Only for regions big enough that flatness means something: a plugin's panel background is
/// genuinely one colour, and every `pixel()` tile would otherwise raise this. The test itself
/// samples rather than scans, and gives up at the first pixel that differs, so the cost on a
/// normal capture is a handful of comparisons.
fn watch_for_blank(rgba: &[u8], w: i32, h: i32) {
    if w < 64 || h < 64 {
        return;
    }
    if !looks_flat(rgba) {
        FLAT_RUN.store(0, Ordering::Relaxed);
        return;
    }
    let run = FLAT_RUN.fetch_add(1, Ordering::Relaxed) + 1;
    if run != FLAT_RUN_ALARM {
        return;
    }
    FLAT_RUN.store(0, Ordering::Relaxed);
    if !check_screen_recording("several captures in a row came back a single flat colour")
        || FLAT_WARNED.swap(true, Ordering::Relaxed)
    {
        return;
    }
    crate::logging::line(
        "macos",
        &format!(
            "{FLAT_RUN_ALARM} captures in a row of a {w}x{h} point region came back one flat \
             colour, and Screen Recording IS granted — so the region is covered, off-screen, or \
             the window is not where the module thinks it is. Logged once."
        ),
    );
}

/// Is every sampled pixel the same colour? Sampled on a stride so a full-screen capture is
/// not scanned byte by byte; a real plugin UI differs within the first few samples.
fn looks_flat(rgba: &[u8]) -> bool {
    let count = rgba.len() / 4;
    if count < 2 {
        return false;
    }
    let stride = (count / 512).max(1);
    let first = &rgba[0..3];
    for i in (0..count).step_by(stride) {
        let px = &rgba[i * 4..i * 4 + 3];
        if px != first {
            return false;
        }
    }
    true
}

/// Preflight Screen Recording and, if it is missing, say so in the log in words the user can
/// act on. Returns whether it is granted. The warning is written once per session however
/// many times this is asked, because the caller that notices is in the capture path.
fn check_screen_recording(reason: &str) -> bool {
    if CGPreflightScreenCaptureAccess() {
        crate::logging::trace("macos", || {
            format!("screen recording permission is granted ({reason})")
        });
        return true;
    }
    if !PERMISSION_WARNED.swap(true, Ordering::Relaxed) {
        // Stated as a report rather than a verdict, because this check has been caught
        // being wrong. Measured on macOS 12.7.6 with the application added to the Screen
        // Recording list by hand: the preflight answered "not granted" at startup and again
        // at the first capture, while the capture itself came back with the plugin's real
        // window in it — OCR read fifty-three words of its interface out of the very frame
        // this line was complaining about.
        //
        // So the wording matters. Shouting that a permission is missing, at somebody who
        // cannot see the screen and cannot check, sends them to fix something that is not
        // broken; and a silent wrong answer here is exactly what this message exists to
        // prevent. The honest form says what was asked, what the answer was, and what it
        // would look like if the answer were right.
        crate::logging::line(
            "macos",
            &format!(
                "screen recording: the system reports this application as NOT permitted \
                 ({reason}). If that is true, captures come back showing the desktop with \
                 other applications' windows removed — image search and OCR then never \
                 match and nothing else looks wrong. If image search and OCR are working, \
                 the report is wrong and there is nothing to do; this check has been \
                 observed answering 'no' for an application that could capture perfectly \
                 well. Grant it in {} > Screen Recording (called Screen & System Audio \
                 Recording from macOS 15) and restart the application.",
                super::perm::privacy_pane()
            ),
        );
    }
    false
}
