//! A change wait, with no thread and no clock of its own: what `host.screen.snapshotAsync { change
//! = … }` decides about each picture its polling rounds take, stepped by the `screen-capture`
//! thread (`snap_queue.rs`) and by the tests below.
//!
//! **What "changed" means.** At least `min_pixels` of the watched pixels differ from the baseline
//! by more than `tolerance` in some channel. The baseline is the `from` snapshot the module passed
//! — or, without one, the first picture this wait takes. Only the watched part counts, so a
//! blinking cursor or an animated background left out of `watch` never ends a wait.
//!
//! **Settling.** With `settle` of 0 the first picture that differs is the answer — right for a
//! help bubble that is on screen for a moment. Otherwise the wait goes on until the watched part
//! has stayed within `tolerance` of the picture that started the still spell for `settle`, and
//! answers the newest picture: a menu that slides its cursor over three frames is read once it
//! stands. A change that undid itself — a flash, a cursor that moved off and back — is no change:
//! a still spell that ends on a picture no different from the baseline sends the wait back to
//! looking, so `changed` is never said of a picture that equals the baseline.
//!
//! **Bounded.** At the deadline the newest picture is the answer, `changed` saying whether it was
//! settling and still differs from the baseline (a change was seen, it had not stood still yet)
//! and `settled` false; a wait whose every capture failed answers no picture, and the last
//! capture's reason.
//!
//! **What it holds.** Its newest picture, cut to its region by the lane — the one it may answer
//! with — and two pictures it only compares with, the baseline and the one a still spell began
//! with, as copies of the watched part's bounding box without macOS's backing image
//! (`Frame::pixels_only`). Never a round's shared capture of a larger rectangle: that is what the
//! request's reservation (`snapshot::reservation_for`) counts on.
//!
//! **One path per wait.** Two capture paths do not give byte-identical pictures of the same
//! screen (desktop duplication against the standard path under HDR, say), so a difference between
//! them is not a change. A picture from another path than the wait's first — a fallback, or a
//! `from` taken the other way — becomes the new baseline, and the answer says `rebased`. So does
//! a `from` that does not hold every watched rectangle.
//!
//! Pure, apart from the frame type: borrowed by `crates/macos-check`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::backend::frame::{Frame, FrameVia};

use super::types::Rect;

/// What a wait looks for, and for how long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeSpec {
    /// A channel may differ by this much, 0 to 255, without the pixel counting as changed.
    pub tolerance: u8,
    /// How many watched pixels must differ for the picture to have changed; at least 1.
    pub min_pixels: u32,
    /// How long the watched part must stand still after a change; zero answers the change.
    pub settle: Duration,
    /// From the wait's start to its deadline.
    pub timeout: Duration,
}

