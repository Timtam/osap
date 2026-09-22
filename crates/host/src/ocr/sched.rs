//! Who is photographed and recognised next — the queue behind `host.ocr.read`, with no threads
//! and no clock of its own, so every rule can be tested by stepping it.
//!
//! A job goes through four stages: waiting for its capture, being captured, captured, and being
//! recognised. It carries one or more TICKETS — the calls waiting for its answer. Two calls that
//! ask for the same thing (the same regions, language and capture source) while the first has
//! not been photographed yet become one job: the picture is taken after both calls, so it is
//! new enough for both. Nothing joins a job once its picture is being taken.
//!
//! The rules, each with a test below:
//!
//! - **Latest wins per key.** A ticket with a key makes older tickets of the same module VM with
//!   the same key STALE, as long as their job has not started recognising. A job left with no
//!   ticket is dropped, and its pixels with it. A job already recognising is never made stale:
//!   its answer is delivered as it is, marked `newer` by the host. Otherwise a poll slower than
//!   its own period would never deliver anything.
//! - **Interactive first**, unless a background job has waited `AGING` — for the capture as for
//!   the recognition, or a stream of key presses would keep a poll from ever being photographed;
//!   within a lane, module VMs take turns.
//! - **A job's priority is its tickets'.** A job is interactive while an interactive ticket is on
//!   it; when that ticket goes (superseded, evicted, its module disabled) the job is background
//!   again.
//! - **Bounded.** `PER_OWNER` tickets per module VM (the oldest unkeyed one is evicted, or the new
//!   one refused when every waiting ticket has a key) and `TOTAL` jobs.
//! - **A budget for pictures.** While captured-but-unrecognised pixels exceed the budget the
//!   caller passes, background captures wait; interactive ones never do.
//! - **The module about to act goes first.** `expedite` marks a module's pictures still to be
//!   taken as urgent — the input barrier does, before a click — and an urgent job is
//!   photographed before any other, whatever the budget.
//!
//! Generic over the job's description `S` (compared to find a job to join) and its pixels `P`,
//! so the tests need neither a screen nor an engine. Std only; borrowed by `crates/macos-check`.

use std::time::Instant;

use super::policy::{AGING, PER_OWNER, TOTAL};
use super::types::Priority;

pub type TicketId = u64;
pub type JobId = u64;

/// A module VM: its index, and which incarnation of it (a reload makes a new one).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Owner {
    pub idx: usize,
    pub gen: u64,
}

/// One call waiting for a job's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub id: TicketId,
    pub owner: Owner,
    pub key: Option<String>,
    pub prio: Priority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    AwaitCapture,
    Capturing,
    Captured,
    Recognising,
}

struct Job<S, P> {
    id: JobId,
    spec: S,
    prio: Priority,
    /// A module waiting for this picture is about to act (`expedite`).
    urgent: bool,
    tickets: Vec<Ticket>,
    stage: Stage,
    asked: Instant,
    capture_started: Option<Instant>,
    captured_at: Option<Instant>,
    pixels: Option<P>,
    bytes: usize,
}

impl<S, P> Job<S, P> {
    /// The module VM that takes its turn for this job: the first caller's.
    fn lead(&self) -> usize {
        self.tickets.first().map_or(usize::MAX, |t| t.owner.idx)
    }
}

/// What became of a submission.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Submitted {
    /// The job the new ticket is on; `None` when it was refused.
    pub job: Option<JobId>,
    /// It joined a job another call had asked for.
    pub joined: bool,
    /// Tickets this one superseded: delivered as `stale`.
    pub stale: Vec<TicketId>,
    /// Tickets evicted to make room: delivered as `failed` with [`TOO_MANY`].
    pub evicted: Vec<TicketId>,
    /// Why the new ticket itself was refused.
    pub refused: Option<String>,
}

/// The reason an evicted ticket, or a refused one, carries.
pub const TOO_MANY: &str = "too many reads waiting — put regions that belong together in one \
                            call (up to 64)";

