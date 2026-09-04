//! Where what the overlay says comes out.
//!
//! One place, because the choice of voice is not the caller's business: a control announces
//! itself, and something else decides whether that reaches a screen reader's own queue or a
//! second synthesiser talking over it.
//!
//! Both platforms have the same shape: a preferred path that reaches the user's own screen
//! reader — prism on Windows, VoiceOver on macOS — and behind it a plain synthesiser for
//! when the preferred one is not there or has stopped answering. The preferred path is
//! preferred because it speaks in the user's voice, at their rate, into their reading order,
//! and on their braille display, which nothing else can do.
//!
//! Most of the design here is about the second half: a preferred path that can fail has to
//! fail **loudly into the fallback** rather than quietly into silence. So a line the screen
//! reader turns down comes back through [`Speech::pump`] and is said the other way, once,
//! rather than being lost — because a screen reader that goes quiet without explanation is
//! worse than one that says the wrong thing.

#[cfg(target_os = "macos")]
mod avspeech;
#[cfg(target_os = "macos")]
mod voiceover;
#[cfg(windows)]
mod fallback;
#[cfg(windows)]
mod prism;

#[cfg(any(windows, target_os = "macos"))]
use std::cell::Cell;
#[cfg(any(windows, target_os = "macos"))]
use std::collections::HashMap;
#[cfg(any(windows, target_os = "macos"))]
use std::cell::RefCell;

use anyhow::Result;

/// What `use("voiceover")` names, and the one id on macOS that is a TRANSPORT rather than a
/// voice: it hands the line to the user's screen reader instead of choosing how to say it.
#[cfg(target_os = "macos")]
const VOICEOVER_ID: &str = "voiceover";

/// One thing that can speak, described without naming any platform's machinery.
///
/// The shape is what macOS has to fit into later, so nothing here is prism's: an `id` a
/// module can compare and store, a `name` meant to be SAID rather than parsed, whether it is
/// the user's own screen reader or a plain voice, and whether it could speak right now.
///
/// `id` is the only field with a stability promise. `name` is whatever the thing calls
/// itself and may change with the version of somebody's screen reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engine {
    pub id: String,
    pub name: String,
    /// The user's own reader — their voice, their rate, their reading order, and their
    /// braille display. A plain voice is none of those things.
    pub screen_reader: bool,
    /// Whether it could speak right now. Not "is it installed": a screen reader that is not
    /// running cannot be spoken through, and saying otherwise would invite a module to pick
    /// something that will never answer.
    pub available: bool,
}

pub struct Speech {
    /// The plain voice on macOS: AVFoundation directly, on a thread, built on first use.
    ///
    /// Replaced `tts`, and not to save a dependency — `tts` cannot reach Personal Voice, and
    /// a voice the user recorded of themselves is not a novelty to somebody who listens to a
    /// synthesiser all day. See `avspeech.rs`.
    #[cfg(target_os = "macos")]
    av: avspeech::AvSpeech,
    /// The plain voice on Windows: a prism speech engine opened in the background on a
    /// thread that cannot be wedged by the screen-reader path, since the moment it is needed
    /// is usually the moment that path has stopped answering.
    #[cfg(windows)]
    fallback: fallback::Fallback,
    /// The Windows screen reader. `RefCell` because re-arming after a failure replaces the
    /// whole worker: the backend it spoke through is dead, and prism's binding to it is
    /// fixed for the lifetime of the instance and never retried.
    #[cfg(windows)]
    prism: RefCell<prism::Prism>,
    #[cfg(target_os = "macos")]
    vo: voiceover::VoiceOver,
    /// The switch as it was last seen, so ticking it again re-arms a path that had failed.
    #[cfg(any(windows, target_os = "macos"))]
    last_switch: Cell<bool>,
    /// The Personal Voice switch as it was last seen.
    ///
    /// Watched rather than wired to the checkbox, because the checkbox cannot reach here:
    /// `run_gui` is handed closures, not the host. A rising edge is the deliberate act this
    /// permission needs, and `pump` sees it on the next tick.
    #[cfg(target_os = "macos")]
    last_personal: Cell<bool>,
    /// Which engine each module VM has chosen for itself, by engine id.
    ///
    /// Keyed by the VM, because that is the unit the API can honestly promise. `host.speech`
    /// falls through to the VM owner rather than to the module that defined the code, so a
    /// choice made inside a shared framework belongs to whichever module inherited it — which
    /// is the module somebody installed and enabled, and therefore the right owner of the
    /// decision.
    #[cfg(any(windows, target_os = "macos"))]
    chosen: RefCell<HashMap<usize, String>>,
    /// One long-lived worker per chosen engine, shared by every VM that chose it.
    ///
    /// Never torn down. Opening a library is the expensive and fragile part — 2.0 s for SAPI,
    /// 3.5 s for OneCore, and repeatedly opening and closing one has crashed and hung here —
    /// so once a voice exists it stays.
    #[cfg(windows)]
    voices: RefCell<HashMap<String, fallback::Fallback>>,
}

