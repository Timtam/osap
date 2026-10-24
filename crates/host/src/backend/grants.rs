//! What the Permissions page and the start announcement say about the permissions: each line,
//! the head that counts them, what a second look found changed, and the sentence the application
//! starts with. Words only, from the list `backend::permissions` gives — the page itself is in
//! `gui.rs`, which needs wxWidgets, and this is what of it can be tested where it is written.
//!
//! **What counts.** Only the permissions the application needs: those whose `blocking` is set.
//! Input Monitoring is optional until the run shows captured keys not reaching the overlay, and
//! Automation always is (`macos/evidence.rs`); a page that counted them as missing told a tester
//! whose overlay took its keys perfectly well that the application could not work yet.
//!
//! **Why a second look says what changed.** The page reads the states again whenever it comes
//! forward, and relabels its lines in place. A line that changed while a screen reader's focus
//! is elsewhere on the page is not read out by anybody, so what changed is said, once, as an
//! announcement — its state, whether it is needed, or, where neither changed, what decided it —
//! and nothing is said when nothing changed, which is most of the time.

use super::{Grant, Permission};

/// One permission as the page last showed it, for [`changes`]: everything its line is made of
/// that can change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub name: &'static str,
    pub state: Grant,
    pub blocking: bool,
    pub judged: Option<String>,
}

/// What the page shows now, to compare the next look with.
pub fn shown(perms: &[Permission]) -> Vec<Shown> {
    perms
        .iter()
        .map(|p| Shown { name: p.name, state: p.state, blocking: p.blocking, judged: p.judged.clone() })
        .collect()
}

/// One permission's line: its name and state — and that it is optional where it is — then what
/// decided it where something was weighed, then what it is for.
pub fn line(p: &Permission) -> String {
    let optional = if p.blocking { "" } else { ", optional" };
    let stale = p.stale.as_ref().map(|s| format!(" {s}")).unwrap_or_default();
    match &p.judged {
        Some(judged) => format!("{} — {}{optional}. {judged} {}{stale}", p.name, p.state.word(), p.without),
        None => format!("{} — {}{optional}. {}{stale}", p.name, p.state.word(), p.without),
    }
}

/// How many of the permissions the application needs are granted: the first sentence of the head,
/// and what `Re-check now` says.
pub fn count(perms: &[Permission]) -> String {
    let needed = perms.iter().filter(|p| p.blocking).count();
    let done = perms.iter().filter(|p| p.blocking && p.state == Grant::Granted).count();
    format!("Granted: {done} of the {needed} permissions this application needs.")
}

/// The page's head: the count, which of the rest are optional, and that none of them is the
/// application's to grant.
pub fn head(perms: &[Permission]) -> String {
    let optional: Vec<&str> = perms.iter().filter(|p| !p.blocking).map(|p| p.name).collect();
    let optional = match optional.as_slice() {
        [] => String::new(),
        names => format!(" Optional: {}.", and_list(names)),
    };
    format!("{}{optional} None of these can be granted by the application itself.", count(perms))
}

/// The permissions the application needs and does not have.
pub fn missing(perms: &[Permission]) -> Vec<&'static str> {
    perms.iter().filter(|p| p.blocking && p.state == Grant::Missing).map(|p| p.name).collect()
}

/// What changed since `before`, a sentence each, in the page's order: a state, and whether it is
/// needed — or, where neither changed and its line did, what decided it now, which is all of the
/// line that can change besides. Empty when nothing did.
pub fn changes(before: &[Shown], perms: &[Permission]) -> Vec<String> {
    let mut said = Vec::new();
    for p in perms {
        let Some(was) = before.iter().find(|b| b.name == p.name) else { continue };
        if was.state != p.state {
            said.push(format!("{}: now {}.", p.name, p.state.word()));
        }
        // Said only where nothing else is: a new state or a new need is the news, and the line
        // has the reason beside it.
        if was.state == p.state && was.blocking == p.blocking && was.judged != p.judged {
            if let Some(judged) = &p.judged {
                said.push(format!("{}: {judged}", p.name));
            }
        }
        if was.blocking != p.blocking {
            said.push(if p.blocking {
                format!("{} is needed now.", p.name)
            } else {
                format!("{} is optional again.", p.name)
            });
        }
    }
    said
}

