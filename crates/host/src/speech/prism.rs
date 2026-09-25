//! Saying things through the screen reader on Windows.
//!
//! A structural twin of `voiceover.rs`: a worker thread, a preferred path that can fail, and
//! a refusal channel so that a line the preferred path turned down is still said rather than
//! silently lost. The four methods below carry the same meanings as VoiceOver's, deliberately
//! — the two platforms are one design, and they are not made into a trait only because
//! `voiceover.rs` is borrowed standalone by `crates/macos-check`, which has no `speech`
//! module to name a trait from.
//!
//! The reasons for a thread of its own are in `docs/prism-speech-design.md`, and none of
//! them is performance: a healthy call costs 0.11 ms. It is that `prism_init` decides the
//! COM apartment of whichever thread calls it, that `prism_shutdown` calls `CoUninitialize`
//! on that same thread, that a backend handle is not safe for concurrent use, and that a
//! blocking RPC call into a wedged screen reader cannot be cancelled — so it must at least
//! be wedged somewhere that is not the keyboard.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use prism_sys::{feature, Context, SCREEN_READERS};

/// How long a line may go unanswered before the screen-reader path is given up on.
///
/// The reason this exists at all: `osascript` can be killed, and a synchronous RPC call into
/// a hung NVDA cannot — it blocks with no timeout, inside our own process. So the deadline
/// lives with the caller rather than with the call. 300 ms is not a round number picked for
/// looks: it is the same budget the event loop is written around — macOS switches off an
/// event tap whose thread stops answering for about that long, and on Windows it was the
/// `LowLevelHooksTimeout` budget while the keyboard hook shared the event loop (it has a
/// thread of its own now).
///
/// What it cannot do is un-say something. A screen reader that is merely SLOW rather than
/// wedged will finish its line after the deadline has already handed the same words to the
/// fallback, and the user hears them twice. That is the deliberate trade: the call cannot be
/// cancelled, so the choice is between hearing a line twice and risking not hearing it at
/// all, and in this application the second is much worse than the first.
const STALL_MS: u64 = 300;

/// A line handed over longer ago than this is not worth saying any more.
///
/// It matters when a wedged screen reader finally unblocks: everything queued behind it is
/// handed back at once, and speaking a control's name from two minutes ago is worse than
/// saying nothing — the user moved on long since, and a screen reader that describes the past
/// cannot be trusted about the present.
const STALE_MS: u64 = 1_000;

/// How often to look for a screen reader that has gone, and for how long at that pace.
///
/// Quitting and restarting a screen reader is an ordinary thing to do — it is how people fix
/// one that has got confused — and having to restart this application afterwards would be a
/// poor answer. So the path comes back on its own, and the first look happens immediately.
///
/// Measured, which is what set these numbers: one sweep of the seven screen-reader backends
/// with only NVDA present costs 86 ms, of which **62 ms is `prism_init`** — the library, not
/// the looking. The four readers this machine cannot test are the rest of it, at 5–6.5 ms
/// each, because each one goes looking through the process list; JAWS and ZoomText answer in
/// under 100 µs. So the searching worker opens the library ONCE and sleeps between sweeps
/// rather than being started afresh each time, and 3 seconds costs about 0.8% of one core
/// while somebody is plausibly mid-restart.
///
/// **The slow pace is only for a screen reader that is really gone.** A JAWS whose first line
/// came back `INTERNAL` could not be opened again for over a minute while, by its user's
/// account, it kept running, and the fixed schedule had slowed to thirty seconds by the time it
/// could: the user heard OneCore for 90 seconds against a page promising three. Whether prism
/// saw that JAWS as running is not known — the log of that session could not say, which is why
/// the searcher now writes what it sees. So each look that opens nothing also asks prism
/// whether a screen reader is RUNNING — `IS_SUPPORTED_AT_RUNTIME`, which creates the backend
/// and asks without initialising it: JAWS's UI window and class factory, NVDA's RPC endpoint,
/// a window, a class factory, a status call or the process list for the others — and while
/// one is, the pace stays at three seconds for as long as it takes (`next_look`). That
/// question does about what the failed opens did again (ZDSR and Boy PC Reader search the
/// process list; JAWS and NVDA answer in microseconds), so a look is estimated from those
/// measured parts — not measured itself — at roughly twice the 24 ms above: about 1.6% of one
/// core at three seconds, about 0.5% at ten, and about 0.2% at thirty.
///
/// **The fast pace has an end, and not every "running" earns it** (the maintainer's decision,
/// 2026-09-25). A reader whose runtime check looks at the reader itself — a window, an
/// endpoint, a status call ([`CHECKED_BY_THE_READER`]) — keeps three seconds for
/// [`RUNNING_FAST_FOR`] from the look that first saw it running and refusing, then ten. ZDSR
/// and Boy PC Reader are checked through the process list, which includes their background
/// services, so theirs says the reader is installed rather than in use: they get the pace of
/// a search that found none running. Without both, an installed ZDSR whose service runs, or a
/// JAWS that passes its check and will never open, kept the fast pace for the whole session.
const RETRY_SOON: Duration = Duration::from_secs(3);
const RETRY_CAPPED: Duration = Duration::from_secs(10);
const RETRY_LATER: Duration = Duration::from_secs(30);
const RETRY_SOON_FOR: Duration = Duration::from_secs(60);
const RUNNING_FAST_FOR: Duration = Duration::from_secs(300);

/// The readers whose "running" is a look at the reader itself: NVDA's control endpoint, JAWS's
/// window and class factory, ZoomText's window, PC-Talker's status call, Sense Reader's window
/// and class factory. By the names in [`SCREEN_READERS`].
const CHECKED_BY_THE_READER: &[&str] = &["NVDA", "JAWS", "ZoomText", "PCTalker", "SenseReader"];

/// The schedule, said the same way by every line that promises it.
const SCHEDULE: &str = "It is looked for again at once, then every 3 s for five minutes and \
                        every 10 s after that while a screen reader is running, and every 3 s \
                        for a minute and every 30 s after that while none is (ZDSR and Boy PC \
                        Reader count as none here: their check sees their background services).";

