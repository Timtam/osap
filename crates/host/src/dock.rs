//! Who currently wants the application to look like a regular, Dock-visible application.
//!
//! macOS only in effect, deliberately not in structure. There, the application runs as an
//! agent with no Dock icon and no entry in the application switcher; a window that opens has
//! to promote it, and the last window to close has to demote it again. With one window that
//! is a pair of calls. With two it is a refcount, and getting it wrong is not cosmetic: the
//! manager window used to demote unconditionally when it was hidden, which stripped the Dock
//! icon out from under an error window that was still on screen — reproducing, on the
//! platform nobody here can test, exactly the unreachable-window bug the error frame was
//! written to fix.
//!
//! Keyed by owner rather than counted, so a second `want` from the same window is not a
//! second claim. The bookkeeping is compiled everywhere and only the policy call is gated,
//! because a rule that exists only on the platform nobody can run is a rule nobody can test.
//!
//! The icon is taken away only if the first claim put it there. Whether it does is the setting
//! `dock_while_open`, asked at the first claim and not again at the last release: the setting
//! applies as it is ticked, so a release that asked it would find it off after it was turned off
//! with the manager open, and leave the icon and the Command-Tab entry until the application
//! quits — no later open or close would take them away. So the first claim keeps what the policy
//! call answered, and the last release demotes on that alone.
//!
//! Main thread only. The claims are kept per thread, and the policy call refuses any other
//! thread (`backend::set_regular`), so a claim made elsewhere would be one the main thread's
//! release never sees. Every caller is on it: `gui.rs`'s windows, and the Mac's app page in the
//! backend.
//!
//! A file of its own rather than a corner of `gui.rs`, because not every window that claims the
//! icon is wx's: the Mac's app page (stage 4, TODO.md) is the macOS backend's, and the backend
//! cannot reach into the file that talks to wxdragon. Nothing here does, so `macos-check` borrows
//! it, and its macOS half — the setting it asks and the backend's policy call — meets the
//! compiler for that target before it meets a Mac.

use std::cell::RefCell;
use std::collections::BTreeSet;

/// The windows holding a claim, and whether the first of them put the icon there.
struct Claims {
    wanters: BTreeSet<&'static str>,
    /// What the policy call answered for the first claim: true only when the setting was on
    /// then and AppKit accepted. Never true on Windows, where nothing is called.
    promoted: bool,
}

thread_local! {
    static CLAIMS: RefCell<Claims> =
        const { RefCell::new(Claims { wanters: BTreeSet::new(), promoted: false }) };
}

/// Records `owner` as wanting the icon. True when this is the FIRST wanter.
fn add(owner: &'static str) -> bool {
    CLAIMS.with(|c| {
        let mut c = c.borrow_mut();
        let was_empty = c.wanters.is_empty();
        let is_new = c.wanters.insert(owner);
        was_empty && is_new
    })
}

/// Keeps what promoting answered for the first wanter, for the last release to go by.
fn note_promoted(promoted: bool) {
    CLAIMS.with(|c| c.borrow_mut().promoted = promoted);
}

/// Drops `owner`. True when that was the LAST wanter and the first one put the icon there —
/// never true for an owner that was not holding a claim, which is what stops a stray release
/// from demoting, and never for an icon no claim put there.
fn release_owner(owner: &'static str) -> bool {
    CLAIMS.with(|c| {
        let mut c = c.borrow_mut();
        let had = c.wanters.remove(owner);
        if !(had && c.wanters.is_empty()) {
            return false;
        }
        std::mem::take(&mut c.promoted)
    })
}

pub fn want(owner: &'static str, why: &str) {
    if add(owner) {
        note_promoted(promote(why));
    }
}

pub fn release(owner: &'static str, why: &str) {
    if release_owner(owner) {
        demote(why);
    }
}

/// Whether the icon is there now because of this call.
#[cfg(target_os = "macos")]
fn promote(why: &str) -> bool {
    crate::appcfg::dock_while_open() && crate::backend::set_regular(true, why)
}

#[cfg(not(target_os = "macos"))]
fn promote(_why: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
fn demote(why: &str) {
    crate::backend::set_regular(false, why);
}

#[cfg(not(target_os = "macos"))]
fn demote(_why: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_window_out_turns_the_icon_off_and_no_earlier_one_does() {
        // Two windows want it; the first release must NOT demote.
        assert!(add("manager"), "the first wanter promotes");
        note_promoted(true);
        assert!(!add("errors"), "the second does not promote again");
        assert!(!release_owner("manager"), "hiding the manager must not demote");
        assert!(release_owner("errors"), "the last one out demotes");
        // A second window asking twice is still one claim.
        assert!(add("errors"));
        note_promoted(true);
        assert!(!add("errors"));
        assert!(release_owner("errors"), "one release clears one claim");
        // And a release from someone who never asked demotes nothing.
        assert!(!release_owner("manager"));
    }

    #[test]
    fn only_an_icon_the_first_claim_put_there_is_taken_away() {
        // Promoted with the setting on, then the setting turned off with the manager open: the
        // release does not ask the setting, so the icon goes with the window.
        assert!(add("manager"));
        note_promoted(true);
        assert!(release_owner("manager"), "the icon the claim put there goes with it");
        // The setting off at the first claim, or the policy call refused: there is nothing to
        // take away, whatever the setting says by the time the last window goes.
        assert!(add("manager"));
        note_promoted(false);
        assert!(!add("errors"));
        assert!(!release_owner("manager"));
        assert!(!release_owner("errors"), "no claim put the icon there, so none demotes");
        // The next first claim starts afresh rather than inheriting the last one's answer.
        assert!(add("errors"));
        note_promoted(true);
        assert!(release_owner("errors"));
    }
}
