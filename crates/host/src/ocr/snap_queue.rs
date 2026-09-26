//! The snapshot lane of the `screen-capture` thread: which of `host.screen.snapshotAsync`'s
//! requests are photographed next, in which captures, and what each is answered — with no thread
//! and no clock of its own, so every rule can be tested by stepping it.
//!
//! **One capture thread.** The pictures of `host.ocr.read` and of `snapshotAsync` are the same
//! act — a capture at the call, off the event loop — so they are taken by one thread, the one
//! `ocr/service.rs` runs, and weighed against each other by one rule: the class of `sched.rs`
//! (urgent, aged, interactive, background), then whichever has been due longer, and on a tie the
//! text read ([`choose`]). A stream of change-wait rounds a controller keeps asking for cannot
//! starve a poll's text read beyond `AGING`, and a long text capture delays a due round by that
//! one capture.
//!
//! **Three kinds of request.** A plain one is photographed as soon as the thread can; a timed one
//! (`at`) not before its moment; a change wait (`change`, `change.rs`) once per round from its
//! start until it ends, rounds no closer than [`min_round`] from start to start and never later
//! than its deadline.
//!
//! **A round** is the most urgent due request and the others that can share its picture
//! ([`SnapLane::take`]). Through the standard path that is ONE capture: every due request whose
//! region lies inside the round's rectangle, or whose union with it covers at most `UNION_MAX`
//! pixels, on the same display — in order of urgency, so a request about to be clicked past is
//! never photographed after another module's large capture; the others go next round. Through
//! desktop duplication every due request of that source goes, each distinct region a piece of
//! one request ([`plan_round`]). The comparisons of change waits, and the cutting of each answer
//! out of a shared capture, happen in [`Round::run`], outside the service's lock; a change wait
//! is handed its own region cut out, never the shared capture (`change.rs`, "What it holds").
//!
//! **Exactly one answer per request** while the thread runs — a picture, a failure, or
//! `Cancelled` for one whose flag the event loop set (a newer request with its key, its module
//! disabled, reloaded or removed; the event loop has already answered those or dropped them).
//!
//! **Read, then act.** A plain request, and a change wait without `at` whose baseline will be its
//! own first picture, hold the owner's `host.input.*` and `host.window.focus` (the service's
//! barrier) until that picture is taken, the way a pending text read does: the picture is of the
//! screen before the click ([`holds_input`]).
//!
//! **Which input a picture predates.** Each picture carries `host.inputEpoch()` as it stood once
//! the capture had come back (`Frame::input_epoch`, from the service's mirror of it): input the
//! host drives after that turns the epoch over after the picture was taken.
//!
//! Pure apart from the frame type; borrowed by `crates/macos-check`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::backend::frame::{Frame, FrameVia};
use crate::backend::{CaptureSource, CAPTURE_FAILED};

use super::change::{Outcome, Seen, Wait, NO_PICTURE};
use super::policy::{AGING, MIN_ROUND_DUPLICATION, MIN_ROUND_MACOS, MIN_ROUND_STANDARD, UNION_MAX};
use super::sched::{Cand, Class, Owner};
use super::types::{Priority, Rect};

pub type SnapId = u64;

/// Whose request it is, and in which lane it waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapTicket {
    pub id: SnapId,
    pub owner: Owner,
    pub prio: Priority,
}

pub enum SnapKind {
    /// One picture, as soon as the thread can take it.
    Plain,
    /// One picture, not taken before this moment.
    At(Instant),
    /// Pictures from `start` on, until the wait ends.
    Change { wait: Box<Wait>, start: Instant },
}

/// One request, as the event loop hands it to the thread.
pub struct SnapReq {
    pub ticket: SnapTicket,
    /// Screen coordinates: what the answer is a picture of.
    pub region: Rect,
    pub source: CaptureSource,
    pub kind: SnapKind,
    pub asked: Instant,
    /// Holds the owner's input until its first picture is taken (see the file's comment).
    pub holds: bool,
    /// Set by the event loop when nobody waits for the answer any more.
    pub cancel: Arc<AtomicBool>,
}

impl SnapReq {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }

    fn first_due(&self) -> Instant {
        match &self.kind {
            SnapKind::Plain => self.asked,
            SnapKind::At(t) => *t,
            SnapKind::Change { start, .. } => *start,
        }
    }

    fn is_wait(&self) -> bool {
        matches!(self.kind, SnapKind::Change { .. })
    }
}

/// How a change wait ended, beside its picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeInfo {
    pub changed: bool,
    pub settled: bool,
    pub rebased: bool,
}

pub enum SnapOutcome {
    /// Of exactly the request's region; `frames` pictures were taken for it.
    Picture { frame: Arc<Frame>, frames: u32, change: Option<ChangeInfo> },
    Failed { why: String, frames: u32, change: Option<ChangeInfo> },
    /// Its flag was set: the event loop answers or drops it itself.
    Cancelled,
}

/// A request's one answer.
pub struct SnapDone {
    pub id: SnapId,
    pub outcome: SnapOutcome,
    pub asked: Instant,
    /// When the thread answered it.
    pub ended: Instant,
}

/// What a request is answered when the host's own code for its round panicked.
pub const ROUND_FAILED: &str = "the snapshot failed inside the application; the log has the details";

/// The least time between the starts of two rounds of a change wait through `source`, decided
/// by the platform, not by the module (see `policy`).
pub fn min_round(source: CaptureSource) -> Duration {
    if cfg!(target_os = "macos") {
        return MIN_ROUND_MACOS;
    }
    match source {
        CaptureSource::Standard => MIN_ROUND_STANDARD,
        CaptureSource::Duplication { .. } => MIN_ROUND_DUPLICATION,
    }
}

/// Which queue the capture thread serves next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Ocr,
    Snap,
}

/// The lower class first, then the one due longer; on a tie the text read, which was there
/// before the lane was.
pub fn choose(ocr: Option<Cand>, lane: Option<Cand>) -> Option<Pick> {
    match (ocr, lane) {
        (None, None) => None,
        (Some(_), None) => Some(Pick::Ocr),
        (None, Some(_)) => Some(Pick::Snap),
        (Some(o), Some(l)) => Some(if (l.class, l.due) < (o.class, o.due) { Pick::Snap } else { Pick::Ocr }),
    }
}

