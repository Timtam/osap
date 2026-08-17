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
    state: &'static AtomicBool,
}

static TRACE: AtomicBool = AtomicBool::new(false);
static CALIBRATE: AtomicBool = AtomicBool::new(false);
static OCR_DEBUG: AtomicBool = AtomicBool::new(false);
static IGNORE_SUPPORTED_OS: AtomicBool = AtomicBool::new(false);
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// Every application setting, in the order the tab shows them: the ones that take effect
/// immediately first, so the two that need a restart are not the first thing read out.
pub const SWITCHES: &[Switch] = &[
    Switch {
        key: "trace",
        label: "Detailed (trace) logging — takes effect immediately",
        help: "Records every OS call that failed and every decision the key handling made. \
               Makes the log large, so it is meant for chasing one problem rather than for \
               leaving on.",
        state: &TRACE,
    },
    Switch {
        key: "ocr_debug",
        label: "Save the images OCR was given — takes effect immediately",
        help: "Writes what the recogniser actually saw next to the application, which is the \
               only way to tell 'the text was unreadable' from 'the region was wrong'.",
        state: &OCR_DEBUG,
    },
    Switch {
        key: "calibrate",
        label: "Calibration keys in overlays — reload modules to apply",
        help: "Arms the measuring keys inside whichever overlay is active: a screenshot with \
               a crosshair on every control, cropping a template around the focused one, and \
               counting that template's matches. For authoring an overlay, not for using one.",
        state: &CALIBRATE,
    },
    Switch {
        key: "ignore_supported_os",
        label: "Load modules not meant for this system — restart to apply",
        help: "A module may declare which systems it is written for, and one that excludes \
               this system is normally skipped. Turn this on when the module works here and \
               its manifest is simply behind the code.",
        state: &IGNORE_SUPPORTED_OS,
    },
    Switch {
        key: "headless",
        label: "Run without a window — restart to apply",
        help: "No tray icon and no manager window; modules with hotkeys, captured keys or \
               window triggers still run. For testing and automation. Turning this on means \
               this tab will not be reachable next time.",
        state: &HEADLESS,
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
        s.state.store(on || forced_by_env(key), Ordering::Relaxed);
    }
}

/// Loads every switch from the stored values, OR-ed with the environment.
///
/// Called once, before anything reads one — early enough that the session header can report
/// them, which is where a remote tester's log gets its account of how the application was
/// configured.
pub fn load(stored: impl Fn(&str) -> Option<bool>) {
    for s in SWITCHES {
        let on = stored(s.key).unwrap_or(false) || forced_by_env(s.key);
        s.state.store(on, Ordering::Relaxed);
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
        // Every switch has to tell the reader whether pressing OK is enough, because two of
        // them are read once at startup and a control that appears inert is a bug report.
        for s in SWITCHES {
            let l = s.label.to_ascii_lowercase();
            assert!(
                l.contains("immediately") || l.contains("reload") || l.contains("restart"),
                "{} does not say when it applies",
                s.key
            );
        }
    }
}