/// When to look again after a look that opened nothing. For a reader that is `running` by its
/// own check, `since` is how long it has been seen running and refusing: [`RETRY_SOON`] for
/// the first [`RUNNING_FAST_FOR`], [`RETRY_CAPPED`] after that. Otherwise `since` is how long
/// the search has been going: [`RETRY_SOON`] for the first [`RETRY_SOON_FOR`], [`RETRY_LATER`]
/// after that.
fn next_look(since: Duration, running: bool) -> Duration {
    match (running, since) {
        (true, s) if s < RUNNING_FAST_FOR => RETRY_SOON,
        (true, _) => RETRY_CAPPED,
        (false, s) if s < RETRY_SOON_FOR => RETRY_SOON,
        (false, _) => RETRY_LATER,
    }
}

/// Whether a sighting keeps the running pace: a reader that runs by its own check and refuses.
fn runs_by_its_own_check(seen: &Sighting) -> bool {
    matches!(seen, Sighting::Refusing { name, .. } if CHECKED_BY_THE_READER.contains(&name.as_str()))
}

/// What a look that opened nothing saw, as far as the log and the pace care.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Sighting {
    /// No screen reader is running: prism's runtime check says no for every one.
    NoneRunning,
    /// `name` is running and would not open, with the error opening it gave.
    Refusing { name: String, why: String },
}

impl Sighting {
    /// Whether two sightings are the same news. The error is left out: it is in the line, but a
    /// reader that alternates between two errors must not write a line every three seconds.
    fn same_as(&self, other: &Sighting) -> bool {
        match (self, other) {
            (Sighting::NoneRunning, Sighting::NoneRunning) => true,
            (Sighting::Refusing { name: a, .. }, Sighting::Refusing { name: b, .. }) => a == b,
            _ => false,
        }
    }
}

/// The searcher's memory of what it has said, so it logs changes and not looks.
///
/// Pure — no clock, no prism — so the schedule and its lines are tested without a screen
/// reader (`tests::the_schedule`).
struct Search {
    /// What the last look saw, or what was already said before the search began.
    last: Option<Sighting>,
    /// The wait chosen after the last look.
    nap: Duration,
    /// When, since the search began, the last look's news was first seen — where a reader
    /// that runs and refuses starts its [`RUNNING_FAST_FOR`].
    news_since: Duration,
}

impl Search {
    /// With nothing said yet, so the first look is always logged. That holds for a search sent
    /// out at start-up too: the start-up line says only that nothing opened, because it cannot
    /// tell a reader that is not running from one that runs and refuses (`open_screen_reader`).
    fn new() -> Self {
        Self { last: None, nap: RETRY_SOON, news_since: Duration::ZERO }
    }

    /// After a look that opened nothing, `since` the search began: how long to wait before the
    /// next one, and the line to log when this look is news.
    fn after_look(&mut self, since: Duration, seen: Sighting) -> (Duration, Option<String>) {
        let running = runs_by_its_own_check(&seen);
        let changed = !self.last.as_ref().is_some_and(|l| l.same_as(&seen));
        if changed {
            self.news_since = since;
        }
        let nap = if running {
            next_look(since.saturating_sub(self.news_since), true)
        } else {
            next_look(since, false)
        };
        let line = if changed {
            Some(match &seen {
                Sighting::Refusing { name, why } if running && nap == RETRY_SOON => format!(
                    "{name} is running but would not open ({why}), so the plain voice goes on \
                     speaking; looking again every {} s for five minutes, then every {} s, for \
                     as long as it runs",
                    RETRY_SOON.as_secs(),
                    RETRY_CAPPED.as_secs()
                ),
                Sighting::Refusing { name, why } if running => format!(
                    "{name} is running but would not open ({why}), so the plain voice goes on \
                     speaking; looking again every {} s for as long as it runs",
                    nap.as_secs()
                ),
                Sighting::Refusing { name, why } => format!(
                    "{name} would not open ({why}); some of its processes run, but they include \
                     its background services, so that does not say it is in use. The plain \
                     voice goes on speaking; looking again every {}",
                    if nap == RETRY_SOON {
                        format!(
                            "{} s until a minute has passed, then every {} s",
                            RETRY_SOON.as_secs(),
                            RETRY_LATER.as_secs()
                        )
                    } else {
                        format!("{} s", nap.as_secs())
                    }
                ),
                Sighting::NoneRunning if nap == RETRY_SOON => format!(
                    "no screen reader is running, so the plain voice goes on speaking; looking \
                     again every {} s until a minute has passed, then every {} s",
                    RETRY_SOON.as_secs(),
                    RETRY_LATER.as_secs()
                ),
                Sighting::NoneRunning => format!(
                    "no screen reader is running, so the plain voice goes on speaking; looking \
                     again every {} s",
                    nap.as_secs()
                ),
            })
        } else if nap != self.nap {
            // Only the pace moved: five minutes of a reader running and refusing, or a minute
            // of the search with none running (or only one checked through its processes).
            Some(match &seen {
                Sighting::Refusing { name, .. } if running => format!(
                    "{name} still would not open five minutes after it was first seen running; \
                     looking every {} s from now",
                    nap.as_secs()
                ),
                Sighting::Refusing { name, .. } => format!(
                    "{name} still would not open a minute after the search began; looking every \
                     {} s from now",
                    nap.as_secs()
                ),
                Sighting::NoneRunning => format!(
                    "still no screen reader running a minute after the search began; looking \
                     every {} s from now",
                    nap.as_secs()
                ),
            })
        } else {
            None
        };
        self.last = Some(seen);
        self.nap = nap;
        (nap, line)
    }
}

/// A line slower than this is worth naming on its own — past the point where an announcement
/// stops feeling like an answer to the keystroke that caused it.
const SLOW_LINE_MS: u64 = 150;

/// A line for the screen reader: what to say, whether it displaces what is waiting, and when
/// the user's keystroke asked for it.
struct Utterance {
    text: String,
    interrupt: bool,
    /// Stamped when the caller handed it over, not when the worker picked it up. The point of
    /// the deadline is how long the USER has been waiting, which includes time spent opening
    /// a backend and time spent queued behind a call that will never return.
    at: Instant,
}

/// The oldest line that has been handed over and not yet answered.
type Outstanding = Arc<Mutex<Option<(Instant, String)>>>;

/// Why a worker is being started.
#[derive(Clone, Copy, PartialEq)]
enum Start {
    /// Application start-up: say what was found, and fall back to a speech engine when there
    /// is no screen reader, because a module that asks to be heard should be heard.
    First,
    /// Looking for a screen reader to speak through. Logs what it finds and what changes
    /// between looks (see [`Search`]), and never opens a speech engine — the fallback is
    /// already speaking, and the only reason to be here is to stop needing it.
    Retry(Why),
}