impl Speech {
    pub fn new() -> Result<Self> {
        Ok(Self {
            #[cfg(target_os = "macos")]
            av: avspeech::AvSpeech::new(),
            #[cfg(windows)]
            fallback: fallback::Fallback::new(),
            #[cfg(windows)]
            prism: RefCell::new(prism::Prism::new()),
            #[cfg(target_os = "macos")]
            vo: voiceover::VoiceOver::new(),
            #[cfg(target_os = "macos")]
            last_switch: Cell::new(crate::appcfg::voiceover_speech()),
            // Seeded with the STORED value, so a switch that was already on when the
            // application started does not count as a rising edge and put a dialog up at
            // launch — which is the one thing this whole arrangement exists to avoid.
            #[cfg(target_os = "macos")]
            last_personal: Cell::new(crate::appcfg::personal_voice()),
            #[cfg(windows)]
            last_switch: Cell::new(crate::appcfg::screen_reader_speech()),
            #[cfg(any(windows, target_os = "macos"))]
            chosen: RefCell::new(HashMap::new()),
            #[cfg(windows)]
            voices: RefCell::new(HashMap::new()),
        })
    }

    /// Says `text` for a particular module VM, honouring whatever engine it chose.
    ///
    /// A choice that cannot speak — its engine gone, its worker never opened — falls through
    /// to the ordinary path rather than to silence. The module hears its own voice when it
    /// can and the user's when it cannot, which is the right way round.
    #[cfg(any(windows, target_os = "macos"))]
    pub fn say_for(&self, vm: usize, text: &str, interrupt: bool) {
        let chosen = self.chosen.borrow().get(&vm).cloned();
        if let Some(id) = chosen {
            #[cfg(windows)]
            {
                let spoke =
                    self.voices.borrow().get(&id).is_some_and(|v| v.say(text, interrupt));
                if spoke {
                    return;
                }
            }
            // macOS needs no worker per voice, because the voice is a property of the
            // UTTERANCE rather than of the synthesiser: one thread says everything, in
            // whichever voice each line asks for. VoiceOver is the exception — it is a
            // different transport, not a different voice — so it goes the ordinary way.
            #[cfg(target_os = "macos")]
            {
                if id == VOICEOVER_ID {
                    if voiceover::is_running() && self.vo.say(text, interrupt) {
                        return;
                    }
                } else if self.av.say(text, interrupt, Some(&id)) {
                    return;
                }
            }
        }
        self.say(text, interrupt);
    }

    /// Chooses the engine this module VM speaks through, or returns to the ordinary path.
    ///
    /// Returns false when the engine is not there — unknown to this build, or its screen
    /// reader not running. Refusing is the honest answer: accepting and then quietly speaking
    /// somewhere else would be a control claiming what it cannot honour.
    ///
    /// The first line after a choice may wait for the engine to open, which was measured at
    /// 2.0 s for SAPI and 3.5 s for OneCore. That happens on the new voice's own thread, so
    /// nothing else waits with it, and the lines queue rather than being lost.
    #[cfg(any(windows, target_os = "macos"))]
    pub fn use_engine(&self, vm: usize, id: Option<&str>) -> bool {
        let Some(id) = id else {
            self.chosen.borrow_mut().remove(&vm);
            return true;
        };
        let Some(engine) = self.engines().into_iter().find(|e| e.id == id) else {
            return false;
        };
        if !engine.available {
            return false;
        }
        #[cfg(windows)]
        self.voices
            .borrow_mut()
            .entry(engine.id.clone())
            .or_insert_with(|| fallback::Fallback::for_engine(&engine.name));
        // Nothing to ask for here, and an earlier draft asked in exactly the wrong place: it
        // requested authorisation when the chosen voice was already marked personal — but
        // macOS does not put a Personal Voice into `speechVoices()` UNTIL it is authorised,
        // so the condition could never be true and the feature was unreachable. The request
        // is a deliberate act by the user now, on the Personal Voice switch, and by the time
        // one appears in this list it is already usable.
        self.chosen.borrow_mut().insert(vm, engine.id);
        true
    }