/// What `Re-check now` says: the count, then what changed, or that nothing did — and, for each
/// permission still missing after its pane was opened that may have an earlier copy's entry, what
/// to do about that entry. "Nothing has changed" alone, said twice over two whole sessions of the
/// second Mac test while Accessibility sat switched on for an earlier build, left nothing to try.
pub fn rechecked(before: &[Shown], perms: &[Permission]) -> String {
    let changed = changes(before, perms);
    let tail = if changed.is_empty() { "Nothing has changed.".to_string() } else { changed.join(" ") };
    let stale: String = perms
        .iter()
        .filter(|p| p.state == Grant::Missing && p.pane_opened)
        .filter_map(|p| p.stale.as_ref().map(|s| format!(" {}: {s}", p.name)))
        .collect();
    format!("{} {tail}{stale}", count(perms))
}

/// What the page says when the answer to a request its button made comes in: what changed since
/// `before`, and then — only where the system did not answer granted — the answer's own `say`, the
/// next thing to try. Never "press this button again" for a permission that is granted.
pub fn answered(before: &[Shown], perms: &[Permission], answer: &super::PermissionAnswer) -> Vec<String> {
    let mut said = changes(before, perms);
    if !answer.granted {
        if let Some(say) = answer.say {
            said.push(say.to_string());
        }
    }
    said
}

/// The sentence the application starts with, and whether the Permissions page comes forward
/// with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Start {
    pub text: String,
    pub open_page: bool,
}

/// The start: `hint` when everything the application needs is granted, otherwise what is missing,
/// what decided it, and in which order to grant it — and either way, after it, `translocated`
/// (`portable::Translocation::spoken`). The page comes forward when something needed is missing,
/// and when `loads_nothing`: a translocated copy whose original was not found loads no module.
pub fn start(
    perms: &[Permission],
    hint: &str,
    translocated: Option<&str>,
    loads_nothing: bool,
) -> Start {
    let missing = missing(perms);
    let moved = translocated.map(|t| format!(" {t}")).unwrap_or_default();
    if missing.is_empty() {
        return Start { text: format!("{hint}{moved}"), open_page: loads_nothing };
    }
    // Why each was judged missing, where more than the system's check was weighed: Screen
    // Recording's check has been caught answering "not granted" for an application that captured
    // perfectly well, so the evidence is said, not only the verdict.
    let judged: String = perms
        .iter()
        .filter(|p| missing.contains(&p.name))
        .filter_map(|p| p.judged.as_ref().map(|j| format!(" {}: {j}", p.name)))
        .collect();
    // The order matters and the tester found out how: on macOS 14.5 this application did not
    // appear in the Screen Recording list at all until Accessibility had been granted, after
    // which it was there. It is also the order the application asks in — Screen Recording the
    // moment Accessibility is granted, so that the two system dialogs never stand on top of each
    // other (backend/macos/perm.rs). Said here, because the person setting this up is looking for
    // a switch that is not on the pane yet.
    let order = if missing.contains(&"Accessibility") && missing.contains(&"Screen Recording") {
        " Grant Accessibility first: Screen Recording is asked for as soon as it is, and asking is \
         what normally puts this application into the Screen Recording list. On newer macOS that \
         list is called Screen and System Audio Recording."
    } else if missing.contains(&"Screen Recording") {
        // Accessibility is granted, so the request goes out on the first tick, right after this:
        // its dialog is the next thing on screen.
        " macOS may ask for Screen Recording in a dialog of its own in a moment; its button that \
         opens the settings leads to the list."
    } else {
        ""
    };
    Start {
        text: format!(
            "Automation Platform cannot work yet: {} {} not been granted.{judged} The Permissions \
             page is open, and lists what to do.{order}{moved}",
            and_list(&missing),
            if missing.len() == 1 { "has" } else { "have" }
        ),
        open_page: true,
    }
}