/// Whether the event loop sends a searcher out, and why, once the screen-reader path is out of
/// service: `retrying`, one was sent already; `stopped`, that one stopped because the setting
/// was switched off (and the caller only asks while it is on again); `had_reader`, the worker
/// now serving this path has had a screen reader in hand — the first worker's, or a searcher's
/// that found one. Unset means that worker found none, or is still looking.
///
/// A searcher sent already is left alone only while it is still looking. `had_reader` with the
/// path out of service means the reader it found has refused a line or stalled since — whether
/// or not a pass of the loop saw the path healthy in between — so nobody is looking any more.
/// Without that, a reader found and then refusing before the next pass (a line handed over in
/// the same tick, which is the flapping error-9 state) left the path with the plain voice for
/// the rest of the session.
///
/// Pure, so the rule is tested without a thread or a screen reader (`tests::when_a_searcher_goes`).
fn searcher_due(retrying: bool, stopped: bool, had_reader: bool) -> Option<Why> {
    if retrying && !stopped && !had_reader {
        return None;
    }
    Some(if stopped {
        Why::Ticked
    } else if had_reader {
        Why::Lost
    } else {
        Why::NoneAtStart
    })
}

/// Why a searcher is out looking.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Why {
    /// The screen reader in use stopped answering or refused a line.
    Lost,
    /// None opened when the application started — none was running, or one ran and refused.
    /// Looked for all the same, so one started after the application — at a login where both
    /// start together, say — is used without a restart.
    NoneAtStart,
    /// "Speak through the screen reader" was ticked again.
    Ticked,
}

/// The screen reader, spoken to on a thread of its own.
pub struct Prism {
    to_worker: Sender<Utterance>,
    refused_rx: Receiver<String>,
    /// False once the screen-reader path has been given up on. Everything after goes straight
    /// to the fallback without paying for another call.
    healthy: Arc<AtomicBool>,
    /// Handed over but not yet answered, so `is_speaking` has something true to say. Counted
    /// here rather than asked of prism, because `prism_backend_is_speaking` is usually not
    /// implemented — NVDA answers it only when the running copy exposes a newer RPC
    /// interface, so a caller that treated that failure as a failure would give up on a
    /// perfectly healthy screen reader on its first wait.
    pending: Arc<AtomicUsize>,
    outstanding: Outstanding,
    /// Whether what opened was a screen reader rather than a plain speech engine.
    ///
    /// The difference matters only to the application's own announcements: those are
    /// addressed to somebody who cannot see the screen, so with no screen reader listening
    /// they must not be spoken at all. A module that asks to be heard is heard either way.
    reader: Arc<AtomicBool>,
    /// Whether a worker is already out looking for the screen reader, so that exactly one is.
    retrying: bool,
    /// Set by a searcher that stopped because "Speak through the screen reader" was switched
    /// off. `retrying` still says one was sent, so this is what lets the event loop send
    /// another once the setting is on again (`retry_if_due`).
    stopped: Arc<AtomicBool>,
    /// The refusal channels of the workers this one replaced.
    ///
    /// A replaced worker can still be handing lines back: the rest of the batch it was saying
    /// when the reader failed, lines queued before it gave up, one that was wedged and has just
    /// come back. Dropping its channel with the worker lost those lines in the moment between
    /// the refusal and the replacement. Kept until the worker has gone (its end is dropped), and
    /// drained with this one's; a worker that stays wedged keeps its channel, one per wedge.
    leftovers: Vec<Receiver<String>>,
}

impl Prism {
    pub fn new() -> Self {
        Self::spawn(Start::First, Vec::new())
    }

    fn spawn(start: Start, leftovers: Vec<Receiver<String>>) -> Self {
        let (to_worker, rx) = channel::<Utterance>();
        let (refused_tx, refused_rx) = channel::<String>();
        // A retry starts UNHEALTHY and is promoted only once it has a screen reader in hand.
        // Starting it healthy would open a window — short, but real — in which lines were
        // handed to a worker that was about to find nothing and give up, and every one of
        // those would reach the user late, through the fallback, for no reason at all.
        let healthy = Arc::new(AtomicBool::new(start == Start::First));
        let pending = Arc::new(AtomicUsize::new(0));
        let outstanding: Outstanding = Arc::new(Mutex::new(None));
        let reader = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let (h, p, o, r, s) =
            (healthy.clone(), pending.clone(), outstanding.clone(), reader.clone(), stopped.clone());
        std::thread::Builder::new()
            .name("prism-speech".into())
            .spawn(move || run(start, rx, refused_tx, h, p, o, r, s))
            .ok();
        Self {
            to_worker,
            refused_rx,
            healthy,
            pending,
            outstanding,
            reader,
            retrying: false,
            stopped,
            leftovers,
        }
    }

    /// Replaces this worker with a searcher, keeping the old one's refusal channel.
    fn replace_with_searcher(&mut self, why: Why) {
        let mut leftovers = std::mem::take(&mut self.leftovers);
        // A placeholder takes the old channel's place for the moment the struct is rebuilt.
        let (_, placeholder) = channel::<String>();
        leftovers.push(std::mem::replace(&mut self.refused_rx, placeholder));
        *self = Self::spawn(Start::Retry(why), leftovers);
        self.retrying = true;
    }

    /// Whether a screen reader is what would say the next line.
    ///
    /// False when a plain speech engine is speaking instead, and false while the path is out
    /// of service — in both cases the words would come out of the speakers at somebody who
    /// may not have asked for them.
    pub fn via_screen_reader(&self) -> bool {
        self.healthy.load(Ordering::Relaxed) && self.reader.load(Ordering::Relaxed)
    }

