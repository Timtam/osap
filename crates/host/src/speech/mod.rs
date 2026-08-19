//! Where what the overlay says comes out.
//!
//! One place, because the choice of voice is not the caller's business: a control announces
//! itself, and something else decides whether that reaches a screen reader's own queue or a
//! second synthesiser talking over it.
//!
//! On Windows the `tts` crate already routes through the running screen reader (Tolk), so
//! there is nothing to choose. On macOS there is: VoiceOver will say what we hand it, in the
//! user's voice, at their rate, **and on their braille display** — which no separate
//! synthesiser can do — but only through AppleScript, and only if the user has allowed it.
//! So the macOS path is a preference that can fail, and most of the design here is about
//! failing loudly into the fallback rather than quietly into silence.

#[cfg(target_os = "macos")]
mod voiceover;

#[cfg(target_os = "macos")]
use std::cell::Cell;
use std::cell::RefCell;

use anyhow::{Context, Result};
use tts::Tts;

pub struct Speech {
    tts: RefCell<Tts>,
    #[cfg(target_os = "macos")]
    vo: voiceover::VoiceOver,
    /// The switch as it was last seen, so ticking it again re-arms a path that had failed.
    #[cfg(target_os = "macos")]
    last_switch: Cell<bool>,
}

impl Speech {
    pub fn new() -> Result<Self> {
        let tts = Tts::default().context("failed to initialize TTS engine")?;
        Ok(Self {
            tts: RefCell::new(tts),
            #[cfg(target_os = "macos")]
            vo: voiceover::VoiceOver::new(),
            #[cfg(target_os = "macos")]
            last_switch: Cell::new(crate::appcfg::voiceover_speech()),
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
            if on && self.vo.say(text, interrupt) {
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
    }
}
