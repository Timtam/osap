//! Whether VoiceOver lets this application speak through it: "Allow VoiceOver to be controlled
//! with AppleScript", a box in VoiceOver Utility's General pane, and what is done while it is
//! not ticked.
//!
//! **Why this exists.** A Mac session of 2026-10-01 had every permission granted, "Speak through
//! VoiceOver" on, that box unticked — and silence. Each line goes to VoiceOver as an Apple Event
//! sent without waiting for a reply (`voiceover.rs`), so macOS refusing to SEND it is visible and
//! VoiceOver dropping it after it arrived is not: the log said "speech is going to VoiceOver",
//! the first line "took 0 ms", and no refusal ever came, so the fallback that exists for
//! refusals (`vo_park.rs`) never spoke. The maintainer's decision: catch it; the user is never
//! left in silence.
//!
//! **How it is known**, cheapest first, and nothing here writes anything — the box is the
//! user's security setting, and VoiceOver Utility asks for an administrator's password to change
//! it:
//!
//! - **the file** VoiceOver Utility writes when the box is ticked, [`MARKER`]. Its existence is
//!   what VoiceOver gates AppleScript on (guidepup/setup issue 66, on macOS 26; the guidepup
//!   setup code polls for it as an ordinary user on every version it supports), and it is
//!   written by the box since at least macOS 12 (the guidepup author's write-up, on Monterey).
//!   It is root's: the box writes it through an operation an administrator authorises, and
//!   otherwise only root can — SIP does not guard it (`sudo touch` makes it, issue 66), which
//!   is how test machines make it without the box;
//! - **the preference** [`KEY`] in [`DOMAIN`], which the box sets as well. Read through
//!   CFPreferences, it is only evidence below macOS 15 ([`KEY_MOVED_IN`]): from 15 on VoiceOver
//!   keeps its preferences in its group container (actions/runner-images issue 11257, and the
//!   guidepup code's own switch at Darwin 24), which is not read here — another application's
//!   container is the kind of place recent macOS guards with a privacy question, and nothing
//!   here may put one on screen — and on 26 the key is not honoured at all (guidepup/setup
//!   issue 66);
//! - **VoiceOver itself**, asked one read-only question through `osascript` ([`PROBE`]) that
//!   waits for its answer. With the box unticked VoiceOver answers its own suite's commands with
//!   -1708, "doesn't understand the … message" (guidepup/setup issue 66, actions/runner-images
//!   issue 11257); whether a `get` of one of its properties is answered the same way has not
//!   been seen yet, which is why an answer it does not recognise leaves the decision to the
//!   reads above rather than guessing, and why the raw answer always goes into the log.
//!
//! **The two mistakes are not alike, and the rules lean one way.** Taken for unticked when it is
//! ticked, the system voice speaks and says why — heard, and the log shows the reads that were
//! wrong. Taken for ticked when it is not, every line is dropped: the silence this exists to end.
//! So VoiceOver refusing counts against reads that say "ticked", and VoiceOver answering never
//! counts against reads that say "unticked": a `get` belongs to the standard suite, which issue
//! 66 found still answered with the box unticked (`quit`), so its answer may come either way.
//!
//! Pure — no clock of its own, no Objective-C — so it runs under `cargo test` on Windows, the
//! way `vo_park.rs` does; the reads and the question are in `voiceover.rs`.

use std::time::Instant;

use super::pace::next_look;
use super::vo_park::{codes_in, NOT_PERMITTED, PROC_NOT_FOUND, WOULD_REQUIRE_CONSENT};

/// The file VoiceOver Utility writes when its box is ticked, and removes when it is unticked.
pub(super) const MARKER: &str = "/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled";
/// The preferences domain the box's key is in, below macOS 15.
pub(super) const DOMAIN: &str = "com.apple.VoiceOver4/default";
/// The key the box sets.
pub(super) const KEY: &str = "SCREnableAppleScript";
/// The macOS whose VoiceOver no longer reads [`DOMAIN`] from where CFPreferences finds it.
pub(super) const KEY_MOVED_IN: i64 = 15;
/// `errAEEventNotHandled`: what VoiceOver answers its own commands with while the box is
/// unticked.
pub(super) const NOT_HANDLED: i64 = -1708;
/// VoiceOver Utility's bundle identifier: when it quits, the box may just have been ticked. One
/// source only (doesitarm.com), so its bundle's file name counts as well ([`UTILITY_BUNDLE`]).
pub(super) const UTILITY: &str = "com.apple.VoiceOverUtility";
/// VoiceOver Utility's bundle as it is named on disk, in `/System/Applications/Utilities` on
/// macOS 26 (guidepup/setup issue 66) — not localised, unlike the name the user hears.
pub(super) const UTILITY_BUNDLE: &str = "VoiceOver Utility.app";

/// What the question prints when VoiceOver answered it.
pub(super) const ANSWERED: &str = "answered";
/// What it prints when VoiceOver was not running by the time it was asked.
pub(super) const NOT_RUNNING: &str = "not running";

/// The question, one `osascript -e` argument per line.
///
/// `text under cursor of vo cursor` is a read: it moves nothing, and says nothing. The `vo
/// cursor` property is in VoiceOver's dictionary (docs/voiceover-scripting-codes.md), and this
/// read of it is the one guidepup's `itemText` makes. Its answer is never printed — `run script`
/// discards it and `answered` is printed instead — so what is under the user's cursor does not
/// reach the log.
///
/// **Asked only if VoiceOver runs, and compiled only then.** `tell application "VoiceOver"`
/// STARTS an application that is not running, and so does compiling a script that uses its
/// terminology, which VoiceOver hands over only from a running process. `is running` asks
/// without starting it, and `run script` compiles the `tell` only once that has been answered —
/// so a VoiceOver that quit between the event loop's look and this question is not started for
/// somebody who switched it off.
pub(super) const PROBE: [&str; 5] = [
    r#"if application id "com.apple.VoiceOver" is running then"#,
    r#"run script "tell application id \"com.apple.VoiceOver\" to get text under cursor of vo cursor""#,
    r#"return "answered""#,
    "end if",
    r#"return "not running""#,
];

/// Said through the system voice, once per episode, when VoiceOver does not accept AppleScript.
/// English, as the application's interface is; short, because it is heard rather than read and
/// the next interrupting line cuts it off; what matters first — who cannot, and the box's name
/// word for word, because that is what the user then looks for. VO-Fn-F8 is Apple's own
/// spelling of the command (VoiceOver's General commands): on a Mac laptop as it ships, F8
/// without Fn is a media key.
pub(super) const TELL: &str = "Automation Platform cannot speak through VoiceOver. Turn on 'Allow \
     VoiceOver to be controlled with AppleScript' in VoiceOver Utility, General; VO-Fn-F8 opens it. \
     Until then, this voice speaks.";