    /// Sends somebody to look for the screen reader, once, as soon as it is gone.
    ///
    /// Called from the event loop, like everything else here that has to happen whether or not
    /// anything is being said. It starts a fresh worker rather than asking the old one to try
    /// again: after a stall the old one is stuck inside a call that will never return, and
    /// after a refusal prism's binding to that backend is fixed for the lifetime of the
    /// instance. Neither can be talked round, so neither is asked.
    ///
    /// One searcher, not one per attempt — it keeps its own library open and sleeps between
    /// looks, because opening the library is 62 ms of the 86 a look costs.
    ///
    /// The old worker is dropped by this assignment, which closes its channel and lets it
    /// finish — except a wedged one, which stays parked with its library. That leak is
    /// deliberate: a parked thread costs less than a wedged application. Its refusal channel
    /// is kept (see `leftovers`).
    ///
    /// Also reached when no screen reader was running at start: the first worker gives up the
    /// path after its one look, and this sends the searcher out, so a screen reader started
    /// later is found on the same schedule as one that came back.
    ///
    /// The caller calls this only while "Speak through the screen reader" is on, and a searcher
    /// stops at its next look once the setting is off (`wait_for_screen_reader`). So the first
    /// call after the setting is on again finds that searcher gone and sends a new one.
    pub fn retry_if_due(&mut self) {
        // `reader` BEFORE `healthy`, with Acquire against the Release a searcher stores it with
        // after storing `healthy` (`run`): seeing it set means seeing the `healthy` stored with
        // it, or a later one. Read the other way round, a searcher caught between its two
        // stores would look like one whose reader had already failed, and be replaced for
        // nothing (`searcher_due`).
        let had_reader = self.reader.load(Ordering::Acquire);
        if self.healthy.load(Ordering::Relaxed) {
            self.retrying = false;
            return;
        }
        let due = searcher_due(self.retrying, self.stopped.load(Ordering::Relaxed), had_reader);
        if let Some(why) = due {
            self.replace_with_searcher(why);
        }
    }

    /// Hands `text` to the screen reader. `false` means it cannot be, and the caller has to
    /// say it another way — now, not later.
    pub fn say(&self, text: &str, interrupt: bool) -> bool {
        if !self.healthy.load(Ordering::Relaxed) {
            return false;
        }
        let at = Instant::now();
        self.pending.fetch_add(1, Ordering::Relaxed);
        // Stamped HERE, before the worker has seen it, so the deadline covers everything after
        // the hand-over: a wedged `speak`, but also a backend that hangs while opening and a
        // worker that has not reached its loop yet. Only when nothing is outstanding, so what
        // is watched stays the OLDEST unanswered line.
        if let Ok(mut slot) = self.outstanding.lock() {
            if slot.is_none() {
                *slot = Some((at, text.to_string()));
            }
        }
        if self.to_worker.send(Utterance { text: text.to_string(), interrupt, at }).is_err() {
            self.forget_one();
            self.healthy.store(false, Ordering::Relaxed);
            return false;
        }
        true
    }

    /// Whether anything is still waiting to be said.
    ///
    /// Answered only while the path is healthy. Once it has been given up on, whatever the
    /// abandoned worker still holds is not going to be spoken by it, and the fallback's own
    /// `is_speaking` covers what happens instead. Without this, one stall would make every
    /// later shutdown sit out its full fifteen-second wait.
    pub fn pending(&self) -> bool {
        self.healthy.load(Ordering::Relaxed) && self.pending.load(Ordering::Relaxed) > 0
    }

    /// Everything the screen reader turned down, to be said another way.
    ///
    /// Also where the stall deadline is enforced, because this runs on the event loop while
    /// the worker may be stuck inside a call that cannot be cancelled. Every event loop calls
    /// it — the GUI timer tick, and the headless one through `HostEvents::on_tick`.
    pub fn refused(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        // Replaced workers first: what they hand back was said to them earlier.
        self.leftovers.retain(|rx| loop {
            match rx.try_recv() {
                Ok(t) => out.push(t),
                Err(TryRecvError::Empty) => break true,
                // That worker has gone, and everything it sent has been read.
                Err(TryRecvError::Disconnected) => break false,
            }
        });
        while let Ok(t) = self.refused_rx.try_recv() {
            out.push(t);
        }
        if let Some(text) = self.stalled() {
            out.push(text);
        }
        out
    }

    /// The text of a line that has gone unanswered too long, once.
    fn stalled(&self) -> Option<String> {
        if !self.healthy.load(Ordering::Relaxed) {
            return None;
        }
        let mut slot = self.outstanding.lock().ok()?;
        let (since, _) = slot.as_ref()?;
        if since.elapsed() < Duration::from_millis(STALL_MS) {
            return None;
        }
        let (_, text) = slot.take()?;
        self.healthy.store(false, Ordering::Relaxed);
        crate::logging::line(
            "speech",
            &format!(
                "the screen reader has not answered a line in {STALL_MS} ms, so that line and \
                 the ones after it go to the plain voice (OneCore or SAPI). The waiting call \
                 cannot be cancelled, so its thread is left where it is. {SCHEDULE}"
            ),
        );
        Some(text)
    }

    /// Undoes one `pending` increment, clearing the watch when nothing is left.
    fn forget_one(&self) {
        self.pending.fetch_sub(1, Ordering::Relaxed);
        clear(&self.outstanding);
    }