    /// The engine this module VM chose, if it chose one.
    #[cfg(any(windows, target_os = "macos"))]
    pub fn chosen_engine(&self, vm: usize) -> Option<String> {
        self.chosen.borrow().get(&vm).cloned()
    }

    /// Says `text`. `interrupt` drops whatever has not been said yet.
    pub fn say(&self, text: &str, interrupt: bool) {
        #[cfg(target_os = "macos")]
        {
            let on = crate::appcfg::voiceover_speech();
            // Ticking the setting again asks for another try: a failure earlier in the
            // session parks the path, and this is the only way back without a restart.
            if on && !self.last_switch.replace(on) {
                self.vo.rearm();
            } else {
                self.last_switch.set(on);
            }
            // Asked here rather than on the worker, and asked EVERY time. `tell application
            // "VoiceOver"` starts VoiceOver if it is not running, and this setting is on by
            // default — so without this the first thing the overlay ever said would switch a
            // screen reader on for somebody who had not asked for one. Not a failure either:
            // VoiceOver can be started later in the session and this simply starts working.
            if on && voiceover::is_running() && self.vo.say(text, interrupt) {
                return;
            }
        }
        #[cfg(windows)]
        {
            let on = crate::appcfg::screen_reader_speech();
            // Ticking the setting again asks for another try. Unlike VoiceOver's re-arm this
            // cannot simply raise a flag: the backend is dead and prism's binding to it is
            // fixed for the instance's lifetime, so the whole worker is replaced.
            if on && !self.last_switch.replace(on) {
                self.prism.borrow_mut().rearm();
            } else {
                self.last_switch.set(on);
            }
            // No "is it running" question first, unlike VoiceOver: that one exists only
            // because `tell application` STARTS VoiceOver. prism's NVDA backend probes an RPC
            // endpoint and starts nothing, so a backend that will not initialise IS the
            // answer, and asking twice would only cost time.
            if on && self.prism.borrow().say(text, interrupt) {
                return;
            }
            if !self.fallback.say(text, interrupt) {
                // Nowhere left to fall. Written down rather than dropped, because an
                // application that has gone completely mute should at least be able to say
                // why afterwards.
                crate::logging::line("speech", &format!("nothing could say: {text}"));
            }
            return;
        }
        #[cfg(target_os = "macos")]
        if !self.av.say(text, interrupt, None) {
            // Nowhere left to fall, the same as the Windows arm above. Written down rather
            // than dropped: an application that has gone completely mute should at least be
            // able to say why afterwards.
            crate::logging::line("speech", &format!("nothing could say: {text}"));
        }
    }

