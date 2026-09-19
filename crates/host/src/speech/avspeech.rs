//! The system voice on macOS, and the user's own Personal Voice, over AVFoundation directly.
//!
//! Its own file rather than a block inside `speech`, for the reason `voiceover.rs` gives:
//! `crates/macos-check` borrows it by path and asks the compiler whether it is true, which is
//! the only compiler this project can run at home.
//!
//! **Why hand-written rather than a crate.** This replaces `tts`, and the reason is not the
//! dependency count. It is Personal Voice: a voice the user recorded of themselves, which for
//! somebody who spends their day listening to a synthesiser is not a novelty. `tts` does not
//! expose it, and prism — the other candidate — asks for it in `initialize()`, at start-up.
//! Nobody here has measured what that costs; what is known is that prism's own vendor
//! documentation puts a 120-second bound on the call, which is a bound rather than a
//! measurement and is exactly why it must not be paid at launch. Asking only when the user
//! deliberately switches Personal Voice on is the whole difference, and it needs the API
//! directly.
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
use objc2::runtime::NSObjectProtocol;
use objc2::{sel, ClassType};
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisPersonalVoiceAuthorizationStatus, AVSpeechSynthesisVoice,
    AVSpeechSynthesisVoiceTraits, AVSpeechSynthesizer, AVSpeechUtterance,
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
    /// Re-read the installed voices and the Personal Voice status into the shared snapshot.
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
    /// Why a Personal Voice request was not granted — refused, unsupported, or no answer
    /// before the timeout — waiting for `Speech::pump`, which switches `personal_voice` off,
    /// says it, and lets the settings box untick. Written by the worker after the request; the
    /// request's own answer used to go only to the log, so a refusal left a ticked box and
    /// silence.
    personal_note: Arc<Mutex<Option<String>>>,
}

impl AvSpeech {
    pub fn new() -> Self {
        let (to_worker, rx) = channel::<Job>();
        let pending = Arc::new(AtomicUsize::new(0));
        let healthy = Arc::new(AtomicBool::new(true));
        let voices = Arc::new(Mutex::new(Vec::new()));
        let personal_note = Arc::new(Mutex::new(None));
        let (p, h, v, n) = (pending.clone(), healthy.clone(), voices.clone(), personal_note.clone());
        std::thread::Builder::new()
            .name("avspeech".into())
            .spawn(move || run(rx, p, h, v, n))
            .ok();
        // The list is wanted before anything is said — `engines()` may be the first call —
        // and reading it is what the worker can do while nothing else is happening.
        let me = Self { to_worker, pending, healthy, voices, personal_note };
        me.refresh();
        me
    }