    /// Try the screen reader again — what ticking the setting means.
    ///
    /// Unlike VoiceOver's, this cannot simply raise a flag: the backend it would speak
    /// through is dead, and prism's binding to it is fixed for the lifetime of the instance
    /// and never retried. So the whole worker is replaced, and the old one — which may still
    /// be inside a call that will never return — is abandoned with everything it holds.
    pub fn rearm(&mut self) {
        if self.healthy.load(Ordering::Relaxed) {
            return;
        }
        crate::logging::line("speech", "trying the screen reader again, because its setting was ticked");
        self.replace_with_searcher(Why::Ticked);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    start: Start,
    rx: Receiver<Utterance>,
    refused: Sender<String>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    outstanding: Outstanding,
    reader: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
) {
    // Everything prism owns lives on this thread and dies with it. `Context` is `!Send`,
    // which is what makes that a compiler guarantee rather than a comment.
    let ctx = match Context::open() {
        Ok(ctx) => ctx,
        Err(why) => {
            healthy.store(false, Ordering::Relaxed);
            if start == Start::First {
                crate::logging::line(
                    "speech",
                    &format!("prism would not start ({why}), so speech falls back to the other path"),
                );
            }
            return refuse_forever(&rx, &refused, &pending, &outstanding);
        }
    };

    // Screen readers only, on both paths. The plain voice lives on its own thread — see
    // `fallback.rs` — because this one is the one that gets wedged, and that is exactly when
    // the other has to speak.
    let backend = match start {
        Start::First => match open_screen_reader(&ctx) {
            Some(b) => {
                reader.store(true, Ordering::Relaxed);
                Some(b)
            }
            None => {
                // Nothing to speak through: the path is given up at once, so lines go straight
                // to the plain voice rather than through this thread, and the event loop sends
                // a searcher out (`retry_if_due`) for a screen reader started later. Lines
                // handed over before this moment are answered below, as they always were.
                healthy.store(false, Ordering::Relaxed);
                None
            }
        },
        Start::Retry(why) => {
            match wait_for_screen_reader(&ctx, why, &rx, &refused, &pending, &outstanding, &stopped) {
                Some(b) => {
                    // Found. Only NOW does anything start coming this way, which is why the
                    // fallback never loses a line to a search that was going to come up empty.
                    // `healthy` first and `reader` second, released: the event loop reads
                    // `reader` first (`retry_if_due`) and takes it, with the path out of
                    // service, for a reader that has failed since — so it must never see it
                    // set before this `healthy` is.
                    healthy.store(true, Ordering::Relaxed);
                    reader.store(true, Ordering::Release);
                    Some(b)
                }
                None => return,
            }
        }
    };
    // Decided once per worker rather than per line: whether a display is attached does not
    // change while a session runs, and neither does whether this backend can write to one.
    // The setting can change, and is read per line — turning it off is somebody saying "stop
    // putting that on my display", which should take effect at once.
    let brailling = backend.as_ref().is_some_and(|b| b.supports(feature::OUTPUT));
    if brailling {
        crate::logging::line("speech", "braille goes out with the speech");
    }

    let mut first_line = true;
    // Said once, and only to answer a question the design could not: whether a screen reader
    // that has gone leaves the call blocked for ever or merely for a long time. The first
    // live loss was noticed by the deadline rather than by an error, so this is the way to
    // find out whether the thread that was abandoned is really parked for good.
    let mut abandoned = false;

    while let Ok(first) = rx.recv() {
        // Take everything already queued in one go, so an interrupting line can be judged
        // against what is actually waiting rather than one message at a time.
        let mut batch = vec![first];
        loop {
            match rx.try_recv() {
                Ok(next) => batch.push(next),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }

        // An interrupting line makes stale everything queued BEFORE it — not after. Saying an
        // overtaken line anyway would announce where the cursor used to be, several controls
        // late. But keeping only the newest message would be just as wrong: the overlay says
        // a control's name interrupting and its value straight after, so dropping everything
        // after the interrupt would speak a value whose name was never said.
        if let Some(i) = batch.iter().rposition(|u| u.interrupt) {
            if i > 0 {
                pending.fetch_sub(i, Ordering::Relaxed);
                batch.drain(..i);
            }
        }

        for u in batch {
            // Given up on while this thread was blocked, or earlier in this very batch. From
            // here the worker is a drain rather than a voice: it answers for what it holds so
            // the counter cannot leak, and hands back whatever is still worth hearing.
            if !healthy.load(Ordering::Relaxed) {
                answer(&u, &refused, &pending, &outstanding);
                continue;
            }
            let Some(b) = backend.as_ref() else {
                answer(&u, &refused, &pending, &outstanding);
                continue;
            };

            watch(&outstanding, &u);
            let started = Instant::now();
            // Decided per line, because the setting can change between two; kept, because
            // the log line for a refusal names which call it was.
            let braille_too = brailling && crate::appcfg::braille();
            let outcome = say(b, &u.text, u.interrupt, braille_too);
            let took = started.elapsed().as_millis() as u64;
            pending.fetch_sub(1, Ordering::Relaxed);
            // Cleared after EVERY line, not only when the queue empties. Left in place, a
            // line that was merely slow — say 400 ms — would still be the thing the deadline
            // is looking at in the moment between finishing it and picking up the next one,
            // and a successful announcement would demote a healthy screen reader.
            clear(&outstanding);

            // The caller may have given up on this thread while that call was blocked. What
            // it said has already been spoken another way, so nothing is reported from here
            // and nothing goes down the refusal channel twice.
            if !healthy.load(Ordering::Relaxed) {
                if !abandoned {
                    abandoned = true;
                    crate::logging::line(
                        "speech",
                        &format!(
                            "the call that was given up on came back after {took} ms; its \
                             thread can stop now"
                        ),
                    );
                }
                continue;
            }

            if first_line {
                first_line = false;
                crate::logging::line(
                    "speech",
                    &format!("the first line through {} took {took} ms", b.name()),
                );
            } else if took >= SLOW_LINE_MS {
                crate::logging::line("speech", &format!("a line through {} took {took} ms", b.name()));
            }

            if let Err(why) = outcome {
                // A rejected string says nothing about the screen reader and must not cost it.
                // prism answers `speak("")` with `INVALID_UTF8`, and an untitled plug-in window
                // reaches `host.speech.output` with an empty string today through
                // `modules/daw-hosts`. `prism-sys` already declines to pass those on, so
                // reaching here means something else about the text was wrong — still not
                // evidence that the reader has gone.
                if why.is_our_fault() {
                    crate::logging::line(
                        "speech",
                        &format!("the screen reader would not take a line of ours ({why}); it was skipped"),
                    );
                    continue;
                }
                healthy.store(false, Ordering::Relaxed);
                // Handed back BEFORE the line is written, so nothing here can hold it up.
                let _ = refused.send(u.text);
                // What happened, and nothing it cannot know. An error is not evidence that the
                // reader was closed: JAWS answers INTERNAL for a `SayString` — or, when braille
                // goes too, a `BrailleString` — that returned false, while it goes on running.
                // Whether it still runs is the searcher's first look, which the log names next.
                crate::logging::line(
                    "speech",
                    &format!(
                        "{name} refused a line ({why}, from the call that {what}), so that line \
                         and the ones after it go to the plain voice (OneCore or SAPI). {SCHEDULE} \
                         Ticking \"Speak through the screen reader\" off and on again looks at \
                         once.",
                        name = b.name(),
                        what = if braille_too {
                            "speaks it and writes it to the braille display"
                        } else {
                            "speaks it"
                        },
                    ),
                );
                // Said once, because one strike is the whole rule: prism's binding to NVDA is
                // fixed for the lifetime of the instance and never retried, so the second
                // error is guaranteed by the first. The thread stays alive as a drain — one
                // that exited here would leave a window in which `say` still saw a live
                // channel and dropped a line into nobody's hands.
            }
        }
    }
}

/// Looks for a screen reader until one opens, until nobody is waiting any more, or until
/// "Speak through the screen reader" is switched off.
///
/// A line per look would bury everything in the log that matters, so a look is logged only
/// when it is news (see [`Search`]): the first one's answer — which reader is running and will
/// not open, and why, or that none is running — a change of that answer, and the pace dropping
/// to every 30 seconds. Finding one is always logged unless nobody is listening any more, and
/// so is stopping because the setting is off.
#[allow(clippy::too_many_arguments)]
fn wait_for_screen_reader(
    ctx: &Context,
    why: Why,
    rx: &Receiver<Utterance>,
    refused: &Sender<String>,
    pending: &AtomicUsize,
    outstanding: &Outstanding,
    stopped: &AtomicBool,
) -> Option<prism_sys::Backend> {
    let began = Instant::now();
    let mut search = Search::new();
    loop {
        // Asked before every look, and again before a find is used: a searcher replaced while
        // it slept — ticking the setting off and on replaces it (`rearm`) — must neither pay
        // for another look nor log a find that nobody will speak through.
        if nobody_listening(rx, refused, pending, outstanding) {
            return None;
        }
        let seen = match look(ctx) {
            Ok(b) => {
                if nobody_listening(rx, refused, pending, outstanding) {
                    return None;
                }
                crate::logging::line("speech", &found_line(why, &b.name()));
                return Some(b);
            }
            Err(seen) => seen,
        };
        // Switched off: that is an explicit answer, and looking on would be work done against
        // it. Checked after a look rather than before one, so a look that finds the reader
        // still counts; the event loop sends a new searcher when the setting is on again
        // (`retry_if_due`), and ticking it looks at once (`rearm`, from `Speech::pump`).
        if !crate::appcfg::screen_reader_speech() {
            stopped.store(true, Ordering::Relaxed);
            crate::logging::line(
                "speech",
                "\"Speak through the screen reader\" is off, so the screen reader is not looked \
                 for any more; ticking it again looks at once",
            );
            return None;
        }
        let (nap, news) = search.after_look(began.elapsed(), seen);
        if let Some(line) = news {
            crate::logging::line("speech", &line);
        }
        std::thread::sleep(nap);
    }
}

/// Whether nobody is listening to this searcher any more: it has been replaced, or the
/// application is going away. Without asking, a searcher would sleep and sweep for the life of
/// the process, holding a library nobody can reach.
///
/// A line found waiting is handed back. Not expected — `say` refuses while the path is out of
/// service, and it is for as long as a searcher runs — but "not expected" is not "impossible",
/// and a lost line is the failure that matters.
fn nobody_listening(
    rx: &Receiver<Utterance>,
    refused: &Sender<String>,
    pending: &AtomicUsize,
    outstanding: &Outstanding,
) -> bool {
    loop {
        match rx.try_recv() {
            Err(TryRecvError::Disconnected) => return true,
            Err(TryRecvError::Empty) => return false,
            Ok(u) => answer(&u, refused, pending, outstanding),
        }
    }
}

/// The line a searcher logs when a screen reader opens, by why it was looking.
///
/// A search sent out at start-up does not say the reader was started after the application:
/// the start-up look cannot tell a reader that is not running from one that runs and would not
/// open yet (`open_screen_reader`), so either may be what just opened.
fn found_line(why: Why, name: &str) -> String {
    match why {
        Why::Lost => format!("back to {name}"),
        Why::NoneAtStart => format!("{name} is open now; speaking through it from now on"),
        Why::Ticked => format!("speaking through {name}"),
    }
}

/// One look: the first screen reader that opens, or what stood in the way.
///
/// Every reader is tried in order, as at start-up, and a reader that does not open is then
/// asked whether it is running at all ([`Context::is_available`] — prism's
/// `IS_SUPPORTED_AT_RUNTIME`, without initialising). The opening is not skipped for a reader
/// that says it is not running: that answer is prism's advice, and for ZDSR, say, it is a
/// search for six process names while opening asks the reader's own library, so skipping would
/// risk missing one that the start-up path finds.
fn look(ctx: &Context) -> Result<prism_sys::Backend, Sighting> {
    let mut failed = Vec::new();
    for name in SCREEN_READERS {
        match ctx.open_backend(name) {
            Ok(b) => return Ok(b),
            Err(why) => failed.push((*name, why)),
        }
    }
    for (name, why) in failed {
        if ctx.is_available(name) {
            return Err(Sighting::Refusing { name: name.to_string(), why: why.to_string() });
        }
    }
    Err(Sighting::NoneRunning)
}

/// Says a line, and writes it to a braille display as well where that is possible.
///
/// `output` is `speak` plus the braille display, in one call — which is the whole of the
/// braille support, and deliberately so. There is no separate braille channel in this API,
/// because there is none on macOS either: VoiceOver brailles what it says, and a
/// `host.braille` that did something on one platform and nothing on the other would be an
/// API making a promise it cannot keep. A backend without braille answers `output` by
/// speaking, so this is a strict superset rather than a fork in the road.
fn say(b: &prism_sys::Backend, text: &str, interrupt: bool, braille_too: bool) -> Result<(), prism_sys::Error> {
    if braille_too {
        b.output(text, interrupt)
    } else {
        b.speak(text, interrupt)
    }
}

/// Records which line the deadline is now watching.
fn watch(outstanding: &Outstanding, u: &Utterance) {
    if let Ok(mut slot) = outstanding.lock() {
        *slot = Some((u.at, u.text.clone()));
    }
}

/// Stops watching: nothing is outstanding until the next line is picked up.
fn clear(outstanding: &Outstanding) {
    if let Ok(mut slot) = outstanding.lock() {
        *slot = None;
    }
}

/// Accounts for a line this thread will not say, and hands it back if it is still worth
/// hearing.
fn answer(
    u: &Utterance,
    refused: &Sender<String>,
    pending: &AtomicUsize,
    outstanding: &Outstanding,
) {
    pending.fetch_sub(1, Ordering::Relaxed);
    clear(outstanding);
    if u.at.elapsed() < Duration::from_millis(STALE_MS) {
        let _ = refused.send(u.text.clone());
    }
}

/// Answers for everything, for as long as anybody keeps handing lines over.
///
/// Reached when prism could not start at all. The thread cannot speak, but it must still
/// account for what it is given: a channel nobody reads leaves `pending` above zero forever,
/// and `is_speaking` would then answer yes for the rest of the session.
fn refuse_forever(
    rx: &Receiver<Utterance>,
    refused: &Sender<String>,
    pending: &AtomicUsize,
    outstanding: &Outstanding,
) {
    while let Ok(u) = rx.recv() {
        answer(&u, refused, pending, outstanding);
    }
}

/// The screen reader that is actually running, if one is.
///
/// By name and in order, never `create_best`: that walks priority order and lands on a speech
/// engine whenever no screen reader is up — and opening one of those was measured at 2.9
/// seconds, which is not a thing to do while somebody waits for a window.
/// prism refuses one whose reader is not running — measured: with NVDA up, every other
/// backend answered `BACKEND_NOT_AVAILABLE`. That is what makes looking again safe, and
/// `prism-sys`'s smoke tests assert it, because the automatic return would otherwise adopt a
/// dead backend and the user would hear nothing at all.
///
/// The start-up look. A searcher looks with [`look`], which also says why nothing opened.
///
/// So this one's line says only that nothing opened, not that nothing runs: a JAWS that runs
/// and refuses (the error-9 state) fails to open here exactly as an absent one does. What runs
/// is the first thing the searcher that follows logs.
fn open_screen_reader(ctx: &Context) -> Option<prism_sys::Backend> {
    for name in SCREEN_READERS {
        if let Ok(b) = ctx.open_backend(name) {
            crate::logging::line("speech", &format!("speaking through {}", b.name()));
            return Some(b);
        }
    }
    crate::logging::line("speech", NOTHING_OPENED_AT_START);
    None
}

/// The start-up line when no screen reader opened.
const NOTHING_OPENED_AT_START: &str = "no screen reader would open, so the plain voice speaks. \
     While \"Speak through the screen reader\" is on, a search follows at once and logs whether \
     one is running and how often it looks.";


#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn jaws(why: &str) -> Sighting {
        Sighting::Refusing { name: "JAWS".into(), why: why.into() }
    }

