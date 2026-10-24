//! Every limit and timing of `host.ocr.recognize`, and of the snapshots and change waits taken on its
//! capture thread, in one place, so the documentation, the tests and the code quote the same
//! numbers. Std only; borrowed by `crates/macos-check`.

use std::time::Duration;

/// A region up to this size gets the small-text treatment on both platforms: cropped to its
/// content, upscaled, the blank guard, and on Windows the second recogniser. Shared so the two
/// platforms draw the line in the same place — it is observable from Luau.
pub const SMALL_W: i32 = 400;
pub const SMALL_H: i32 = 200;

/// Regions in one call.
pub const MAX_REGIONS: usize = 64;

/// Pixels in one call, all regions together: about ten 4K screens. Corners past it are a
/// mistake in the call (a corner mistyped into the millions) and raise; a window region that
/// would take the call past it, at the window's size then, is answered `"failed"` instead.
pub const MAX_CALL_PIXELS: i64 = 40_000_000;

/// Reads of one module that may be waiting or running at once. The 17th evicts that module's
/// oldest unkeyed read, or is refused when every waiting read has a key.
pub const PER_OWNER: usize = 16;

/// Jobs waiting or running in the whole application.
pub const TOTAL: usize = 64;

/// A background read that has waited this long goes before interactive ones — to be
/// photographed, and to be recognised.
pub const AGING: Duration = Duration::from_millis(500);

/// A recognition that has answered no region for this long counts as a hang: new reads fail
/// at once with a reason until it answers one. Counted per region, not per job, so that 64
/// small regions at a quarter of a second each are not a hang.
pub const HANG: Duration = Duration::from_secs(5);

/// One capture of the regions' bounding box instead of one per region, when the box is no
/// larger than this...
pub const BBOX_MAX: i64 = 8_000_000;

/// ...and no more than this many times the regions' own area (or `BBOX_FLOOR`, whichever is
/// larger — tiny read-outs a few pixels apart are one capture whatever the ratio says).
pub const BBOX_WASTE: i64 = 4;
pub const BBOX_FLOOR: i64 = 65_536;

/// Pixels captured and not yet recognised, in bytes, above which background captures wait.
/// Interactive captures always proceed.
pub const CAPTURED_BUDGET: usize = 256 * 1024 * 1024;

/// How often, at most, the language list is read again after a language did not resolve — a
/// language pack installed while the application runs is picked up without a restart.
pub const LANG_REREAD: Duration = Duration::from_secs(30);

/// How long the exit waits for the two OCR threads.
pub const SHUTDOWN: Duration = Duration::from_secs(1);

/// A job slower than this, from the call to the answer, gets a log line naming where the time
/// went — at most one per module per `SLOW_LOG_EVERY`.
pub const SLOW_JOB: Duration = Duration::from_millis(100);
pub const SLOW_LOG_EVERY: Duration = Duration::from_secs(10);

// ── The snapshot requests, on the same capture thread ────────────────────────────────────────

/// Snapshot requests of one module VM waiting at once — plain, timed and change waits together.
/// The 17th ends that module's oldest one: the newest always proceeds, since the newest is the
/// press somebody is waiting for.
pub const SNAP_PER_OWNER: usize = 16;

/// Change waits of one module VM at once; the 5th ends that module's oldest wait.
pub const SNAP_WAITS_PER_OWNER: usize = 4;

/// Change waits in the whole application; the 17th is not started.
pub const SNAP_WAITS: usize = 16;

/// Snapshot requests in the whole application; the 65th is not taken.
pub const SNAP_TOTAL: usize = 64;

// ── The image worker's searches (image_search.rs) ─────────────────────────────────────────────

/// Requests with a callback of one module VM — every image worker call's, `imageSearchEach`'s
/// among them — waiting for the image worker at once. The 65th ends that module's oldest: the newest always
/// proceeds, as with snapshots.
///
/// 64, not the snapshots' 16: the cap is for a timer that searches faster than the worker
/// answers, and must never be met by a module working as designed. A sample-library module's
/// VM holds an overlay per library and Kontakt cell, and every one of their landmark gates asks
/// in the same poll — one search each, a near-miss probe beside a full-region miss, a value read
/// or a resize-grip search on top — so one VM has reached about 14 waiting at once (the uptime
/// review of 2026-09-27, counted for the shipped Soundiron module), and the header's reads come
/// on top. At 16, a poll that asked for more ended its first gates' searches, every poll and
/// always the same ones, and those libraries could never have been detected.
pub const IMAGE_PER_OWNER: usize = 64;

/// The region of a change wait may cover at most this many pixels: one 4K screen. A wait holds
/// three pictures of it at a time and compares one every round.
pub const MAX_WAIT_PIXELS: i64 = 8_294_400;

/// Rectangles one change wait may watch.
pub const MAX_WATCH: usize = 16;

/// The longest a change wait may run, and how far ahead of `host.now()` a timed snapshot may be
/// asked for.
pub const WAIT_MAX: Duration = Duration::from_secs(2);
pub const AT_MAX: Duration = Duration::from_secs(2);

/// A change wait's defaults: how long it runs, how far a channel may differ without the pixel
/// counting as changed, how many pixels must differ, and how long a change must stand still.
pub const WAIT_TIMEOUT: Duration = Duration::from_millis(500);
pub const WAIT_TOLERANCE: u8 = 16;
pub const WAIT_MIN_PIXELS: u32 = 4;
pub const WAIT_SETTLE: Duration = Duration::ZERO;