/// The captures that answer `regions` through `source`, and which capture each region is cut
/// from ([`group`]): the standard path unions regions up to `UNION_MAX` pixels a capture;
/// desktop duplication gets every region no other piece holds as a piece of one request.
pub fn plan_round(regions: &[Rect], source: CaptureSource) -> (Vec<Rect>, Vec<usize>) {
    group(regions, if source == CaptureSource::Standard { UNION_MAX } else { 0 })
}

/// `regions` as few captures as `max` allows, in order: a region inside a capture planned
/// already is cut from it; otherwise, with `max` above 0, the first capture whose union with it
/// covers at most `max` pixels grows to hold it; otherwise it is a capture of its own. An empty
/// region is always one of its own, so it fails alone. Which capture each region is cut from.
pub fn group(regions: &[Rect], max: i64) -> (Vec<Rect>, Vec<usize>) {
    let mut rects: Vec<Rect> = Vec::new();
    let at = regions
        .iter()
        .map(|r| {
            if !r.is_empty() {
                if let Some(i) = rects.iter().position(|c| c.contains(r)) {
                    return i;
                }
                if max > 0 {
                    if let Some(i) = rects.iter().position(|c| !c.is_empty() && c.union(r).area() <= max) {
                        rects[i] = rects[i].union(r);
                        return i;
                    }
                }
            }
            rects.push(*r);
            rects.len() - 1
        })
        .collect();
    (rects, at)
}

/// Whether a request holds its module's input until its first picture is taken: it is taken
/// at once (not `timed`), and either it is not a change wait, or it is one whose baseline will be
/// that first picture — it has no `from`, or one [`from_usable`] says it cannot compare with. A
/// click made before that picture would be in it.
pub fn holds_input(timed: bool, change: bool, from_usable: bool) -> bool {
    !timed && !(change && from_usable)
}

/// The way a request through `source` is photographed when nothing falls back: the standard
/// path's `Gdi`, desktop duplication's `Duplication` — or `None` on macOS, where the call cannot
/// tell ScreenCaptureKit's pictures from the older functions' in advance.
pub fn first_choice_via(source: CaptureSource) -> Option<FrameVia> {
    if cfg!(target_os = "macos") {
        return None;
    }
    Some(match source {
        CaptureSource::Standard => FrameVia::Gdi,
        CaptureSource::Duplication { .. } => FrameVia::Duplication,
    })
}

/// Whether a change wait over `region` watching `watch` (screen rectangles, all of `region`
/// when empty) can compare its first round with `from`, as far as the call can tell: `from`
/// holds every watched rectangle cut to the region, and was taken the way the rounds will be
/// when nothing falls back. The wait itself decides again at its first round (`change.rs`).
pub fn from_usable(from: &Frame, region: Rect, watch: &[Rect], source: CaptureSource) -> bool {
    let holds = |w: &Rect| region.intersect(w).is_some_and(|c| from.rect.contains(&c));
    let covered = if watch.is_empty() { holds(&region) } else { watch.iter().all(holds) };
    covered && first_choice_via(source).is_none_or(|v| v == from.via)
}

struct Entry {
    req: SnapReq,
    /// Not photographed before this.
    due: Instant,
    /// Its owner is about to act (`expedite`).
    urgent: bool,
}

/// The requests waiting for a round. See the file's comment.
pub struct SnapLane {
    waiting: Vec<Entry>,
    /// The requests of the round being taken now: (id, owner index, holds input).
    taking: Vec<(SnapId, usize, bool)>,
    /// Which display a point lies on (`OcrWorker::display_of`): one standard capture never
    /// spans two.
    display_of: fn(i32, i32) -> u32,
}

impl Default for SnapLane {
    fn default() -> SnapLane {
        SnapLane::new(|_, _| 0)
    }
}

/// One round: the captures to take, and the requests they answer.
pub struct Round {
    pub source: CaptureSource,
    pub rects: Vec<Rect>,
    /// Every request is a change wait: the macOS flat-picture watch is not run on a picture that
    /// is asked for again and again.
    pub poll: bool,
    members: Vec<(SnapReq, usize)>,
    started: Instant,
}

enum Step {
    Done { id: SnapId, asked: Instant, outcome: SnapOutcome, cancel: Arc<AtomicBool> },
    /// A change wait that goes on.
    Again(SnapReq),
}

/// A round's results, to be handed back to the lane under the lock.
pub struct RoundDone {
    steps: Vec<Step>,
    source: CaptureSource,
    started: Instant,
    /// What panicked in the host's own code, for the log.
    pub panics: Vec<String>,
}

impl SnapLane {
    /// An empty lane whose rounds ask `display_of` which display a region's centre lies on.
    pub fn new(display_of: fn(i32, i32) -> u32) -> SnapLane {
        SnapLane { waiting: Vec::new(), taking: Vec::new(), display_of }
    }

    fn display(&self, r: &Rect) -> u32 {
        let centre = |a: i32, len: i32| (a as i64 + len as i64 / 2).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        (self.display_of)(centre(r.x, r.w), centre(r.y, r.h))
    }

    pub fn submit(&mut self, req: SnapReq) {
        let due = req.first_due();
        self.waiting.push(Entry { req, due, urgent: false });
    }

    /// Requests waiting or being taken.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.waiting.len() + self.taking.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Requests in the round being taken now.
    #[cfg(test)]
    pub fn taking(&self) -> usize {
        self.taking.len()
    }

    fn class(e: &Entry, now: Instant) -> Class {
        if e.urgent {
            return Class::Urgent;
        }
        match e.req.ticket.prio {
            Priority::Interactive => Class::Interactive,
            Priority::Background if now.saturating_duration_since(e.due) >= AGING => Class::Aged,
            Priority::Background => Class::Background,
        }
    }

    fn ready(e: &Entry, now: Instant) -> bool {
        e.due <= now && !e.req.cancelled()
    }

    /// The most urgent request that is due, as the capture thread weighs it against the text
    /// reads' next capture.
    pub fn candidate(&self, now: Instant) -> Option<Cand> {
        self.waiting
            .iter()
            .filter(|e| Self::ready(e, now))
            .map(|e| Cand { class: Self::class(e, now), due: e.due })
            .min_by_key(|c| (c.class, c.due))
    }