    /// The pace: 3 s for five minutes and 10 s after while a reader runs, 3 s for a minute and
    /// 30 s after while none does.
    #[test]
    fn the_schedule() {
        for s in [0, 3, 59, 60, 61, 90, 299] {
            assert_eq!(next_look(secs(s), true), RETRY_SOON, "running, {s} s in");
        }
        for s in [300, 301, 3600] {
            assert_eq!(next_look(secs(s), true), RETRY_CAPPED, "running, {s} s in");
        }
        for s in [0, 3, 57, 59] {
            assert_eq!(next_look(secs(s), false), RETRY_SOON, "none running, {s} s in");
        }
        for s in [60, 61, 90, 3600] {
            assert_eq!(next_look(secs(s), false), RETRY_LATER, "none running, {s} s in");
        }
    }

    /// The session that set this: JAWS refused its first line and would not open again for
    /// over a minute while it ran. The pace must stay at 3 s past the minute, so it is picked
    /// up within 3 s of opening again, and the log says what stood in the way once.
    #[test]
    fn a_reader_that_runs_and_will_not_open_keeps_the_fast_pace() {
        let mut search = Search::new();
        let (nap, line) = search.after_look(secs(0), jaws("prism error 17: Unknown error"));
        assert_eq!(nap, RETRY_SOON);
        let line = line.expect("the first look is news");
        assert!(line.starts_with("JAWS is running but would not open (prism error 17: Unknown error)"), "{line}");
        assert!(line.contains("every 3 s for five minutes, then every 10 s, for as long as it runs"), "{line}");
        let mut t = 3;
        while t < 300 {
            // A different error from the same reader is not a new line every three seconds.
            let why = if t % 2 == 0 { "prism error 17: Unknown error" } else { "prism error 16: Backend not available" };
            let (nap, line) = search.after_look(secs(t), jaws(why));
            assert_eq!(nap, RETRY_SOON, "{t} s in");
            assert_eq!(line, None, "{t} s in");
            t += 3;
        }
        // Five minutes of it: the pace drops to 10 s, said once.
        let (nap, line) = search.after_look(secs(300), jaws("prism error 17: Unknown error"));
        assert_eq!(nap, RETRY_CAPPED);
        let line = line.expect("the pace dropping is news");
        assert!(line.starts_with("JAWS still would not open five minutes"), "{line}");
        assert!(line.ends_with("every 10 s from now"), "{line}");
        assert_eq!(search.after_look(secs(310), jaws("prism error 17: Unknown error")), (RETRY_CAPPED, None));
    }