    /// Everything that could speak on this machine, whether or not it can right now.
    ///
    /// Answered without opening anything: opening a speech engine costs seconds, and this is
    /// a question a module may ask. See `prism_sys::Context::is_available` for how, and for
    /// the one backend that must never be asked twice.
    ///
    /// The list is the same for every caller and carries no notion of "mine" — a module's own
    /// choice is asked for separately, so this record can be logged, cached or handed on
    /// without carrying somebody else's preference with it.
    pub fn engines(&self) -> Vec<Engine> {
        #[cfg(windows)]
        {
            self.fallback.engines()
        }
        #[cfg(target_os = "macos")]
        {
            // VoiceOver first, because it is the one that is not a voice but a reader: the
            // user's own rate, reading order and braille display. Then every installed
            // system voice, Personal ones included — those are listed before they are
            // authorised, because choosing one is what asks, and a list that hid them would
            // make an unavailable feature invisible rather than merely unavailable.
            let mut out = vec![Engine {
                id: VOICEOVER_ID.to_string(),
                name: "VoiceOver".to_string(),
                screen_reader: true,
                available: voiceover::is_running(),
            }];
            for v in self.av.voices() {
                // Every voice in this list is usable, Personal ones included: macOS only
                // shows a Personal Voice at all once it has been authorised, so its presence
                // IS its availability. Listing one as unavailable would describe a state that
                // cannot occur.
                out.push(Engine {
                    id: v.id,
                    name: v.name,
                    screen_reader: false,
                    available: true,
                });
            }
            out
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Vec::new()
        }
    }

    /// Whether a screen reader is RUNNING — not whether we speak through it.
    ///
    /// Asked in one place only: when the application has something to say on its own behalf
    /// and there was no notification to show it in. Those words are addressed to somebody
    /// who cannot see the screen, so with nobody listening they are not said at all. A
    /// module that calls `host.speech` is never asked — whoever installed and enabled it
    /// decided that already.
    ///
    /// **Presence and transport are two questions**, and this used to ask both at once as
    /// `via_screen_reader`, which cost the macOS tester his startup announcement. "Speak
    /// through VoiceOver" chooses a transport; whether anybody is listening is a separate
    /// fact. With that switch off and VoiceOver running the honest answer is "yes, somebody
    /// is there" — and the line then goes out through the system voice, which is what a
    /// plain-voice user asked for by leaving the switch alone. On macOS there is no balloon
    /// and no Dock icon either, so that announcement was the only signal the application had
    /// started, and it was silent on every Mac by default.
    ///
    /// The rule this protects is unchanged: with no reader running, the application stays
    /// quiet. Somebody who never asked for speech is not spoken at.
    pub fn a_reader_is_present(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            voiceover::is_running()
        }
        #[cfg(windows)]
        {
            self.prism.borrow().via_screen_reader()
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            false
        }
    }

    /// Whether anything is still waiting to be said. On macOS this is only ever a lower
    /// bound: once VoiceOver has been handed a line, its own queue is not ours to inspect.
    pub fn is_speaking(&self) -> bool {
        #[cfg(target_os = "macos")]
        if self.vo.pending() {
            return true;
        }
        #[cfg(windows)]
        {
            return self.prism.borrow().pending() || self.fallback.pending();
        }
        #[cfg(target_os = "macos")]
        return self.av.pending();
        #[cfg(not(any(windows, target_os = "macos")))]
        false
    }

    /// Speaks anything VoiceOver turned down. Called from the event loop.
    ///
    /// A line handed to a helper that then fails would otherwise simply never be said, and a
    /// screen reader that goes silent without explanation is worse than one that says the
    /// wrong thing. So the refusal comes back here and the fallback says it instead.
    pub fn pump(&self) {
        // The rising edge of the Personal Voice switch is the request. Falling is not the
        // reverse: macOS keeps the grant, and nothing here should try to hand it back.
        #[cfg(target_os = "macos")]
        {
            let on = crate::appcfg::personal_voice();
            if on && !self.last_personal.replace(on) {
                // What the status says decides whether there is anything to ask. Asking again
                // when it is already granted would put a dialog up for nothing; asking again
                // after a refusal does not bring the dialog back, so the log names the pane
                // instead. Same shape as the VoiceOver Automation permission next door.
                match self.av.personal_granted() {
                    Some(true) => crate::logging::line(
                        "speech",
                        "Personal Voice: already granted — it is among the voices a module                          can choose",
                    ),
                    Some(false) => crate::logging::line(
                        "speech",
                        "Personal Voice: refused earlier, and macOS does not ask twice.                          System Settings > Privacy & Security > Speech Recognition is not                          it — a Personal Voice is allowed per application from the dialog                          alone, so the switch has to be turned off and on with the grant                          reset, or the voice re-shared in Accessibility settings.",
                    ),
                    None => {
                        crate::logging::line(
                            "speech",
                            "Personal Voice was switched on — asking macOS, on the speech                              worker so the dialog cannot hold the event loop",
                        );
                        self.av.authorise_personal();
                    }
                }
            } else {
                self.last_personal.set(on);
            }
        }
        #[cfg(target_os = "macos")]
        for text in self.vo.refused() {
            // Not `interrupt`: these are lines VoiceOver turned down, said late and out of
            // order already. Cutting off whatever the system voice is in the middle of would
            // lose a second line to recover a first.
            self.av.say(&text, false, None);
        }
        // Also where the Windows stall deadline is enforced, because this runs on the event
        // loop while the speech thread may be stuck inside a call that cannot be cancelled —
        // and where the screen reader is looked for again after it has gone.
        #[cfg(windows)]
        {
            // Two statements rather than one loop: `refused` borrows, `retry_if_due` borrows
            // mutably, and a `for` loop holds its borrow for the whole body.
            let refused = self.prism.borrow().refused();
            for text in refused {
                self.fallback.say(&text, false);
            }
            // Only while the setting is on: switched off, the screen-reader path is not
            // wanted, and starting a worker every few seconds to look for one would be work
            // done against the user's explicit answer.
            if crate::appcfg::screen_reader_speech() {
                self.prism.borrow_mut().retry_if_due();
            }
        }
    }
}

