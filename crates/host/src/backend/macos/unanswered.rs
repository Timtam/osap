//! The reads an application could not complete inside one accessibility walk, counted for the log.
//!
//! Its own file for the reason `budget.rs` has one: bookkeeping per thread, with nothing macOS about
//! it, so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)` and its rules run where they
//! are written. `ax.rs` begins a walk's count ([`begin`]), says where its walk is ([`enter`],
//! [`leave`], [`read`]) and which read answered `kAXErrorCannotComplete` ([`note`]); the walk's line
//! comes from [`Counted::end`].
//!
//! **Why they are counted.** On Windows a read that is not answered ends the question
//! (`UIA_E_TIMEOUT`); on a Mac the walk goes on, so every later read of an application that has
//! stopped answering can wait up to the messaging timeout. Stopping the walk as Windows does is
//! right only if a plug-in that works never answers so for one of its elements now and then — its
//! identity checks would fail then — and no line said that: `note_error`'s comes once in five
//! seconds at most and names no element and no walk. So a walk that met one writes a line of its
//! own: what the walk was for, the application and the walk's root, each element by its place and
//! what it is, each read of it that was not completed, and how many walks of its kind in that
//! application so far met one. Nothing in the walk changes for it.
//!
//! **Once in five seconds.** Two of the reads counted here put the application in no quarantine (a
//! slot of the batched read, and whether `AXFocused` is settable), so nothing spaces out the walks
//! that meet them: the control walk runs at every recheck, every 500 ms while a landmark poll runs.
//! A line for a kind of walk in an application is written once in [`QUIET`] at most, and the walks
//! that met one meanwhile are counted into the next line; the counts so far keep the rate.
//!
//! **Only inside a walk.** The same reads serve the window in front, the focus chain and the list
//! of an application's windows; a read outside a walk is counted nowhere here, and `note_error`'s
//! line still says it.

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// How many elements a walk's line names; the rest are counted.
const NAMED: usize = 6;
/// How much of an element's title a line quotes, in characters.
const TITLE_CHARS: usize = 40;
/// How long after a line about a kind of walk in an application the next one about them waits.
const QUIET: Duration = Duration::from_secs(5);

/// One element a walk met a read at that was not completed.
struct Element {
    /// Its place: the child numbers from the walk's root, from 1; empty for the root.
    path: Vec<u16>,
    /// Its role and title as the walk read them, either empty when that read was not answered.
    role: String,
    title: String,
    /// Each attribute not answered, with how often in this walk.
    reads: Vec<(String, u32)>,
}

/// The element at one level of the walk's place.
#[derive(Default)]
struct Level {
    /// Whether the walk has read the element there yet.
    read: bool,
    /// Its role and title as read, either empty when that read was not answered.
    role: String,
    title: String,
}

/// The walk being made, and between walks what it leaves: kept for the thread, so that a walk
/// allocates nothing for an element unless a read of it goes unanswered — the control walk runs on
/// every recheck.
struct Tally {
    /// Whether a walk is being made.
    active: bool,
    /// What it is for, in the line's words (`budget::Walk::words`).
    kind: &'static str,
    /// Where it is: the child numbers from its root, as [`Element::path`].
    path: Vec<u16>,
    /// The element at each level of `path`, the root's first: `levels[path.len()]` is the one the
    /// walk is at. Levels past it are kept for their buffers.
    levels: Vec<Level>,
    /// How many elements it has read.
    read: u32,
    /// The elements it met a read at that was not completed, in the order it met them.
    misses: Vec<Element>,
}

impl Tally {
    /// The element the walk is at.
    fn here(&mut self) -> &mut Level {
        let depth = self.path.len();
        if self.levels.len() <= depth {
            self.levels.resize_with(depth + 1, Level::default);
        }
        &mut self.levels[depth]
    }
}

/// The walks of one kind made in one application, for the session.
struct Walks {
    kind: &'static str,
    /// The application's process: one started again is counted afresh.
    pid: i32,
    /// How many have ended, and how many of them met a read that was not completed.
    ended: u32,
    met: u32,
    /// When the last line about them was written, and how many that met one no line has said
    /// since.
    said: Option<Instant>,
    unsaid: u32,
}