    /// The five minutes count from when the reader was first seen running and refusing, not
    /// from when the search began: a JAWS started ten minutes into a search gets them in full.
    #[test]
    fn the_fast_pace_counts_from_the_reader_first_seen() {
        let mut search = Search::new();
        assert_eq!(search.after_look(secs(0), Sighting::NoneRunning).0, RETRY_SOON);
        assert_eq!(search.after_look(secs(600), Sighting::NoneRunning).0, RETRY_LATER);
        let (nap, line) = search.after_look(secs(630), jaws("prism error 9: Internal backend error"));
        assert_eq!(nap, RETRY_SOON);
        assert!(line.expect("news").contains("every 3 s for five minutes"));
        assert_eq!(search.after_look(secs(929), jaws("prism error 9: Internal backend error")).0, RETRY_SOON);
        assert_eq!(search.after_look(secs(930), jaws("prism error 9: Internal backend error")).0, RETRY_CAPPED);
    }

    /// ZDSR and Boy PC Reader are "running" while any of their processes is, services
    /// included, so they get the pace of a search that found none: 3 s for a minute, then 30 s.
    #[test]
    fn a_reader_checked_through_its_processes_does_not_keep_the_fast_pace() {
        for name in ["ZDSR", "BoyPCReader"] {
            let seen = || Sighting::Refusing { name: name.into(), why: "prism error 16: Backend not available".into() };
            let mut search = Search::new();
            let (nap, line) = search.after_look(secs(0), seen());
            assert_eq!(nap, RETRY_SOON, "{name}");
            let line = line.expect("the first look is news");
            assert!(line.starts_with(&format!("{name} would not open (prism error 16")), "{line}");
            assert!(line.contains("background services"), "{line}");
            assert!(line.ends_with("every 3 s until a minute has passed, then every 30 s"), "{line}");
            assert_eq!(search.after_look(secs(57), seen()), (RETRY_SOON, None), "{name}");
            let (nap, line) = search.after_look(secs(60), seen());
            assert_eq!(nap, RETRY_LATER, "{name}");
            assert!(line.expect("news").starts_with(&format!("{name} still would not open a minute")));
        }
        for name in CHECKED_BY_THE_READER {
            assert!(SCREEN_READERS.contains(name), "{name} is not a screen-reader backend name");
        }
    }

