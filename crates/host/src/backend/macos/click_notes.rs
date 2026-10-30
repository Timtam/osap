//! What the log says of a click made through the system's input, and of an application that comes
//! to the front after one — F7 of the second Mac tester's log (2026-10-30).
//!
//! Ten times of ten there, an overlay's click that opens a plug-in's menu was followed within the
//! second by another application in front that named no window, and the overlay let go of its
//! keys. Two readings fit, and they want opposite fixes: the click landed on a window of that
//! application's drawn over the plug-in, which took it; or the plug-in's popup brought that
//! application forward with nothing of its under the point. These lines tell the two apart in the
//! next log, for any application — none is named here:
//!
//! - at each click through the input, before it is posted: the window the system's hit test names
//!   at the point — its number, its application, its layer — and whether it is the window the click
//!   is for ([`click_line`]);
//! - when an application comes to the front naming no window: its activation policy, its windows on
//!   screen — number, layer, place, alpha, name — and which of them holds the last click's point
//!   ([`windowless_line`]);
//! - at the first activation after a click: how long after it ([`after_click_line`]).
//!
//! They decide nothing. What they read is the window server's list of windows, metadata and never a
//! picture of the screen: one copy of it per click, and one per such activation. A window's name may
//! be a user's own text — a document's title — and is written as it is: it stays in that user's log.
//!
//! Its own file for the reason `post_click.rs` has one: nothing macOS in it, so `backend/mod.rs`
//! borrows it by `#[path]` under `cfg(test)`, and it runs where it is written.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

/// The window the hit test names at a click's point, as the window server's list of windows on
/// screen has it.
pub(crate) struct Hit {
    pub number: u32,
    /// Its application's pid and name, and its layer; `None` when the list does not carry the
    /// number.
    pub owner: Option<(i32, String, i32)>,
}

/// What a click is for.
pub(crate) enum For {
    /// The window the module named (`host.input.click`'s `window`): its handle, and the window
    /// server's number of the window it is or lies in — 0 when it could not be paired with one.
    Window { handle: isize, number: u32 },
    /// No window named: the application in front, as the workspace has it, by pid and name; `None`
    /// when none is.
    Front(Option<(i32, String)>),
}

/// The line written at a click through the input at (x, y), before it is posted: where the system
/// would deliver it — `hit`, `None` where the hit test names no window — and whether that is what
/// the click is for.
pub(crate) fn click_line(x: i32, y: i32, hit: Option<&Hit>, for_: &For) -> String {
    let Some(Hit { number: hit, owner }) = hit else {
        return format!("click at {x},{y}: the system's hit test names no window there; {}", unnamed(for_));
    };
    let lands = match owner {
        None => format!("lands in window {hit}, which the window server's list of windows on screen does not carry"),
        Some((pid, exe, layer)) => format!("lands in window {hit} of '{exe}' (pid {pid}, layer {layer})"),
    };
    let relation = match for_ {
        For::Window { handle, number: 0 } => {
            format!(" — window {handle} it is for could not be paired with the window server's, so whether it is that one is not known")
        }
        For::Window { handle, number } if number == hit => format!(": window {handle}, the one it is for"),
        For::Window { handle, number } => format!(", not in window {handle} it is for (the window server's {number})"),
        For::Front(front) => match (owner, front) {
            (Some((pid, ..)), Some((front, _))) if pid == front => format!(", a window of the application in front; {}", unnamed(for_)),
            (Some(_), Some(_)) => format!(", not a window of the application in front; {}", unnamed(for_)),
            _ => format!("; {}", unnamed(for_)),
        },
    };
    format!("click at {x},{y} {lands}{relation}")
}

/// The words for a click that names no window, or for one that does where nothing was hit.
fn unnamed(for_: &For) -> String {
    match for_ {
        For::Window { handle, number } => format!("it is for window {handle} (the window server's {number})"),
        For::Front(Some((pid, exe))) => format!("no window was named, and the application in front is '{exe}' (pid {pid})"),
        For::Front(None) => "no window was named, and no application is in front".to_string(),
    }
}

/// A window of an application's on screen, as the window server's list has it: its number, its
/// layer, its place in points from the top-left of the main display, its alpha, and its name — `None`
/// where the list gives none (without Screen Recording, the system withholds every other
/// application's).
pub(crate) struct Listed {
    pub number: u32,
    pub layer: i32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub alpha: f64,
    pub name: Option<String>,
}