/// "A", "A and B", "A, B and C".
fn and_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &'static str, state: Grant, blocking: bool, judged: Option<&str>) -> Permission {
        Permission {
            name,
            state,
            without: "What it is for.",
            judged: judged.map(str::to_string),
            anchor: "",
            can_ask: true,
            blocking,
            stale: None,
            pane_opened: false,
        }
    }

    /// The four as the first session of the Avenger test had them once Accessibility was granted:
    /// Screen Recording judged granted by its captures, Input Monitoring denied and optional.
    fn first_session() -> Vec<Permission> {
        vec![
            p("Accessibility", Grant::Granted, true, None),
            p("Screen Recording", Grant::Granted, true, Some("Judged by a capture.")),
            p("Input Monitoring", Grant::Missing, false, Some("Keys an overlay took have reached it.")),
            p("Automation, for VoiceOver", Grant::Unknown, false, None),
        ]
    }

    #[test]
    fn optional_permissions_are_not_counted_or_missing() {
        let perms = first_session();
        assert_eq!(count(&perms), "Granted: 2 of the 2 permissions this application needs.");
        assert!(missing(&perms).is_empty());
        let head = head(&perms);
        assert!(head.contains("Optional: Input Monitoring and Automation, for VoiceOver."), "{head}");
        assert!(head.ends_with("None of these can be granted by the application itself."), "{head}");
    }

    #[test]
    fn a_line_says_its_state_what_decided_and_what_it_is_for() {
        let perms = first_session();
        assert_eq!(
            line(&perms[1]),
            "Screen Recording — granted. Judged by a capture. What it is for."
        );
        assert_eq!(
            line(&perms[2]),
            "Input Monitoring — NOT granted, optional. Keys an overlay took have reached it. What it is for."
        );
        assert_eq!(line(&perms[0]), "Accessibility — granted. What it is for.");
    }

    #[test]
    fn the_start_names_only_what_is_needed_and_missing() {
        // The first session's start: Accessibility not yet granted, Screen Recording missing by
        // the evidence there was, Input Monitoring denied — and not named.
        let perms = vec![
            p("Accessibility", Grant::Missing, true, None),
            p("Screen Recording", Grant::Missing, true, Some("Judged by the system's own check alone.")),
            p("Input Monitoring", Grant::Missing, false, None),
            p("Automation, for VoiceOver", Grant::Unknown, false, None),
        ];
        let s = start(&perms, "Hint.", None, false);
        assert!(s.open_page);
        assert!(
            s.text.starts_with(
                "Automation Platform cannot work yet: Accessibility and Screen Recording have not been granted."
            ),
            "{}",
            s.text
        );
        assert!(!s.text.contains("Input Monitoring"), "{}", s.text);
        assert!(s.text.contains("Screen Recording: Judged by the system's own check alone."), "{}", s.text);
        assert!(s.text.contains("Grant Accessibility first"), "{}", s.text);

        let s = start(&first_session(), "Hint.", None, false);
        assert_eq!(s, Start { text: "Hint.".into(), open_page: false });
    }

    #[test]
    fn screen_recording_alone_says_its_dialog_is_coming() {
        let perms = vec![
            p("Accessibility", Grant::Granted, true, None),
            p("Screen Recording", Grant::Missing, true, None),
        ];
        let s = start(&perms, "Hint.", None, false);
        assert!(s.text.contains("has not been granted") && s.text.contains("in a dialog of its own"), "{}", s.text);
        assert!(!s.text.contains("Grant Accessibility first"), "{}", s.text);
    }

    #[test]
    fn input_monitoring_shown_to_be_needed_is_named() {
        let perms = vec![
            p("Accessibility", Grant::Granted, true, None),
            p("Screen Recording", Grant::Granted, true, None),
            p("Input Monitoring", Grant::Missing, true, Some("Needed now.")),
        ];
        assert_eq!(missing(&perms), ["Input Monitoring"]);
        let s = start(&perms, "Hint.", None, false);
        assert!(s.text.contains("Input Monitoring has not been granted. Input Monitoring: Needed now."), "{}", s.text);
        assert!(!s.text.contains("Screen Recording"), "{}", s.text);
    }

    #[test]
    fn a_translocated_start_says_so_after_everything_else() {
        let s = start(&first_session(), "Hint.", Some("Runs from a copy."), false);
        assert_eq!(s, Start { text: "Hint. Runs from a copy.".into(), open_page: false });
        // A copy whose original was not found loads no module: the page comes forward.
        assert!(start(&first_session(), "Hint.", Some("No modules."), true).open_page);
        let mut perms = first_session();
        perms[0].state = Grant::Missing;
        let s = start(&perms, "Hint.", Some("Runs from a copy."), false);
        assert!(s.text.ends_with(" Runs from a copy."), "{}", s.text);
    }

    #[test]
    fn a_second_look_says_what_changed_and_nothing_else() {
        let mut perms = first_session();
        perms[1].state = Grant::Missing;
        let before = shown(&perms);
        assert!(changes(&before, &perms).is_empty());
        assert_eq!(
            rechecked(&before, &perms),
            "Granted: 1 of the 2 permissions this application needs. Nothing has changed."
        );
        perms[1].state = Grant::Granted;
        perms[2].blocking = true;
        assert_eq!(
            changes(&before, &perms),
            ["Screen Recording: now granted.", "Input Monitoring is needed now."]
        );
        let after = shown(&perms);
        perms[2].blocking = false;
        assert_eq!(changes(&after, &perms), ["Input Monitoring is optional again."]);
    }

    /// The second Mac test's finding 1: Accessibility switched on for an earlier ad-hoc build.
    /// Its line says what to do about that entry; Re-check now says it once the pane was opened
    /// and the permission is still missing — not before, and not once it is granted.
    #[test]
    fn an_earlier_copys_entry_is_said_on_the_line_and_by_recheck_after_the_pane() {
        let stale = "If it is in the list and on already, that entry is an earlier copy's.";
        let mut perms = first_session();
        perms[0].state = Grant::Missing;
        perms[0].stale = Some(stale.into());
        assert_eq!(line(&perms[0]), format!("Accessibility — NOT granted. What it is for. {stale}"));
        let before = shown(&perms);
        assert_eq!(
            rechecked(&before, &perms),
            "Granted: 1 of the 2 permissions this application needs. Nothing has changed.",
            "the pane not opened yet"
        );
        perms[0].pane_opened = true;
        assert_eq!(
            rechecked(&before, &perms),
            format!("Granted: 1 of the 2 permissions this application needs. Nothing has changed. Accessibility: {stale}")
        );
        perms[0].state = Grant::Granted;
        assert_eq!(
            rechecked(&before, &perms),
            "Granted: 2 of the 2 permissions this application needs. Accessibility: now granted."
        );
    }

    /// Finding 19: the Input Monitoring button's request answered granted at once, and the page had
    /// already said to press it again. The page says what the answer changed, and the next thing to
    /// try only where it did not answer granted.
    #[test]
    fn a_requests_answer_says_what_changed_and_never_press_again_once_granted() {
        let mut perms = first_session();
        let before = shown(&perms);
        perms[2].state = Grant::Granted;
        let granted = crate::backend::PermissionAnswer { name: "Input Monitoring", granted: true, say: Some("Press again.") };
        assert_eq!(answered(&before, &perms, &granted), ["Input Monitoring: now granted."]);
        let not = crate::backend::PermissionAnswer { granted: false, ..granted };
        let before = shown(&perms);
        perms[2].state = Grant::Missing;
        assert_eq!(answered(&before, &perms, &not), ["Input Monitoring: now NOT granted.", "Press again."]);
        let before = shown(&perms);
        assert_eq!(answered(&before, &perms, &not), ["Press again."], "nothing changed: the next thing to try");
    }

    #[test]
    fn a_line_whose_reason_alone_changed_is_said_too() {
        // Input Monitoring after the first overlay key, Screen Recording after its first capture:
        // the state stays, the line changes, and Re-check now does not say nothing has changed.
        let mut perms = first_session();
        perms[2].judged = Some("No overlay has taken a key in this run yet.".into());
        let before = shown(&perms);
        perms[2].judged = Some("Keys an overlay took have reached it in this run.".into());
        perms[1].judged = Some("Judged by a capture: the screen came back in a picture.".into());
        assert_eq!(
            changes(&before, &perms),
            [
                "Screen Recording: Judged by a capture: the screen came back in a picture.",
                "Input Monitoring: Keys an overlay took have reached it in this run.",
            ]
        );
        assert!(!rechecked(&before, &perms).contains("Nothing has changed."));
        // With the state or the need changed as well, that is what is said.
        perms[2].state = Grant::Granted;
        assert_eq!(changes(&before, &perms)[1], "Input Monitoring: now granted.");
        assert_eq!(changes(&before, &perms).len(), 2);
    }

    #[test]
    fn lists_read_as_sentences() {
        assert_eq!(and_list(&[]), "");
        assert_eq!(and_list(&["A"]), "A");
        assert_eq!(and_list(&["A", "B"]), "A and B");
        assert_eq!(and_list(&["A", "B", "C"]), "A, B and C");
    }
}