/// The requests of one round are taken in one capture of their union when it covers at most
/// this many pixels, and one capture each otherwise (the standard path; desktop duplication takes
/// them all in one request anyway).
pub const UNION_MAX: i64 = 2_000_000;

/// The least time from the start of one polling round of a change wait to the next, per way of
/// capturing. The standard Windows path costs a compositor frame per capture anyway; desktop
/// duplication hands out at most one new frame per display refresh; macOS captures through
/// ScreenCaptureKit, whose cost per call in a loop is not measured yet.
pub const MIN_ROUND_STANDARD: Duration = Duration::from_millis(8);
pub const MIN_ROUND_DUPLICATION: Duration = Duration::from_millis(16);
pub const MIN_ROUND_MACOS: Duration = Duration::from_millis(33);

/// A snapshot round slower than this, capture and comparison together, is logged — the first,
/// and then every hundredth.
pub const ROUND_SLOW: Duration = Duration::from_millis(50);

/// Whether `w` x `h` is a small region.
pub fn is_small(w: i32, h: i32) -> bool {
    w <= SMALL_W && h <= SMALL_H
}

/// The largest processed image, in pixels, a read enlarges a region to: a region handed on as
/// captured (larger than [`SMALL_W`] x [`SMALL_H`]) is enlarged only while the result stays
/// within it, and a small region's upscale is held to it. Four million is a 2000x2000 picture,
/// about what Vision and `Windows.Media.Ocr` read in the time a small read's ladder allows.
pub const ENLARGE_BUDGET: usize = 4_000_000;

/// A line of ink shorter than this, in captured pixels, is small for a recogniser: what a region
/// handed on as captured is enlarged for, besides a capture at one pixel a point.
pub const SMALL_LINE_PX: usize = 16;

/// A display scale under this is one pixel a point: a Mac's backing scale 1, a Windows monitor at
/// 100 % (an effective DPI under 108). Windows' 125 % is not, nor anything above it.
pub const ONE_PIXEL_A_POINT_UNDER: f64 = 1.125;

/// The integer factor a region `w` x `h` pixels handed on as captured is enlarged by, without a
/// crop: 3, or 2 where 3 would pass [`ENLARGE_BUDGET`], when the capture is at one pixel a point
/// (`scale` under [`ONE_PIXEL_A_POINT_UNDER`]) or its tallest line of ink is shorter than
/// [`SMALL_LINE_PX`]; 1 otherwise, and where even 2 would pass the budget. The second Mac test of
/// VPS Avenger (2026-10-24) read a 344x268 area at backing scale 1 as captured — its names about 5
/// pixels tall, the "Factory" label in the report's `Avenger 80 closed.png` (6 in `Avenger 100
/// closed.png`) — into nine garbage words, seven times over. Three times 5 is still a line under
/// [`SMALL_LINE_PX`]: such a grid is read whole only as well as that allows, and row by row better.
pub fn pass_through_up(w: usize, h: usize, scale: f64, tallest_line: Option<usize>) -> usize {
    let small_text = scale < ONE_PIXEL_A_POINT_UNDER || tallest_line.is_some_and(|l| l > 0 && l < SMALL_LINE_PX);
    if !small_text {
        return 1;
    }
    within_budget(w, h, 3)
}

/// `up`, or the largest factor below it that keeps `w` x `h` pixels within [`ENLARGE_BUDGET`] once
/// enlarged by it; at least 1.
pub fn within_budget(w: usize, h: usize, up: usize) -> usize {
    let area = w.saturating_mul(h).max(1);
    let mut f = up.max(1);
    while f > 1 && area.saturating_mul(f * f) > ENLARGE_BUDGET {
        f -= 1;
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tester's 344x268 area at backing scale 1: three times; on a Windows monitor at 125 %, as
    /// captured. The same at 2x with lines of 20 pixels: as captured. A whole 1398x1021 window at 1x: past the budget even twice, so as
    /// captured. Small lines on a Retina capture: enlarged too.
    #[test]
    fn a_region_handed_on_as_captured_is_enlarged_at_1x_or_for_small_lines_within_the_budget() {
        assert_eq!(pass_through_up(344, 268, 1.0, None), 3);
        assert_eq!(pass_through_up(688, 536, 2.0, Some(20)), 1);
        assert_eq!(pass_through_up(688, 536, 2.0, None), 1);
        assert_eq!(pass_through_up(688, 536, 2.0, Some(12)), 3, "small lines on a Retina capture");
        assert_eq!(pass_through_up(900, 700, 2.0, Some(12)), 2, "900x700 three times is past the budget");
        assert_eq!(pass_through_up(1398, 1021, 1.0, None), 1, "a whole window");
        assert_eq!(pass_through_up(900, 700, 1.0, None), 2);
        assert_eq!(pass_through_up(344, 268, 1.25, None), 1, "Windows at 125 %");
        assert_eq!(pass_through_up(344, 268, 1.1, None), 3, "Windows at 110 %, 105.6 DPI");
        assert_eq!(within_budget(100, 100, 6), 6);
        assert_eq!(within_budget(800, 400, 6), 3);
        assert_eq!(within_budget(400, 200, 10), 7, "the largest small region");
        assert_eq!(within_budget(4000, 4000, 3), 1);
    }
}