/// Said through VoiceOver once VoiceOver itself has answered that the box is ticked, after the
/// user was told it was not. Not "again": with the box unticked from the start, nothing had
/// spoken through VoiceOver before.
pub(super) const CONFIRM: &str = "Automation Platform now speaks through VoiceOver.";

/// The file, as `stat` found it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Marker {
    Present,
    Absent,
    /// Anything but "it is there" and "it is not": the reason, as the system words it.
    Unreadable(String),
}

impl Marker {
    /// From a `stat` of [`MARKER`].
    pub(super) fn from_stat(r: std::io::Result<()>) -> Marker {
        match r {
            Ok(()) => Marker::Present,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Marker::Absent,
            Err(e) => Marker::Unreadable(e.to_string()),
        }
    }
}

/// The preference, as CFPreferences answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    On,
    Off,
    /// Not there, or not a boolean.
    Unset,
}

/// What can be read without asking VoiceOver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Reads {
    pub marker: Marker,
    pub key: Key,
    /// Whether this macOS's VoiceOver reads the key from where it was read ([`KEY_MOVED_IN`]).
    pub key_live: bool,
}

impl Reads {
    /// The readings as they are, for the log and the `[env]` line: the file by its path, the
    /// preference by its key, and whether that key counts on this macOS.
    pub(super) fn words(&self) -> String {
        let file = match &self.marker {
            Marker::Present => format!("{MARKER} is there"),
            Marker::Absent => format!("{MARKER} is not there"),
            Marker::Unreadable(e) => format!("{MARKER} could not be looked at ({e})"),
        };
        let key = match self.key {
            Key::On => "on",
            Key::Off => "off",
            Key::Unset => "not set",
        };
        format!(
            "{file}; {KEY}: {key}{}",
            if self.key_live { "" } else { ", not counted on macOS 15 and later" }
        )
    }
}

/// VoiceOver's answer to [`PROBE`], read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Probe {
    /// It answered: the box is ticked — unless the reads say it is not ([`verdict`]).
    Answered,
    /// It does not handle the question (-1708): the box is not ticked. The raw answer.
    Refused(String),
    /// Not asked, or an answer that does not say which: why, with the raw answer.
    Inconclusive(String),
    /// Not asked because macOS's Automation question for VoiceOver is on screen ([`unasked`]):
    /// asking would put it there a second time. Says nothing about the box, like
    /// [`Probe::Inconclusive`] — and while it lasts the user is not told about the box either,
    /// which would be said over macOS's own question ([`Gate::answered`]).
    Waiting(String),
}

impl Probe {
    /// What came back, for the log.
    pub(super) fn raw(&self) -> &str {
        match self {
            Probe::Answered => ANSWERED,
            Probe::Refused(r) | Probe::Inconclusive(r) | Probe::Waiting(r) => r,
        }
    }

    /// What it is taken to mean, for the log.
    pub(super) fn reading(&self) -> &'static str {
        match self {
            Probe::Answered => {
                "VoiceOver answered the read, which counts as the box ticked only where the file and \
                 the preference cannot say"
            }
            Probe::Refused(_) => {
                "VoiceOver does not handle the question (-1708), which is its answer with the box unticked"
            }
            Probe::Inconclusive(_) => "that does not say whether the box is ticked; the file and the preference decide",
            Probe::Waiting(_) => {
                "VoiceOver is asked once the Automation question on screen has been answered; until \
                 then the file and the preference decide"
            }
        }
    }
}

/// What stands for VoiceOver's answer when it was not asked, because the system said `status`
/// when asked — asking nobody — whether this application may send VoiceOver Apple Events; and
/// whether the Automation question is on screen right now, put there by ticking the setting.
///
/// -1744 is the system's answer both while that question is on screen and before anybody has
/// put it ([`WOULD_REQUIRE_CONSENT`]), and only the first is a reason to hold the sentence back:
/// with the setting already on from an earlier run and a new build that macOS has forgotten the
/// grant for, nothing puts the question on screen while the box keeps every line from VoiceOver
/// — the first line to VoiceOver puts it, once the box lets one through.
pub(super) fn unasked(status: i64, question_on_screen: bool) -> Probe {
    match status {
        WOULD_REQUIRE_CONSENT if question_on_screen => Probe::Waiting(format!(
            "not asked: macOS's Automation question for VoiceOver is on screen and not answered yet [{status}]"
        )),
        WOULD_REQUIRE_CONSENT => Probe::Inconclusive(format!(
            "not asked: this application has not been asked yet whether it may send VoiceOver Apple \
             Events, and asking through osascript would put that question on screen; the first line \
             to VoiceOver puts it [{status}]"
        )),
        NOT_PERMITTED => Probe::Inconclusive(format!(
            "not asked: this application may not send VoiceOver Apple Events (Privacy & Security, \
             Automation) [{status}]"
        )),
        PROC_NOT_FOUND => Probe::Inconclusive(format!("not asked: VoiceOver was not running by then [{status}]")),
        other => Probe::Inconclusive(format!(
            "not asked: the system answered {other} when asked whether this application may send \
             VoiceOver Apple Events"
        )),
    }
}

/// Reads what `osascript` did with [`PROBE`]: what it printed when it succeeded, or what it wrote
/// to its error output and whether it had to be stopped at its limit.
pub(super) fn classify(answer: &Result<String, (String, bool)>) -> Probe {
    match answer {
        Ok(out) if out.trim() == ANSWERED => Probe::Answered,
        Ok(out) if out.trim() == NOT_RUNNING => {
            Probe::Inconclusive("VoiceOver was not running by the time it was asked".to_string())
        }
        Ok(out) => Probe::Inconclusive(format!("an answer this does not know: {}", out.trim())),
        Err((why, true)) => Probe::Inconclusive(why.clone()),
        Err((why, false)) => {
            if codes_in(why).first() == Some(&NOT_HANDLED) {
                Probe::Refused(why.clone())
            } else {
                Probe::Inconclusive(why.clone())
            }
        }
    }
}

/// Whether VoiceOver accepts AppleScript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scripting {
    Allowed,
    NotAllowed,
    /// Nothing read says which.
    Unknown,
}

