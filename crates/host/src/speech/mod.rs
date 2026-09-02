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
mod prism;

#[cfg(any(windows, target_os = "macos"))]
use std::cell::Cell;
use std::cell::RefCell;

use anyhow::{Context, Result};
use tts::Tts;

pub struct Speech {
    tts: RefCell<Tts>,
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
}

impl Speech {
    pub fn new() -> Result<Self> {
        let tts = Tts::default().context("failed to initialize TTS engine")?;
        Ok(Self {
            tts: RefCell::new(tts),
            #[cfg(windows)]
            prism: RefCell::new(prism::Prism::new()),
            #[cfg(target_os = "macos")]
            vo: voiceover::VoiceOver::new(),
            #[cfg(target_os = "macos")]
            last_switch: Cell::new(crate::appcfg::voiceover_speech()),
            #[cfg(windows)]
            last_switch: Cell::new(crate::appcfg::screen_reader_speech()),
        })
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
        }
        let _ = self.tts.borrow_mut().speak(text.to_string(), interrupt);
    }

    /// Whether anything is still waiting to be said. On macOS this is only ever a lower
    /// bound: once VoiceOver has been handed a line, its own queue is not ours to inspect.
    pub fn is_speaking(&self) -> bool {
        #[cfg(target_os = "macos")]
        if self.vo.pending() {
            return true;
        }
        #[cfg(windows)]
        if self.prism.borrow().pending() {
            return true;
        }
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
                let _ = self.tts.borrow_mut().speak(text, false);
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