    /// The round of the most urgent due request, first in it, and the other due requests of its
    /// source that share its picture, in order of urgency (see the file's comment): through the
    /// standard path the ones inside the round's rectangle or within `UNION_MAX` pixels of union
    /// with it on the same display, one capture; through desktop duplication all of them. The
    /// rest stay due and go next.
    pub fn take(&mut self, now: Instant) -> Option<Round> {
        let key = |e: &Entry| (Self::class(e, now), e.due);
        let first = (0..self.waiting.len())
            .filter(|&i| Self::ready(&self.waiting[i], now))
            .min_by_key(|&i| key(&self.waiting[i]))?;
        let source = self.waiting[first].req.source;
        let mut others: Vec<usize> = (0..self.waiting.len())
            .filter(|&i| i != first && Self::ready(&self.waiting[i], now) && self.waiting[i].req.source == source)
            .collect();
        others.sort_by_key(|&i| key(&self.waiting[i]));
        let mut chosen = vec![first];
        if source == CaptureSource::Standard {
            let screen = self.display(&self.waiting[first].req.region);
            let mut rect = self.waiting[first].req.region;
            for i in others {
                let r = self.waiting[i].req.region;
                let grown = rect.union(&r);
                if rect.contains(&r) || (grown.area() <= UNION_MAX && self.display(&r) == screen) {
                    rect = grown;
                    chosen.push(i);
                }
            }
        } else {
            chosen.extend(others);
        }
        let mut slots: Vec<Option<Entry>> = std::mem::take(&mut self.waiting).into_iter().map(Some).collect();
        let members: Vec<Entry> = chosen.iter().filter_map(|&i| slots[i].take()).collect();
        self.waiting = slots.into_iter().flatten().collect();
        self.taking = members.iter().map(|e| (e.req.ticket.id, e.req.ticket.owner.idx, e.req.holds)).collect();
        let regions: Vec<Rect> = members.iter().map(|e| e.req.region).collect();
        let (rects, at) = plan_round(&regions, source);
        let poll = members.iter().all(|e| e.req.is_wait());
        Some(Round { source, rects, poll, members: members.into_iter().map(|e| e.req).zip(at).collect(), started: now })
    }

    /// When the next waiting request falls due, for the thread's sleep; `None` when none waits.
    pub fn next_wake(&self) -> Option<Instant> {
        self.waiting.iter().map(|e| e.due).min()
    }

    /// Every waiting request whose flag the event loop set, answered `Cancelled`.
    pub fn sweep_cancelled(&mut self, now: Instant) -> Vec<SnapDone> {
        let (gone, keep): (Vec<Entry>, Vec<Entry>) =
            std::mem::take(&mut self.waiting).into_iter().partition(|e| e.req.cancelled());
        self.waiting = keep;
        gone.into_iter()
            .map(|e| SnapDone { id: e.req.ticket.id, outcome: SnapOutcome::Cancelled, asked: e.req.asked, ended: now })
            .collect()
    }

    /// Whether module `idx` has no request that holds its input.
    pub fn barrier_clear(&self, idx: usize) -> bool {
        !self.waiting.iter().any(|e| e.req.holds && e.req.ticket.owner.idx == idx && !e.req.cancelled())
            && !self.taking.iter().any(|&(_, i, holds)| holds && i == idx)
    }

    /// Module `idx` is about to act: its requests that hold its input go first. True when it has
    /// any waiting.
    pub fn expedite(&mut self, idx: usize) -> bool {
        let mut any = false;
        for e in self.waiting.iter_mut().filter(|e| e.req.holds && e.req.ticket.owner.idx == idx) {
            e.urgent = true;
            any = true;
        }
        any
    }

    /// A round's results: the requests it ended are answered — `Cancelled` when their flag was
    /// set meanwhile — and the change waits that go on wait for their next round, no sooner than
    /// `min_round` after this one started and no later than their deadline.
    pub fn finish(&mut self, done: RoundDone, now: Instant) -> Vec<SnapDone> {
        self.taking.clear();
        let gap = min_round(done.source);
        let mut out = Vec::new();
        for step in done.steps {
            match step {
                Step::Done { id, asked, outcome, cancel } => {
                    let outcome =
                        if cancel.load(Ordering::Acquire) { SnapOutcome::Cancelled } else { outcome };
                    out.push(SnapDone { id, outcome, asked, ended: now });
                }
                Step::Again(mut req) => {
                    if req.cancelled() {
                        out.push(SnapDone { id: req.ticket.id, outcome: SnapOutcome::Cancelled, asked: req.asked, ended: now });
                        continue;
                    }
                    req.holds = false;
                    let deadline = match &req.kind {
                        SnapKind::Change { wait, .. } => wait.deadline(),
                        _ => now,
                    };
                    let due = (done.started + gap).min(deadline);
                    self.waiting.push(Entry { req, due, urgent: false });
                }
            }
        }
        out
    }
}

impl Round {
    #[cfg(test)]
    pub fn ids(&self) -> Vec<SnapId> {
        self.members.iter().map(|(r, _)| r.ticket.id).collect()
    }

    /// The captures as the backend takes them.
    pub fn tuples(&self) -> Vec<(i32, i32, i32, i32)> {
        self.rects.iter().map(Rect::tuple).collect()
    }

    /// Answers every request of the round from `frames`, one per planned capture in order (a
    /// missing one, or one that does not cover its rectangle, is a failed capture), each picture
    /// stamped with `input_epoch` — `host.inputEpoch()` once the captures had come back: a plain
    /// or timed request its picture, cut out of a shared capture when it was one; a change wait
    /// the comparison, and its picture when it ended. A panic in the host's own code for one
    /// request answers that request, and the rest are still answered; one anywhere else in here
    /// answers every request of the round [`ROUND_FAILED`], so the round is finished all the same.
    pub fn run(self, frames: Vec<Result<Frame, String>>, now: Instant, input_epoch: u64) -> RoundDone {
        let answers: Vec<Step> = self
            .members
            .iter()
            .map(|(req, _)| Step::Done {
                id: req.ticket.id,
                asked: req.asked,
                outcome: SnapOutcome::Failed { why: ROUND_FAILED.to_string(), frames: 0, change: req.is_wait().then(ChangeInfo::default) },
                cancel: req.cancel.clone(),
            })
            .collect();
        let (source, started) = (self.source, self.started);
        match crate::logging::contain(move || self.run_members(frames, now, input_epoch)) {
            Ok(done) => done,
            Err(report) => RoundDone { steps: answers, source, started, panics: vec![report] },
        }
    }

