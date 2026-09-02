//! The application's own settings — the ones that are about the platform rather than about
//! any one module.
//!
//! These were environment variables, and environment variables are the wrong shape for them.
//! A variable has to be decided before the process starts, cannot be changed while it runs,
//! is invisible to anyone who did not set it, and — for the person this platform is built
//! for — has to be typed into a terminal to be used at all. A tester who needs trace logging
//! for one keystroke should be able to switch it on, do the thing, and switch it off.
//!
//! So the store is the source of truth and this module is its live face: a handful of atomics
//! that any part of the process can read without borrowing anything, written once at startup
//! from `settings.toml` and again whenever the Application settings tab changes one.
//!
//! **The variables still work**, and deliberately: a CI job runs headless, `bootstrap-macos.sh`
//! tells a tester to launch with tracing on, and neither has a window to click in. A variable
//! that is set FORCES the setting on for that run, and the tab says so rather than
//! pretending it can turn it off. New settings, though, go here and not there.

use std::sync::atomic::{AtomicBool, Ordering};

/// One switch: how it is stored, how it is named to a person, and when it starts to matter.
pub struct Switch {
    /// Key in the `[app]` table of `settings.toml`, and the suffix of its variable.
    pub key: &'static str,
    /// What the tab calls it. Carries the "when does this take effect" note, because a
    /// setting that appears to do nothing is worse than one that says it needs a restart.
    pub label: &'static str,
    /// What it is for, in one sentence.
    pub help: &'static str,
    /// The platform this setting exists on, or `None` for all of them.
    ///
    /// A switch that can do nothing here must not be offered here: a checkbox that a person
    /// can tick and that then changes nothing is the same broken promise as a control that
    /// announces an action it cannot perform, and this project has already paid for that once.
    pub os: Option<&'static str>,
    /// What it is before anyone has ever touched it.
    ///
    /// Held here as well as in the static, so `load` can tell "stored off" from "never
    /// stored" — the difference between a person having turned something off and a fresh
    /// installation. Held here rather than only in the `static` because a default hidden in
    /// a static is a default nobody reading this list would find — and one of them is on.
    pub default_on: bool,
    state: &'static AtomicBool,
}

static TRACE: AtomicBool = AtomicBool::new(false);
static CALIBRATE: AtomicBool = AtomicBool::new(false);
static OCR_DEBUG: AtomicBool = AtomicBool::new(false);
static IGNORE_SUPPORTED_OS: AtomicBool = AtomicBool::new(false);
static HEADLESS: AtomicBool = AtomicBool::new(false);
static SPEAK_VIA_VOICEOVER: AtomicBool = AtomicBool::new(false);
static SCREEN_READER_SPEECH: AtomicBool = AtomicBool::new(true);
static BRAILLE: AtomicBool = AtomicBool::new(true);
static DOCK_WHILE_OPEN: AtomicBool = AtomicBool::new(true);