impl Scripting {
    pub(super) fn word(self) -> &'static str {
        match self {
            Scripting::Allowed => "allowed",
            Scripting::NotAllowed => "not allowed",
            Scripting::Unknown => "unknown",
        }
    }
}

/// An answer, and how it is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Verdict {
    pub state: Scripting,
    pub how: String,
}

/// What the file and the preference say on their own.
///
/// The file decides when it can be looked at: absent, the box is not ticked, whatever the
/// preference says — VoiceOver needs it, and the box removes it. Present, it counts as ticked
/// only where the preference does not say otherwise: below macOS 15 VoiceOver needs the
/// preference as well (the guidepup author's write-up, on Monterey), so the preference off
/// there is "not ticked" — the box writes both, and something else wrote the file — and not
/// set there is a question for VoiceOver rather than an answer. When the file cannot be looked
/// at, the preference decides only where VoiceOver reads it, and only when it is set.
pub(super) fn from_reads(r: &Reads) -> Verdict {
    let (state, how) = match (&r.marker, r.key_live, r.key) {
        (Marker::Absent, ..) => (
            Scripting::NotAllowed,
            "the file VoiceOver Utility writes when the box is ticked is not there".to_string(),
        ),
        (Marker::Present, true, Key::Off) => (
            Scripting::NotAllowed,
            format!(
                "the file VoiceOver Utility writes when the box is ticked is there, but {KEY} is off, \
                 and this macOS's VoiceOver needs both"
            ),
        ),
        (Marker::Present, true, Key::Unset) => (
            Scripting::Unknown,
            format!(
                "the file VoiceOver Utility writes when the box is ticked is there, but {KEY} is not \
                 set, and this macOS's VoiceOver needs both"
            ),
        ),
        (Marker::Present, ..) => (
            Scripting::Allowed,
            "the file VoiceOver Utility writes when the box is ticked is there".to_string(),
        ),
        (Marker::Unreadable(e), true, Key::On) => {
            (Scripting::Allowed, format!("{KEY} is on (the file could not be looked at: {e})"))
        }
        (Marker::Unreadable(e), true, Key::Off) => {
            (Scripting::NotAllowed, format!("{KEY} is off (the file could not be looked at: {e})"))
        }
        (Marker::Unreadable(e), ..) => (
            Scripting::Unknown,
            format!("the file could not be looked at ({e}), and {KEY} does not say"),
        ),
    };
    Verdict { state, how }
}

/// What everything says together. A refusal counts over reads that say "allowed" — VoiceOver is
/// the one that drops the lines. An answer counts where the reads cannot say, never over reads
/// that say "not allowed": the question is a `get`, which may be answered with the box unticked
/// (see the top of this file), and taking that for "ticked" would bring the silence back. Either
/// disagreement is part of how it is known, so the log says which read was wrong on that Mac.
pub(super) fn verdict(r: &Reads, p: Option<&Probe>) -> Verdict {
    let reads = from_reads(r);
    match p {
        Some(Probe::Answered) if reads.state == Scripting::NotAllowed => Verdict {
            state: Scripting::NotAllowed,
            how: format!(
                "{}; VoiceOver did answer a read asked through AppleScript, but a read is not one of \
                 its own commands, so that does not count against the reads",
                reads.how
            ),
        },
        Some(Probe::Answered) => Verdict {
            state: Scripting::Allowed,
            how: "VoiceOver answered a question asked through AppleScript".to_string(),
        },
        Some(Probe::Refused(raw)) => Verdict {
            state: Scripting::NotAllowed,
            how: format!(
                "VoiceOver refused a question asked through AppleScript as one it does not handle ({raw}){}",
                if reads.state == Scripting::Allowed { format!(", although {}", reads.how) } else { String::new() }
            ),
        },
        Some(Probe::Inconclusive(_) | Probe::Waiting(_)) | None => reads,
    }
}

/// What is shown for the reads made now, given what VoiceOver was last asked: its answer counts
/// only when it was given on the same reads — after the box changed, it is about a box that is
/// gone.
pub(super) fn shown(now: &Reads, last: Option<&(Reads, Probe)>) -> Verdict {
    match last {
        Some((asked_on, probe)) if asked_on == now => verdict(now, Some(probe)),
        _ => from_reads(now),
    }
}

/// The value of the `[env]` line `voiceover applescript`: the reads' verdict alone, and the reads
/// it comes from. VoiceOver's own answer is not in it — it is asked on the speech thread at about
/// the time this block is written, and the line must not depend on which came first; it follows
/// in the `[speech]` lines.
pub(super) fn env_value(r: &Reads) -> String {
    format!("{} ({})", from_reads(r).state.word(), r.words())
}

/// The line on the Permissions page: the box's name, its state in plain words, what it is for and
/// where it is. Plain because a screen reader reads it out: the evidence — paths, codes, the raw
/// answer — is in the log.
pub(super) fn page_line(v: &Verdict) -> String {
    let plain = match v.state {
        Scripting::Allowed => "the box is ticked",
        Scripting::NotAllowed => "the box is not ticked",
        Scripting::Unknown => "this application cannot tell; the log says why",
    };
    format!(
        "VoiceOver's own setting \"Allow VoiceOver to be controlled with AppleScript\" — {}: {plain}. \
         Needed only for \"Speak through VoiceOver\": without it VoiceOver drops what this \
         application hands it, without a word, and the system voice speaks instead. This \
         application does not change it. It is in VoiceOver Utility, General pane; VO-Fn-F8 opens \
         VoiceOver Utility, and changing the box asks for an administrator's password. Re-check \
         now reads it again.",
        v.state.word()
    )
}

/// Whether an application that quit is VoiceOver Utility, after which the box may have changed:
/// by its bundle identifier, or by its bundle's file name, either of which may be all there is.
pub(super) fn is_utility(bundle_id: Option<&str>, bundle_file: Option<&str>) -> bool {
    bundle_id == Some(UTILITY) || bundle_file == Some(UTILITY_BUNDLE)
}

/// The schedule while the box is not ticked, in the words the log uses.
const SCHEDULE: &str = "every 3 s for five minutes and then every 10 s while VoiceOver runs, every 3 s \
                        for a minute and then every 30 s while it does not, and at once when VoiceOver \
                        Utility quits, VoiceOver starts again or \"Speak through VoiceOver\" is ticked";

/// What the transport does after a step of [`Gate`].
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Out {
    /// Read the file and the preference now and ask VoiceOver on them, on the speech thread.
    pub ask: bool,
    /// Lines for the log.
    pub log: Vec<String>,
    /// Said through the system voice now.
    pub tell: Option<&'static str>,
    /// Said through VoiceOver now.
    pub confirm: Option<&'static str>,
}