    fn run_members(self, frames: Vec<Result<Frame, String>>, now: Instant, input_epoch: u64) -> RoundDone {
        test_panic_round(&self.rects);
        let mut frames = frames.into_iter();
        let shots: Vec<Result<Arc<Frame>, String>> = self
            .rects
            .iter()
            .map(|r| match frames.next() {
                Some(Ok(mut f)) if f.rect.contains(r) => {
                    f.input_epoch = input_epoch;
                    Ok(Arc::new(f))
                }
                Some(Ok(_)) | None => Err(CAPTURE_FAILED.to_string()),
                Some(Err(why)) => Err(why),
            })
            .collect();
        let mut panics = Vec::new();
        let steps = self
            .members
            .into_iter()
            .map(|(req, at)| {
                let (id, asked, cancel) = (req.ticket.id, req.asked, req.cancel.clone());
                let change = req.is_wait().then(ChangeInfo::default);
                let shot = &shots[at];
                match crate::logging::contain(move || step_member(req, shot, now)) {
                    Ok(step) => step,
                    Err(report) => {
                        panics.push(report);
                        Step::Done {
                            id,
                            asked,
                            outcome: SnapOutcome::Failed { why: ROUND_FAILED.to_string(), frames: 0, change },
                            cancel,
                        }
                    }
                }
            })
            .collect();
        RoundDone { steps, source: self.source, started: self.started, panics }
    }

    /// Every request of the round answered with `why`: the capture itself panicked. A change
    /// wait counts it as a failed round and may go on.
    pub fn fail(self, why: &str, now: Instant) -> RoundDone {
        let n = self.rects.len();
        self.run((0..n).map(|_| Err(why.to_string())).collect(), now, 0)
    }
}

/// A test's way to make the host's own code panic inside a round: for one request at this x,
/// and for the whole round when a capture is planned at `PANIC_ROUND_X`.
#[cfg(test)]
pub(crate) const PANIC_X: i32 = -1_234_567;
#[cfg(test)]
pub(crate) const PANIC_ROUND_X: i32 = -7_654_321;

#[cfg(test)]
fn test_panic(region: Rect) {
    if region.x == PANIC_X {
        panic!("{}inside a snapshot round", crate::EXPECTED_PANIC);
    }
}

#[cfg(test)]
fn test_panic_round(rects: &[Rect]) {
    if rects.iter().any(|r| r.x == PANIC_ROUND_X) {
        panic!("{}inside a snapshot round, outside its requests", crate::EXPECTED_PANIC);
    }
}

#[cfg(not(test))]
fn test_panic(_: Rect) {}

#[cfg(not(test))]
fn test_panic_round(_: &[Rect]) {}

fn step_member(mut req: SnapReq, shot: &Result<Arc<Frame>, String>, now: Instant) -> Step {
    let (id, asked, region, cancel) = (req.ticket.id, req.asked, req.region, req.cancel.clone());
    test_panic(region);
    if let SnapKind::Change { wait, .. } = &mut req.kind {
        // Its own region, cut out of a shared capture: what a wait keeps between rounds must
        // never be a capture of other requests' regions too (`change.rs`, "What it holds").
        let seen = match shot {
            Ok(f) => match cut(f, region) {
                Some(g) => Seen::Frame(g),
                None => Seen::Failed(CAPTURE_FAILED.to_string()),
            },
            Err(why) => Seen::Failed(why.clone()),
        };
        let ended = wait.step(now, seen);
        return match ended {
            None => Step::Again(req),
            Some(o) => Step::Done { id, asked, outcome: outcome_of(o, region), cancel },
        };
    }
    let outcome = match shot {
        Ok(f) => match cut(f, region) {
            Some(g) => SnapOutcome::Picture { frame: g, frames: 1, change: None },
            None => SnapOutcome::Failed { why: CAPTURE_FAILED.to_string(), frames: 0, change: None },
        },
        Err(why) => SnapOutcome::Failed { why: why.clone(), frames: 0, change: None },
    };
    Step::Done { id, asked, outcome, cancel }
}

/// A wait's end as its answer: the picture cut to the wait's region.
fn outcome_of(o: Outcome, region: Rect) -> SnapOutcome {
    let change = Some(ChangeInfo { changed: o.changed, settled: o.settled, rebased: o.rebased });
    match o.frame {
        Some(f) => match cut(&f, region) {
            Some(g) => SnapOutcome::Picture { frame: g, frames: o.frames, change },
            None => SnapOutcome::Failed { why: CAPTURE_FAILED.to_string(), frames: o.frames, change },
        },
        None => SnapOutcome::Failed { why: o.why.unwrap_or_else(|| NO_PICTURE.to_string()), frames: o.frames, change },
    }
}