/// What the recognise stage is handed.
pub struct Started<S, P> {
    pub id: JobId,
    pub spec: S,
    pub pixels: P,
    pub prio: Priority,
    pub asked: Instant,
    pub capture_started: Instant,
    pub captured_at: Instant,
}

pub struct Scheduler<S, P> {
    /// In the order they were asked for.
    jobs: Vec<Job<S, P>>,
    next_job: JobId,
    /// The module VM each lane served last, for taking turns.
    last: [Option<usize>; 2],
}

impl<S, P> Default for Scheduler<S, P> {
    fn default() -> Self {
        Scheduler { jobs: Vec::new(), next_job: 1, last: [None, None] }
    }
}

fn lane(p: Priority) -> usize {
    match p {
        Priority::Interactive => 0,
        Priority::Background => 1,
    }
}

impl<S: Clone + PartialEq, P> Scheduler<S, P> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Jobs in any stage.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Takes `ticket`'s request for `spec`.
    pub fn submit(&mut self, spec: S, ticket: Ticket, now: Instant) -> Submitted {
        let mut out = Submitted::default();

        // Latest wins per key.
        if let Some(key) = ticket.key.as_deref() {
            out.stale = self.supersede(ticket.owner, key);
        }

        // Room for this VM: the oldest unkeyed ticket that has not started makes way, or, when
        // every one has a key, the new one is refused.
        let mine = self
            .jobs
            .iter()
            .flat_map(|j| j.tickets.iter())
            .filter(|t| t.owner == ticket.owner)
            .count();
        if mine >= PER_OWNER {
            // Ticket ids are handed out in order, so the smallest is the oldest.
            let victim = self
                .jobs
                .iter()
                .filter(|j| j.stage != Stage::Recognising)
                .flat_map(|j| j.tickets.iter())
                .filter(|t| t.owner == ticket.owner && t.key.is_none())
                .map(|t| t.id)
                .min();
            match victim {
                Some(v) => {
                    for job in &mut self.jobs {
                        job.tickets.retain(|t| t.id != v);
                    }
                    out.evicted.push(v);
                    self.tickets_removed();
                }
                None => {
                    out.refused = Some(TOO_MANY.to_string());
                    return out;
                }
            }
        }

        // The same request, not yet photographed: one picture for both, taken after both calls.
        if let Some(job) = self
            .jobs
            .iter_mut()
            .find(|j| j.stage == Stage::AwaitCapture && j.spec == spec)
        {
            job.prio = job.prio.max(ticket.prio);
            job.tickets.push(ticket);
            out.job = Some(job.id);
            out.joined = true;
            return out;
        }

        if self.jobs.len() >= TOTAL {
            out.refused = Some(format!(
                "too many reads waiting in the whole application ({TOTAL}); this one was not taken"
            ));
            return out;
        }
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(Job {
            id,
            spec,
            prio: ticket.prio,
            urgent: false,
            tickets: vec![ticket],
            stage: Stage::AwaitCapture,
            asked: now,
            capture_started: None,
            captured_at: None,
            pixels: None,
            bytes: 0,
        });
        out.job = Some(id);
        out
    }

    /// Latest wins per key: `owner`'s tickets with `key` whose job has not started recognising
    /// are taken off, to be answered stale. Done by every `submit` with a key, and on its own for
    /// a newer read the host answers without the queue (one with nothing to photograph).
    pub fn supersede(&mut self, owner: Owner, key: &str) -> Vec<TicketId> {
        let mut stale = Vec::new();
        for job in self.jobs.iter_mut().filter(|j| j.stage != Stage::Recognising) {
            job.tickets.retain(|t| {
                let superseded = t.owner == owner && t.key.as_deref() == Some(key);
                if superseded {
                    stale.push(t.id);
                }
                !superseded
            });
        }
        self.tickets_removed();
        stale
    }

    /// After tickets were taken off their jobs: a job keeps the highest priority of the tickets
    /// still on it — an interactive ticket that went takes the job's interactive priority with
    /// it — and a job nobody waits for is dropped, except the one being recognised (its thread
    /// will say it finished, and there is nothing to free until it does; its priority was handed
    /// to the recogniser already).
    fn tickets_removed(&mut self) {
        self.jobs.retain(|j| !j.tickets.is_empty() || j.stage == Stage::Recognising);
        for job in self.jobs.iter_mut().filter(|j| j.stage != Stage::Recognising) {
            if let Some(p) = job.tickets.iter().map(|t| t.prio).max() {
                job.prio = p;
            }
        }
    }

    /// The next job to photograph: an urgent one (`expedite`) whatever the budget; then a
    /// background job that has waited `AGING`; then interactive ones; then the rest in the order
    /// asked. Background jobs only while the picture budget is not exceeded.
    pub fn take_capture(&mut self, over_budget: bool, now: Instant) -> Option<(JobId, S)> {
        let waiting = |j: &Job<S, P>| j.stage == Stage::AwaitCapture;
        let background = |j: &Job<S, P>| waiting(j) && j.prio == Priority::Background;
        let pick = self
            .jobs
            .iter()
            .position(|j| waiting(j) && j.urgent)
            .or_else(|| {
                (!over_budget)
                    .then(|| {
                        self.jobs
                            .iter()
                            .position(|j| background(j) && now.saturating_duration_since(j.asked) >= AGING)
                    })
                    .flatten()
            })
            .or_else(|| self.jobs.iter().position(|j| waiting(j) && j.prio == Priority::Interactive))
            .or_else(|| (!over_budget).then(|| self.jobs.iter().position(waiting)).flatten())?;
        let job = &mut self.jobs[pick];
        job.stage = Stage::Capturing;
        job.capture_started = Some(now);
        Some((job.id, job.spec.clone()))
    }

    /// Module `idx` is about to act (the input barrier): its pictures still to be taken go
    /// before every other, whatever the budget. True when it has any.
    pub fn expedite(&mut self, idx: usize) -> bool {
        let mut any = false;
        for job in self
            .jobs
            .iter_mut()
            .filter(|j| j.stage == Stage::AwaitCapture && j.tickets.iter().any(|t| t.owner.idx == idx))
        {
            job.urgent = true;
            any = true;
        }
        any
    }

    /// The picture for `id` is taken. `false` when nobody waits for it any more — superseded or
    /// cancelled while it was being taken — and the caller drops the pixels.
    pub fn captured(&mut self, id: JobId, pixels: P, bytes: usize, now: Instant) -> bool {
        match self.jobs.iter_mut().find(|j| j.id == id && j.stage == Stage::Capturing) {
            Some(job) => {
                job.stage = Stage::Captured;
                job.pixels = Some(pixels);
                job.bytes = bytes;
                job.captured_at = Some(now);
                true
            }
            None => false,
        }
    }

    /// The next job to recognise: interactive first, unless the oldest background job has
    /// waited `AGING`; the lane's VMs take turns.
    pub fn next_recognise(&mut self, now: Instant) -> Option<Started<S, P>> {
        let ready = |j: &Job<S, P>| j.stage == Stage::Captured;
        let has_interactive =
            self.jobs.iter().any(|j| ready(j) && j.prio == Priority::Interactive);
        let has_background = self.jobs.iter().any(|j| ready(j) && j.prio == Priority::Background);
        let aged = self
            .jobs
            .iter()
            .filter(|j| ready(j) && j.prio == Priority::Background)
            .any(|j| now.saturating_duration_since(j.asked) >= AGING);
        let want = if has_interactive && !(has_background && aged) {
            Priority::Interactive
        } else {
            Priority::Background
        };
        let lane_i = lane(want);

        // Take turns: the next VM after the one served last, in index order, wrapping.
        let mut leads: Vec<usize> =
            self.jobs.iter().filter(|j| ready(j) && j.prio == want).map(|j| j.lead()).collect();
        if leads.is_empty() {
            return None;
        }
        leads.sort_unstable();
        leads.dedup();
        let next_lead = match self.last[lane_i] {
            Some(last) => leads.iter().copied().find(|&l| l > last).unwrap_or(leads[0]),
            None => leads[0],
        };
        let pick = self
            .jobs
            .iter()
            .position(|j| ready(j) && j.prio == want && j.lead() == next_lead)?;
        self.last[lane_i] = Some(next_lead);
        let job = &mut self.jobs[pick];
        // A captured job always holds its pixels; taken before the stage moves, so a job could
        // never be left recognising with nothing to recognise.
        let pixels = job.pixels.take()?;
        job.stage = Stage::Recognising;
        job.bytes = 0;
        Some(Started {
            id: job.id,
            spec: job.spec.clone(),
            pixels,
            prio: job.prio,
            asked: job.asked,
            capture_started: job.capture_started.unwrap_or(job.asked),
            captured_at: job.captured_at.unwrap_or(job.asked),
        })
    }

    /// `id` is recognised: its tickets, to be answered. Empty when every caller went away.
    pub fn finished(&mut self, id: JobId) -> Vec<Ticket> {
        match self.jobs.iter().position(|j| j.id == id) {
            Some(i) => self.jobs.remove(i).tickets,
            None => Vec::new(),
        }
    }

    /// Every ticket of module `idx`, in every VM generation, taken off its job: the module was
    /// disabled, reloaded or unloaded, and its callbacks are dropped. Jobs another module joined
    /// stay for it.
    pub fn cancel_owner(&mut self, idx: usize) -> Vec<TicketId> {
        let mut gone = Vec::new();
        for job in &mut self.jobs {
            job.tickets.retain(|t| {
                let mine = t.owner.idx == idx;
                if mine {
                    gone.push(t.id);
                }
                !mine
            });
        }
        self.tickets_removed();
        gone
    }

    /// Whether module `idx` has no picture still to be taken — what the input barrier waits for.
    pub fn barrier_clear(&self, idx: usize) -> bool {
        !self.jobs.iter().any(|j| {
            matches!(j.stage, Stage::AwaitCapture | Stage::Capturing)
                && j.tickets.iter().any(|t| t.owner.idx == idx)
        })
    }

    /// Whether interactive work is waiting behind the recognition that is running.
    pub fn interactive_waiting(&self) -> bool {
        self.jobs.iter().any(|j| j.stage != Stage::Recognising && j.prio == Priority::Interactive)
    }

    /// Bytes of pixels captured and not yet recognised.
    pub fn captured_bytes(&self) -> usize {
        self.jobs.iter().filter(|j| j.stage == Stage::Captured).map(|j| j.bytes).sum()
    }

    /// The stage of `id`, for the tests.
    #[cfg(test)]
    pub fn stage(&self, id: JobId) -> Option<Stage> {
        self.jobs.iter().find(|j| j.id == id).map(|j| j.stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    type S = &'static str;
    type Sched = Scheduler<S, u32>;

    const A: Owner = Owner { idx: 1, gen: 10 };
    const B: Owner = Owner { idx: 2, gen: 20 };
    const C: Owner = Owner { idx: 3, gen: 30 };

    fn t(id: TicketId, owner: Owner, key: Option<&str>, prio: Priority) -> Ticket {
        Ticket { id, owner, key: key.map(str::to_string), prio }
    }

    fn bg(id: TicketId, owner: Owner, key: Option<&str>) -> Ticket {
        t(id, owner, key, Priority::Background)
    }

    fn fg(id: TicketId, owner: Owner, key: Option<&str>) -> Ticket {
        t(id, owner, key, Priority::Interactive)
    }

    /// Captures and recognises job `id` at once; returns its tickets' ids.
    fn run(s: &mut Sched, now: Instant) -> Option<(JobId, Vec<TicketId>)> {
        let (id, _) = s.take_capture(false, now)?;
        assert!(s.captured(id, 7, 100, now));
        let started = s.next_recognise(now)?;
        assert_eq!(started.id, id);
        Some((id, s.finished(id).into_iter().map(|t| t.id).collect()))
    }

    #[test]
    fn the_same_request_before_its_picture_is_one_job_across_modules() {
        let now = Instant::now();
        let mut s = Sched::new();
        let a = s.submit("menu", bg(1, A, None), now);
        let b = s.submit("menu", fg(2, B, None), now);
        assert!(!a.joined && b.joined);
        assert_eq!(a.job, b.job);
        assert_eq!(s.len(), 1);
        // The joined job takes the higher priority.
        assert!(s.interactive_waiting());
        let other = s.submit("value", bg(3, A, None), now);
        assert_ne!(other.job, a.job);
        let (_, tickets) = run(&mut s, now).unwrap();
        assert_eq!(tickets, vec![1, 2], "interactive first, and both callers answered");
    }

    #[test]
    fn nothing_joins_a_job_whose_picture_is_being_taken() {
        let now = Instant::now();
        let mut s = Sched::new();
        let first = s.submit("menu", bg(1, A, None), now).job.unwrap();
        let (id, _) = s.take_capture(false, now).unwrap();
        assert_eq!(id, first);
        let second = s.submit("menu", bg(2, B, None), now);
        assert!(!second.joined, "a picture taken before the call is not new enough for it");
        assert_ne!(second.job, Some(first));
    }

    #[test]
    fn a_key_makes_older_tickets_stale_until_recognition_starts() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("r1", bg(1, A, Some("k")), now);
        // Captured but not recognising: still superseded.
        let (id, _) = s.take_capture(false, now).unwrap();
        assert!(s.captured(id, 1, 10, now));
        let second = s.submit("r2", bg(2, A, Some("k")), now);
        assert_eq!(second.stale, vec![1]);
        assert_eq!(s.stage(id), None, "a job nobody waits for is dropped, pixels and all");
        assert_eq!(s.captured_bytes(), 0);
        // Recognising: never stale.
        let (id2, _) = s.take_capture(false, now).unwrap();
        assert!(s.captured(id2, 2, 10, now));
        assert!(s.next_recognise(now).is_some());
        let third = s.submit("r3", bg(3, A, Some("k")), now);
        assert!(third.stale.is_empty(), "a started recognition is delivered as it is");
        assert_eq!(s.finished(id2).len(), 1);
    }

    #[test]
    fn a_key_only_supersedes_the_same_vm_and_the_same_key() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("r", bg(1, A, Some("k")), now);
        s.submit("r2", bg(2, A, Some("other")), now);
        s.submit("r3", bg(3, A, None), now);
        s.submit("r4", bg(4, B, Some("k")), now);
        // The same module index in a newer VM generation is another VM.
        s.submit("r5", bg(5, Owner { idx: 1, gen: 11 }, Some("k")), now);
        let out = s.submit("r6", bg(6, A, Some("k")), now);
        assert_eq!(out.stale, vec![1]);
    }

    /// Superseding without submitting — a newer read the host answered itself, with nothing to
    /// photograph — follows the same rule, and adds no job.
    #[test]
    fn superseding_on_its_own_follows_the_same_rule() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("r1", bg(1, A, Some("k")), now);
        let (id, _) = s.take_capture(false, now).unwrap();
        assert!(s.captured(id, 1, 10, now));
        assert!(s.next_recognise(now).is_some(), "1 is recognising");
        s.submit("r2", bg(2, A, Some("k")), now);
        s.submit("r3", bg(3, B, Some("k")), now);
        s.submit("r4", bg(4, A, None), now);
        assert_eq!(s.supersede(A, "k"), vec![2], "the waiting one, not the one recognising");
        assert_eq!(s.len(), 3, "r2's job is gone, and nothing was added");
        assert!(s.supersede(A, "k").is_empty());
    }

    #[test]
    fn a_superseded_capture_in_progress_is_dropped_when_it_lands() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("r1", bg(1, A, Some("k")), now);
        let (id, _) = s.take_capture(false, now).unwrap();
        let out = s.submit("r2", bg(2, A, Some("k")), now);
        assert_eq!(out.stale, vec![1]);
        assert!(!s.captured(id, 1, 10, now), "nobody waits for it: the caller drops the pixels");
    }

    #[test]
    fn the_seventeenth_evicts_the_oldest_unkeyed_or_is_refused_when_all_are_keyed() {
        let now = Instant::now();
        let mut s = Sched::new();
        for i in 0..PER_OWNER as u64 {
            let key = if i == 0 { Some("k0") } else { None };
            s.submit(if i % 2 == 0 { "even" } else { "odd" }, bg(i + 1, A, key), now);
        }
        // Tickets 1 (keyed), 2..16 unkeyed, joined into two jobs.
        let out = s.submit("new", bg(100, A, None), now);
        assert_eq!(out.evicted, vec![2], "the oldest UNKEYED ticket");
        assert!(out.job.is_some());
        // Another VM is not affected by A's cap.
        assert!(s.submit("b", bg(200, B, None), now).refused.is_none());

        let mut s = Sched::new();
        for i in 0..PER_OWNER as u64 {
            let key = format!("k{i}");
            s.submit("x", Ticket { id: i + 1, owner: A, key: Some(key), prio: Priority::Background }, now);
        }
        let out = s.submit("y", bg(99, A, None), now);
        assert!(out.evicted.is_empty());
        assert_eq!(out.refused.as_deref(), Some(TOO_MANY));
        assert_eq!(out.job, None);
    }

    #[test]
    fn the_total_cap_refuses_and_joining_is_still_allowed() {
        let now = Instant::now();
        let mut s: Scheduler<u64, u32> = Scheduler::new();
        for i in 0..TOTAL as u64 {
            let owner = Owner { idx: (i / 8) as usize, gen: 1 };
            assert!(s.submit(i, bg(i + 1, owner, None), now).refused.is_none());
        }
        let out = s.submit(9999, bg(5000, Owner { idx: 99, gen: 1 }, None), now);
        assert!(out.refused.is_some());
        let joined = s.submit(3, bg(5001, Owner { idx: 99, gen: 1 }, None), now);
        assert!(joined.joined && joined.refused.is_none());
    }

    #[test]
    fn interactive_goes_first_until_a_background_job_has_waited_too_long() {
        let t0 = Instant::now();
        let mut s = Sched::new();
        s.submit("poll", bg(1, A, None), t0);
        s.submit("tab", fg(2, B, None), t0);
        // Captured in the order interactive first.
        let (first, _) = s.take_capture(false, t0).unwrap();
        assert!(s.captured(first, 0, 0, t0));
        let (second, _) = s.take_capture(false, t0).unwrap();
        assert!(s.captured(second, 0, 0, t0));
        assert_eq!(s.next_recognise(t0).unwrap().prio, Priority::Interactive);

        let mut s = Sched::new();
        s.submit("poll", bg(1, A, None), t0);
        s.submit("tab", fg(2, B, None), t0);
        for _ in 0..2 {
            let (id, _) = s.take_capture(false, t0).unwrap();
            assert!(s.captured(id, 0, 0, t0));
        }
        let later = t0 + AGING + Duration::from_millis(1);
        assert_eq!(s.next_recognise(later).unwrap().prio, Priority::Background, "aged");
    }

    #[test]
    fn modules_take_turns_within_a_lane() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("a1", bg(1, A, None), now);
        s.submit("a2", bg(2, A, None), now);
        s.submit("a3", bg(3, A, None), now);
        s.submit("b1", bg(4, B, None), now);
        s.submit("c1", bg(5, C, None), now);
        while let Some((id, _)) = s.take_capture(false, now) {
            assert!(s.captured(id, 0, 0, now));
        }
        let mut order = Vec::new();
        while let Some(started) = s.next_recognise(now) {
            order.push(started.spec);
            s.finished(started.id);
        }
        assert_eq!(order, vec!["a1", "b1", "c1", "a2", "a3"]);
    }

    #[test]
    fn cancelling_a_module_keeps_jobs_others_joined() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("shared", bg(1, A, None), now);
        s.submit("shared", bg(2, B, None), now);
        s.submit("mine", bg(3, A, None), now);
        let gone = s.cancel_owner(A.idx);
        assert_eq!(gone, vec![1, 3]);
        assert_eq!(s.len(), 1);
        let (_, tickets) = run(&mut s, now).unwrap();
        assert_eq!(tickets, vec![2]);
        // A recognition in progress stays until its thread says it finished, with nobody to answer.
        s.submit("x", bg(4, A, None), now);
        let (id, _) = s.take_capture(false, now).unwrap();
        assert!(s.captured(id, 0, 0, now));
        let started = s.next_recognise(now).unwrap();
        assert_eq!(s.cancel_owner(A.idx), vec![4]);
        assert_eq!(s.stage(started.id), Some(Stage::Recognising));
        assert!(s.finished(started.id).is_empty());
        assert!(s.is_empty());
    }

    #[test]
    fn the_barrier_clears_once_the_modules_pictures_are_taken() {
        let now = Instant::now();
        let mut s = Sched::new();
        assert!(s.barrier_clear(A.idx), "nothing asked: nothing to wait for");
        s.submit("r", bg(1, A, None), now);
        assert!(!s.barrier_clear(A.idx));
        assert!(s.barrier_clear(B.idx), "another module's reads do not hold A's input");
        let (id, _) = s.take_capture(false, now).unwrap();
        assert!(!s.barrier_clear(A.idx), "being taken is not taken");
        assert!(s.captured(id, 0, 0, now));
        assert!(s.barrier_clear(A.idx));
    }

    #[test]
    fn background_captures_wait_while_the_budget_is_exceeded() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("poll", bg(1, A, None), now);
        assert!(s.take_capture(true, now).is_none());
        s.submit("tab", fg(2, B, None), now);
        let (_, spec) = s.take_capture(true, now).unwrap();
        assert_eq!(spec, "tab", "interactive captures always proceed");
        let (_, spec) = s.take_capture(false, now).unwrap();
        assert_eq!(spec, "poll");
        let mut s = Sched::new();
        s.submit("a", bg(1, A, None), now);
        let (id, _) = s.take_capture(false, now).unwrap();
        assert!(s.captured(id, 0, 4096, now));
        assert_eq!(s.captured_bytes(), 4096);
        assert!(s.next_recognise(now).is_some());
        assert_eq!(s.captured_bytes(), 0, "handed to the recogniser, out of the budget");
    }

    /// A game module reading on every controller event at 60 Hz, one capture per frame: the
    /// poll beside it is photographed once it has waited `AGING`, not never.
    #[test]
    fn a_stream_of_interactive_reads_does_not_starve_a_background_capture() {
        let t0 = Instant::now();
        let mut s: Scheduler<u64, u32> = Scheduler::new();
        let (poll, pad) = (Owner { idx: 2, gen: 1 }, Owner { idx: 1, gen: 1 });
        s.submit(9999, bg(1, poll, Some("menu")), t0);
        let mut photographed_at = None;
        for frame in 0..600u64 {
            let now = t0 + Duration::from_millis(frame * 16);
            // A new region each frame (the stick moved), superseding the last one.
            s.submit(frame, fg(10 + frame, pad, Some("hover")), now);
            if let Some((job, spec)) = s.take_capture(false, now) {
                if spec == 9999 {
                    photographed_at = Some(now - t0);
                    break;
                }
                assert!(s.captured(job, 0, 0, now));
                let started = s.next_recognise(now).unwrap();
                s.finished(started.id);
            }
        }
        let at = photographed_at.expect("the poll was never photographed");
        assert!(at >= AGING && at < AGING + Duration::from_millis(40), "{at:?}");
    }

    /// Aging does not reach past the budget: an old background job still waits while it is
    /// exceeded, and an interactive one still goes.
    #[test]
    fn an_aged_background_capture_still_respects_the_budget() {
        let t0 = Instant::now();
        let mut s = Sched::new();
        s.submit("poll", bg(1, A, None), t0);
        s.submit("tab", fg(2, B, None), t0);
        let later = t0 + AGING * 2;
        let (_, spec) = s.take_capture(true, later).unwrap();
        assert_eq!(spec, "tab");
        assert!(s.take_capture(true, later).is_none());
    }

    /// A background ticket that joined an interactive one's job does not keep the job
    /// interactive once the interactive ticket is gone — superseded, evicted or cancelled.
    #[test]
    fn a_job_is_interactive_only_while_an_interactive_ticket_is_on_it() {
        let now = Instant::now();
        // Superseded.
        let mut s = Sched::new();
        s.submit("poll", bg(1, B, None), now);
        s.submit("poll", fg(2, A, Some("k")), now);
        assert!(s.interactive_waiting());
        let out = s.submit("other", bg(3, A, Some("k")), now);
        assert_eq!(out.stale, vec![2]);
        assert!(!s.interactive_waiting(), "only background tickets are left");
        assert!(s.take_capture(true, now).is_none(), "and background waits for the budget");

        // Cancelled with its module.
        let mut s = Sched::new();
        s.submit("poll", bg(1, B, None), now);
        s.submit("poll", fg(2, A, None), now);
        assert_eq!(s.cancel_owner(A.idx), vec![2]);
        assert!(!s.interactive_waiting());

        // Evicted: A's seventeenth read pushes out its oldest unkeyed one, the interactive one.
        let mut s = Sched::new();
        s.submit("shared", fg(1, A, None), now);
        s.submit("shared", bg(2, B, None), now);
        for i in 0..(PER_OWNER as u64 - 1) {
            s.submit("mine", bg(10 + i, A, Some(&format!("k{i}"))), now);
        }
        let out = s.submit("mine", bg(99, A, Some("last")), now);
        assert_eq!(out.evicted, vec![1]);
        assert!(!s.interactive_waiting());
    }

    /// The input barrier's module goes first — before an interactive job asked earlier, and
    /// whatever the budget — and only its own pictures do.
    #[test]
    fn an_expedited_modules_picture_is_taken_first_whatever_the_budget() {
        let now = Instant::now();
        let mut s = Sched::new();
        s.submit("tab", fg(1, B, None), now);
        s.submit("mine", bg(2, A, None), now);
        s.submit("other", bg(3, C, None), now);
        assert!(!s.expedite(9), "a module with nothing waiting has nothing to hurry");
        assert!(s.expedite(A.idx));
        let (_, spec) = s.take_capture(true, now).unwrap();
        assert_eq!(spec, "mine");
        let (_, spec) = s.take_capture(true, now).unwrap();
        assert_eq!(spec, "tab");
        assert!(s.take_capture(true, now).is_none(), "C's is not hurried");
        // A picture already being taken is not waiting any more: nothing to mark.
        let mut s = Sched::new();
        s.submit("mine", bg(1, A, None), now);
        s.take_capture(false, now).unwrap();
        assert!(!s.expedite(A.idx));
    }

    /// The poll that is slower than its own period: a keyed read every 150 ms against a
    /// recognition of 200 ms, in simulated time. Every recognition that started is delivered,
    /// so the poll keeps making progress; only reads that never started are stale.
    #[test]
    fn a_poll_slower_than_its_period_still_delivers_every_started_recognition() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut s = Sched::new();
        let mut running: Option<(JobId, u64)> = None; // (job, ends at)
        let mut delivered = 0;
        let mut stale = 0;
        for tick in 0..=40u64 {
            let now_ms = tick * 50;
            if let Some((id, ends)) = running {
                if now_ms >= ends {
                    delivered += s.finished(id).len();
                    running = None;
                }
            }
            if now_ms % 150 == 0 {
                let out = s.submit("poll", bg(tick + 1, A, Some("menu")), at(now_ms));
                stale += out.stale.len();
                // Photographed at once, as the capture stage does.
                if let Some((id, _)) = s.take_capture(false, at(now_ms)) {
                    assert!(s.captured(id, 0, 0, at(now_ms)));
                }
            }
            if running.is_none() {
                if let Some(started) = s.next_recognise(at(now_ms)) {
                    running = Some((started.id, now_ms + 200));
                }
            }
        }
        assert!(delivered >= 8, "delivered {delivered}, stale {stale}");
        assert!(stale > 0, "some reads never started and were stale");
    }
}