/// Whether lines go to VoiceOver as far as its AppleScript box is concerned, and when it is looked
/// at again. Every step says in its [`Out`] what the transport does next; the transport keeps its
/// gate in step with [`Gate::open`].
///
/// - **A trigger** — the start, "Speak through VoiceOver" ticked, VoiceOver starting or
///   restarting, VoiceOver Utility quitting: the reads decide at once when they say "not
///   allowed" (the lines go to the system voice from that moment), and VoiceOver is asked. Lines
///   handed over meanwhile wait behind the question on the speech thread and go where its answer
///   says.
/// - **An answer:** "not allowed" closes the gate and parks it — the user hears [`TELL`], once per
///   episode — and "allowed" opens it, with [`CONFIRM`] through VoiceOver when the user had been
///   told and VoiceOver itself answered: opened on the reads alone — VoiceOver not asked (no
///   Automation grant yet, or a refused one), or its reply saying neither — the word could be
///   untrue, and nothing is said.
///   "Unknown" changes nothing: a parked gate stays parked, an open one open, as it was before
///   any of this existed.
/// - **While parked**, the reads are looked at again on the schedule the refusals use
///   ([`next_look`]), and VoiceOver is asked again only when they changed since it was last
///   asked and do not say "not allowed", or while the user is owed [`TELL`]: it is not said
///   while macOS's Automation question is on screen ([`Probe::Waiting`]), a second voice over
///   the one dialog the user has to answer, and it is said once a question finds that answered.
///   Reads that cannot say anything ask nothing on the looks — each question is a child process,
///   and the box changes only in VoiceOver Utility, whose quitting is a trigger.
///
/// An episode ends when the box is found ticked, and a new one starts when the setting is ticked
/// again ([`Gate::rearmed`]): a deliberate act, after which hearing the answer again is the
/// point.
#[derive(Debug)]
pub(super) struct Gate {
    open: bool,
    /// The last answer that was written about.
    known: Option<Scripting>,
    /// A question to VoiceOver is under way, asked because of this.
    asking: Option<String>,
    /// Another trigger came while it was: ask again once it is answered.
    again: bool,
    /// Not allowed: since when, and when the reads are looked at next.
    parked: Option<(Instant, Instant)>,
    /// The reads VoiceOver was last asked on.
    asked_on: Option<Reads>,
    /// The user has heard [`TELL`] in this episode.
    told: bool,
    /// [`TELL`] was held back while the Automation question was on screen ([`Probe::Waiting`]).
    owed: bool,
    /// The last raw answer written in full.
    said_raw: Option<String>,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            open: true,
            known: None,
            asking: None,
            again: false,
            parked: None,
            asked_on: None,
            told: false,
            owed: false,
            said_raw: None,
        }
    }
}

impl Gate {
    /// Whether lines may go to VoiceOver.
    pub(super) fn open(&self) -> bool {
        self.open
    }

    /// Something happened after which the box may read differently (`why`); `reads` were made
    /// just now.
    pub(super) fn triggered(&mut self, reads: &Reads, why: &str) -> Out {
        let mut out = Out::default();
        let first = from_reads(reads);
        if first.state == Scripting::NotAllowed && self.open {
            self.open = false;
        }
        if self.asking.is_some() {
            self.again = true;
            return out;
        }
        self.asking = Some(why.to_string());
        out.ask = true;
        out.log.push(format!(
            "asking VoiceOver whether it accepts AppleScript ({why}); read first: {}{}",
            reads.words(),
            if self.open { "" } else { " — the lines go to the system voice until it answers" }
        ));
        out
    }

    /// VoiceOver's answer to a question asked on `reads`, at `now`; `running` says whether
    /// VoiceOver runs, for the pace.
    pub(super) fn answered(&mut self, reads: Reads, probe: Probe, now: Instant, running: bool) -> Out {
        let mut out = Out::default();
        let why = self.asking.take().unwrap_or_else(|| "asked".to_string());
        let v = verdict(&reads, Some(&probe));
        if self.said_raw.as_deref() != Some(probe.raw()) {
            out.log.push(format!(
                "VoiceOver, asked through AppleScript ({why}), answered: {} — {}",
                probe.raw(),
                probe.reading()
            ));
            self.said_raw = Some(probe.raw().to_string());
        }
        let news = self.known != Some(v.state);
        let waiting = matches!(probe, Probe::Waiting(_));
        match v.state {
            Scripting::NotAllowed => {
                self.open = false;
                let since = self.parked.map_or(now, |(since, _)| since);
                self.parked = Some((since, now + next_look(now.saturating_duration_since(since), running)));
                let tell = !self.told && !waiting;
                let held = !self.told && waiting && !self.owed;
                self.owed = !self.told && waiting;
                if tell {
                    self.told = true;
                    out.tell = Some(TELL);
                }
                if news || tell || held {
                    out.log.push(format!(
                        "VoiceOver does not accept AppleScript: {}. It drops what it is handed without \
                         a word, so every line goes to the system voice until it does{}. The box is \
                         \"Allow VoiceOver to be controlled with AppleScript\", in VoiceOver Utility's \
                         General pane; it is looked at again {SCHEDULE}. Read: {}",
                        v.how,
                        if tell {
                            ", and the user is told so once"
                        } else if self.owed {
                            ", and the user is told so once the Automation question on screen has been \
                             answered"
                        } else {
                            ""
                        },
                        reads.words()
                    ));
                }
            }
            Scripting::Allowed => {
                self.open = true;
                self.owed = false;
                let parked = self.parked.take();
                // The word through VoiceOver only when VoiceOver itself answered: opened on the
                // reads with VoiceOver not asked, it may refuse the word — and the system voice
                // would then say that VoiceOver speaks.
                let told = std::mem::take(&mut self.told);
                if told && probe == Probe::Answered {
                    out.confirm = Some(CONFIRM);
                }
                if news {
                    out.log.push(match parked {
                        Some((since, _)) => format!(
                            "VoiceOver accepts AppleScript now ({}), after {} s; lines go to VoiceOver \
                             again{}",
                            v.how,
                            now.saturating_duration_since(since).as_secs(),
                            if out.confirm.is_some() {
                                ", and it says so".to_string()
                            } else if told {
                                format!("; VoiceOver itself did not say so ({}), so nothing is said", probe.raw())
                            } else {
                                String::new()
                            }
                        ),
                        None => format!("VoiceOver accepts AppleScript ({})", v.how),
                    });
                }
            }
            Scripting::Unknown => {
                if let Some((since, _)) = self.parked {
                    self.parked = Some((since, now + next_look(now.saturating_duration_since(since), running)));
                }
                if news {
                    out.log.push(format!(
                        "whether VoiceOver accepts AppleScript cannot be told ({}); {}",
                        v.how,
                        if self.open {
                            "lines go to VoiceOver as before. If nothing is heard, the box is \"Allow \
                             VoiceOver to be controlled with AppleScript\", in VoiceOver Utility's \
                             General pane"
                        } else {
                            "the lines stay with the system voice until VoiceOver answers"
                        }
                    ));
                }
            }
        }
        self.known = Some(v.state);
        self.asked_on = Some(reads);
        out.ask = std::mem::take(&mut self.again);
        out
    }