/// `r` out of `f`: `f` itself when it is exactly `r`, a copy of the part otherwise.
fn cut(f: &Arc<Frame>, r: Rect) -> Option<Arc<Frame>> {
    if f.rect == r {
        Some(f.clone())
    } else {
        f.crop(r).map(Arc::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::frame::FrameVia;
    use crate::backend::CapturedImage;
    use crate::ocr::change::ChangeSpec;
    use crate::ocr::sched::{Scheduler, Ticket};
    use std::collections::HashMap;

    const MS: Duration = Duration::from_millis(1);
    const A: Owner = Owner { idx: 1, gen: 1 };
    const B: Owner = Owner { idx: 2, gen: 1 };
    const DUP: CaptureSource = CaptureSource::Duplication { or_standard: true };

    fn solid(r: Rect, v: u8, taken: Instant) -> Frame {
        let img = CapturedImage { w: r.w as u32, h: r.h as u32, rgba: vec![v; (r.w * r.h * 4) as usize] };
        Frame::from_image(r, img, taken, FrameVia::Gdi, None).expect("a whole frame")
    }

    fn req(id: SnapId, owner: Owner, region: Rect, kind: SnapKind, asked: Instant) -> SnapReq {
        let holds = matches!(kind, SnapKind::Plain);
        SnapReq {
            ticket: SnapTicket { id, owner, prio: Priority::Background },
            region,
            source: CaptureSource::Standard,
            kind,
            asked,
            holds,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn wait_kind(region: Rect, timeout_ms: u64, settle_ms: u64, start: Instant) -> SnapKind {
        let spec = ChangeSpec {
            tolerance: 16,
            min_pixels: 4,
            settle: Duration::from_millis(settle_ms),
            timeout: Duration::from_millis(timeout_ms),
        };
        SnapKind::Change { wait: Box::new(Wait::new(region, &[], spec, None, start)), start }
    }

    /// One turn of the thread: sweep, take a round if one is due, photograph it with `shoot`,
    /// run it and finish it. The rectangles captured, and every answer.
    fn turn(lane: &mut SnapLane, now: Instant, shoot: &dyn Fn(Rect) -> Result<Frame, String>) -> (Vec<Rect>, Vec<SnapDone>) {
        let mut out = lane.sweep_cancelled(now);
        let Some(round) = lane.take(now) else { return (Vec::new(), out) };
        let rects = round.rects.clone();
        let frames = rects.iter().map(|r| shoot(*r)).collect();
        let done = round.run(frames, now, 0);
        out.extend(lane.finish(done, now));
        (rects, out)
    }

    fn picture(d: &SnapDone) -> &Arc<Frame> {
        match &d.outcome {
            SnapOutcome::Picture { frame, .. } => frame,
            _ => panic!("request {} has no picture", d.id),
        }
    }

    #[test]
    fn identical_plain_requests_share_one_capture() {
        let t0 = Instant::now();
        let r = Rect::new(10, 10, 20, 20);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::Plain, t0));
        lane.submit(req(2, B, r, SnapKind::Plain, t0));
        let (rects, done) = turn(&mut lane, t0, &|r| Ok(solid(r, 9, t0)));
        assert_eq!(rects, vec![r], "one capture");
        assert_eq!(done.len(), 2);
        assert!(Arc::ptr_eq(picture(&done[0]), picture(&done[1])), "one picture for both");
        assert!(lane.is_empty());
    }

    #[test]
    fn union_capture_le_2mpx_else_each() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(900, 900, 100, 100);
        let (rects, at) = plan_round(&[a, b, a], CaptureSource::Standard);
        assert_eq!(rects, vec![Rect::new(0, 0, 1000, 1000)], "a million pixels: one capture of the union");
        assert_eq!(at, vec![0, 0, 0]);
        let far = Rect::new(3000, 3000, 100, 100);
        let (rects, at) = plan_round(&[a, far, a], CaptureSource::Standard);
        assert_eq!(rects, vec![a, far], "a union of 3100x3100 is past 2 million pixels: one capture each");
        assert_eq!(at, vec![0, 1, 0]);
    }

    #[test]
    fn duplication_members_go_in_one_request() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(20, 0, 10, 10);
        let (rects, at) = plan_round(&[a, b, a], DUP);
        assert_eq!(rects, vec![a, b], "each distinct region a piece of one request");
        assert_eq!(at, vec![0, 1, 0]);
        // In the lane: a round takes only the requests of the source it was chosen for.
        let t0 = Instant::now();
        let mut lane = SnapLane::default();
        let mut d1 = req(1, A, a, SnapKind::Plain, t0);
        d1.source = DUP;
        let mut d2 = req(2, A, b, SnapKind::Plain, t0);
        d2.source = DUP;
        lane.submit(d1);
        lane.submit(req(3, A, a, SnapKind::Plain, t0 + MS));
        lane.submit(d2);
        let round = lane.take(t0 + MS).unwrap();
        assert_eq!(round.source, DUP);
        assert_eq!(round.ids(), vec![1, 2]);
        assert_eq!(round.rects, vec![a, b]);
    }

    #[test]
    fn at_is_not_captured_before_due() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::At(t0 + 80 * MS), t0));
        assert!(lane.candidate(t0 + 79 * MS).is_none());
        assert_eq!(lane.next_wake(), Some(t0 + 80 * MS));
        let (rects, done) = turn(&mut lane, t0 + 79 * MS, &|r| Ok(solid(r, 1, t0)));
        assert!(rects.is_empty() && done.is_empty());
        let (_, done) = turn(&mut lane, t0 + 80 * MS, &|r| Ok(solid(r, 1, t0 + 80 * MS)));
        assert_eq!(done.len(), 1);
        assert!(lane.barrier_clear(A.idx), "a timed snapshot never holds input");
    }

    #[test]
    fn at_in_the_past_is_immediate() {
        let t0 = Instant::now();
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, Rect::new(0, 0, 10, 10), SnapKind::At(t0), t0 + 30 * MS));
        assert!(lane.candidate(t0 + 30 * MS).is_some());
    }

    /// Rounds of a change wait start `min_round` apart, and none after its deadline.
    #[test]
    fn wait_rounds_respect_min_round_per_source() {
        for source in [CaptureSource::Standard, DUP] {
            let gap = min_round(source);
            let t0 = Instant::now();
            let r = Rect::new(0, 0, 10, 10);
            let mut lane = SnapLane::default();
            let mut w = req(1, A, r, wait_kind(r, 200, 0, t0), t0);
            w.source = source;
            lane.submit(w);
            let mut starts = Vec::new();
            let mut ended = None;
            // A 1 ms clock, as fine as anything the thread could wake at.
            for ms in 0..400u32 {
                let now = t0 + ms * MS;
                let (rects, done) = turn(&mut lane, now, &|r| Ok(solid(r, 5, now)));
                if !rects.is_empty() {
                    starts.push(ms);
                }
                if let Some(d) = done.into_iter().next() {
                    ended = Some(ms);
                    assert!(matches!(d.outcome, SnapOutcome::Picture { change: Some(ChangeInfo { changed: false, .. }), .. }));
                    break;
                }
            }
            let gap_ms = gap.as_millis() as u32;
            // The last round is at the deadline, however soon after the one before.
            assert!(starts.windows(2).all(|w| w[1] - w[0] >= gap_ms || w[1] == 200), "{source:?}: {starts:?}");
            assert_eq!(ended, Some(200), "{source:?}: the round at the deadline ends it");
        }
    }

    #[test]
    fn cancel_stops_captures_at_the_next_round() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        let w = req(1, A, r, wait_kind(r, 500, 0, t0), t0);
        let cancel = w.cancel.clone();
        lane.submit(w);
        let (rects, _) = turn(&mut lane, t0, &|r| Ok(solid(r, 5, t0)));
        assert_eq!(rects.len(), 1);
        cancel.store(true, Ordering::Release);
        let later = t0 + 20 * MS;
        let (rects, done) = turn(&mut lane, later, &|_| panic!("a cancelled wait was photographed"));
        assert!(rects.is_empty());
        assert!(matches!(done[0].outcome, SnapOutcome::Cancelled));
        assert!(lane.is_empty());
    }

    #[test]
    fn cancel_during_a_round_is_answered_on_finish() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        let p = req(1, A, r, SnapKind::Plain, t0);
        let flag = p.cancel.clone();
        lane.submit(p);
        let w = req(2, A, r, wait_kind(r, 500, 0, t0), t0);
        let wflag = w.cancel.clone();
        lane.submit(w);
        let round = lane.take(t0).unwrap();
        let done = round.run(vec![Ok(solid(r, 1, t0))], t0, 0);
        flag.store(true, Ordering::Release);
        wflag.store(true, Ordering::Release);
        let out = lane.finish(done, t0);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|d| matches!(d.outcome, SnapOutcome::Cancelled)));
        assert!(lane.is_empty(), "the cancelled wait is not queued again");
    }

    #[test]
    fn barrier_counts_plain_and_first_frame_without_from_only() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        assert!(lane.barrier_clear(A.idx));
        lane.submit(req(1, A, r, SnapKind::At(t0 + 50 * MS), t0));
        assert!(lane.barrier_clear(A.idx), "a timed one does not hold");
        let mut w = req(2, A, r, wait_kind(r, 500, 0, t0), t0);
        w.holds = true; // no from, no at
        lane.submit(w);
        assert!(!lane.barrier_clear(A.idx), "a wait's first picture holds");
        assert!(lane.barrier_clear(B.idx), "another module's requests do not hold A's input");
        let round = lane.take(t0).unwrap();
        assert!(!lane.barrier_clear(A.idx), "being taken is not taken");
        let done = round.run(vec![Ok(solid(r, 1, t0))], t0, 0);
        lane.finish(done, t0);
        assert!(lane.barrier_clear(A.idx), "its later rounds do not hold");
        lane.submit(req(3, A, r, SnapKind::Plain, t0));
        assert!(!lane.barrier_clear(A.idx), "a plain one holds");
    }

    #[test]
    fn expedite_makes_plain_urgent() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::Plain, t0));
        assert_eq!(lane.candidate(t0).unwrap().class, Class::Background);
        assert!(!lane.expedite(B.idx));
        assert!(lane.expedite(A.idx));
        assert_eq!(lane.candidate(t0).unwrap().class, Class::Urgent);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::At(t0), t0));
        assert!(!lane.expedite(A.idx), "a timed one is never hurried: it does not hold input");
    }

    #[test]
    fn choose_orders_classes_then_due() {
        let t0 = Instant::now();
        let c = |class, ms: u32| Some(Cand { class, due: t0 + ms * MS });
        assert_eq!(choose(None, None), None);
        assert_eq!(choose(c(Class::Background, 0), None), Some(Pick::Ocr));
        assert_eq!(choose(None, c(Class::Background, 0)), Some(Pick::Snap));
        assert_eq!(choose(c(Class::Interactive, 0), c(Class::Urgent, 9)), Some(Pick::Snap), "the lower class");
        assert_eq!(choose(c(Class::Aged, 9), c(Class::Interactive, 0)), Some(Pick::Ocr));
        assert_eq!(choose(c(Class::Interactive, 5), c(Class::Interactive, 3)), Some(Pick::Snap), "due longer");
        assert_eq!(choose(c(Class::Interactive, 3), c(Class::Interactive, 3)), Some(Pick::Ocr), "a tie: the text read");
    }

    /// A change wait asked from a controller press polls as fast as its rounds may for two
    /// seconds; a poll's text read asked meanwhile is photographed once it has waited `AGING`,
    /// not after the wait. Simulated: each capture takes 17 ms, as a standard one does on
    /// Windows — or the platform's least time between rounds when that is longer (33 ms on
    /// macOS), so the rounds stay back to back and the lane always has one due.
    #[test]
    fn a_stream_of_interactive_rounds_lets_an_aged_ocr_capture_through() {
        let cost = min_round(CaptureSource::Standard).max(17 * MS);
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        let mut w = req(1, A, r, wait_kind(r, 2000, 0, t0), t0);
        w.ticket.prio = Priority::Interactive;
        lane.submit(w);
        let mut sched: Scheduler<&'static str, u32> = Scheduler::new();
        sched.submit("poll", Ticket { id: 1, owner: B, key: None, prio: Priority::Background }, t0 + 5 * MS);
        let mut now = t0;
        let mut ocr_at = None;
        while now < t0 + 2100 * MS {
            match choose(sched.peek_capture(false, now), lane.candidate(now)) {
                Some(Pick::Ocr) => {
                    sched.take_capture(false, now).unwrap();
                    ocr_at = Some(now - t0);
                    break;
                }
                Some(Pick::Snap) => {
                    let round = lane.take(now).unwrap();
                    now += cost;
                    let done = round.run(vec![Ok(solid(r, 5, now))], now, 0);
                    lane.finish(done, now);
                }
                None => now += MS,
            }
        }
        let at = ocr_at.expect("the poll's picture was never taken");
        assert!(at >= AGING && at < AGING + 40 * MS, "{at:?}");
    }

    #[test]
    fn a_panicking_round_answers_its_members() {
        crate::quiet_expected_panics();
        let t0 = Instant::now();
        let bad = Rect::new(PANIC_X, 0, 10, 10);
        let good = Rect::new(PANIC_X + 20, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, bad, SnapKind::Plain, t0));
        lane.submit(req(2, A, good, SnapKind::Plain, t0));
        let round = lane.take(t0).unwrap();
        assert_eq!(round.ids(), vec![1, 2], "one round, one capture of both");
        let frames = round.rects.iter().map(|r| Ok(solid(*r, 1, t0))).collect();
        let done = round.run(frames, t0, 0);
        assert_eq!(done.panics.len(), 1);
        let out = lane.finish(done, t0);
        let by_id: HashMap<SnapId, &SnapOutcome> = out.iter().map(|d| (d.id, &d.outcome)).collect();
        assert!(matches!(by_id[&1], SnapOutcome::Failed { why, .. } if why == ROUND_FAILED));
        assert!(matches!(by_id[&2], SnapOutcome::Picture { .. }), "the other request is answered all the same");
    }

    /// A capture that failed answers a plain request with its reason, and is a failed round for
    /// a change wait, which goes on.
    #[test]
    fn a_failed_capture_fails_plain_and_steps_waits() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::Plain, t0));
        lane.submit(req(2, A, r, wait_kind(r, 500, 0, t0), t0));
        let (_, done) = turn(&mut lane, t0, &|_| Err("screen capture failed".into()));
        assert_eq!(done.len(), 1);
        assert!(matches!(&done[0].outcome, SnapOutcome::Failed { why, frames: 0, change: None } if why == "screen capture failed"));
        assert_eq!(lane.len(), 1, "the wait goes on");
        // A frame that does not cover its rectangle is never cut from. The wait's next round is
        // due the platform's least time between rounds later.
        let later = t0 + min_round(CaptureSource::Standard);
        let round = lane.take(later).unwrap();
        let done = round.run(vec![Ok(solid(Rect::new(0, 0, 5, 5), 1, t0))], later, 0);
        assert!(lane.finish(done, later).is_empty());
    }

    /// `group`: a region inside a planned capture is cut from it, the standard path grows a
    /// capture up to its limit, duplication never unions, and an empty region fails alone.
    #[test]
    fn group_cuts_contained_regions_and_unions_up_to_the_limit() {
        let big = Rect::new(0, 0, 1000, 1000);
        let inside = Rect::new(10, 10, 20, 20);
        let far = Rect::new(3000, 0, 10, 10);
        assert_eq!(group(&[inside, big, inside], 0), (vec![inside, big], vec![0, 1, 0]), "a region planned before its container is its own piece");
        assert_eq!(group(&[big, inside, far], 0), (vec![big, far], vec![0, 0, 1]), "inside a planned piece: cut from it");
        assert_eq!(group(&[big, inside, far], UNION_MAX), (vec![big, far], vec![0, 0, 1]), "3010x1000 is past the limit");
        let near = Rect::new(1000, 0, 500, 100);
        assert_eq!(group(&[big, near], UNION_MAX), (vec![Rect::new(0, 0, 1500, 1000)], vec![0, 0]));
        let empty = Rect::new(5, 5, 0, 4);
        assert_eq!(group(&[big, empty], UNION_MAX), (vec![big, empty], vec![0, 1]), "an empty region is its own and fails alone");
        // A region larger than the limit takes what lies inside it, and nothing beside it.
        let huge = Rect::new(0, 0, 3000, 1000);
        assert_eq!(group(&[huge, inside, Rect::new(3000, 0, 10, 10)], UNION_MAX).0.len(), 2);
    }

    /// A module about to act (urgent) is photographed alone when the other due requests cannot
    /// share its capture: never after another module's large capture in the same round, and the
    /// barrier waits for its round only. The other goes next round.
    #[test]
    fn an_urgent_request_is_not_bundled_behind_a_large_capture() {
        let t0 = Instant::now();
        let small = Rect::new(3000, 3000, 50, 20);
        let full = Rect::new(0, 0, 1920, 1080);
        let mut lane = SnapLane::default();
        lane.submit(req(1, B, full, wait_kind(full, 500, 0, t0), t0));
        lane.submit(req(2, A, small, SnapKind::Plain, t0 + MS));
        assert!(lane.expedite(A.idx));
        let round = lane.take(t0 + MS).unwrap();
        assert_eq!(round.ids(), vec![2], "only the urgent request, whose union with the screen is past the limit");
        assert_eq!(round.rects, vec![small]);
        assert!(!lane.barrier_clear(A.idx));
        let done = round.run(vec![Ok(solid(small, 1, t0))], t0 + MS, 0);
        lane.finish(done, t0 + MS);
        assert!(lane.barrier_clear(A.idx));
        let next = lane.take(t0 + MS).unwrap();
        assert_eq!(next.ids(), vec![1], "the large one next");
    }

    /// Requests inside the most urgent one's rectangle, or close enough to it, share its one
    /// capture — even when its own region is past the union limit; the first capture is its.
    #[test]
    fn a_round_takes_what_shares_its_one_capture() {
        let t0 = Instant::now();
        let screen = Rect::new(0, 0, 2560, 1440);
        let inside = Rect::new(100, 100, 30, 30);
        let beside = Rect::new(3000, 0, 30, 30);
        let mut lane = SnapLane::default();
        lane.submit(req(1, B, inside, SnapKind::Plain, t0));
        lane.submit(req(2, B, beside, SnapKind::Plain, t0));
        let mut first = req(3, A, screen, SnapKind::Plain, t0);
        first.ticket.prio = Priority::Interactive;
        lane.submit(first);
        let round = lane.take(t0).unwrap();
        assert_eq!(round.ids(), vec![3, 1], "the interactive one first, then what lies inside it");
        assert_eq!(round.rects, vec![screen]);
        let done = lane.finish(round.run(vec![Ok(solid(screen, 3, t0))], t0, 0), t0);
        assert_eq!(done.len(), 2);
        assert_eq!(lane.take(t0).unwrap().ids(), vec![2]);
    }

    /// On a Mac two displays can differ in scale: regions on two displays are never one
    /// capture, however close.
    #[test]
    fn regions_on_two_displays_are_not_one_capture() {
        let t0 = Instant::now();
        let mut lane = SnapLane::new(|x, _| if x >= 1000 { 2 } else { 1 });
        let left = Rect::new(900, 0, 50, 50);
        let right = Rect::new(1010, 0, 50, 50);
        lane.submit(req(1, A, left, SnapKind::Plain, t0));
        lane.submit(req(2, A, right, SnapKind::Plain, t0));
        let round = lane.take(t0).unwrap();
        assert_eq!((round.ids(), round.rects.clone()), (vec![1], vec![left]));
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, left, SnapKind::Plain, t0));
        lane.submit(req(2, A, right, SnapKind::Plain, t0));
        assert_eq!(lane.take(t0).unwrap().rects, vec![Rect::new(900, 0, 160, 50)], "one display: one union");
    }

    /// A change wait that shares a round's union capture is handed its own region: it never
    /// keeps the union between rounds.
    #[test]
    fn a_wait_in_a_shared_round_keeps_only_its_region() {
        let t0 = Instant::now();
        let small = Rect::new(0, 0, 10, 10);
        let other = Rect::new(500, 500, 100, 100);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, small, wait_kind(small, 500, 0, t0), t0));
        lane.submit(req(2, B, other, SnapKind::Plain, t0));
        let (rects, done) = turn(&mut lane, t0, &|r| Ok(solid(r, 9, t0)));
        assert_eq!(rects, vec![Rect::new(0, 0, 600, 600)], "one capture of the union");
        assert_eq!(done.len(), 1);
        let held = match &lane.waiting[0].req.kind {
            SnapKind::Change { wait, .. } => wait.held(),
            _ => panic!("the wait is gone"),
        };
        assert!(held.iter().all(|(r, _)| *r == small), "{held:?}");
    }

    /// Every picture of a round carries the input epoch the thread read once its captures were
    /// back — a plain request's and a change wait's alike.
    #[test]
    fn pictures_carry_the_input_epoch_of_their_round() {
        let t0 = Instant::now();
        let r = Rect::new(0, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, r, SnapKind::Plain, t0));
        lane.submit(req(2, A, r, wait_kind(r, 20, 0, t0), t0));
        let round = lane.take(t0).unwrap();
        let out = lane.finish(round.run(vec![Ok(solid(r, 1, t0))], t0, 41), t0);
        assert_eq!(picture(&out[0]).input_epoch, 41);
        let round = lane.take(t0 + 20 * MS).unwrap();
        let out = lane.finish(round.run(vec![Ok(solid(r, 1, t0))], t0 + 20 * MS, 42), t0 + 20 * MS);
        assert_eq!(picture(&out[0]).input_epoch, 42, "the wait's answer, from its last round");
    }

    /// A panic in the round's own code outside any one request still answers every request of
    /// the round, and the round is finished: nothing is left "being taken".
    #[test]
    fn a_panic_outside_the_requests_answers_the_whole_round() {
        crate::quiet_expected_panics();
        let t0 = Instant::now();
        let bad = Rect::new(PANIC_ROUND_X, 0, 10, 10);
        let mut lane = SnapLane::default();
        lane.submit(req(1, A, bad, SnapKind::Plain, t0));
        lane.submit(req(2, A, bad, wait_kind(bad, 500, 0, t0), t0));
        let round = lane.take(t0).unwrap();
        let done = round.run(vec![Ok(solid(Rect::new(0, 0, 1, 1), 1, t0))], t0, 0);
        assert_eq!(done.panics.len(), 1);
        let out = lane.finish(done, t0);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|d| matches!(&d.outcome, SnapOutcome::Failed { why, .. } if why == ROUND_FAILED)));
        assert!(lane.is_empty() && lane.barrier_clear(A.idx));
    }

    /// Which requests hold their module's input until their first picture.
    #[test]
    fn holds_input_by_kind() {
        assert!(holds_input(false, false, false), "a plain request");
        assert!(!holds_input(true, false, false), "a timed one never");
        assert!(holds_input(false, true, false), "a wait without a usable from: its first picture is its baseline");
        assert!(!holds_input(false, true, true), "a wait that compares with its from");
        assert!(!holds_input(true, true, false), "a timed wait never");
    }

    /// A `from` counts as usable at the call when it holds every watched rectangle cut to the
    /// region and, off macOS, was taken the way the rounds will be.
    #[test]
    fn from_usable_needs_the_watch_and_the_path() {
        let region = Rect::new(100, 100, 40, 30);
        let from = |r: Rect, via| Frame::from_image(r, crate::backend::CapturedImage { w: r.w as u32, h: r.h as u32, rgba: vec![0; (r.w * r.h * 4) as usize] }, Instant::now(), via, None).unwrap();
        let whole = from(region, FrameVia::Gdi);
        assert!(from_usable(&whole, region, &[], CaptureSource::Standard));
        let part = from(Rect::new(100, 100, 10, 30), FrameVia::Gdi);
        assert!(!from_usable(&part, region, &[], CaptureSource::Standard), "not all of the region");
        assert!(from_usable(&part, region, &[Rect::new(90, 100, 15, 30)], CaptureSource::Standard), "the watch cut to the region is inside it");
        assert!(!from_usable(&part, region, &[Rect::new(90, 100, 15, 30), Rect::new(120, 100, 5, 5)], CaptureSource::Standard));
        if !cfg!(target_os = "macos") {
            assert!(!from_usable(&whole, region, &[], DUP), "taken the standard way, compared through duplication");
            assert!(from_usable(&from(region, FrameVia::Duplication), region, &[], DUP));
        }
    }

    /// Requests of every kind, some cancelled at random moments, all through a simulated thread
    /// until the lane is empty: every one is answered exactly once.
    #[test]
    fn exactly_one_done_per_request() {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut rnd = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let t0 = Instant::now();
        let mut lane = SnapLane::default();
        let mut flags: Vec<Arc<AtomicBool>> = Vec::new();
        let mut answers: HashMap<SnapId, u32> = HashMap::new();
        let mut next: SnapId = 1;
        for ms in 0..3000u32 {
            let now = t0 + ms * MS;
            if ms < 1000 && rnd(10) == 0 {
                let r = Rect::new(rnd(4) as i32 * 20, 0, 10, 10);
                let kind = match rnd(3) {
                    0 => SnapKind::Plain,
                    1 => SnapKind::At(now + rnd(300) as u32 * MS),
                    _ => wait_kind(r, 1 + rnd(600), rnd(100), now + rnd(100) as u32 * MS),
                };
                let mut q = req(next, if rnd(2) == 0 { A } else { B }, r, kind, now);
                if rnd(2) == 0 {
                    q.source = DUP;
                }
                flags.push(q.cancel.clone());
                lane.submit(q);
                next += 1;
            }
            if rnd(25) == 0 && !flags.is_empty() {
                flags[rnd(flags.len() as u64) as usize].store(true, Ordering::Release);
            }
            let v = (ms / 50) as u8;
            let (_, done) = turn(&mut lane, now, &|r| if rnd_free(ms) { Err("no".into()) } else { Ok(solid(r, v, now)) });
            for d in done {
                *answers.entry(d.id).or_default() += 1;
            }
        }
        assert!(lane.is_empty(), "{} left", lane.len());
        assert_eq!(answers.len() as u64, next - 1, "every request answered");
        assert!(answers.values().all(|n| *n == 1), "exactly once");
    }

    /// Every seventh millisecond's captures fail, deterministically.
    fn rnd_free(ms: u32) -> bool {
        ms % 7 == 3
    }
}
