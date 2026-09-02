//! The plain voice, for when no screen reader is listening or the one that was has gone.
//!
//! Deliberately a second worker with its own prism library rather than a second backend on
//! the first one: the screen-reader path is usually lost by the **stall deadline**, which
//! means its thread is wedged inside a call that will never return — and that is precisely
//! the moment this has to speak. Anything sharing that thread would be wedged with it.
//!
//! It opens in the background and queues whatever arrives meanwhile, because opening a
//! speech engine is not quick: measured on this machine, OneCore takes 3.5 s and SAPI 2.0 s.
//! What it replaced — `Tts::default()` from the `tts` crate — cost **2872 ms of blocking
//! start-up** in the real application, for a voice that on a machine with a screen reader
//! never says a word, and still wanted 567 ms for its first line when it did.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use prism_sys::{Context, SCREEN_READERS, SYNTHESISERS};

/// A line for the plain voice, and whether it displaces what is already waiting.
struct Utterance {
    text: String,
    interrupt: bool,
}

/// What this worker is being asked for.
///
/// It answers questions as well as speaking, and that is not a convenience. Opening a prism
/// library fixes the calling thread's COM apartment and closing it unfixes that again, and
/// doing so repeatedly has misbehaved in three different ways here: a crash when OneCore was
/// asked about itself from a second library, a process that could not exit after every one of
/// its tests had passed, and a hang under a handful of concurrent queries. The one pattern
/// that has never misbehaved is the one the application already runs on — a library opened
/// once by a long-lived thread and kept. So the questions come here, where such a library
/// already lives, rather than each caller bringing its own.
///
/// This worker rather than the screen reader's, because this one cannot wedge: it speaks
/// through a speech engine, not through an RPC call into somebody else's process.
enum Job {
    Say(Utterance),
    /// Look at what can speak and leave the answer in the shared slot.
    Refresh,
}

/// A speech engine on a thread of its own.
pub struct Fallback {
    to_worker: Sender<Job>,
    /// Handed over but not yet said. There is nothing behind this one, so the count is only
    /// used to answer "is anything still waiting" at shutdown.
    pending: Arc<AtomicUsize>,
    /// False once nothing can speak at all — no engine opened, or the worker is gone.
    healthy: Arc<AtomicBool>,
    /// What could speak, as of the last time the worker looked.
    known: Arc<Mutex<Vec<super::Engine>>>,
}

impl Fallback {
    pub fn new() -> Self {
        let (to_worker, rx) = channel::<Job>();
        let pending = Arc::new(AtomicUsize::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let known = Arc::new(Mutex::new(Vec::new()));
        let (p, h, k) = (pending.clone(), healthy.clone(), known.clone());
        std::thread::Builder::new()
            .name("prism-fallback".into())
            .spawn(move || run(rx, p, h, k))
            .ok();
        Self { to_worker, pending, healthy, known }
    }

    /// What could speak, as of the last look, and ask for another.
    ///
    /// The caller gets an answer at once — a clone of a small vector — because it is the
    /// event loop, whose whole budget is 300 ms before Windows stops waiting for our
    /// keyboard hook, and a fresh look was measured at 35 ms idle and 2.7 s while an engine
    /// was opening. For "which screen readers are running" a moment-old answer is the right
    /// kind: it changes when somebody starts or quits one, not between two lines of Luau.
    pub fn engines(&self) -> Vec<super::Engine> {
        let snapshot = self.known.lock().map(|k| k.clone()).unwrap_or_default();
        let _ = self.to_worker.send(Job::Refresh);
        snapshot
    }

    /// Says `text`, or reports that nothing can.
    ///
    /// Returning false here means the application has no voice at all — there is nowhere
    /// further to fall — so the caller's only remaining move is to write it down.
    pub fn say(&self, text: &str, interrupt: bool) -> bool {
        if !self.healthy.load(Ordering::Relaxed) {
            return false;
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        if self.to_worker.send(Job::Say(Utterance { text: text.to_string(), interrupt })).is_err() {
            self.pending.fetch_sub(1, Ordering::Relaxed);
            self.healthy.store(false, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }
}

fn run(
    rx: Receiver<Job>,
    pending: Arc<AtomicUsize>,
    healthy: Arc<AtomicBool>,
    known: Arc<Mutex<Vec<super::Engine>>>,
) {
    // Opened before the first line is read from the channel, so anything said in the
    // meantime waits rather than being lost — and none of that waiting is start-up time,
    // because nobody is blocked on this thread.
    let ctx = match Context::open() {
        Ok(ctx) => ctx,
        Err(why) => {
            healthy.store(false, Ordering::Relaxed);
            crate::logging::line("speech", &format!("prism would not start ({why}); nothing can speak"));
            return drain(&rx, &pending);
        }
    };
    let backend = SYNTHESISERS.iter().find_map(|name| ctx.open_backend(name).ok());
    let Some(backend) = backend else {
        healthy.store(false, Ordering::Relaxed);
        crate::logging::line("speech", "no speech engine on this machine; only a screen reader can speak here");
        return drain(&rx, &pending);
    };
    crate::logging::line("speech", &format!("the plain voice is {}", backend.name()));
    // Once before anything is said, so the first module to ask has something true to read.
    look(&ctx, &known);

    while let Ok(job) = rx.recv() {
        let mut u = match job {
            Job::Say(u) => u,
            Job::Refresh => {
                look(&ctx, &known);
                continue;
            }
        };
        let mut dropped = 0usize;
        // Same rule as the screen-reader path: an interrupting line makes stale everything
        // queued before it, so those are dropped rather than said several controls late.
        // Nothing is kept after it, because this side never queues a name-then-value pair —
        // it is handed one refused line at a time.
        if u.interrupt {
            loop {
                match rx.try_recv() {
                    Ok(Job::Say(next)) => {
                        dropped += 1;
                        u = next;
                    }
                    // A question waiting behind a line is answered after it, not instead of
                    // it: speaking is what this thread is for.
                    Ok(Job::Refresh) => look(&ctx, &known),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        if let Err(why) = backend.speak(&u.text, u.interrupt) {
            // Not a strike: there is nothing to demote to. A speech engine is not going
            // anywhere the way a screen reader does, and refusing one line is no reason to
            // stop trying the next.
            if !why.is_our_fault() {
                crate::logging::line("speech", &format!("the plain voice would not say a line ({why})"));
            }
        }
        pending.fetch_sub(1 + dropped, Ordering::Relaxed);
    }
}

/// Accounts for everything still waiting, so `pending` cannot sit above zero for ever.
fn drain(rx: &Receiver<Job>, pending: &AtomicUsize) {
    while let Ok(job) = rx.recv() {
        if matches!(job, Job::Say(_)) {
            pending.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Asks this worker's own library what can speak, and leaves the answer where the caller
/// will find it.
fn look(ctx: &Context, known: &Arc<Mutex<Vec<super::Engine>>>) {
    let list: Vec<super::Engine> = prism_sys::BACKENDS
        .iter()
        .map(|name| super::Engine {
            id: id_for(name),
            name: (*name).to_string(),
            screen_reader: SCREEN_READERS.contains(name),
            available: ctx.is_available(name),
        })
        .collect();
    if let Ok(mut slot) = known.lock() {
        *slot = list;
    }
}

/// The name a module uses for an engine, from the name prism reports.
///
/// Lower case and without punctuation, so it survives a screen reader renaming itself between
/// versions: "PCTalker" and "PC-Talker" are the same id. This is the only string in the list
/// a module should ever compare, which is why it is derived rather than passed through.
fn id_for(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}