    /// Whether the reads are due to be looked at again, at `now`.
    pub(super) fn due(&self, now: Instant) -> bool {
        self.asking.is_none() && self.parked.is_some_and(|(_, due)| now >= due)
    }

    /// The reads looked at again while parked ([`Gate::due`]). VoiceOver is asked when it may
    /// now say yes — the reads do not say "not allowed" — and they changed since it was last
    /// asked; and while the user is owed [`TELL`], so that the question that finds the Automation
    /// question answered says it. Reads that cannot say, unchanged, ask nothing: a child process
    /// every few seconds for the rest of a session would buy only what VoiceOver Utility quitting
    /// asks for anyway.
    pub(super) fn looked(&mut self, reads: &Reads, now: Instant, running: bool) -> Out {
        let mut out = Out::default();
        let Some((since, _)) = self.parked else {
            return out;
        };
        self.parked = Some((since, now + next_look(now.saturating_duration_since(since), running)));
        let first = from_reads(reads);
        let changed = self.asked_on.as_ref() != Some(reads);
        let may_open = first.state != Scripting::NotAllowed && changed;
        if running && (may_open || self.owed) {
            self.asking = Some(if self.owed {
                "the user is owed the answer, once the Automation question has been answered".to_string()
            } else {
                format!("the reads changed: {}", reads.words())
            });
            out.ask = true;
        }
        out
    }

