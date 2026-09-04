//! The system voice on macOS, and the user's own Personal Voice, over AVFoundation directly.
//!
//! Its own file rather than a block inside `speech`, for the reason `voiceover.rs` gives:
//! `crates/macos-check` borrows it by path and asks the compiler whether it is true, which is
//! the only compiler this project can run at home.
//!
//! **Why hand-written rather than a crate.** This replaces `tts`, and the reason is not the
//! dependency count. It is Personal Voice: a voice the user recorded of themselves, which for
//! somebody who spends their day listening to a synthesiser is not a novelty. `tts` does not
//! expose it, and prism — the other candidate — asks for it in `initialize()`, at start-up,
//! where the authorisation call blocks for up to two minutes. Asking only when somebody
//! actually chooses a Personal Voice is the whole difference, and it needs the API directly.
//!
//! **Why a worker thread.** `speakUtterance` queues and returns, so the speaking itself does
//! not block — but building the synthesiser and enumerating the installed voices do, and this
//! event loop also carries the keyboard. `AVSpeechSynthesizer` carries no main-thread
//! requirement (objc2 marks main-thread classes and does not mark this one), so it can live
//! on a thread of its own, be built there on first use, and never be paid for by a session
//! that says nothing.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use objc2::rc::Retained;
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisVoice, AVSpeechSynthesisVoiceTraits, AVSpeechSynthesizer,
    AVSpeechUtterance,
};
use objc2_foundation::NSString;

/// One installed voice, in the shape `speech::Engine` needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voice {
    /// AVFoundation's own identifier, e.g. `com.apple.voice.compact.en-GB.Daniel`. Stable
    /// across sessions, which is what a module storing a choice needs.
    pub id: String,
    /// What the voice calls itself — "Daniel", "Anna". Meant to be said, not parsed.
    pub name: String,
    /// A voice the user recorded of themselves. Not usable until authorised, and asking is
    /// what `authorise_personal` is for.
    pub personal: bool,
}

enum Job {
    Say { text: String, interrupt: bool, voice: Option<String> },
    /// Re-read the installed voices into the shared snapshot.
    Refresh,
    /// Ask macOS for permission to use Personal Voice. Only ever sent when somebody has
    /// chosen one — see the note on the file.
    AuthorisePersonal,
}

/// The system speech synthesiser, on a thread of its own.
pub struct AvSpeech {
    to_worker: Sender<Job>,
    /// Handed over but not yet said, so `is_speaking` has something true to say.
    pending: Arc<AtomicUsize>,
    /// False once the worker has gone — the channel closed, or the synthesiser could not be
    /// built. Everything after goes nowhere, and the caller is told so it can say otherwise.
    healthy: Arc<AtomicBool>,
    /// What is installed. Written by the worker, read by whoever asks — the same shape the
    /// Windows side uses, and for the same reason: enumerating voices is not free, and
    /// `engines()` is a question a module may ask on any tick.
    voices: Arc<Mutex<Vec<Voice>>>,
    /// Whether Personal Voice has been granted. `None` until anybody has asked.
    personal_granted: Arc<Mutex<Option<bool>>>,
}