impl Listed {
    /// Whether the point is inside the window's rectangle: its left and top edges in, its right and
    /// bottom edges out.
    fn holds(&self, x: i32, y: i32) -> bool {
        let (x, y) = (f64::from(x), f64::from(y));
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// The windows a line names, at most; the rest are counted.
pub(crate) const LISTED_MAX: usize = 12;
/// A window's name is quoted to this many characters.
pub(crate) const NAME_MAX: usize = 60;

/// An application's activation policy, in words: `NSApplicationActivationPolicy`'s value — 0 an
/// ordinary application with a Dock icon and a menu bar, 1 an accessory one, which can come to the
/// front with neither, 2 one that never should.
pub(crate) fn policy_words(policy: isize) -> String {
    match policy {
        0 => "regular".to_string(),
        1 => "accessory".to_string(),
        2 => "prohibited".to_string(),
        n => format!("unknown ({n})"),
    }
}

/// The line written when `app` (`pid`) comes to the front naming no window: its activation policy
/// (`policy_words`), its windows on screen, front-most first, and which of them hold `click`, the
/// last click's point — `None` when this application has made none.
pub(crate) fn windowless_line(app: &str, pid: i32, policy: &str, windows: &[Listed], click: Option<(i32, i32)>) -> String {
    let mut line = format!("{app} (pid {pid}) came to the front naming no window — activation policy {policy}; ");
    if windows.is_empty() {
        line += "no window of it on screen";
    } else {
        let each: Vec<String> = windows
            .iter()
            .take(LISTED_MAX)
            .map(|w| {
                let name = match &w.name {
                    None => "no name given".to_string(),
                    Some(n) if n.chars().count() > NAME_MAX => format!("named '{}…'", n.chars().take(NAME_MAX).collect::<String>()),
                    Some(n) => format!("named '{n}'"),
                };
                format!(
                    "{} layer {} at {:.0},{:.0} {:.0}x{:.0} alpha {:.2} {name}",
                    w.number, w.layer, w.x, w.y, w.w, w.h, w.alpha
                )
            })
            .collect();
        line += &format!("{} window(s) on screen: {}", windows.len(), each.join(", "));
        if windows.len() > LISTED_MAX {
            line += &format!(", and {} more", windows.len() - LISTED_MAX);
        }
    }
    match click {
        None => line += "; this application has made no click through the input",
        Some((x, y)) => {
            let holding: Vec<String> = windows.iter().filter(|w| w.holds(x, y)).map(|w| w.number.to_string()).collect();
            if holding.is_empty() {
                line += &format!("; the last click, at {x},{y}, is in none of them");
            } else {
                line += &format!("; the last click, at {x},{y}, is in {}", holding.join(", "));
            }
        }
    }
    line
}

/// The line written at the first activation after a click: `app` (`pid`) came to the front `ago`
/// after this application's click at (x, y).
pub(crate) fn after_click_line(app: &str, pid: i32, ago: Duration, x: i32, y: i32) -> String {
    format!("{app} (pid {pid}) came to the front {} ms after this application's click at {x},{y}", ago.as_millis())
}

/// The last click through the input: its point and its moment, as time since an origin the caller
/// keeps, for the activations that follow it. Atomics, so that any thread may note one: the point is
/// written before the moment, and read after it.
pub(crate) struct LastClick {
    /// x in the high 32 bits, y in the low.
    point: AtomicU64,
    /// Microseconds since the origin, plus one: 0 is no click yet.
    at: AtomicU64,
    /// Not yet told to an activation ([`LastClick::take`]).
    fresh: AtomicBool,
}

impl LastClick {
    pub(crate) const fn new() -> LastClick {
        LastClick { point: AtomicU64::new(0), at: AtomicU64::new(0), fresh: AtomicBool::new(false) }
    }

    /// A click at (x, y), `since` the origin.
    pub(crate) fn note(&self, x: i32, y: i32, since: Duration) {
        self.point.store((u64::from(x as u32) << 32) | u64::from(y as u32), Ordering::Relaxed);
        self.at.store((since.as_micros() as u64).saturating_add(1), Ordering::Release);
        self.fresh.store(true, Ordering::Release);
    }

    /// The last click's point, told or not; `None` before the first.
    pub(crate) fn last(&self) -> Option<(i32, i32)> {
        if self.at.load(Ordering::Acquire) == 0 {
            return None;
        }
        Some(self.unpacked())
    }

    /// The last click's point and how long before `now` (since the origin) it was made — once per
    /// click: the first activation after it is told, and none after that.
    pub(crate) fn take(&self, now: Duration) -> Option<(i32, i32, Duration)> {
        if !self.fresh.swap(false, Ordering::AcqRel) {
            return None;
        }
        let at = Duration::from_micros(self.at.load(Ordering::Acquire).saturating_sub(1));
        let (x, y) = self.unpacked();
        Some((x, y, now.saturating_sub(at)))
    }

    fn unpacked(&self) -> (i32, i32) {
        let p = self.point.load(Ordering::Relaxed);
        ((p >> 32) as u32 as i32, p as u32 as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(number: u32, pid: i32, exe: &str, layer: i32) -> Hit {
        Hit { number, owner: Some((pid, exe.to_string(), layer)) }
    }

    /// The click's line names the window the hit test found and says whether it is the one the
    /// click is for — by the module's window, or, with none named, by the application in front.
    #[test]
    fn a_clicks_line_says_where_it_lands_and_whether_that_is_what_it_is_for() {
        let named = For::Window { handle: 12, number: 98 };
        assert_eq!(
            click_line(356, 131, Some(&hit(98, 512, "REAPER", 0)), &named),
            "click at 356,131 lands in window 98 of 'REAPER' (pid 512, layer 0): window 12, the one it is for"
        );
        assert_eq!(
            click_line(356, 131, Some(&hit(4711, 703, "Helper", 25)), &named),
            "click at 356,131 lands in window 4711 of 'Helper' (pid 703, layer 25), not in window 12 it is for (the window server's 98)"
        );
        assert_eq!(
            click_line(356, 131, Some(&hit(98, 512, "REAPER", 0)), &For::Window { handle: 12, number: 0 }),
            "click at 356,131 lands in window 98 of 'REAPER' (pid 512, layer 0) — window 12 it is for could not be paired with the \
             window server's, so whether it is that one is not known"
        );
        let front = For::Front(Some((512, "REAPER".to_string())));
        assert_eq!(
            click_line(413, 143, Some(&hit(98, 512, "REAPER", 0)), &front),
            "click at 413,143 lands in window 98 of 'REAPER' (pid 512, layer 0), a window of the application in front; no window \
             was named, and the application in front is 'REAPER' (pid 512)"
        );
        assert_eq!(
            click_line(413, 143, Some(&hit(4711, 703, "Helper", 25)), &front),
            "click at 413,143 lands in window 4711 of 'Helper' (pid 703, layer 25), not a window of the application in front; no \
             window was named, and the application in front is 'REAPER' (pid 512)"
        );
        assert_eq!(
            click_line(1, 2, Some(&Hit { number: 77, owner: None }), &For::Front(None)),
            "click at 1,2 lands in window 77, which the window server's list of windows on screen does not carry; no window was \
             named, and no application is in front"
        );
        assert_eq!(
            click_line(1, 2, None, &named),
            "click at 1,2: the system's hit test names no window there; it is for window 12 (the window server's 98)"
        );
    }

    /// An application in front naming no window: its policy, its windows front-most first — each
    /// by number, layer, place, alpha and name — and which of them hold the last click; a long list
    /// and a long name are cut, and said to be.
    #[test]
    fn a_windowless_activation_lists_its_windows_and_the_one_holding_the_click() {
        let w = |number: u32, x: f64, y: f64, name: Option<&str>| Listed {
            number,
            layer: 25,
            x,
            y,
            w: 600.0,
            h: 40.0,
            alpha: 1.0,
            name: name.map(str::to_string),
        };
        let windows = [w(4711, 270.0, 98.0, Some("selected, has keyboard focus")), Listed { h: 120.0, ..w(4712, 0.0, 0.0, None) }];
        assert_eq!(
            windowless_line("Helper", 703, &policy_words(1), &windows, Some((356, 131))),
            "Helper (pid 703) came to the front naming no window — activation policy accessory; 2 window(s) on screen: 4711 \
             layer 25 at 270,98 600x40 alpha 1.00 named 'selected, has keyboard focus', 4712 layer 25 at 0,0 600x120 alpha 1.00 \
             no name given; the last click, at 356,131, is in 4711"
        );
        assert!(windowless_line("Helper", 703, "accessory", &windows, Some((5, 5))).ends_with("the last click, at 5,5, is in 4712"));
        assert!(windowless_line("Helper", 703, "accessory", &windows, Some((870, 131))).ends_with("is in none of them"), "the right edge is out");
        assert!(windowless_line("Helper", 703, "accessory", &windows, Some((300, 100))).ends_with("is in 4711, 4712"), "both");
        assert_eq!(
            windowless_line("Agent", 9, &policy_words(2), &[], None),
            "Agent (pid 9) came to the front naming no window — activation policy prohibited; no window of it on screen; this \
             application has made no click through the input"
        );
        let many: Vec<Listed> = (0..14).map(|n| w(n, 0.0, 0.0, Some(&"x".repeat(70)))).collect();
        let line = windowless_line("Helper", 703, "regular", &many, None);
        assert!(line.contains("14 window(s) on screen: 0 layer 25"), "{line}");
        assert!(line.contains(&format!("named '{}…'", "x".repeat(NAME_MAX))), "{line}");
        assert!(line.contains(", and 2 more; this application"), "{line}");
        assert_eq!(line.matches(" layer ").count(), LISTED_MAX);
        assert_eq!((policy_words(0), policy_words(7)), ("regular".to_string(), "unknown (7)".to_string()));
    }

    /// Where the lines are written: the click's before it is posted, from the hit test a click's
    /// `window` is checked by — not the list's front-most window at the point, which names windows
    /// that let clicks through — and the activation's from every activation, the windowless one for
    /// an application that answered none and for one that did not answer. No application is named
    /// in any of it.
    #[test]
    fn the_lines_are_written_where_a_click_and_an_activation_happen() {
        let body = |src: &'static str, sig: &str| -> &'static str {
            let at = src.find(sig).unwrap_or_else(|| panic!("no {sig}"));
            &src[at..at + src[at..].find("\n}\n").expect("its end")]
        };
        let note = body(include_str!("input.rs"), "pub fn note_click(");
        assert!(note.find("super::ax::click_hit(x, y)").unwrap() < note.find("LAST_CLICK.note(").unwrap());
        let hit = body(include_str!("ax.rs"), "pub(super) fn click_hit(");
        assert!(hit.contains("hit_window_at(x, y)?") && !hit.contains("listed_window_at"), "{hit}");
        assert!(include_str!("mod.rs").contains("fn note_click(&self, x: i32, y: i32, window: Option<isize>) {\n        input::note_click(x, y, window);"));
        let activated = body(include_str!("watch.rs"), "fn activated(");
        assert!(activated.contains("super::input::after_click_line(&name, pid)"));
        let windowless = activated.find("if answered.is_none_or(|w| w == 0) {").expect("for both kinds of naming none");
        assert!(activated[windowless..].contains("super::click_notes::windowless_line(&name, pid, &policy, &windows, super::input::last_click())"));
        assert!(!include_str!("click_notes.rs").contains(concat!("Voice", "Over")), "general, for any application");
    }

    /// The last click is told to the first activation after it, how long after it, and to none
    /// after that; its point stays for a windowless activation's line. Negative points — a display
    /// left of or above the main one — come back as they went in.
    #[test]
    fn the_last_click_is_told_once_and_its_point_kept() {
        let last = LastClick::new();
        assert_eq!((last.last(), last.take(Duration::from_secs(1))), (None, None), "no click yet");
        last.note(-1913, 131, Duration::from_millis(5000));
        assert_eq!(last.last(), Some((-1913, 131)));
        assert_eq!(last.take(Duration::from_millis(5212)), Some((-1913, 131, Duration::from_millis(212))));
        assert_eq!(last.take(Duration::from_millis(9000)), None, "told once");
        assert_eq!(last.last(), Some((-1913, 131)), "the point stays");
        last.note(356, -40, Duration::ZERO);
        assert_eq!(last.take(Duration::from_millis(3)), Some((356, -40, Duration::from_millis(3))), "a click at the origin");
        assert_eq!(
            after_click_line("Helper", 703, Duration::from_millis(212), 356, 131),
            "Helper (pid 703) came to the front 212 ms after this application's click at 356,131"
        );
    }
}