    /// The reason the last Personal Voice request was not granted, once.
    pub fn take_personal_note(&self) -> Option<String> {
        self.personal_note.lock().ok().and_then(|mut n| n.take())
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
    personal_note: Arc<Mutex<Option<String>>>,
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
                // The authorisation status is deliberately NOT read into a snapshot here any
                // more. It was, on every refresh, and its one reader — the rising edge of the
                // switch in `Speech::pump` — could then act on an answer from launch: a
                // `denied` read before the user recorded a voice or allowed applications to
                // ask would have kept the dialog away for the rest of the session, with the
                // switch on and nothing to show for it. `personal_status` is a property read
                // that raises nothing, so whoever needs the answer asks at the moment it
                // matters.
            }
            Job::AuthorisePersonal => {
                // A grant made outside the request counts as one — in System Settings while the
                // dialog was up, or answered after the 120 s timeout — so it takes the path that
                // re-reads the voices rather than being told "no answer" and unticked.
                let granted =
                    request_personal_voice() || personal_status() == PersonalVoice::Granted;
                // The voices change with the answer: a granted Personal Voice appears in the
                // list that did not contain it a moment ago.
                if granted {
                    let list = installed_voices();
                    if let Ok(mut v) = voices.lock() {
                        *v = list;
                    }
                } else {
                    // Not granted. Handed to the pump, which unticks the switch and says this,
                    // because the user ticked a box and would otherwise hear nothing. Short, and
                    // not `explanation()`: this comes after macOS's own dialog, often refused a
                    // moment ago, and that text is written for the moment before one. A
                    // timeout or a dropped handler leaves the status at "not asked", and is said
                    // as what it is rather than left silent with the box still ticked.
                    let why = match personal_status() {
                        PersonalVoice::Unsupported => "This Mac does not support Personal Voice.",
                        PersonalVoice::Denied => "Personal Voice was not allowed.",
                        PersonalVoice::NotAsked | PersonalVoice::Granted => {
                            "macOS gave no answer about Personal Voice."
                        }
                    };
                    if let Ok(mut n) = personal_note.lock() {
                        *n = Some(why.to_string());
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
///
/// **`voiceTraits` is asked for rather than assumed**, and that is not caution for its own
/// sake. This bundle promises macOS 12.0, the tester's machine is 12.7.6, and the objc2
/// bindings carry no availability information at all — every selector in them compiles to a
/// bare `objc_msgSend`, so a method that does not exist on the running system is not a
/// compile error and not a `None`: it is an unrecognised selector, an Objective-C exception
/// crossing a Rust frame, and an application that dies at launch. For somebody who cannot
/// see the screen that is indistinguishable from the application not being installed.
///
/// Which macOS introduced `voiceTraits` was, when this was written, answered three different
/// ways by three different sources (10.15, 13.0, 14.0). Asking the runtime settles it without
/// anybody having to be right — and the answer goes in the log, so the next session turns the
/// disagreement into a measurement.
fn installed_voices() -> Vec<Voice> {
    let t = Instant::now();
    let mut out = Vec::new();
    // SAFETY: a class method with no arguments returning an array of voices; nothing here
    // outlives the call except the strings, which are copied.
    let list = unsafe { AVSpeechSynthesisVoice::speechVoices() };
    let traits_known = list.iter().next().is_some_and(|v| v.respondsToSelector(sel!(voiceTraits)));
    for v in list.iter() {
        let (id, name) = unsafe { (v.identifier(), v.name()) };
        out.push(Voice {
            id: id.to_string(),
            name: name.to_string(),
            // Without the selector there are no Personal Voices to find: the trait bit and
            // the authorisation call arrived together, so a system that cannot answer this
            // cannot have one either. `false` is the true answer there, not a guess — and it
            // is also what keeps `authorise_personal` unreachable, since the only caller
            // asks for it exactly when a chosen voice reports `personal`.
            personal: traits_known
                && unsafe { v.voiceTraits() }
                    .contains(AVSpeechSynthesisVoiceTraits::IsPersonalVoice),
        });
    }
    let personal = out.iter().filter(|v| v.personal).count();
    crate::logging::line(
        "speech",
        &format!(
            "{} system voice(s) installed, {personal} of them personal, read in {} ms{}",
            out.len(),
            t.elapsed().as_millis(),
            if traits_known {
                ""
            } else {
                " — this macOS has no voiceTraits, so Personal Voice does not exist here"
            }
        ),
    );
    out
}

/// Does this macOS have Personal Voice at all?
///
/// Asked by the settings switch, before it is worth telling the user anything. The tester's
/// report is what this exists for: he ticked the box, the log said the API is absent, and the
/// interface said nothing — "the checkbox appears to be ticked but nothing happens". A
/// setting that is on and inert looks exactly like a setting that is broken, and he could not
/// tell which he had.
///
/// The same metaclass check `personal_status` makes; see there for why it is a metaclass and
/// not a class. Cheap, local, and does not ask macOS for permission to anything.
pub fn supported() -> bool {
    AVSpeechSynthesizer::class().metaclass().responds_to(sel!(personalVoiceAuthorizationStatus))
}

/// Where this application stands with the user's Personal Voice, as macOS reports it.
///
/// Three answers that used to be two. `Denied` and `Unsupported` both arrived as the same
/// `Some(false)`, and the log then told a tester who had never seen a dialog that he had
/// refused one. His Mac gave one of these two without any dialog, and nothing here could say
/// which — so the sentence in `explanation` is written for what is actually known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonalVoice {
    /// Granted from the dialog: a Personal Voice is among what `installed_voices` returns.
    Granted,
    /// macOS said no, and does not say why. Apple's documentation describes the answer as a
    /// refusal by the user; developers report it coming with no dialog when there is nothing
    /// to grant. Which of the two the tester's Mac gave is not known — the old `Some(false)`
    /// could not tell this from `Unsupported`, and that is the question this split answers.
    Denied,
    /// This Mac cannot do it at all.
    Unsupported,
    /// Nobody has asked yet — or this macOS has no such API, which is the same answer to every
    /// caller: there is nothing to know until somebody asks, and asking is what
    /// `request_personal_voice` guards.
    NotAsked,
}

impl PersonalVoice {
    /// Why no dialog is going to come, when none is: `Denied` and `Unsupported` explained for
    /// the user, `None` for the two answers that need no explaining.
    ///
    /// One sentence in one place: the dialog the settings switch puts up in `gui.rs` and the
    /// log line beside it both say this, and a tester who reads the one after hearing the
    /// other must not find them disagreeing. SHORT, because it is read out: the first version
    /// was about 140 words of diagnosis, the tester spent 46 seconds in front of it, and his
    /// note was "could be shorter/simpler". What he needed was the option's name, which is
    /// what he then followed and what got the grant — so that stays word for word. The count
    /// of Personal Voices this application can see is left to the log line `installed_voices`
    /// writes on every read.
    ///
    /// It says nothing about whether a dialog was shown, because it is not only read before
    /// one: `Denied` also comes after the user has refused macOS's own dialog. Nor does it say
    /// what became of the box — the caller that unticks it says that.
    pub fn explanation(self) -> Option<String> {
        match self {
            PersonalVoice::Granted | PersonalVoice::NotAsked => None,
            PersonalVoice::Unsupported => Some(
                "This Mac does not support Personal Voice. It needs macOS 14 on a Mac with \
                 Apple silicon."
                    .to_string(),
            ),
            // Two causes, told apart by nothing macOS reports: a refusal of its dialog, and
            // applications not being allowed to use a Personal Voice at all. The second was
            // measured on the tester's Mac: the status read 'denied' before anything had been
            // asked, and 'not asked' once he had turned that option on — after which macOS showed
            // its dialog and granted. The option's name is quoted from
            // Apple's macOS 14 guide and was confirmed by the tester following it. "If it is
            // listed there" covers the refusal: after one, the switch is already on, and what
            // is left is this application's own entry — which nobody here has seen yet.
            PersonalVoice::Denied => Some(
                "macOS does not allow this application to use your Personal Voice yet. In \
                 System Settings > Accessibility > Personal Voice, turn on 'Allow applications \
                 to use your Personal Voice', and this application too if it is listed there. \
                 Then tick this box again."
                    .to_string(),
            ),
        }
    }
}

/// The Personal Voice authorisation as it stands, without asking anybody.
///
/// A property read that raises nothing, cheap enough to make at the moment the answer is
/// needed — see `Job::Refresh` for why it is no longer kept in a snapshot.
///
/// Guarded by `respondsToSelector` on the CLASS, for the reason `installed_voices` gives:
/// these bindings carry no availability information, and a selector that does not exist is
/// not a `NotAsked` but a dead process.
pub fn personal_status() -> PersonalVoice {
    // The METACLASS, not the class: `personalVoiceAuthorizationStatus` is a class method, and
    // `responds_to` on a class object answers about its INSTANCE methods. Asking the wrong one
    // returns false for a selector that exists, which would have quietly reported "nobody has
    // been asked" on every macOS including the ones that can answer.
    if !AVSpeechSynthesizer::class().metaclass().responds_to(sel!(personalVoiceAuthorizationStatus))
    {
        return PersonalVoice::NotAsked;
    }
    // SAFETY: a class property with no arguments, returning an enum by value.
    let status = unsafe { AVSpeechSynthesizer::personalVoiceAuthorizationStatus() };
    match status {
        AVSpeechSynthesisPersonalVoiceAuthorizationStatus::Authorized => PersonalVoice::Granted,
        AVSpeechSynthesisPersonalVoiceAuthorizationStatus::Denied => PersonalVoice::Denied,
        AVSpeechSynthesisPersonalVoiceAuthorizationStatus::Unsupported => {
            PersonalVoice::Unsupported
        }
        // NotDetermined, and anything a later macOS adds: nobody has been asked.
        _ => PersonalVoice::NotAsked,
    }
}

/// Asks macOS whether this application may use the user's Personal Voice.
///
/// Blocking, deliberately: this runs on the worker, and the caller that sent the job is not
/// waiting for it. The completion handler is turned back into a straight answer with a
/// channel, because everything above it is written as "ask, then act on the answer" and a
/// callback threaded through the queue would buy nothing.
fn request_personal_voice() -> bool {
    // The same metaclass check `personal_status` makes above, for the reason it gives there:
    // these bindings carry no availability information, and a selector that does not exist
    // is not a `NotAsked` but a dead process.
    //
    // It was missing here, and the path that reaches this function is exactly the one the
    // guard exists for. `personal_status` answers `NotAsked` on a macOS without the API;
    // `NotAsked` is read by `Speech::pump` as "nobody has been asked yet"; and that is the
    // arm that sends `Job::AuthorisePersonal`. So on any macOS before 14 — the tester's is
    // 12.7.6 — ticking the Personal Voice switch would have sent an unrecognised selector to
    // the class and taken the application down, at the moment somebody deliberately asked
    // for something. Found by review before it reached him.
    if !AVSpeechSynthesizer::class()
        .metaclass()
        .responds_to(sel!(requestPersonalVoiceAuthorizationWithCompletionHandler:))
    {
        crate::logging::line(
            "speech",
            "Personal Voice: this macOS has no such API — it arrived in macOS 14 — so there is \
             nothing to ask for.",
        );
        return false;
    }
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
    // A bound rather than a wait, and 120 s because that is the bound prism's own vendor
    // documentation gives this call — not something measured here. What it protects against
    // is a consent dialog nobody answers: the worker also says every line, so an unbounded
    // wait would hold the overlay's voice behind a window the user may not have noticed.
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