thread_local! {
    static TALLY: RefCell<Tally> = const {
        RefCell::new(Tally { active: false, kind: "", path: Vec::new(), levels: Vec::new(), read: 0, misses: Vec::new() })
    };
    /// Each kind of walk in each application it was made in, the whole run.
    static WALKS: RefCell<Vec<Walks>> = const { RefCell::new(Vec::new()) };
}

/// The walk being made, if there is one.
fn with(f: impl FnOnce(&mut Tally)) {
    TALLY.with(|t| {
        let mut t = t.borrow_mut();
        if t.active {
            f(&mut t);
        }
    });
}

/// A walk being counted, from [`begin`] to [`Counted::end`].
///
/// Dropped without being ended — a panic in the walk, which the host's guard around a module's
/// call catches, and the session goes on — it stops the count all the same: the walk is counted
/// for nothing, the next one begins a count of its own, and a read outside a walk is noted nowhere.
pub(crate) struct Counted(());

impl Drop for Counted {
    fn drop(&mut self) {
        let _ = TALLY.try_with(|t| {
            if let Ok(mut t) = t.try_borrow_mut() {
                t.active = false;
            }
        });
    }
}

/// A walk begins, `kind` saying what it is for. `None` inside a walk already: a walk made inside
/// another is counted as part of it, and only the outer one ends it.
pub(crate) fn begin(kind: &'static str) -> Option<Counted> {
    TALLY.with(|t| {
        let mut t = t.borrow_mut();
        if t.active {
            return None;
        }
        t.active = true;
        t.kind = kind;
        t.path.clear();
        t.read = 0;
        t.misses.clear();
        t.here().read = false;
        Some(Counted(()))
    })
}

/// The walk goes into child `index` (from 0) of the element it is at.
pub(crate) fn enter(index: usize) {
    with(|t| {
        t.path.push(u16::try_from(index + 1).unwrap_or(u16::MAX));
        t.here().read = false;
    });
}

/// And comes back out of it, to its parent.
pub(crate) fn leave() {
    with(|t| {
        t.path.pop();
    });
}

/// The walk has read the element it is at: its role and title, either empty when that read was
/// not answered. What it met at that element before, in that same read, is then named by them.
pub(crate) fn read(role: &str, title: &str) {
    with(|t| {
        t.read += 1;
        let here = t.here();
        here.read = true;
        here.role.clear();
        here.role.push_str(role);
        here.title.clear();
        here.title.push_str(title);
        if let Some(last) = t.misses.last_mut().filter(|e| e.path == t.path) {
            last.role = role.to_string();
            last.title = title.to_string();
        }
    });
}

/// A read of the element the walk is at answered `kAXErrorCannotComplete`; `attribute` is what it
/// asked for. Outside a walk, nothing.
pub(crate) fn note(attribute: &str) {
    with(|t| {
        if !t.misses.last().is_some_and(|e| e.path == t.path) {
            let path = t.path.clone();
            let here = t.here();
            let (role, title) = if here.read { (here.role.clone(), here.title.clone()) } else { Default::default() };
            t.misses.push(Element { path, role, title, reads: Vec::new() });
        }
        let reads = &mut t.misses.last_mut().expect("just made").reads;
        match reads.iter_mut().find(|(a, _)| a == attribute) {
            Some((_, n)) => *n += 1,
            None => reads.push((attribute.to_string(), 1)),
        }
    });
}

impl Counted {
    /// The walk is over, at `now`, made in the application `pid` says: counted for its kind there
    /// either way, and its line when it met a read that was not completed — unless a line about
    /// that kind's walks there was written less than [`QUIET`] before, when the next line counts it.
    /// `app` names the application, asked only for a line. Both are asked after the count has
    /// stopped: a read of their own is counted for nothing.
    pub(crate) fn end(self, now: Instant, pid: impl FnOnce() -> i32, app: impl FnOnce(i32) -> String) -> Option<String> {
        drop(self);
        let (kind, met) = TALLY.with(|t| {
            let t = t.borrow();
            (t.kind, !t.misses.is_empty())
        });
        let pid = pid();
        let (ended, with_misses, unsaid) = WALKS.with(|w| {
            let mut w = w.borrow_mut();
            let at = match w.iter().position(|e| e.kind == kind && e.pid == pid) {
                Some(at) => at,
                None => {
                    w.push(Walks { kind, pid, ended: 0, met: 0, said: None, unsaid: 0 });
                    w.len() - 1
                }
            };
            let entry = &mut w[at];
            entry.ended += 1;
            if !met {
                return None;
            }
            entry.met += 1;
            if entry.said.is_some_and(|said| now.saturating_duration_since(said) < QUIET) {
                entry.unsaid += 1;
                return None;
            }
            entry.said = Some(now);
            Some((entry.ended, entry.met, std::mem::take(&mut entry.unsaid)))
        })?;
        // Asked outside the borrow: naming the application is a read of its own.
        let app = app(pid);
        Some(TALLY.with(|t| line(&t.borrow(), &app, ended, with_misses, unsaid)))
    }
}