#[cfg(all(test, windows))]
mod engine_list {
    //! Honest, cheap, and repeatable — and the process has to be able to EXIT afterwards,
    //! which it could not while the query opened its library on the caller's thread.
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// The two tests below must not run beside each other.
    ///
    /// What they exercise is process-global — one speech library, one worker thread per
    /// `Speech`, and the machine's actual synthesisers. And the waiting is self-inflicted:
    /// **every `engines()` call queues a refresh on that worker**, so a poll loop asking for
    /// a usable voice is also generating the work it is waiting behind. One test doing that
    /// is fine (a look is about 35 ms, the loop asks every 200). Two of them in parallel,
    /// with the other test opening SAPI at the same time — a look while an engine is opening
    /// was measured at 2.7 s — starve each other, and the wait never completes. Measured:
    /// alone, both pass in 4.7 s; together in the full suite, one spent its entire ten-second
    /// budget and still found no usable voice.
    ///
    /// Serialised here rather than with `--test-threads=1`, which would slow every other test
    /// in the crate for the sake of these two. The lock is deliberately taken for the whole
    /// body, and poisoning is ignored: a panic in one test must fail that test, not turn the
    /// other into a second, confusing failure.
    static SPEECH_TESTS: Mutex<()> = Mutex::new(());

    /// Waits for a voice that can actually be CHOSEN, not merely for a list to exist.
    ///
    /// Both tests here used to wait for `!engines().is_empty()`, and that is the wrong
    /// condition. The nine Windows entries appear together, but the two that are usable —
    /// `sapi` and `onecore` — are marked available only once the library has answered for
    /// them. So the list goes non-empty while nothing in it can speak, and a test that
    /// proceeds at that moment fails on a machine that is merely slow. That is what
    /// `no plain voice available` was: not a broken machine, a race.
    ///
    /// The size of the window came out of writing `tools/speech-probe`, which asks the same
    /// question from Luau: two consecutive runs on ONE Windows machine took **500 ms and
    /// 5500 ms** before a plain voice could be chosen, against 282 ms on a macOS CI runner.
    /// Ten seconds is generous against the larger of those and still bounded.
    fn wait_for_a_usable_voice(speech: &super::Speech) -> Vec<super::Engine> {
        // Every 200 ms rather than every 100: the call itself queues the refresh it is
        // waiting for, so asking twice as often does not make the answer arrive sooner — it
        // makes the worker do twice the work between answers.
        for _ in 0..50 {
            let engines = speech.engines();
            if engines.iter().any(|e| e.available && !e.screen_reader) {
                return engines;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        speech.engines()
    }

    /// The cheapest of several reads, rather than one.
    ///
    /// What is being asserted is that reading the snapshot does no WORK — that the answer is
    /// not being computed on the caller's thread, which would cost the 35 ms a real look
    /// takes. A single wall-clock sample cannot tell that apart from the thread simply being
    /// descheduled: `cargo test` runs its tests in parallel, on a shared CI runner with a
    /// couple of cores, and one sample there measured **6.4 ms** for a clone of a
    /// nine-element vector and turned main red for no defect at all.
    ///
    /// Scheduling can only ever inflate a sample, never deflate it, so the minimum is the
    /// honest estimator here — and the property still holds: if the work moved back onto
    /// this thread, EVERY sample would be tens of milliseconds and the minimum with them.
    fn cheapest_read(speech: &super::Speech) -> Duration {
        let mut best = Duration::from_secs(1);
        for _ in 0..5 {
            let start = Instant::now();
            let _ = speech.engines();
            best = best.min(start.elapsed());
        }
        best
    }

    #[test]
    fn what_can_speak_here() {
        let _serial = SPEECH_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let speech = super::Speech::new().expect("speech");
        let engines = wait_for_a_usable_voice(&speech);
        let took = cheapest_read(&speech);
        for e in &engines {
            println!(
                "  {:<14} {:<14} {}  {}",
                e.id,
                e.name,
                if e.screen_reader { "reader" } else { "voice " },
                if e.available { "available" } else { "-" }
            );
        }
        println!("  asked in {took:.1?}");

        assert!(!engines.is_empty(), "nothing at all can speak, which cannot be right");
        assert!(
            engines.iter().any(|e| !e.screen_reader && e.available),
            "no plain voice is available, and Windows ships two"
        );
        // The snapshot is a clone of a small vector. If this ever takes milliseconds, the
        // answer has started being computed on the caller's thread again — which is the
        // mistake this shape exists to prevent.
        assert!(
            took < Duration::from_millis(5),
            "the cheapest of five reads took {took:?}, so it is not a snapshot any more"
        );

        // Ids are the only field a module compares, so they must be unique and unsurprising.
        let mut ids: Vec<&str> = engines.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "two engines share an id");
        assert!(
            engines
                .iter()
                .all(|e| e.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())),
            "an id contains something a module would have to quote"
        );

        assert_eq!(speech.engines(), engines, "the answer changed between two askings");
    }