    /// "Speak through VoiceOver" was ticked: a new episode, so the user hears the answer again.
    pub(super) fn rearmed(&mut self) {
        self.told = false;
        self.owed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn reads(marker: Marker, key: Key, key_live: bool) -> Reads {
        Reads { marker, key, key_live }
    }

    fn absent() -> Reads {
        reads(Marker::Absent, Key::Unset, false)
    }

    fn present() -> Reads {
        reads(Marker::Present, Key::Unset, false)
    }

    fn blind() -> Reads {
        reads(Marker::Unreadable("Permission denied (os error 13)".into()), Key::Unset, false)
    }

    fn refused() -> Probe {
        Probe::Refused(
            "execution error: VoiceOver got an error: vo cursor doesn’t understand the “get” message. (-1708)".into(),
        )
    }

    #[test]
    fn the_file_is_read_as_there_not_there_or_unreadable() {
        assert_eq!(Marker::from_stat(Ok(())), Marker::Present);
        assert_eq!(Marker::from_stat(Err(std::io::ErrorKind::NotFound.into())), Marker::Absent);
        let denied = Marker::from_stat(Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
        assert!(matches!(denied, Marker::Unreadable(_)), "{denied:?}");
    }

    /// VoiceOver's answer: only "answered" and -1708 decide; everything else is left to the reads.
    #[test]
    fn only_an_answer_and_minus_1708_say_anything() {
        assert_eq!(classify(&Ok("answered\n".into())), Probe::Answered);
        assert!(matches!(classify(&Ok("not running".into())), Probe::Inconclusive(w) if w.contains("not running")));
        assert!(matches!(classify(&Ok("42".into())), Probe::Inconclusive(w) if w.contains("42")));
        let off = "execution error: VoiceOver got an error: right doesn't understand the \"move\" message. (-1708)";
        assert_eq!(classify(&Err((off.into(), false))), Probe::Refused(off.into()));
        for other in [
            "execution error: Not authorized to send Apple events to VoiceOver. (-1743)",
            "execution error: VoiceOver got an error: Can’t get text under cursor of vo cursor. (-1728)",
            "execution error: VoiceOver got an error: AppleEvent handler failed. (-10000)",
            "execution error: VoiceOver got an error: AppleEvent timed out. (-1712)",
            "osascript could not be started (No such file or directory)",
            "error (-17080)",
        ] {
            assert!(matches!(classify(&Err((other.into(), false))), Probe::Inconclusive(_)), "{other}");
        }
        let stalled = classify(&Err(("osascript did not come back within 5 s (-1708)".into(), true)));
        assert!(matches!(stalled, Probe::Inconclusive(_)), "a stopped child says nothing, whatever it printed");
        assert_eq!(refused().raw(), refused().raw());
        assert_eq!(Probe::Answered.raw(), "answered");
        for p in [Probe::Answered, refused(), Probe::Inconclusive(String::new())] {
            assert!(!p.reading().is_empty());
        }
    }

    /// VoiceOver not asked, by what the system said: only a question on screen holds anything
    /// back. -1744 with nothing on screen is "not asked yet" — a new build with the setting on.
    #[test]
    fn minus_1744_waits_only_while_the_question_is_on_screen() {
        assert!(matches!(unasked(-1744, true), Probe::Waiting(w) if w.contains("on screen") && w.ends_with("[-1744]")));
        assert!(matches!(unasked(-1744, false), Probe::Inconclusive(w) if w.contains("not been asked yet")));
        assert!(matches!(unasked(-1743, false), Probe::Inconclusive(w) if w.contains("Automation") && w.contains("[-1743]")));
        assert!(matches!(unasked(-1743, true), Probe::Inconclusive(_)), "answered: nothing on screen to wait for");
        assert!(matches!(unasked(-600, true), Probe::Inconclusive(w) if w.contains("not running") && w.contains("[-600]")));
        assert!(matches!(unasked(-50, false), Probe::Inconclusive(w) if w.contains("-50")));
    }

    #[test]
    fn the_file_decides_and_the_key_only_where_voiceover_reads_it() {
        assert_eq!(from_reads(&absent()).state, Scripting::NotAllowed);
        assert_eq!(from_reads(&reads(Marker::Absent, Key::On, true)).state, Scripting::NotAllowed, "the file decides");
        assert_eq!(from_reads(&present()).state, Scripting::Allowed);
        assert_eq!(from_reads(&reads(Marker::Present, Key::On, true)).state, Scripting::Allowed);
        assert_eq!(from_reads(&reads(Marker::Present, Key::Off, false)).state, Scripting::Allowed, "a key not read");
        let both = from_reads(&reads(Marker::Present, Key::Off, true));
        assert_eq!(both.state, Scripting::NotAllowed, "below 15 VoiceOver needs both");
        assert!(both.how.contains("needs both"), "{}", both.how);
        assert_eq!(from_reads(&reads(Marker::Present, Key::Unset, true)).state, Scripting::Unknown, "VoiceOver asked");
        let unreadable = Marker::Unreadable("denied".into());
        assert_eq!(from_reads(&reads(unreadable.clone(), Key::On, true)).state, Scripting::Allowed);
        assert_eq!(from_reads(&reads(unreadable.clone(), Key::Off, true)).state, Scripting::NotAllowed);
        assert_eq!(from_reads(&reads(unreadable.clone(), Key::Unset, true)).state, Scripting::Unknown);
        assert_eq!(from_reads(&reads(unreadable, Key::On, false)).state, Scripting::Unknown, "a key not read");
    }

    /// The two mistakes are not alike: a refusal counts over reads that say "ticked", an answer
    /// never over reads that say "unticked" — and either disagreement is said.
    #[test]
    fn a_refusal_counts_over_the_reads_and_an_answer_only_where_they_cannot_say() {
        let v = verdict(&absent(), Some(&Probe::Answered));
        assert_eq!(v.state, Scripting::NotAllowed, "an answered `get` must not bring the silence back");
        assert!(v.how.contains("not there") && v.how.contains("does not count against"), "{}", v.how);
        let v = verdict(&present(), Some(&refused()));
        assert_eq!(v.state, Scripting::NotAllowed);
        assert!(v.how.contains("(-1708)") && v.how.contains("although the file"), "{}", v.how);
        assert_eq!(verdict(&present(), Some(&Probe::Answered)).how, "VoiceOver answered a question asked through AppleScript");
        assert_eq!(verdict(&blind(), Some(&Probe::Answered)).state, Scripting::Allowed, "the reads cannot say");
        assert_eq!(verdict(&blind(), Some(&refused())).state, Scripting::NotAllowed);
        assert_eq!(verdict(&absent(), Some(&Probe::Inconclusive("x".into()))), from_reads(&absent()));
        assert_eq!(verdict(&blind(), None).state, Scripting::Unknown);
        // The page: VoiceOver's answer only for the reads it was given on.
        let last = (present(), refused());
        assert_eq!(shown(&present(), Some(&last)).state, Scripting::NotAllowed);
        assert_eq!(shown(&absent(), Some(&last)).state, Scripting::NotAllowed, "the reads changed: they decide");
        assert_eq!(shown(&reads(Marker::Present, Key::On, true), Some(&last)).state, Scripting::Allowed);
        assert_eq!(shown(&absent(), Some(&(absent(), Probe::Answered))).state, Scripting::NotAllowed);
        assert_eq!(shown(&blind(), None).state, Scripting::Unknown);
    }

    /// The words a person reads or hears: who cannot, the box's exact name, where it is and the
    /// key that opens it as Apple spells it; on the page plain words, the evidence in the log.
    #[test]
    fn the_words_name_the_box_and_where_it_is() {
        const NAME: &str = "Allow VoiceOver to be controlled with AppleScript";
        assert!(TELL.starts_with("Automation Platform cannot speak through VoiceOver."), "{TELL}");
        assert!(TELL.contains(NAME) && TELL.contains("VoiceOver Utility, General") && TELL.contains("VO-Fn-F8"));
        assert!(TELL.ends_with("this voice speaks."));
        assert!(TELL.split_whitespace().count() <= 30, "said aloud, and cut off by the next interrupting line: short");
        assert_eq!(CONFIRM, "Automation Platform now speaks through VoiceOver.");
        let page = page_line(&from_reads(&absent()));
        assert!(page.starts_with(&format!("VoiceOver's own setting \"{NAME}\" — not allowed: the box is not ticked.")), "{page}");
        assert!(page.contains("General pane") && page.contains("VO-Fn-F8") && page.contains("Speak through VoiceOver"));
        assert!(page.contains("administrator's password"));
        assert!(page.contains("does not change it"), "the page says it writes nothing");
        assert!(!page.contains(MARKER) && !page.contains("(-"), "no evidence read out on the page: {page}");
        assert!(page_line(&from_reads(&present())).contains("— allowed: the box is ticked."));
        assert!(page_line(&from_reads(&blind())).contains("— unknown: this application cannot tell; the log says why."));
        assert_eq!(
            env_value(&present()),
            format!("allowed ({MARKER} is there; {KEY}: not set, not counted on macOS 15 and later)")
        );
        assert!(env_value(&blind()).starts_with("unknown (") && env_value(&blind()).contains("Permission denied"));
        assert!(env_value(&absent()).starts_with(&format!("not allowed ({MARKER} is not there;")));
        assert_eq!(reads(Marker::Present, Key::On, true).words(), format!("{MARKER} is there; {KEY}: on"));
        assert_eq!(Scripting::Allowed.word(), "allowed");
        assert!(is_utility(Some("com.apple.VoiceOverUtility"), None));
        assert!(is_utility(None, Some("VoiceOver Utility.app")), "by its bundle's name, should the id be wrong");
        assert!(!is_utility(Some("com.apple.VoiceOver"), Some("VoiceOver.app")), "VoiceOver quitting is not its Utility");
        assert!(!is_utility(None, None));
        assert_eq!((DOMAIN, KEY, KEY_MOVED_IN), ("com.apple.VoiceOver4/default", "SCREnableAppleScript", 15));
    }

    /// The question compiles VoiceOver's terms only once VoiceOver is known to run, and prints
    /// what [`classify`] reads.
    #[test]
    fn the_question_starts_nothing_and_prints_what_is_read() {
        assert!(PROBE[0].contains("is running then"));
        assert!(PROBE[1].starts_with("run script \"tell application id \\\"com.apple.VoiceOver\\\""), "{}", PROBE[1]);
        assert!(PROBE[1].contains("get text under cursor of vo cursor"));
        assert!(!PROBE.join("\n").contains("output"), "the question must not speak");
        assert_eq!(PROBE[2], format!("return \"{ANSWERED}\""));
        assert_eq!(PROBE[4], format!("return \"{NOT_RUNNING}\""));
    }

    /// The tester's case: the box unticked at start. The reads close the gate at once, VoiceOver
    /// confirms, the user is told once; ticking the box is found on the next look, VoiceOver is
    /// asked, and the gate opens with a word through VoiceOver.
    #[test]
    fn the_box_unticked_at_start_is_told_once_and_found_when_ticked() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        assert!(g.open(), "nothing known: as before");
        let out = g.triggered(&absent(), "at start");
        assert!(out.ask && !g.open(), "the reads say not allowed: closed at once");
        assert!(out.log[0].contains("at start") && out.log[0].contains("until it answers"), "{:?}", out.log);
        let out = g.answered(absent(), refused(), t0, true);
        assert_eq!(out.tell, Some(TELL));
        assert!(!out.ask && out.confirm.is_none());
        assert!(out.log.iter().any(|l| l.contains("(-1708)")), "the raw answer: {:?}", out.log);
        assert!(out.log.iter().any(|l| l.contains("does not accept AppleScript") && l.contains("told so once")));
        assert!(!g.open());
        // The looks: 3 s apart, nothing asked while the reads do not change.
        assert!(!g.due(t0 + secs(2)));
        assert!(g.due(t0 + secs(3)));
        assert_eq!(g.looked(&absent(), t0 + secs(3), true), Out::default());
        assert!(!g.due(t0 + secs(5)) && g.due(t0 + secs(6)));
        // A trigger with nothing changed: asked again, but not told again.
        g.triggered(&absent(), "VoiceOver Utility quit");
        let out = g.answered(absent(), refused(), t0 + secs(7), true);
        assert_eq!(out.tell, None, "once per episode");
        assert!(out.log.is_empty(), "nothing new to write: {:?}", out.log);
        // The box ticked: the file appears, the next look asks, the answer opens the gate.
        assert!(g.due(t0 + secs(10)));
        let out = g.looked(&present(), t0 + secs(10), true);
        assert!(out.ask, "the reads changed");
        assert!(!g.open(), "still closed until VoiceOver says so");
        assert!(!g.due(t0 + secs(60)), "not looked at while asking");
        let out = g.answered(present(), Probe::Answered, t0 + secs(11), true);
        assert!(g.open());
        assert_eq!(out.confirm, Some(CONFIRM), "the user was told: now told it works");
        assert!(out.log.iter().any(|l| l.contains("accepts AppleScript now") && l.contains("after 11 s")), "{:?}", out.log);
        assert!(!g.due(t0 + secs(600)), "open: no looks");
        // Unticked again in the same session: told again — a new episode.
        g.triggered(&absent(), "VoiceOver Utility quit");
        assert_eq!(g.answered(absent(), refused(), t0 + secs(700), true).tell, Some(TELL));
    }

    /// The normal case says one line and nothing else.
    #[test]
    fn the_box_ticked_at_start_opens_without_a_word() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        let out = g.triggered(&present(), "at start");
        assert!(out.ask && g.open(), "the reads say allowed: lines wait behind the question, not diverted");
        let out = g.answered(present(), Probe::Answered, t0, true);
        assert_eq!((out.tell, out.confirm, out.ask), (None, None, false));
        assert_eq!(out.log.len(), 2, "the answer, and the verdict: {:?}", out.log);
        assert!(out.log[1].starts_with("VoiceOver accepts AppleScript (VoiceOver answered"));
        g.triggered(&present(), "VoiceOver runs as a new process");
        assert!(g.answered(present(), Probe::Answered, t0 + secs(1), true).log.is_empty(), "said once");
    }