/// Every application setting, in the order the tab shows them: the ones that take effect
/// immediately first, so the two that need a restart are not the first thing read out.
pub const SWITCHES: &[Switch] = &[
    Switch {
        key: "trace",
        label: "Detailed (trace) logging — takes effect immediately",
        help: "Records every OS call that failed and every decision the key handling made. \
               Makes the log large, so it is meant for chasing one problem rather than for \
               leaving on.",
        os: None,
        default_on: false,
        state: &TRACE,
    },
    Switch {
        key: "ocr_debug",
        label: "Save the images OCR was given — takes effect immediately",
        help: "Writes what the recogniser actually saw next to the application, which is the \
               only way to tell 'the text was unreadable' from 'the region was wrong'.",
        os: None,
        default_on: false,
        state: &OCR_DEBUG,
    },
    Switch {
        key: "calibrate",
        label: "Calibration keys in overlays — reload modules to apply",
        help: "Arms the measuring keys inside whichever overlay is active: a screenshot with \
               a crosshair on every control, cropping a template around the focused one, and \
               counting that template's matches. For authoring an overlay, not for using one.",
        os: None,
        default_on: false,
        state: &CALIBRATE,
    },
    Switch {
        key: "ignore_supported_os",
        label: "Load modules not meant for this system — restart to apply",
        help: "A module may declare which systems it is written for, and one that excludes \
               this system is normally skipped. Turn this on when the module works here and \
               its manifest is simply behind the code.",
        os: None,
        default_on: false,
        state: &IGNORE_SUPPORTED_OS,
    },
    Switch {
        key: "headless",
        label: "Run without a window — restart to apply",
        help: "No tray icon and no manager window; modules with hotkeys, captured keys or \
               window triggers still run. For testing and automation. Turning this on means \
               this tab will not be reachable next time.",
        os: None,
        default_on: false,
        state: &HEADLESS,
    },
    Switch {
        key: "voiceover_speech",
        label: "Speak through VoiceOver, so it comes out the way VoiceOver says everything \
                else — takes effect immediately, and asks macOS for permission the first time",
        help: "Hands what the overlay says to VoiceOver instead of speaking it with a \
               separate voice, so it arrives in your voice, at your rate, and through \
               whatever VoiceOver already outputs to — including a braille display, which \
               nothing else here can reach. Two synthesisers talk over each other; \
               VoiceOver's own queue does not. Turning it off gives the overlay a distinct \
               voice, which some people prefer for telling the two apart. Needs VoiceOver \
               running, and \"Allow VoiceOver to be controlled with AppleScript\" ticked in \
               VoiceOver Utility's General pane.",
        os: Some("macos"),
        // Off until somebody asks for it, and the reason is the permission rather than the
        // feature. Speaking through VoiceOver means an Apple Event, and the first Apple Event
        // makes macOS put a consent dialog on screen. On by default, that dialog appears at
        // startup — before the user has asked for anything, about a thing they may not want,
        // in front of a person who cannot see it to dismiss it. Ticking the box is a request,
        // and that is the moment to ask.
        default_on: false,
        state: &SPEAK_VIA_VOICEOVER,
    },
    Switch {
        key: "screen_reader_speech",
        label: "Speak through the screen reader, so it comes out the way it says everything                 else — takes effect immediately",
        help: "Hands what a module says to NVDA, JAWS or whichever screen reader is running,                instead of speaking it with a separate voice. It then arrives in your voice,                at your rate, and in the reading order you are used to. Turn it off to go                back to the separate voice, which is also what happens by itself if the                screen reader stops answering — turning this off and on again is how you ask                for another try after that, without restarting. With no screen reader                running, nothing changes either way: a module that asks to be heard is heard                through the system voice.",
        os: Some("windows"),
        // On, unlike its macOS counterpart, and for the opposite reason: there is no consent
        // dialog to spring on anybody here, and this is the path that speaks the way the
        // user's screen reader already does. It is the way out rather than the way in — if a
        // newly compiled speech library misbehaves, this is the switch that gets the old
        // voice back without a restart.
        default_on: true,
        state: &SCREEN_READER_SPEECH,
    },
    Switch {
        key: "braille",
        label: "Also send what is said to a braille display — takes effect immediately",
        help: "Writes each announcement to the braille display as well as speaking it, \
               through whichever screen reader is running. It does nothing at all without a \
               display, and nothing on a screen reader that has no braille support. Turn it \
               off if you would rather your display kept showing what your screen reader put \
               there, since what the overlay writes replaces that line until the next \
               announcement.",
        os: Some("windows"),
        // On, because the alternative is that a braille reader gets nothing from the
        // overlays at all — which is what this application did until now, while its own
        // documentation claimed otherwise. macOS needs no switch: VoiceOver brailles whatever
        // it is told to say, so braille has always followed speech there.
        default_on: true,
        state: &BRAILLE,
    },
    Switch {
        key: "dock_while_open",
        label: "Show a Dock icon while the module manager is open, so Command-Tab can \
                reach it — takes effect the next time the window is opened",
        help: "A Dock icon and an entry in the application switcher are one and the same \
               bit, so a window that Command-Tab can return to is a window with a Dock \
               icon while it is open. Turn it off and the only way back is the menu-bar \
               icon, which VO-M twice reaches. On by default since the tester asked for it: \
               it was held back over a reported side effect — a promoted application's menu \
               bar staying unresponsive until you switch away and back — which did not \
               happen on his machine.",
        os: Some("macos"),
        default_on: true,
        state: &DOCK_WHILE_OPEN,
    },
];

/// The environment variable that forces a switch on, for the launches that have no window.
pub fn env_name(key: &str) -> String {
    format!("AUTOMATION_PLATFORM_{}", key.to_ascii_uppercase())
}

/// Is a variable forcing this switch on for this run?
///
/// Reported by the tab, which must not offer to turn off something it cannot: the
/// variable wins until the process is restarted without it, and saying so is the difference
/// between a control that lies and one that explains.
pub fn forced_by_env(key: &str) -> bool {
    std::env::var_os(env_name(key)).is_some_and(|v| v != "0")
}

impl Switch {
    /// Whether this setting means anything on the system it is running on.
    ///
    /// A switch for another platform is not shown, not stored and not reported: it would be
    /// a checkbox that changes nothing, which is the one thing a control must never be.
    pub fn applies_here(&self) -> bool {
        self.os.is_none_or(|o| o == std::env::consts::OS)
    }
}