impl AvSpeech {
    pub fn new() -> Self {
        let (to_worker, rx) = channel::<Job>();
        let pending = Arc::new(AtomicUsize::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let voices = Arc::new(Mutex::new(Vec::new()));
        let personal_granted = Arc::new(Mutex::new(None));
        let (p, h, v, g) =
            (pending.clone(), healthy.clone(), voices.clone(), personal_granted.clone());
        std::thread::Builder::new()
            .name("avspeech".into())
            .spawn(move || run(rx, p, h, v, g))
            .ok();
        // The list is wanted before anything is said — `engines()` may be the first call —
        // and reading it is what the worker can do while nothing else is happening.
        let me = Self { to_worker, pending, healthy, voices, personal_granted };
        me.refresh();
        me
    }

    /// Says `text` in `voice`, or in the system default when none is given.
    ///
    /// `false` means it could not be handed over at all, and the caller has to say it another
    /// way — now, not later.
    pub fn say(&self, text: &str, interrupt: bool, voice: Option<&str>) -> bool {
        if !self.healthy.load(Ordering::Relaxed) {
            return false;
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        let job =
            Job::Say { text: text.to_string(), interrupt, voice: voice.map(str::to_string) };
        if self.to_worker.send(job).is_err() {
            self.pending.fetch_sub(1, Ordering::Relaxed);
            self.healthy.store(false, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }

    /// The installed voices as last read. Cheap: a clone of a small vector.
    pub fn voices(&self) -> Vec<Voice> {
        self.voices.lock().map(|v| v.clone()).unwrap_or_default()
    }

    /// Asks the worker to re-read the installed voices. Answers arrive in `voices()`.
    pub fn refresh(&self) {
        let _ = self.to_worker.send(Job::Refresh);
    }

    /// Whether Personal Voice may be used. `None` means nobody has asked yet.
    pub fn personal_granted(&self) -> Option<bool> {
        self.personal_granted.lock().ok().and_then(|g| *g)
    }

    /// Asks macOS for permission to use Personal Voice.
    ///
    /// Sent only when somebody has chosen one. The authorisation call can sit for a long time
    /// — prism's version of this, made unconditionally at start-up, is why prism is not used
    /// on this platform — so it happens on the worker, after a choice, and never otherwise.
    pub fn authorise_personal(&self) {
        let _ = self.to_worker.send(Job::AuthorisePersonal);
    }
}

/// A line that took longer than this is worth a log line of its own.
const SLOW_LINE_MS: u64 = 150;

fn run(
    rx: Receiver<Job>,
    pending: Arc<AtomicUsize>,
    healthy: Arc<AtomicBool>,
    voices: Arc<Mutex<Vec<Voice>>>,
    personal_granted: Arc<Mutex<Option<bool>>>,
) {
    // Built on first use rather than here, and that is the point of the rewrite: `tts` cost
    // 2872 ms of blocking start-up on Windows for a voice that usually never spoke, and this
    // path is the same shape. A session that says nothing pays nothing.
    let mut synth: Option<Retained<AVSpeechSynthesizer>> = None;
    let mut said = 0u64;

    while let Ok(job) = rx.recv() {
        match job {
            Job::Refresh => {
                let list = installed_voices();
                if let Ok(mut v) = voices.lock() {
                    *v = list;
                }
            }
            Job::AuthorisePersonal => {
                let granted = request_personal_voice();
                if let Ok(mut g) = personal_granted.lock() {
                    *g = Some(granted);
                }
                // The voices change with the answer: a granted Personal Voice appears in the
                // list that did not contain it a moment ago.
                if granted {
                    let list = installed_voices();
                    if let Ok(mut v) = voices.lock() {
                        *v = list;
                    }
                }
            }
            Job::Say { text, interrupt, voice } => {
                let started = Instant::now();
                if synth.is_none() {
                    let t = Instant::now();
                    synth = Some(unsafe { AVSpeechSynthesizer::new() });
                    crate::logging::line(
                        "speech",
                        &format!(
                            "the system voice took {} ms to build, on its own thread",
                            t.elapsed().as_millis()
                        ),
                    );
                }
                let Some(s) = synth.as_ref() else {
                    healthy.store(false, Ordering::Relaxed);
                    pending.fetch_sub(1, Ordering::Relaxed);
                    continue;
                };
                unsafe {
                    if interrupt {
                        // Immediate rather than at a word boundary: `interrupt` means the
                        // thing being said is already out of date, and finishing the word
                        // spends time on something nobody needs to hear.
                        s.stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
                    }
                    let u = AVSpeechUtterance::speechUtteranceWithString(&NSString::from_str(
                        &text,
                    ));
                    if let Some(id) = voice.as_deref() {
                        // A voice that has gone — uninstalled, or a Personal Voice that is no
                        // longer authorised — leaves the utterance on the system default
                        // rather than silent. Saying it in the wrong voice beats not saying it.
                        let v = AVSpeechSynthesisVoice::voiceWithIdentifier(&NSString::from_str(
                            id,
                        ));
                        if v.is_some() {
                            u.setVoice(v.as_deref());
                        }
                    }
                    s.speakUtterance(&u);
                }
                let took = started.elapsed().as_millis() as u64;
                pending.fetch_sub(1, Ordering::Relaxed);
                said += 1;
                // `speakUtterance` queues and returns, so this measures the HAND-OVER, not the
                // speaking. It is reported when it is slow because a slow hand-over is the
                // symptom that would matter: it would mean this thread is where the time goes.
                if took >= SLOW_LINE_MS {
                    crate::logging::line(
                        "speech",
                        &format!(
                            "handing a line to the system voice took {took} ms — that is the \
                             hand-over alone, not the speaking (line {said})"
                        ),
                    );
                }
            }
        }
    }
    healthy.store(false, Ordering::Relaxed);
}

/// Every installed voice, with the Personal ones marked.
fn installed_voices() -> Vec<Voice> {
    let t = Instant::now();
    let mut out = Vec::new();
    // SAFETY: a class method with no arguments returning an array of voices; nothing here
    // outlives the call except the strings, which are copied.
    let list = unsafe { AVSpeechSynthesisVoice::speechVoices() };
    for v in list.iter() {
        let (id, name, traits) = unsafe { (v.identifier(), v.name(), v.voiceTraits()) };
        out.push(Voice {
            id: id.to_string(),
            name: name.to_string(),
            personal: traits.contains(AVSpeechSynthesisVoiceTraits::IsPersonalVoice),
        });
    }
    let personal = out.iter().filter(|v| v.personal).count();
    crate::logging::line(
        "speech",
        &format!(
            "{} system voice(s) installed, {personal} of them personal, read in {} ms",
            out.len(),
            t.elapsed().as_millis()
        ),
    );
    out
}

/// Asks macOS whether this application may use the user's Personal Voice.
///
/// Blocking, deliberately: this runs on the worker, and the caller that sent the job is not
/// waiting for it. The completion handler is turned back into a straight answer with a
/// channel, because everything above it is written as "ask, then act on the answer" and a
/// callback threaded through the queue would buy nothing.
fn request_personal_voice() -> bool {
    let (tx, rx) = channel::<bool>();
    let t = Instant::now();
    // SAFETY: the block is called once, on some queue, with the status. `tx` is moved into it
    // and dropped with it, so a handler that never runs closes the channel rather than
    // leaking — which is why the receive below cannot wait for ever.
    unsafe {
        let handler = block2::RcBlock::new(move |status: objc2_avf_audio::AVSpeechSynthesisPersonalVoiceAuthorizationStatus| {
            let _ = tx.send(
                status == objc2_avf_audio::AVSpeechSynthesisPersonalVoiceAuthorizationStatus::Authorized,
            );
        });
        AVSpeechSynthesizer::requestPersonalVoiceAuthorizationWithCompletionHandler(&handler);
    }
    // A bound rather than a wait: prism asks this at start-up and has been measured sitting
    // in it for two minutes, which is what a consent dialog with nobody at the keyboard costs.
    // Here the wait is on a worker and only after a deliberate choice, but an unbounded one
    // would still hold the queue behind it for ever.
    let granted = rx.recv_timeout(std::time::Duration::from_secs(120)).unwrap_or(false);
    crate::logging::line(
        "speech",
        &format!(
            "Personal Voice authorisation: {} after {} ms",
            if granted { "granted" } else { "not granted" },
            t.elapsed().as_millis()
        ),
    );
    granted
}
