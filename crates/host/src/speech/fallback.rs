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
use std::sync::Arc;

use prism_sys::{Context, SYNTHESISERS};

/// A line for the plain voice, and whether it displaces what is already waiting.
struct Utterance {
    text: String,
    interrupt: bool,
}

/// A speech engine on a thread of its own.
pub struct Fallback {
    to_worker: Sender<Utterance>,
    /// Handed over but not yet said. There is nothing behind this one, so the count is only
    /// used to answer "is anything still waiting" at shutdown.
    pending: Arc<AtomicUsize>,
    /// False once nothing can speak at all — no engine opened, or the worker is gone.
    healthy: Arc<AtomicBool>,
}

impl Fallback {
    pub fn new() -> Self {
        let (to_worker, rx) = channel::<Utterance>();
        let pending = Arc::new(AtomicUsize::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let (p, h) = (pending.clone(), healthy.clone());
        std::thread::Builder::new()
            .name("prism-fallback".into())
            .spawn(move || run(rx, p, h))
            .ok();
        Self { to_worker, pending, healthy }
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
        if self.to_worker.send(Utterance { text: text.to_string(), interrupt }).is_err() {
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

fn run(rx: Receiver<Utterance>, pending: Arc<AtomicUsize>, healthy: Arc<AtomicBool>) {
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

    while let Ok(mut u) = rx.recv() {
        let mut dropped = 0usize;
        // Same rule as the screen-reader path: an interrupting line makes stale everything
        // queued before it, so those are dropped rather than said several controls late.
        // Nothing is kept after it, because this side never queues a name-then-value pair —
        // it is handed one refused line at a time.
        if u.interrupt {
            loop {
                match rx.try_recv() {
                    Ok(next) => {
                        dropped += 1;
                        u = next;
                    }
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
fn drain(rx: &Receiver<Utterance>, pending: &AtomicUsize) {
    while rx.recv().is_ok() {
        pending.fetch_sub(1, Ordering::Relaxed);
    }
}
