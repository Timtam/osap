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
mod voiceover;
#[cfg(windows)]
mod fallback;
#[cfg(windows)]
mod prism;

#[cfg(any(windows, target_os = "macos"))]
use std::cell::Cell;
#[cfg(windows)]
use std::collections::HashMap;
#[cfg(any(windows, target_os = "macos"))]
use std::cell::RefCell;

use anyhow::Result;
#[cfg(target_os = "macos")]
use anyhow::Context;
#[cfg(target_os = "macos")]
use tts::Tts;

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
    /// The plain voice on macOS, where `tts` reaches AVFoundation. Windows has its own, in
    /// `fallback`, because `tts` cost 2872 ms of blocking start-up there for a voice that
    /// usually never speaks.
    #[cfg(target_os = "macos")]
    tts: RefCell<Tts>,
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
    /// Which engine each module VM has chosen for itself, by engine id.
    ///
    /// Keyed by the VM, because that is the unit the API can honestly promise. `host.speech`
    /// falls through to the VM owner rather than to the module that defined the code, so a
    /// choice made inside a shared framework belongs to whichever module inherited it — which
    /// is the module somebody installed and enabled, and therefore the right owner of the
    /// decision.
    #[cfg(windows)]
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
            tts: RefCell::new(Tts::default().context("failed to initialize TTS engine")?),
            #[cfg(windows)]
            fallback: fallback::Fallback::new(),
            #[cfg(windows)]
            prism: RefCell::new(prism::Prism::new()),
            #[cfg(target_os = "macos")]
            vo: voiceover::VoiceOver::new(),
            #[cfg(target_os = "macos")]
            last_switch: Cell::new(crate::appcfg::voiceover_speech()),
            #[cfg(windows)]
            last_switch: Cell::new(crate::appcfg::screen_reader_speech()),
            #[cfg(windows)]
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
    #[cfg(windows)]
    pub fn say_for(&self, vm: usize, text: &str, interrupt: bool) {
        let chosen = self.chosen.borrow().get(&vm).cloned();
        if let Some(id) = chosen {
            let spoke = self.voices.borrow().get(&id).is_some_and(|v| v.say(text, interrupt));
            if spoke {
                return;
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
    #[cfg(windows)]
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
        self.voices
            .borrow_mut()
            .entry(engine.id.clone())
            .or_insert_with(|| fallback::Fallback::for_engine(&engine.name));
        self.chosen.borrow_mut().insert(vm, engine.id);
        true
    }

    /// The engine this module VM chose, if it chose one.
    #[cfg(windows)]
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
        let _ = self.tts.borrow_mut().speak(text.to_string(), interrupt);
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
        #[cfg(not(windows))]
        {
            // macOS fits in here: VoiceOver (available while it is running), the system
            // voice, and — once the objc2 wrapper exists — Personal Voice. Nothing about the
            // shape above needs to change for that.
            Vec::new()
        }
    }

    /// Whether what we say is reaching a screen reader rather than a plain voice.
    ///
    /// Asked in one place only: when the application has something to say on its own behalf
    /// and there was no notification to show it in. Those words are addressed to somebody who
    /// cannot see the screen, so with nobody listening they are not said at all. A module
    /// that calls `host.speech` is never asked — whoever installed and enabled it decided
    /// that already.
    pub fn via_screen_reader(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            crate::appcfg::voiceover_speech() && voiceover::is_running()
        }
        #[cfg(windows)]
        {
            crate::appcfg::screen_reader_speech() && self.prism.borrow().via_screen_reader()
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
        #[cfg(not(windows))]
        self.tts.borrow().is_speaking().unwrap_or(false)
    }

    /// Speaks anything VoiceOver turned down. Called from the event loop.
    ///
    /// A line handed to a helper that then fails would otherwise simply never be said, and a
    /// screen reader that goes silent without explanation is worse than one that says the
    /// wrong thing. So the refusal comes back here and the fallback says it instead.
    pub fn pump(&self) {
        #[cfg(target_os = "macos")]
        for text in self.vo.refused() {
            let _ = self.tts.borrow_mut().speak(text, false);
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
    use std::time::{Duration, Instant};

    #[test]
    fn what_can_speak_here() {
        let speech = super::Speech::new().expect("speech");
        // The first look is started when `Speech` is built; give it a moment to land, the
        // way a module asking during a session would find it long since landed.
        for _ in 0..40 {
            if !speech.engines().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let start = Instant::now();
        let engines = speech.engines();
        let took = start.elapsed();
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
            "reading the snapshot took {took:?}, so it is not a snapshot any more"
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
        let speech = super::Speech::new().expect("speech");
        for _ in 0..40 {
            if !speech.engines().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let engines = speech.engines();
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