    /// A reader that is really gone: 3 s for a minute, one line when the pace drops, and a line
    /// again when one turns up running but refusing, or when it goes again.
    #[test]
    fn a_reader_that_is_gone_slows_down_after_a_minute() {
        let mut search = Search::new();
        let (nap, line) = search.after_look(secs(0), Sighting::NoneRunning);
        assert_eq!(nap, RETRY_SOON);
        let line = line.expect("the first look is news");
        assert!(line.starts_with("no screen reader is running"), "{line}");
        assert!(line.contains("every 3 s until a minute has passed, then every 30 s"), "{line}");
        assert_eq!(search.after_look(secs(3), Sighting::NoneRunning), (RETRY_SOON, None));
        assert_eq!(search.after_look(secs(57), Sighting::NoneRunning), (RETRY_SOON, None));
        let (nap, line) = search.after_look(secs(60), Sighting::NoneRunning);
        assert_eq!(nap, RETRY_LATER);
        assert!(line.expect("the pace dropping is news").contains("every 30 s from now"));
        assert_eq!(search.after_look(secs(90), Sighting::NoneRunning), (RETRY_LATER, None));
        // Started again, but not answering yet: back to the fast pace, and said.
        let (nap, line) = search.after_look(secs(120), jaws("prism error 17: Unknown error"));
        assert_eq!(nap, RETRY_SOON);
        assert!(line.expect("news").starts_with("JAWS is running but would not open"));
        // And gone again, past the minute: straight to the slow pace.
        let (nap, line) = search.after_look(secs(123), Sighting::NoneRunning);
        assert_eq!(nap, RETRY_LATER);
        let line = line.expect("news");
        assert!(line.starts_with("no screen reader is running"), "{line}");
        assert!(line.ends_with("every 30 s"), "{line}");
    }

    /// A JAWS that runs and refuses when the application starts fails the start-up look as an
    /// absent one does. So the start-up line says only that nothing opened, the search that
    /// follows says what it saw on its first look, and what it logs on finding one does not
    /// claim the reader was started after the application.
    #[test]
    fn a_search_from_start_up_says_what_it_saw() {
        assert!(NOTHING_OPENED_AT_START.starts_with("no screen reader would open"));
        assert!(!NOTHING_OPENED_AT_START.contains("no screen reader is running"), "{NOTHING_OPENED_AT_START}");
        let mut search = Search::new();
        let (nap, line) = search.after_look(secs(0), jaws("prism error 9: Internal backend error"));
        assert_eq!(nap, RETRY_SOON);
        assert!(line.expect("the first look is news").starts_with("JAWS is running but would not open"));
        let mut search = Search::new();
        let (_, line) = search.after_look(secs(0), Sighting::NoneRunning);
        assert!(line.expect("the first look is news").starts_with("no screen reader is running"));
        assert_eq!(found_line(Why::NoneAtStart, "JAWS"), "JAWS is open now; speaking through it from now on");
        assert_eq!(found_line(Why::Lost, "JAWS"), "back to JAWS");
        assert_eq!(found_line(Why::Ticked, "NVDA"), "speaking through NVDA");
    }

    /// Who is sent out, and when: one searcher at a time, another once the first has stopped
    /// because the setting was off or the reader it found has failed, and the reason the log
    /// names.
    #[test]
    fn when_a_searcher_goes() {
        // Nobody out yet: sent, and why depends on whether a reader was ever in hand.
        assert_eq!(searcher_due(false, false, true), Some(Why::Lost));
        assert_eq!(searcher_due(false, false, false), Some(Why::NoneAtStart));
        // One is out and still looking: nobody else.
        assert_eq!(searcher_due(true, false, false), None);
        // One was out, found a reader, and that reader refused a line before any pass of the
        // loop saw the path healthy: nobody is looking any more, so another goes.
        assert_eq!(searcher_due(true, false, true), Some(Why::Lost));
        // It stopped because the setting was off, and the setting is on again.
        assert_eq!(searcher_due(true, true, true), Some(Why::Ticked));
        assert_eq!(searcher_due(true, true, false), Some(Why::Ticked));
    }

    /// The line a refusal writes names the error in prism's words.
    #[test]
    fn a_prism_error_names_itself() {
        assert_eq!(prism_sys::Error(9).to_string(), "prism error 9: Internal backend error");
        assert_eq!(prism_sys::Error(16).to_string(), "prism error 16: Backend not available");
        // Out of range is prism's own "Unknown error", not a crash.
        assert_eq!(prism_sys::Error(999).to_string(), "prism error 999: Unknown error");
        assert_eq!(prism_sys::Error(-1).to_string(), "prism error -1: Unknown error");
    }

    /// A screen reader that is there again is picked up, and quickly.
    ///
    /// This is the test the first live attempt needed and did not have. The way back worked,
    /// but the eager tier was keyed off the wrong instant, so it took thirty seconds instead
    /// of three — which from a keyboard is indistinguishable from not working at all.
    ///
    /// It leans on this machine having a screen reader running: losing one is simulated, and
    /// the one that is already there stands in for the one that came back. With none present
    /// there is nothing to find, and the test says so rather than failing.
    #[test]
    fn a_lost_screen_reader_is_found_again() {
        let Ok(ctx) = prism_sys::Context::open() else {
            eprintln!("prism will not start here; nothing to test");
            return;
        };
        if !SCREEN_READERS.iter().any(|n| ctx.open_backend(n).is_ok()) {
            eprintln!("no screen reader on this machine; nothing to come back");
            return;
        }
        drop(ctx);

        let mut prism = Prism::new();
        // What a stall or a refusal does, without needing either to happen.
        prism.healthy.store(false, Ordering::Relaxed);

        // The event loop would call this every tick; once is enough, because the searcher it
        // starts keeps looking on its own.
        prism.retry_if_due();
        assert!(prism.retrying, "nobody was sent to look");

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !prism.healthy.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            prism.healthy.load(Ordering::Relaxed),
            "the screen reader was there the whole time and was not picked up within five \
             seconds"
        );
    }

    /// Asking twice does not send a second searcher.
    #[test]
    fn only_one_searcher_at_a_time() {
        let mut prism = Prism::new();
        prism.healthy.store(false, Ordering::Relaxed);
        prism.retry_if_due();
        // Identity by the shared flag rather than the channel: a second searcher would come
        // with a whole new set of them.
        let first = Arc::as_ptr(&prism.healthy);
        prism.retry_if_due();
        assert!(
            std::ptr::eq(first, Arc::as_ptr(&prism.healthy)),
            "a second searcher was started while the first was still looking"
        );
    }
}