fn switch(key: &str) -> Option<&'static Switch> {
    SWITCHES.iter().find(|s| s.key == key)
}

/// Current value of one switch by key, or `false` for a key that does not exist.
pub fn get(key: &str) -> bool {
    switch(key).is_some_and(|s| s.state.load(Ordering::Relaxed))
}

/// Sets one switch for the running process. Persisting is the caller's business.
///
/// A variable forcing it on wins: turning the setting off would otherwise appear to work and
/// then be undone by the next read of an environment nobody can see.
pub fn set(key: &str, on: bool) {
    if let Some(s) = switch(key) {
        s.state.store(s.applies_here() && (on || forced_by_env(key)), Ordering::Relaxed);
    }
}

/// Loads every switch from the stored values, OR-ed with the environment.
///
/// Called once, before anything reads one — early enough that the session header can report
/// them, which is where a remote tester's log gets its account of how the application was
/// configured.
pub fn load(stored: impl Fn(&str) -> Option<bool>) {
    for s in SWITCHES {
        let on = stored(s.key).unwrap_or(s.default_on) || forced_by_env(s.key);
        s.state.store(s.applies_here() && on, Ordering::Relaxed);
    }
}

/// Every switch that is on, for the log.
pub fn active() -> Vec<String> {
    SWITCHES
        .iter()
        .filter(|s| s.state.load(Ordering::Relaxed))
        .map(|s| {
            if forced_by_env(s.key) {
                format!("{} (forced by {})", s.key, env_name(s.key))
            } else {
                s.key.to_string()
            }
        })
        .collect()
}

pub fn trace() -> bool {
    TRACE.load(Ordering::Relaxed)
}
pub fn calibrate() -> bool {
    CALIBRATE.load(Ordering::Relaxed)
}
pub fn ocr_debug() -> bool {
    OCR_DEBUG.load(Ordering::Relaxed)
}
pub fn ignore_supported_os() -> bool {
    IGNORE_SUPPORTED_OS.load(Ordering::Relaxed)
}
pub fn headless() -> bool {
    HEADLESS.load(Ordering::Relaxed)
}
/// Only asked on Windows — everywhere else `applies_here` has already pinned it off.
#[cfg(windows)]
pub fn screen_reader_speech() -> bool {
    SCREEN_READER_SPEECH.load(Ordering::Relaxed)
}
/// Only asked on Windows. On macOS braille is not ours to route: VoiceOver brailles what it
/// says, so it follows speech with nothing to switch.
#[cfg(windows)]
pub fn braille() -> bool {
    BRAILLE.load(Ordering::Relaxed)
}
/// Only asked on macOS — everywhere else `applies_here` has already pinned it off.
#[cfg(target_os = "macos")]
pub fn voiceover_speech() -> bool {
    SPEAK_VIA_VOICEOVER.load(Ordering::Relaxed)
}
#[cfg(target_os = "macos")]
pub fn dock_while_open() -> bool {
    DOCK_WHILE_OPEN.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_switch_has_a_variable_name_of_the_documented_shape() {
        // The names are what the scripts and the docs already use, so they are part of the
        // contract rather than an implementation detail.
        assert_eq!(env_name("trace"), "AUTOMATION_PLATFORM_TRACE");
        assert_eq!(env_name("headless"), "AUTOMATION_PLATFORM_HEADLESS");
        for s in SWITCHES {
            assert!(s.key.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{}", s.key);
            assert!(!s.label.is_empty() && !s.help.is_empty(), "{}", s.key);
        }
    }

    #[test]
    fn a_stored_value_is_loaded_and_can_be_changed() {
        load(|k| Some(k == "trace"));
        assert!(trace());
        assert!(!ocr_debug());
        set("ocr_debug", true);
        assert!(ocr_debug());
        set("ocr_debug", false);
        assert!(!ocr_debug());
    }

    #[test]
    fn the_label_says_when_a_switch_takes_effect() {
        // Every switch has to tell the reader whether ticking it is enough, because some are
        // read once at startup and a control that appears inert is a bug report.
        //
        // The list of accepted phrases is deliberately short and deliberately grows: a
        // setting whose moment is none of these has invented a fourth kind of "later", and
        // adding it here should be a decision rather than something that slips through.
        const MOMENTS: [&str; 4] = ["immediately", "reload", "restart", "next time"];
        for s in SWITCHES {
            let l = s.label.to_ascii_lowercase();
            assert!(
                MOMENTS.iter().any(|m| l.contains(m)),
                "{} does not say when it applies",
                s.key
            );
        }
    }
}
