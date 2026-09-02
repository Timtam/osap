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

use prism_sys::{Context, SCREEN_READERS, SYNTHESISERS};

/// How long a line may go unanswered before the screen-reader path is given up on.
///
/// The reason this exists at all: `osascript` can be killed, and a synchronous RPC call into
/// a hung NVDA cannot — it blocks with no timeout, inside our own process. So the deadline
/// lives with the caller rather than with the call. 300 ms is not a round number picked for
/// looks: it is the same budget the event loop is already written around, because Windows
/// removes a low-level keyboard hook that takes longer than `LowLevelHooksTimeout`.
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
const RETRY_SOON: Duration = Duration::from_secs(3);
const RETRY_LATER: Duration = Duration::from_secs(30);
const RETRY_SOON_FOR: Duration = Duration::from_secs(60);

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
    /// Looking for a screen reader that has come back. Silent unless it finds one, and never
    /// opens a speech engine — the fallback is already speaking, and the only reason to be
    /// here is to stop needing it.
    Retry,
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
    /// Whether a worker is already out looking for the screen reader, so that exactly one is.
    retrying: bool,
}

impl Prism {
    pub fn new() -> Self {
        Self::spawn(Start::First)
    }

    fn spawn(start: Start) -> Self {
        let (to_worker, rx) = channel::<Utterance>();
        let (refused_tx, refused_rx) = channel::<String>();
        // A retry starts UNHEALTHY and is promoted only once it has a screen reader in hand.
        // Starting it healthy would open a window — short, but real — in which lines were
        // handed to a worker that was about to find nothing and give up, and every one of
        // those would reach the user late, through the fallback, for no reason at all.
        let healthy = Arc::new(AtomicBool::new(start == Start::First));
        let pending = Arc::new(AtomicUsize::new(0));
        let outstanding: Outstanding = Arc::new(Mutex::new(None));
        let (h, p, o) = (healthy.clone(), pending.clone(), outstanding.clone());
        std::thread::Builder::new()
            .name("prism-speech".into())
            .spawn(move || run(start, rx, refused_tx, h, p, o))
            .ok();
        Self { to_worker, refused_rx, healthy, pending, outstanding, retrying: false }
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
    /// deliberate: a parked thread costs less than a wedged application.
    pub fn retry_if_due(&mut self) {
        if self.healthy.load(Ordering::Relaxed) {
            self.retrying = false;
            return;
        }
        if self.retrying {
            return;
        }
        *self = Self::spawn(Start::Retry);
        self.retrying = true;
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
    pub fn refused(&self) -> Vec<String> {
        let mut out = Vec::new();
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
                "the screen reader has not answered in {STALL_MS} ms, so the overlay's own \
                 voice takes over from here. The waiting call cannot be cancelled, so its \
                 thread is left where it is."
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
        *self = Self::spawn(Start::Retry);
        self.retrying = true;
    }
}

fn run(
    start: Start,
    rx: Receiver<Utterance>,
    refused: Sender<String>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    outstanding: Outstanding,
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

    // Opened here rather than on the first line that needs one. Opening a speech engine was
    // measured at 2.9 seconds, and doing it lazily would spend all of that with somebody
    // already waiting for an answer to a keystroke — inside the very call the deadline exists
    // to bound. Here it is paid while nothing is waiting on it.
    let backend = match start {
        Start::First => open_screen_reader(&ctx, start).or_else(|| open_synthesiser(&ctx)),
        Start::Retry => match wait_for_screen_reader(&ctx, &rx, &refused, &pending, &outstanding)
        {
            Some(b) => {
                // Found. Only NOW does anything start coming this way, which is why the
                // fallback never loses a line to a search that was going to come up empty.
                healthy.store(true, Ordering::Relaxed);
                Some(b)
            }
            None => return,
        },
    };
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
            let outcome = b.speak(&u.text, u.interrupt);
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
                crate::logging::line(
                    "speech",
                    &format!(
                        "{} stopped taking what we said ({why}), so the overlay's own voice \
                         takes over from here. Usually this means the screen reader was \
                         closed; ticking its setting off and on again tries it afresh.",
                        b.name()
                    ),
                );
                let _ = refused.send(u.text);
                // Said once, because one strike is the whole rule: prism's binding to NVDA is
                // fixed for the lifetime of the instance and never retried, so the second
                // error is guaranteed by the first. The thread stays alive as a drain — one
                // that exited here would leave a window in which `say` still saw a live
                // channel and dropped a line into nobody's hands.
            }
        }
    }
}

/// Looks for a screen reader until one turns up, or until nobody is waiting any more.
///
/// Nothing is logged while it looks: this runs every few seconds, and a line each time would
/// bury everything in the log that matters. The one thing that IS worth a line — finding one —
/// is logged by `open_screen_reader`.
fn wait_for_screen_reader(
    ctx: &Context,
    rx: &Receiver<Utterance>,
    refused: &Sender<String>,
    pending: &AtomicUsize,
    outstanding: &Outstanding,
) -> Option<prism_sys::Backend> {
    let began = Instant::now();
    loop {
        if let Some(b) = open_screen_reader(ctx, Start::Retry) {
            return Some(b);
        }
        // Nobody is listening any more — this searcher has been replaced, or the application
        // is going away. Without this it would sleep and sweep for the life of the process,
        // holding a library nobody can reach.
        match rx.try_recv() {
            Err(TryRecvError::Disconnected) => return None,
            // Not expected: `say` refuses while the path is unhealthy, and it is unhealthy
            // for as long as this is running. Handed back rather than dropped, because "not
            // expected" is not "impossible" and a lost line is the failure that matters.
            Ok(u) => answer(&u, refused, pending, outstanding),
            Err(TryRecvError::Empty) => {}
        }
        let nap = if began.elapsed() < RETRY_SOON_FOR { RETRY_SOON } else { RETRY_LATER };
        std::thread::sleep(nap);
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
fn open_screen_reader(ctx: &Context, start: Start) -> Option<prism_sys::Backend> {
    for name in SCREEN_READERS {
        if let Ok(b) = ctx.open_backend(name) {
            let how = if start == Start::First { "speaking through" } else { "back to" };
            crate::logging::line("speech", &format!("{how} {}", b.name()));
            return Some(b);
        }
    }
    if start == Start::First {
        crate::logging::line("speech", "no screen reader is running");
    }
    None
}

/// A plain speech engine, for a module that wants to be heard with no screen reader present.
fn open_synthesiser(ctx: &Context) -> Option<prism_sys::Backend> {
    for name in SYNTHESISERS {
        if let Ok(b) = ctx.open_backend(name) {
            crate::logging::line("speech", &format!("speaking through {}", b.name()));
            return Some(b);
        }
    }
    crate::logging::line("speech", "nothing on this machine can speak");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