/// An element as a line names it: its role, and its title quoted when it has one.
fn what(role: &str, title: &str) -> String {
    let role = if role.is_empty() { "an element whose role was not read" } else { role };
    if title.is_empty() {
        return role.to_string();
    }
    let mut quoted: String = title.chars().take(TITLE_CHARS).collect();
    if title.chars().count() > TITLE_CHARS {
        quoted.push('…');
    }
    format!("{role} '{quoted}'")
}

/// A place as a line names it: `the root`, or the child numbers from it, `at 2.1.4`.
fn place(path: &[u16]) -> String {
    if path.is_empty() {
        return "the root".to_string();
    }
    let numbers: Vec<String> = path.iter().map(u16::to_string).collect();
    format!("at {}", numbers.join("."))
}

/// The line of a walk that met a read that was not completed, the `ended`-th of its kind in `app`
/// so far, the `met`-th of them to meet one; `unsaid` of those met one since the last line said.
fn line(t: &Tally, app: &str, ended: u32, met: u32, unsaid: u32) -> String {
    let app = if app.is_empty() { "an application whose name was not read" } else { app };
    let root = match t.levels.first().filter(|l| l.read) {
        Some(l) => what(&l.role, &l.title),
        None => "a root that was not read".to_string(),
    };
    let total: u32 = t.misses.iter().flat_map(|e| e.reads.iter().map(|(_, n)| n)).sum();
    let mut named: Vec<String> = t
        .misses
        .iter()
        .take(NAMED)
        .map(|e| {
            let reads: Vec<String> = e
                .reads
                .iter()
                .map(|(a, n)| if *n > 1 { format!("{a} ×{n}") } else { a.clone() })
                .collect();
            format!("{}, {}: {}", place(&e.path), what(&e.role, &e.title), reads.join(", "))
        })
        .collect();
    if t.misses.len() > NAMED {
        named.push(format!("and {} more element(s)", t.misses.len() - NAMED));
    }
    let mut said = format!(
        "an accessibility {kind} walk in {app}, under {root}, met {total} read(s) the application \
         could not complete (kAXErrorCannotComplete) at {elements} element(s): {list}; {read} \
         element(s) read — {met} of {ended} {kind} walk(s) in {app} so far met one",
        kind = t.kind,
        elements = t.misses.len(),
        list = named.join("; "),
        read = t.read,
    );
    if unsaid > 0 {
        said.push_str(&format!(
            "; {unsaid} more such walk(s) since the last line, which comes once in {} s at most",
            QUIET.as_secs()
        ));
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// The clock of the walks below: each ends a quiet spell after the one before, so each that met
    /// a read not completed has its line.
    fn later() -> Instant {
        thread_local! {
            static AT: Cell<Option<Instant>> = const { Cell::new(None) };
        }
        AT.with(|a| {
            let at = a.get().map_or_else(Instant::now, |t| t + QUIET);
            a.set(Some(at));
            at
        })
    }

    /// A walk of a root window with two children, the second with one child: what `ax.rs`'s walk
    /// says, as it says it, in REAPER. `misses` are noted at the place they are listed for, before
    /// (`true`) or after (`false`) the element there is read.
    fn walk(kind: &'static str, misses: &[(&[usize], bool, &str)]) -> Option<String> {
        let counted = begin(kind).expect("no walk being made");
        let at = |path: &[usize], before: bool| {
            for (p, early, attribute) in misses {
                if *p == path && *early == before {
                    note(attribute);
                }
            }
        };
        let element = |path: &[usize], role: &str, title: &str| {
            at(path, true);
            read(role, title);
            at(path, false);
        };
        element(&[], "AXWindow", "FX: Track 1");
        enter(0);
        element(&[0], "AXGroup", "Main");
        leave();
        enter(1);
        element(&[1], "AXGroup", "");
        enter(0);
        element(&[1, 0], "", "");
        leave();
        leave();
        counted.end(later(), || 41, |pid| {
            assert_eq!(pid, 41);
            "REAPER".to_string()
        })
    }

    /// A walk that met none says nothing, and is counted; one that met some names the application,
    /// the root, each element by its place and what it is, each read, and the count so far of its
    /// kind in that application — the walks before it included, those that met none too.
    #[test]
    fn a_walk_that_met_an_unanswered_read_says_where() {
        assert_eq!(walk("search", &[]), None);
        assert_eq!(walk("search", &[]), None);
        let said = walk("search", &[(&[0], false, "AXChildren"), (&[1, 0], true, "multiple attributes"), (&[1, 0], true, "AXRole")]);
        assert_eq!(
            said.as_deref(),
            Some(
                "an accessibility search walk in REAPER, under AXWindow 'FX: Track 1', met 3 read(s) the \
                 application could not complete (kAXErrorCannotComplete) at 2 element(s): at 1, AXGroup \
                 'Main': AXChildren; at 2.1, an element whose role was not read: multiple attributes, \
                 AXRole; 4 element(s) read — 1 of 3 search walk(s) in REAPER so far met one"
            )
        );
        // Each kind is counted apart; a root whose own read was not answered is said so.
        let counted = begin("dump").unwrap();
        note("multiple attributes");
        read("", "");
        let dump = counted.end(later(), || 41, |_| "REAPER".to_string()).unwrap();
        assert!(dump.contains("under an element whose role was not read, met 1 read(s)"), "{dump}");
        assert!(dump.contains("the root, an element whose role was not read: multiple attributes;"), "{dump}");
        assert!(dump.ends_with("1 of 1 dump walk(s) in REAPER so far met one"), "{dump}");
        let again = walk("search", &[(&[1], false, "AXChildren")]).unwrap();
        assert!(again.contains("at 2, AXGroup: AXChildren;"), "a title not given is not quoted: {again}");
        assert!(again.ends_with("2 of 4 search walk(s) in REAPER so far met one"), "{again}");
        // And each application apart, by its process.
        let counted = begin("search").unwrap();
        read("AXWindow", "");
        note("AXChildren");
        let other = counted.end(later(), || 7, |_| "Logic Pro".to_string()).unwrap();
        assert!(other.ends_with("1 of 1 search walk(s) in Logic Pro so far met one"), "{other}");
    }

    /// The element a miss was met at is named by what the walk read of it, also when the miss came
    /// first, in that same read; a read noted twice is counted, not listed twice.
    #[test]
    fn an_element_is_named_by_what_was_read_of_it() {
        let said = walk("controls", &[(&[0], true, "AXSubrole"), (&[0], false, "AXChildren"), (&[0], false, "AXChildren")]).unwrap();
        assert!(said.contains("at 1 element(s): at 1, AXGroup 'Main': AXSubrole, AXChildren ×2;"), "{said}");
        assert!(said.contains("met 3 read(s)"), "{said}");
    }

    /// Outside a walk nothing is noted; a walk inside a walk is the outer one's, which alone ends
    /// it; and the application is named only for a walk that met one, after the count has stopped.
    #[test]
    fn only_a_walk_counts_and_only_the_outer_one_ends() {
        note("AXRole");
        read("AXGroup", "x");
        enter(3);
        leave();
        let counted = begin("focus").unwrap();
        read("AXWindow", "");
        assert_eq!(counted.end(later(), || 41, |_| unreachable!("the application is not named for a walk that met none")), None);

        let counted = begin("focus").unwrap();
        read("AXWindow", "Kontakt 8");
        assert!(begin("search").is_none(), "inside a walk already");
        enter(4);
        note("AXChildren");
        leave();
        let said = counted.end(later(), || 41, |_| "Kontakt 8".to_string()).unwrap();
        assert!(said.starts_with("an accessibility focus walk in Kontakt 8, under AXWindow 'Kontakt 8', met 1 read(s)"), "{said}");
        assert!(said.contains("at 5, an element whose role was not read: AXChildren;"), "{said}");
        assert!(said.ends_with("1 of 2 focus walk(s) in Kontakt 8 so far met one"), "{said}");
        note("AXChildren");
        let counted = begin("focus").unwrap();
        read("AXWindow", "");
        assert_eq!(counted.end(later(), || 41, |_| unreachable!()), None, "ended, and nothing noted after it");

        // Asking for the application and naming it are reads of their own, which can meet the same
        // answer: they are made after the walk, and counted for nothing.
        let counted = begin("controls").unwrap();
        read("AXWindow", "");
        note("AXChildren");
        let said = counted
            .end(
                later(),
                || {
                    note("pid");
                    41
                },
                |_| {
                    note("pid");
                    "Logic Pro".to_string()
                },
            )
            .unwrap();
        assert!(said.contains("met 1 read(s)") && !said.contains("pid"), "{said}");
    }

    /// A line about a kind of walk in an application comes once in five seconds at most: a walk of
    /// that kind there that met one within them has none, and the next line says how many — while
    /// the other kinds, the other applications, and the walks that met none go on as they were.
    #[test]
    fn a_line_comes_once_in_five_seconds_for_a_kind_in_an_application() {
        let t = Instant::now();
        let at = |s: u64| t + Duration::from_secs(s);
        let made = |kind: &'static str, pid: i32, met: bool, when: Instant| {
            let counted = begin(kind).unwrap();
            read("AXWindow", "");
            if met {
                note("AXChildren");
            }
            counted.end(when, || pid, |pid| format!("app {pid}"))
        };
        let first = made("controls", 1, true, at(0)).unwrap();
        assert!(first.ends_with("1 of 1 controls walk(s) in app 1 so far met one"), "{first}");
        assert_eq!(made("controls", 1, true, at(1)), None);
        assert_eq!(made("controls", 1, false, at(2)), None);
        assert_eq!(made("controls", 1, true, at(4)), None);
        assert!(made("controls", 2, true, at(4)).unwrap().ends_with("1 of 1 controls walk(s) in app 2 so far met one"));
        assert!(made("search", 1, true, at(4)).unwrap().ends_with("1 of 1 search walk(s) in app 1 so far met one"));
        let next = made("controls", 1, true, at(5)).unwrap();
        assert!(
            next.ends_with(
                "4 of 5 controls walk(s) in app 1 so far met one; 2 more such walk(s) since the last line, which \
                 comes once in 5 s at most"
            ),
            "{next}"
        );
        assert_eq!(made("controls", 1, true, at(9)), None);
        let after = made("controls", 1, true, at(10)).unwrap();
        assert!(after.ends_with("6 of 7 controls walk(s) in app 1 so far met one; 1 more such walk(s) since the last line, which comes once in 5 s at most"), "{after}");
        let quiet = made("controls", 1, true, at(16)).unwrap();
        assert!(quiet.ends_with("7 of 8 controls walk(s) in app 1 so far met one"), "none since: {quiet}");
    }

    /// A walk that panics stops the count as it unwinds: nothing after it is noted outside a walk,
    /// and the next walk is counted, on its own.
    #[test]
    fn a_walk_that_panicked_leaves_no_count_behind() {
        let caught = std::panic::catch_unwind(|| {
            let _counted = begin("search").unwrap();
            read("AXWindow", "");
            note("AXChildren");
            panic!("in the walk");
        });
        assert!(caught.is_err());
        note("AXRole");
        let counted = begin("search").expect("the count stopped");
        read("AXWindow", "");
        assert_eq!(counted.end(later(), || 41, |_| unreachable!("nothing carried over")), None);
    }

    /// A long title is cut, a long list of elements is counted past the sixth, and an application
    /// whose name was not read is said so.
    #[test]
    fn a_line_stays_a_line() {
        let counted = begin("search").unwrap();
        read("AXWindow", &"x".repeat(60));
        for i in 0..8 {
            enter(i);
            read("AXButton", "");
            note("AXChildren");
            leave();
        }
        let said = counted.end(later(), || 0, |_| String::new()).unwrap();
        assert!(said.contains(&format!("under AXWindow '{}…',", "x".repeat(40))), "{said}");
        assert!(said.contains("in an application whose name was not read,"), "{said}");
        assert!(said.contains("at 6, AXButton: AXChildren; and 2 more element(s); 9 element(s) read"), "{said}");
        assert!(!said.contains("at 7,"), "{said}");
        assert!(said.contains("met 8 read(s)") && said.contains("at 8 element(s)"), "{said}");
    }
}