    /// VoiceOver answering its read does not open the gate over an absent file — the user is
    /// told, and the looks go on; and a refusal closes it over a file that is there.
    #[test]
    fn an_answer_never_opens_over_an_absent_file() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        g.triggered(&absent(), "at start");
        assert!(!g.open());
        let out = g.answered(absent(), Probe::Answered, t0, true);
        assert!(!g.open(), "a `get` answered with the box unticked must not bring the silence back");
        assert_eq!(out.tell, Some(TELL));
        assert!(out.log.iter().any(|l| l.contains("does not count against")), "{:?}", out.log);
        assert!(g.due(t0 + secs(3)), "parked: looked at again");
        let mut g = Gate::default();
        g.triggered(&present(), "at start");
        let out = g.answered(present(), refused(), t0, true);
        assert!(!g.open() && out.tell == Some(TELL));
        assert!(out.log.iter().any(|l| l.contains("although the file")), "{:?}", out.log);
    }

    /// An answer that says nothing changes nothing: open stays open, parked stays parked and is
    /// looked at on the pace. Reads that cannot say ask nothing on the looks — VoiceOver is asked
    /// on the triggers.
    #[test]
    fn an_answer_that_says_nothing_changes_nothing() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        g.triggered(&blind(), "at start");
        assert!(g.open());
        let out = g.answered(blind(), Probe::Inconclusive("-1728".into()), t0, true);
        assert!(g.open() && out.tell.is_none());
        assert!(out.log.iter().any(|l| l.contains("cannot be told") && l.contains("as before")), "{:?}", out.log);
        // Parked by a refusal, then nothing says: stays parked, and the looks ask nothing.
        g.triggered(&blind(), "VoiceOver Utility quit");
        g.answered(blind(), refused(), t0 + secs(1), true);
        assert!(!g.open());
        assert!(g.due(t0 + secs(4)));
        assert!(!g.looked(&blind(), t0 + secs(4), true).ask, "no child process every few seconds");
        assert!(g.due(t0 + secs(7)), "but the looks go on");
        // VoiceOver Utility quitting asks.
        assert!(g.triggered(&blind(), "VoiceOver Utility quit").ask);
        let out = g.answered(blind(), Probe::Inconclusive("-609".into()), t0 + secs(8), true);
        assert!(!g.open(), "still parked");
        assert!(out.log.iter().any(|l| l.contains("stay with the system voice")), "{:?}", out.log);
        assert!(g.triggered(&blind(), "VoiceOver Utility quit").ask);
        assert!(g.answered(blind(), Probe::Answered, t0 + secs(9), true).confirm.is_some(), "the reads cannot say: VoiceOver decides");
        assert!(g.open());
        // VoiceOver not running: nothing can be asked.
        let mut g = Gate::default();
        g.triggered(&absent(), "at start");
        g.answered(absent(), refused(), t0, true);
        assert_eq!(g.looked(&present(), t0 + secs(3), false), Out::default());
    }

    /// The pace is the refusals' one: 3 s for five minutes, then 10 s while VoiceOver runs; 3 s
    /// for a minute, then 30 s while it does not.
    #[test]
    fn while_parked_the_reads_are_looked_at_on_the_refusals_pace() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        g.triggered(&absent(), "at start");
        g.answered(absent(), refused(), t0, true);
        let mut t = t0;
        for _ in 0..199 {
            t += secs(3);
            if g.due(t) {
                g.looked(&absent(), t, true);
            }
        }
        // Past five minutes: 10 s apart.
        let t = t0 + secs(600);
        assert!(g.due(t));
        g.looked(&absent(), t, true);
        assert!(!g.due(t + secs(9)) && g.due(t + secs(10)));
        g.looked(&absent(), t + secs(10), false);
        assert!(!g.due(t + secs(39)) && g.due(t + secs(40)), "not running and past a minute: 30 s");
        // Reads that say "not allowed" never ask, changed or not.
        let unreadable_off = reads(Marker::Unreadable("x".into()), Key::Off, true);
        assert!(!g.looked(&unreadable_off, t + secs(40), true).ask);
    }

    /// Switching the setting on puts macOS's Automation question on screen; the box's sentence is
    /// not said over it, and is said once a question finds it answered.
    #[test]
    fn the_user_is_not_told_over_the_automation_question() {
        let t0 = Instant::now();
        let waiting = || unasked(-1744, true);
        let mut g = Gate::default();
        g.triggered(&absent(), "VoiceOver runs as pid 7, not asked yet");
        let out = g.answered(absent(), waiting(), t0, true);
        assert!(!g.open(), "the lines go to the system voice at once");
        assert_eq!(out.tell, None, "not said over macOS's own question");
        assert!(out.log.iter().any(|l| l.contains("once the Automation question")), "{:?}", out.log);
        // The looks ask again although the reads did not change, while the user is owed it.
        assert!(g.due(t0 + secs(3)));
        assert!(g.looked(&absent(), t0 + secs(3), true).ask);
        let out = g.answered(absent(), waiting(), t0 + secs(4), true);
        assert_eq!(out.tell, None);
        assert!(out.log.is_empty(), "said once: {:?}", out.log);
        // Answered: VoiceOver is asked, refuses, and the user is told.
        assert!(g.due(t0 + secs(7)));
        assert!(g.looked(&absent(), t0 + secs(7), true).ask);
        assert_eq!(g.answered(absent(), refused(), t0 + secs(8), true).tell, Some(TELL));
        // No longer owed: the looks ask nothing while the reads say no.
        assert!(g.due(t0 + secs(11)));
        assert!(!g.looked(&absent(), t0 + secs(11), true).ask);
        // Waiting says nothing about the box.
        assert_eq!(verdict(&present(), Some(&waiting())).state, Scripting::Allowed);
        assert!(!waiting().reading().is_empty());
        assert!(waiting().raw().ends_with("[-1744]"));
        // Ticking the setting again forgets what was owed with what was told.
        let mut g = Gate::default();
        g.triggered(&absent(), "at start");
        g.answered(absent(), waiting(), t0, true);
        g.rearmed();
        g.triggered(&absent(), "\"Speak through VoiceOver\" was ticked");
        assert_eq!(g.answered(absent(), refused(), t0 + secs(1), true).tell, Some(TELL));
    }

    /// A new build with the setting on from an earlier run: macOS has forgotten the Automation
    /// grant, nothing has put its question on screen, and the box keeps every line from
    /// VoiceOver, so nothing will. The user is told at once.
    #[test]
    fn automation_never_asked_does_not_hold_the_sentence_back() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        g.triggered(&absent(), "VoiceOver runs as pid 7, not asked yet");
        let out = g.answered(absent(), unasked(-1744, false), t0, true);
        assert_eq!(out.tell, Some(TELL));
        assert!(!g.open());
        assert!(out.log.iter().any(|l| l.contains("not been asked yet")), "the raw answer: {:?}", out.log);
        assert!(!g.looked(&absent(), t0 + secs(3), true).ask, "not owed: nothing asked while the reads say no");
    }

    /// The word through VoiceOver needs VoiceOver's own answer: opened on the reads alone, with
    /// VoiceOver not asked, it says nothing — and the next episode still tells the user.
    #[test]
    fn the_word_through_voiceover_needs_voiceover_s_own_answer() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        g.triggered(&absent(), "at start");
        assert_eq!(g.answered(absent(), refused(), t0, true).tell, Some(TELL));
        assert!(g.looked(&present(), t0 + secs(3), true).ask);
        let out = g.answered(present(), unasked(-1743, false), t0 + secs(4), true);
        assert!(g.open(), "the reads say ticked");
        assert_eq!(out.confirm, None, "VoiceOver would refuse it, and the system voice would say it");
        assert!(out.log.iter().any(|l| l.contains("did not say so (not asked: ") && l.contains("so nothing is said")), "{:?}", out.log);
        g.triggered(&absent(), "VoiceOver Utility quit");
        assert_eq!(g.answered(absent(), refused(), t0 + secs(9), true).tell, Some(TELL), "a new episode");
    }

    /// A trigger while VoiceOver is being asked is asked again once it has answered, not twice at
    /// once; and ticking the setting again starts a new episode.
    #[test]
    fn a_trigger_while_asking_is_asked_again_after_and_a_tick_is_a_new_episode() {
        let t0 = Instant::now();
        let mut g = Gate::default();
        assert!(g.triggered(&present(), "at start").ask);
        let out = g.triggered(&absent(), "VoiceOver Utility quit");
        assert!(!out.ask && out.log.is_empty(), "one question at a time");
        assert!(!g.open(), "but the reads still close the gate at once");
        let out = g.answered(present(), Probe::Answered, t0, true);
        assert!(out.ask, "the trigger that came meanwhile is asked now");
        assert!(g.open());
        assert!(!g.triggered(&absent(), "asked again").log.is_empty());
        assert_eq!(g.answered(absent(), refused(), t0 + secs(1), true).tell, Some(TELL));
        g.triggered(&absent(), "VoiceOver Utility quit");
        assert_eq!(g.answered(absent(), refused(), t0 + secs(2), true).tell, None);
        g.rearmed();
        g.triggered(&absent(), "\"Speak through VoiceOver\" was ticked");
        assert_eq!(g.answered(absent(), refused(), t0 + secs(3), true).tell, Some(TELL), "a deliberate act: told again");
        // An answer with no question under way (cannot happen, but must not panic).
        let out = Gate::default().answered(present(), Probe::Answered, t0, true);
        assert!(out.log.iter().any(|l| l.contains("(asked)")), "{:?}", out.log);
    }
}