    /// Choosing is honest about what it can honour, and never ends in silence.
    ///
    /// Nothing here makes a sound: every line is empty, and `prism-sys` answers an empty
    /// line by doing nothing rather than by passing it on. This runs on somebody's working
    /// machine.
    #[test]
    fn a_module_can_choose_what_speaks_for_it() {
        let _serial = SPEECH_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let speech = super::Speech::new().expect("speech");
        let engines = wait_for_a_usable_voice(&speech);
        const ME: usize = 7;
        const SOMEBODY_ELSE: usize = 8;

        assert_eq!(speech.chosen_engine(ME), None, "a module starts with no choice");

        // Something this machine does not have is refused rather than accepted and ignored.
        let absent = engines.iter().find(|e| !e.available).map(|e| e.id.clone());
        if let Some(absent) = absent {
            assert!(!speech.use_engine(ME, Some(&absent)), "accepted {absent}, which is not here");
            assert_eq!(speech.chosen_engine(ME), None, "a refused choice was remembered anyway");
        }
        assert!(!speech.use_engine(ME, Some("no-such-engine")), "accepted a name that does not exist");

        // Something it does have is taken, and belongs to this VM alone.
        let present = engines
            .iter()
            .find(|e| e.available && !e.screen_reader)
            .expect("no plain voice available")
            .id
            .clone();
        assert!(speech.use_engine(ME, Some(&present)), "refused {present}, which is here");
        assert_eq!(speech.chosen_engine(ME), Some(present.clone()));
        assert_eq!(speech.chosen_engine(SOMEBODY_ELSE), None, "one module's choice reached another");

        // Saying something through it must not hang, crash, or take a visible moment: the
        // opening happens on the voice's own thread and the line queues behind it.
        let start = Instant::now();
        speech.say_for(ME, "", true);
        let took = start.elapsed();
        assert!(took < Duration::from_millis(50), "handing a line over took {took:?}");

        assert!(speech.use_engine(ME, None), "could not go back to the ordinary path");
        assert_eq!(speech.chosen_engine(ME), None);
    }
}