/// Whether at least `min` pixels of `watch` (screen rectangles) differ between `a` and `b` by
/// more than `tol` in a channel. The two frames may be of different rectangles — a union capture
/// against a snapshot of the region alone — and are read by screen coordinates; a watched pixel
/// outside either of them is not compared. Stops at the `min`th difference. `watch` should hold
/// no pixel twice ([`disjoint`]), or that pixel counts twice.
pub fn differs(a: &Frame, b: &Frame, watch: &[Rect], tol: u8, min: u32) -> bool {
    let min = min.max(1);
    let mut n: u32 = 0;
    for w in watch {
        let Some(r) = a.rect.intersect(w).and_then(|r| b.rect.intersect(&r)) else { continue };
        let (aw, bw) = (a.img.w as usize, b.img.w as usize);
        let ax = (r.x as i64 - a.rect.x as i64) as usize;
        let ay = (r.y as i64 - a.rect.y as i64) as usize;
        let bx = (r.x as i64 - b.rect.x as i64) as usize;
        let by = (r.y as i64 - b.rect.y as i64) as usize;
        let len = r.w as usize * 4;
        for row in 0..r.h as usize {
            let ai = ((ay + row) * aw + ax) * 4;
            let bi = ((by + row) * bw + bx) * 4;
            let (Some(pa), Some(pb)) = (a.img.rgba.get(ai..ai + len), b.img.rgba.get(bi..bi + len)) else {
                continue;
            };
            for (p, q) in pa.chunks_exact(4).zip(pb.chunks_exact(4)) {
                if p[0].abs_diff(q[0]) > tol || p[1].abs_diff(q[1]) > tol || p[2].abs_diff(q[2]) > tol {
                    n += 1;
                    if n >= min {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// The pixels `rects` cover, as rectangles that do not overlap: a pixel two watched rectangles
/// share is compared, and counted, once. Found on the grid of the rectangles' own edges, so it
/// costs at most 31 x 31 cells for the 16 rectangles a wait may watch.
pub fn disjoint(rects: &[Rect]) -> Vec<Rect> {
    let rs: Vec<Rect> = rects.iter().copied().filter(|r| !r.is_empty()).collect();
    let edges = |f: fn(&Rect) -> (i64, i64)| {
        let mut v: Vec<i64> = rs.iter().flat_map(|r| {
            let (a, b) = f(r);
            [a, b]
        }).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let xs = edges(|r| (r.x as i64, r.x as i64 + r.w as i64));
    let ys = edges(|r| (r.y as i64, r.y as i64 + r.h as i64));
    let covered = |x0: i64, x1: i64, y0: i64, y1: i64| {
        rs.iter().any(|r| {
            r.x as i64 <= x0 && x1 <= r.x as i64 + r.w as i64 && r.y as i64 <= y0 && y1 <= r.y as i64 + r.h as i64
        })
    };
    let mut out = Vec::new();
    for yw in ys.windows(2) {
        let (y0, y1) = (yw[0], yw[1]);
        let mut run: Option<i64> = None;
        for xw in xs.windows(2) {
            let (x0, x1) = (xw[0], xw[1]);
            match (covered(x0, x1, y0, y1), run) {
                (true, None) => run = Some(x0),
                (false, Some(s)) => {
                    out.push(Rect::new(s as i32, y0 as i32, (x0 - s) as i32, (y1 - y0) as i32));
                    run = None;
                }
                _ => {}
            }
        }
        if let (Some(s), Some(&end)) = (run, xs.last()) {
            out.push(Rect::new(s as i32, y0 as i32, (end - s) as i32, (y1 - y0) as i32));
        }
    }
    out
}

/// What one polling round saw.
pub enum Seen {
    /// Of the wait's region — the lane cuts a shared capture down first.
    Frame(Arc<Frame>),
    /// The capture failed, with the reason: the clocks still move.
    Failed(String),
}

/// How a wait ended.
#[derive(Clone)]
pub struct Outcome {
    /// The picture it answers with: one the lane handed it, cut to the wait's region. `None` when
    /// no capture of the wait ever came back.
    pub frame: Option<Arc<Frame>>,
    /// Why there is no picture.
    pub why: Option<String>,
    pub changed: bool,
    pub settled: bool,
    pub rebased: bool,
    /// Pictures taken for it.
    pub frames: u32,
}

/// What `why` says when a wait ended before any round came back, which only a wait whose rounds
/// never ran can.
pub const NO_PICTURE: &str = "no picture was taken before the change wait's timeout";

enum State {
    /// Looking for the first difference from the baseline.
    Seeking,
    /// A difference was seen; `last` began the still spell at `since`.
    Settling { last: Arc<Frame>, since: Instant },
}

/// One change wait. See the file's comment.
pub struct Wait {
    /// Cut to the wait's region, and disjoint.
    watch: Vec<Rect>,
    /// The bounding box of `watch`: what the pictures it only compares with are cut to.
    watch_box: Rect,
    spec: ChangeSpec,
    /// The module's `from`, until the first picture decides whether it can be the baseline.
    from: Option<Arc<Frame>>,
    /// Compared with only: a copy of `watch_box` without the backing image.
    baseline: Option<Arc<Frame>>,
    /// The path of the first picture, and of every one after it until a rebase.
    via: Option<FrameVia>,
    state: State,
    /// The newest picture, as the lane handed it: cut to the wait's region.
    latest: Option<Arc<Frame>>,
    deadline: Instant,
    frames: u32,
    rebased: bool,
    last_error: Option<String>,
}

impl Wait {
    /// A wait over `region` from `start` on, watching `watch` (all of `region` when empty; each
    /// rectangle cut to it, and one that misses it dropped — the caller refuses those first).
    pub fn new(region: Rect, watch: &[Rect], spec: ChangeSpec, from: Option<Arc<Frame>>, start: Instant) -> Wait {
        let cut: Vec<Rect> = if watch.is_empty() {
            vec![region]
        } else {
            watch.iter().filter_map(|w| region.intersect(w)).collect()
        };
        let watch = disjoint(&cut);
        let watch_box = watch.iter().fold(Rect::default(), |a, r| a.union(r));
        Wait {
            watch,
            watch_box,
            spec,
            from,
            baseline: None,
            via: None,
            state: State::Seeking,
            latest: None,
            deadline: start + spec.timeout,
            frames: 0,
            rebased: false,
            last_error: None,
        }
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// `f` as a picture this wait only compares with: `f` itself when it is exactly the watched
    /// box and keeps nothing beside its pixels, else a copy of the box's point-sized pixels —
    /// never a larger capture, nor macOS's backing image, held for the rest of the wait.
    fn compared(&self, f: &Arc<Frame>) -> Arc<Frame> {
        if f.native.is_none() && f.rect == self.watch_box {
            return f.clone();
        }
        f.pixels_only(self.watch_box).map(Arc::new).unwrap_or_else(|| f.clone())
    }

    /// Whether `f` still differs from the baseline: a still spell that ends on a picture that
    /// does not is a change that undid itself.
    fn still_changed(&self, f: &Frame) -> bool {
        self.baseline
            .as_ref()
            .is_some_and(|b| differs(b, f, &self.watch, self.spec.tolerance, self.spec.min_pixels))
    }

    /// One round's picture, or its failure, at `now`. `Some` when the wait has ended.
    pub fn step(&mut self, now: Instant, seen: Seen) -> Option<Outcome> {
        if let Some(done) = self.look(now, seen) {
            return Some(done);
        }
        (now >= self.deadline).then(|| self.expire())
    }

    fn look(&mut self, now: Instant, seen: Seen) -> Option<Outcome> {
        let f = match seen {
            Seen::Failed(why) => {
                self.last_error = Some(why);
                // A failed round is time passing: a still spell that was due completes with the
                // newest picture there is — or, when that one no longer differs from the
                // baseline, the wait looks again.
                if let State::Settling { since, .. } = &self.state {
                    if now.saturating_duration_since(*since) >= self.spec.settle {
                        let latest = self.latest.clone();
                        if latest.as_ref().is_some_and(|l| self.still_changed(l)) {
                            return Some(self.finish(latest, true, true));
                        }
                        self.state = State::Seeking;
                    }
                }
                return None;
            }
            Seen::Frame(f) => f,
        };
        self.frames += 1;
        self.latest = Some(f.clone());
        match self.via {
            None => {
                self.via = Some(f.via);
                let from = self.from.take();
                let usable = from
                    .as_ref()
                    .is_some_and(|fr| fr.via == f.via && self.watch.iter().all(|w| fr.rect.contains(w)));
                match from {
                    Some(fr) if usable => self.baseline = Some(self.compared(&fr)),
                    Some(_) => {
                        self.baseline = Some(self.compared(&f));
                        self.rebased = true;
                        return None;
                    }
                    None => {
                        self.baseline = Some(self.compared(&f));
                        return None;
                    }
                }
            }
            Some(v) if v != f.via => {
                self.via = Some(f.via);
                self.baseline = Some(self.compared(&f));
                self.state = State::Seeking;
                self.rebased = true;
                return None;
            }
            Some(_) => {}
        }
        let (tol, min) = (self.spec.tolerance, self.spec.min_pixels);
        match &self.state {
            State::Seeking => {
                let base = self.baseline.as_ref()?;
                if differs(base, &f, &self.watch, tol, min) {
                    if self.spec.settle.is_zero() {
                        return Some(self.finish(Some(f), true, true));
                    }
                    self.state = State::Settling { last: self.compared(&f), since: now };
                }
            }
            State::Settling { last, since } => {
                if differs(last, &f, &self.watch, tol, min) {
                    self.state = State::Settling { last: self.compared(&f), since: now };
                } else if now.saturating_duration_since(*since) >= self.spec.settle {
                    if self.still_changed(&f) {
                        return Some(self.finish(Some(f), true, true));
                    }
                    // It stood still where it began: the change undid itself.
                    self.state = State::Seeking;
                }
            }
        }
        None
    }

    /// The deadline's answer: the newest picture, changed when a change was seen, was still
    /// settling and the picture still differs from the baseline, never settled; or none, and
    /// why.
    fn expire(&mut self) -> Outcome {
        let settling = matches!(self.state, State::Settling { .. });
        match self.latest.clone() {
            Some(f) => {
                let changed = settling && self.still_changed(&f);
                self.finish(Some(f), changed, false)
            }
            None => Outcome {
                frame: None,
                why: Some(self.last_error.clone().unwrap_or_else(|| NO_PICTURE.to_string())),
                changed: false,
                settled: false,
                rebased: self.rebased,
                frames: self.frames,
            },
        }
    }

    fn finish(&self, frame: Option<Arc<Frame>>, changed: bool, settled: bool) -> Outcome {
        Outcome { frame, why: None, changed, settled, rebased: self.rebased, frames: self.frames }
    }

    /// The pictures it holds now — the newest, the baseline, the one a still spell began with —
    /// as (rectangle, keeps a backing image), for the tests of what a wait may hold.
    #[cfg(test)]
    pub(crate) fn held(&self) -> Vec<(Rect, bool)> {
        let last = match &self.state {
            State::Settling { last, .. } => Some(last),
            State::Seeking => None,
        };
        [self.latest.as_ref(), self.baseline.as_ref(), last]
            .into_iter()
            .flatten()
            .map(|f| (f.rect, f.native.is_some()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::CapturedImage;

    const MS: Duration = Duration::from_millis(1);

    /// A frame of `r`, all `(v, v, v)`, with `patch` (a screen rectangle inside it) `(p, p, p)`.
    fn frame(r: Rect, v: u8, patch: Option<(Rect, u8)>, via: FrameVia) -> Arc<Frame> {
        let mut rgba = vec![0u8; (r.w * r.h * 4) as usize];
        for y in 0..r.h {
            for x in 0..r.w {
                let (sx, sy) = (r.x + x, r.y + y);
                let c = match patch {
                    Some((p, pv)) if p.contains(&Rect::new(sx, sy, 1, 1)) => pv,
                    _ => v,
                };
                let i = ((y * r.w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[c, c, c, 255]);
            }
        }
        let img = CapturedImage { w: r.w as u32, h: r.h as u32, rgba };
        Arc::new(Frame::from_image(r, img, Instant::now(), via, None).expect("a whole frame"))
    }

    const R: Rect = Rect::new(100, 100, 40, 30);

    fn plain(v: u8) -> Arc<Frame> {
        frame(R, v, None, FrameVia::Gdi)
    }

    fn patched(p: Rect, pv: u8) -> Arc<Frame> {
        frame(R, 10, Some((p, pv)), FrameVia::Gdi)
    }

    fn spec(settle_ms: u64) -> ChangeSpec {
        ChangeSpec { tolerance: 16, min_pixels: 4, settle: Duration::from_millis(settle_ms), timeout: Duration::from_millis(500) }
    }

    fn seen(f: &Arc<Frame>) -> Seen {
        Seen::Frame(f.clone())
    }

    #[test]
    fn change_on_first_differing_frame() {
        let t0 = Instant::now();
        let from = plain(10);
        let mut w = Wait::new(R, &[], spec(0), Some(from), t0);
        assert!(w.step(t0 + 8 * MS, seen(&plain(10))).is_none(), "the same picture");
        let bubble = patched(Rect::new(110, 110, 4, 4), 200);
        let o = w.step(t0 + 16 * MS, seen(&bubble)).expect("16 pixels changed");
        assert!(o.changed && o.settled && !o.rebased);
        assert!(Arc::ptr_eq(o.frame.as_ref().unwrap(), &bubble));
        assert_eq!(o.frames, 2);
    }

    #[test]
    fn no_from_first_frame_is_baseline() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        // Whatever the first picture shows, it is not a change: it is what is compared with.
        assert!(w.step(t0, seen(&patched(Rect::new(100, 100, 40, 30), 250))).is_none());
        assert!(w.step(t0 + 8 * MS, seen(&patched(Rect::new(100, 100, 40, 30), 250))).is_none());
        let o = w.step(t0 + 16 * MS, seen(&plain(10))).expect("back to plain is a change");
        assert!(o.changed);
    }

    #[test]
    fn settle_waits_for_stillness() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        let moved = patched(Rect::new(100, 100, 10, 10), 200);
        assert!(w.step(t0 + 10 * MS, seen(&moved)).is_none(), "changed, now settling");
        assert!(w.step(t0 + 30 * MS, seen(&moved)).is_none(), "still for 20 ms");
        let later = patched(Rect::new(100, 100, 10, 10), 205); // within the tolerance
        let o = w.step(t0 + 60 * MS, seen(&later)).expect("still for 50 ms");
        assert!(o.changed && o.settled);
        assert!(Arc::ptr_eq(o.frame.as_ref().unwrap(), &later), "the newest picture is the answer");
    }

    #[test]
    fn settle_restarts_on_further_change() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        assert!(w.step(t0 + 10 * MS, seen(&patched(Rect::new(100, 100, 10, 10), 200))).is_none());
        // The cursor slid on: the still spell starts again at 40.
        assert!(w.step(t0 + 40 * MS, seen(&patched(Rect::new(100, 110, 10, 10), 200))).is_none());
        assert!(w.step(t0 + 70 * MS, seen(&patched(Rect::new(100, 110, 10, 10), 200))).is_none(), "30 ms still");
        let o = w.step(t0 + 90 * MS, seen(&patched(Rect::new(100, 110, 10, 10), 200))).expect("50 ms still");
        assert!(o.changed && o.settled);
        assert_eq!(o.frames, 4);
    }

    /// A help bubble on screen for one round, between two plain pictures: with `settle = 0` the
    /// picture with the bubble is the answer, not the plain one after it.
    #[test]
    fn settle0_keeps_transient() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        assert!(w.step(t0, seen(&plain(10))).is_none());
        let bubble = patched(Rect::new(120, 105, 12, 8), 240);
        let o = w.step(t0 + 16 * MS, seen(&bubble)).expect("the bubble");
        assert!(Arc::ptr_eq(o.frame.as_ref().unwrap(), &bubble));
    }

    /// A flash that is gone before the wait has stood still for `settle` is not a change: the
    /// wait looks again, and at its deadline says `changed = false`.
    #[test]
    fn settle_after_a_change_that_reverts_is_not_changed() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        let bubble = patched(Rect::new(110, 110, 4, 4), 200);
        assert!(w.step(t0 + 10 * MS, seen(&bubble)).is_none(), "changed, now settling");
        assert!(w.step(t0 + 20 * MS, seen(&plain(10))).is_none(), "gone again: a new still spell");
        assert!(w.step(t0 + 80 * MS, seen(&plain(10))).is_none(), "still for 60 ms, but as it began: no change");
        let o = w.step(t0 + 500 * MS, seen(&plain(10))).expect("the deadline");
        assert!(!o.changed && !o.settled, "a picture equal to the baseline is never changed");
        // Back to looking, a real change after the flash is still seen.
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        assert!(w.step(t0 + 10 * MS, seen(&bubble)).is_none());
        assert!(w.step(t0 + 20 * MS, seen(&plain(10))).is_none());
        assert!(w.step(t0 + 80 * MS, seen(&plain(10))).is_none());
        let moved = patched(Rect::new(100, 120, 10, 10), 200);
        assert!(w.step(t0 + 90 * MS, seen(&moved)).is_none());
        let o = w.step(t0 + 150 * MS, seen(&moved)).expect("still for 60 ms, changed");
        assert!(o.changed && o.settled);
    }

    /// Small steps within the tolerance can bring the picture back to the baseline: a still spell
    /// is measured against where it began, the answer against the baseline.
    #[test]
    fn a_still_spell_that_drifted_back_is_not_changed() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        assert!(w.step(t0 + 10 * MS, seen(&plain(27))).is_none(), "17 over: changed");
        assert!(w.step(t0 + 30 * MS, seen(&plain(20))).is_none(), "7 from where the spell began");
        assert!(w.step(t0 + 70 * MS, seen(&plain(20))).is_none(), "still, but within 16 of the baseline");
        let o = w.step(t0 + 500 * MS, seen(&plain(20))).unwrap();
        assert!(!o.changed);
    }

    /// A failed round that completes a still spell answers the newest picture only while it
    /// still differs from the baseline.
    #[test]
    fn a_failed_round_after_a_reverted_change_looks_again() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        assert!(w.step(t0 + 10 * MS, seen(&patched(Rect::new(110, 110, 4, 4), 200))).is_none());
        assert!(w.step(t0 + 20 * MS, seen(&plain(10))).is_none());
        assert!(w.step(t0 + 80 * MS, Seen::Failed("no".into())).is_none(), "due, but back where it began");
        let o = w.step(t0 + 500 * MS, Seen::Failed("no".into())).expect("the deadline");
        assert!(!o.changed && o.frame.is_some());
    }

    /// Two watched rectangles that overlap count a pixel they share once: two changed pixels in
    /// the overlap are two, not four, against `minPixels = 3`.
    #[test]
    fn overlapping_watch_rects_count_a_shared_pixel_once_in_a_wait() {
        let t0 = Instant::now();
        let a = Rect::new(100, 100, 10, 10);
        let b = Rect::new(105, 100, 10, 10);
        let spec3 = ChangeSpec { min_pixels: 3, ..spec(0) };
        let mut w = Wait::new(R, &[a, b], spec3, Some(plain(10)), t0);
        let two = frame(R, 10, Some((Rect::new(106, 100, 2, 1), 200)), FrameVia::Gdi);
        assert!(w.step(t0 + 8 * MS, seen(&two)).is_none(), "two pixels, however many rectangles hold them");
        let three = frame(R, 10, Some((Rect::new(106, 100, 3, 1), 200)), FrameVia::Gdi);
        assert!(w.step(t0 + 16 * MS, seen(&three)).is_some());
    }

    /// What a wait holds: its newest picture as handed, and the pictures it only compares with
    /// as copies of the watched box without a backing image — the baseline even when it was the
    /// module's `from`, and the picture a still spell began with.
    #[test]
    fn a_wait_holds_copies_of_the_watched_box_only() {
        let t0 = Instant::now();
        let column = Rect::new(100, 100, 8, 30);
        let big_from = frame(Rect::new(0, 0, 300, 300), 10, None, FrameVia::Gdi);
        let mut w = Wait::new(R, &[column], spec(50), Some(big_from), t0);
        assert!(w.step(t0, seen(&plain(10))).is_none());
        assert_eq!(w.held(), vec![(R, false), (column, false)], "the from is not kept whole");
        assert!(w.step(t0 + 10 * MS, seen(&patched(Rect::new(100, 110, 8, 4), 200))).is_none());
        assert_eq!(w.held(), vec![(R, false), (column, false), (column, false)]);
        // Watching the whole region, a picture of exactly it is shared, not copied.
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        let first = plain(10);
        assert!(w.step(t0, seen(&first)).is_none());
        assert_eq!(w.held(), vec![(R, false), (R, false)]);
        assert!(Arc::ptr_eq(w.baseline.as_ref().unwrap(), &first));
    }

    #[test]
    fn deadline_unchanged_returns_latest() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        assert!(w.step(t0, seen(&plain(10))).is_none());
        let last = plain(12); // within the tolerance
        let o = w.step(t0 + 500 * MS, seen(&last)).expect("the deadline");
        assert!(!o.changed && !o.settled);
        assert!(Arc::ptr_eq(o.frame.as_ref().unwrap(), &last));
        assert_eq!(w.deadline(), t0 + 500 * MS);
    }

    #[test]
    fn deadline_while_settling_is_changed_unsettled() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(300), Some(plain(10)), t0);
        assert!(w.step(t0 + 400 * MS, seen(&patched(Rect::new(100, 100, 10, 10), 200))).is_none());
        let o = w.step(t0 + 500 * MS, seen(&patched(Rect::new(100, 100, 10, 10), 200))).expect("the deadline");
        assert!(o.changed && !o.settled);
    }

    #[test]
    fn failed_rounds_advance_clocks_and_complete_a_settle() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(50), Some(plain(10)), t0);
        let moved = patched(Rect::new(100, 100, 10, 10), 200);
        assert!(w.step(t0 + 10 * MS, seen(&moved)).is_none());
        assert!(w.step(t0 + 30 * MS, Seen::Failed("no".into())).is_none(), "not yet still for 50 ms");
        let o = w.step(t0 + 60 * MS, Seen::Failed("no".into())).expect("a settle completes without a new picture");
        assert!(o.changed && o.settled);
        assert!(Arc::ptr_eq(o.frame.as_ref().unwrap(), &moved));
        assert_eq!(o.frames, 1);
    }

    #[test]
    fn all_failed_is_none_with_the_last_reason() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        assert!(w.step(t0, Seen::Failed("first".into())).is_none());
        let o = w.step(t0 + 600 * MS, Seen::Failed("screen capture failed".into())).expect("the deadline");
        assert!(o.frame.is_none() && !o.changed);
        assert_eq!(o.why.as_deref(), Some("screen capture failed"));
        assert_eq!(o.frames, 0);
    }

    /// An animation outside `watch` never ends the wait; a change inside it does.
    #[test]
    fn watch_ignores_animation_outside() {
        let t0 = Instant::now();
        let column = Rect::new(100, 100, 8, 30);
        let mut w = Wait::new(R, &[column], spec(0), Some(plain(10)), t0);
        for (i, x) in [120, 124, 128].into_iter().enumerate() {
            let spinner = patched(Rect::new(x, 100, 6, 6), 230);
            assert!(w.step(t0 + (i as u32 + 1) * 8 * MS, seen(&spinner)).is_none(), "the spinner at {x}");
        }
        assert!(w.step(t0 + 40 * MS, seen(&patched(Rect::new(100, 120, 8, 4), 230))).is_some(), "the cursor");
    }

    #[test]
    fn tolerance_vs_tolerance_plus_one() {
        let a = plain(100);
        let b16 = plain(116);
        let b17 = plain(117);
        assert!(!differs(&a, &b16, &[R], 16, 1), "16 is within a tolerance of 16");
        assert!(differs(&a, &b17, &[R], 16, 1));
        assert!(differs(&a, &b16, &[R], 15, 1));
        assert!(!differs(&a, &a, &[R], 0, 1), "a frame never differs from itself");
    }

    #[test]
    fn min_pixels_counts_across_watch_rects() {
        let base = plain(10);
        let two = frame(R, 10, Some((Rect::new(100, 100, 2, 1), 200)), FrameVia::Gdi);
        let left = Rect::new(100, 100, 1, 30);
        let right = Rect::new(101, 100, 1, 30);
        assert!(differs(&base, &two, &[left, right], 16, 2), "one pixel in each rectangle: two");
        assert!(!differs(&base, &two, &[left, right], 16, 3));
        // Overlapping rectangles count a pixel once.
        let watch = disjoint(&[left, Rect::new(100, 100, 2, 30), left]);
        assert!(!differs(&base, &two, &watch, 16, 3), "{watch:?}");
        assert_eq!(watch.iter().map(|r| r.area()).sum::<i64>(), 60);
    }

    #[test]
    fn disjoint_covers_each_pixel_once() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        let d = disjoint(&[a, b]);
        assert_eq!(d.iter().map(|r| r.area()).sum::<i64>(), 175);
        for (i, r) in d.iter().enumerate() {
            for s in &d[i + 1..] {
                assert!(r.intersect(s).is_none(), "{r:?} and {s:?} overlap");
            }
        }
        assert_eq!(disjoint(&[a]), vec![a]);
        assert!(disjoint(&[]).is_empty());
        assert_eq!(disjoint(&[a, a]), vec![a]);
    }

    /// A union capture against a snapshot of the region alone: compared where both lie.
    #[test]
    fn differs_between_frames_of_different_rects() {
        let small = plain(10);
        let big = frame(Rect::new(0, 0, 300, 300), 10, Some((Rect::new(110, 110, 4, 4), 200)), FrameVia::Gdi);
        assert!(differs(&small, &big, &[R], 16, 16));
        assert!(!differs(&small, &big, &[R], 16, 17));
        assert!(!differs(&small, &big, &[Rect::new(200, 200, 50, 50)], 16, 1), "outside one of them: not compared");
    }

    #[test]
    fn via_switch_rebases_not_changes() {
        let t0 = Instant::now();
        let mut w = Wait::new(R, &[], spec(0), None, t0);
        assert!(w.step(t0, seen(&plain(10))).is_none());
        let other = frame(R, 60, None, FrameVia::Duplication);
        assert!(w.step(t0 + 8 * MS, seen(&other)).is_none(), "another path is a new baseline, not a change");
        let o = w.step(t0 + 16 * MS, seen(&frame(R, 120, None, FrameVia::Duplication))).expect("a change on the new path");
        assert!(o.changed && o.rebased);
    }

    #[test]
    fn from_other_path_rebases() {
        let t0 = Instant::now();
        let from = frame(R, 10, None, FrameVia::Duplication);
        let mut w = Wait::new(R, &[], spec(0), Some(from), t0);
        assert!(w.step(t0, seen(&plain(90))).is_none(), "the first picture is the baseline instead");
        let o = w.step(t0 + 500 * MS, seen(&plain(90))).unwrap();
        assert!(!o.changed && o.rebased);
    }

    #[test]
    fn from_missing_watch_rebases() {
        let t0 = Instant::now();
        let from = frame(Rect::new(100, 100, 10, 10), 10, None, FrameVia::Gdi);
        let mut w = Wait::new(R, &[Rect::new(120, 100, 5, 5)], spec(0), Some(from), t0);
        assert!(w.step(t0, seen(&plain(90))).is_none());
        let o = w.step(t0 + 8 * MS, seen(&patched(Rect::new(120, 100, 5, 5), 250))).expect("a change");
        assert!(o.rebased);
    }
}
