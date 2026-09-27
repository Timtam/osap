//! Turning accessibility elements into the small integers the rest of the platform passes
//! around.
//!
//! The host hands modules an `isize` for every window and every control, and the modules
//! keep it: they compare it to decide whether the plugin in front of them is the same one
//! as a moment ago (resume where the user was) or a different one (start over), and they
//! use it as a cache key. Windows can hand out an `HWND` because that is already such an
//! integer. macOS has nothing equivalent — an `AXUIElement` is a pointer, and the pointer
//! is the wrong thing to expose for two independent reasons:
//!
//! - it reaches Lua through a **Luau number, which is an f64**, so a 64-bit pointer loses
//!   its low bits silently; and
//! - two `AXUIElement`s for the same window are different pointers that `CFEqual` says are
//!   equal, so pointer identity would make one window look like a new window every time it
//!   was enumerated, and every overlay would reset to its first control on every poll.
//!
//! So this table interns instead. Same element in, same small number out, for as long as
//! the window lives — which is exactly the contract the modules were written against.
//!
//! The bookkeeping — which window each handle lies in, and what a sweep drops — is in
//! `handle_table.rs`, pure, with its tests.

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use objc2_application_services::AXUIElement;
use objc2_core_foundation::CFRetained;

use super::handle_table::{needs_asking, Kind, Table};

/// One interned thing: the element, who owns it, its window id when it has one, and the window
/// it lies in. See `handle_table::Entry`.
pub type Entry = super::handle_table::Entry<CFRetained<AXUIElement>>;

thread_local! {
    static TABLE: RefCell<Table<CFRetained<AXUIElement>>> = RefCell::new(Table::new(FIRST_SWEEP));
}

/// The size at which the table is first swept.
///
/// It grows by one per newly seen element, and a plugin's tree is hundreds of them, so
/// without a sweep a long session in a DAW would accumulate steadily. The sweep is not on a
/// timer: it only runs when the table has actually grown, which in practice means a few times
/// an hour of plugin work — and then on the pump, after a drain ([`sweep_if_due`]), never in the
/// middle of the walk or the activation whose intern crossed the size.
const FIRST_SWEEP: usize = 4096;

/// How many windows one sweep asks about, oldest first. Each is one attribute read into its
/// application, on the pump; a table with more windows than this is swept further next time.
const WINDOWS_PER_SWEEP: usize = 256;

/// How long one sweep may spend asking windows whether they still exist, on the thread that
/// carries the event tap. The rest are asked at the next sweep. A single read can still take
/// the messaging timeout (`ax::MESSAGING_TIMEOUT`, a second) from an application that stops
/// answering; it then goes into the busy quarantine and is not asked again this sweep.
const SWEEP_BUDGET: Duration = Duration::from_millis(150);

/// The handle for a WINDOW — the same one as last time, if it has been seen before.
///
/// Never returns 0. The host uses 0 for "no window" in the key-scope logic and Lua treats a
/// falsy id as "no origin", so a real handle of 0 would read as absence.
pub fn intern(element: CFRetained<AXUIElement>, pid: i32, window_id: u32) -> isize {
    intern_as(element, pid, window_id, Kind::Window)
}

/// The handle for something INSIDE the window with handle `window` (0 when that is not known):
/// a surface `window_controls` found, a link of the focus chain. Dropped with its window.
pub fn intern_in(element: CFRetained<AXUIElement>, pid: i32, window: isize) -> isize {
    intern_as(element, pid, 0, Kind::In(window))
}

/// Interns only. The sweep a growing table owes is NOT run here: interns happen inside the
/// hot paths — an activation resolving its window, a control walk whose 300 ms budget a sweep
/// would spend, the focus chain — and a sweep asks other applications questions. It runs from
/// the pump instead ([`sweep_if_due`]).
fn intern_as(element: CFRetained<AXUIElement>, pid: i32, window_id: u32, kind: Kind) -> isize {
    TABLE.with(|t| t.borrow_mut().intern(element, pid, window_id, kind))
}

/// What is behind a handle, or `None` if the handle is not one of ours.
pub fn get(id: isize) -> Option<Entry> {
    TABLE.with(|t| t.borrow().get(id).cloned())
}

/// Pairs handle `id` with the `CGWindowID` `window_id`, changing nothing else about it.
pub fn set_window_id(id: isize, window_id: u32) {
    TABLE.with(|t| t.borrow_mut().set_window_id(id, window_id));
}

/// Sweeps the table if it has grown to the point where it should be. Called by the pump after
/// it has delivered everything queued (`MacBackend::pump_pending`), where what a sweep costs —
/// see [`sweep`] — delays nothing already promised: no walk is under way and no activation is
/// being resolved. A check of the table's size when there is nothing to do.
pub fn sweep_if_due() {
    if TABLE.with(|t| t.borrow().sweep_due()) {
        sweep();
    }
}

/// The window a handle lies in: its own handle for a window, 0 when not known.
pub fn owner_of(id: isize) -> isize {
    TABLE.with(|t| t.borrow().get(id).map_or(0, |e| e.owner))
}

/// The links `ids` of a focus chain lie in `window`, found at its end.
pub fn adopt(ids: &[isize], window: isize) {
    TABLE.with(|t| t.borrow_mut().adopt(ids, window));
}

/// Forgets everything of a process that has quit (`system.rs`, on the workspace's notice);
/// returns the handles dropped, for the tables keyed by handle (`ax::forget_handles`).
pub fn forget_pid(pid: i32) -> Vec<isize> {
    TABLE.with(|t| t.borrow_mut().forget_pid(pid))
}

/// Whether process `pid` has exited. Only `ESRCH` says so: `EPERM` is a process that exists
/// and belongs to somebody else (loginwindow, SecurityAgent), whose windows are as live as any.
fn exited(pid: i32) -> bool {
    // SAFETY: signal 0 sends nothing; it only asks whether the process could be signalled.
    let refused = unsafe { libc::kill(pid, 0) } != 0;
    refused && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Drops everything owned by processes that have gone away, every window its application says
/// no longer exists, and everything inside such a window.
///
/// A window paired with a `CGWindowID` that the window server still lists is kept without a
/// question (`handle_table::needs_asking`): one system-wide window-list call covers all of
/// them. The rest are asked, with the table NOT borrowed — each is a read into another
/// application — and at most [`WINDOWS_PER_SWEEP`] of them, within [`SWEEP_BUDGET`]. An
/// application in the busy quarantine is not asked at all, and one that does not answer is not
/// asked again in this sweep: its windows are kept, which is the safe answer, since dropping a
/// live window's handle would make its overlay start over.
///
/// Handles of dead windows must not be *reused* — a module could still be holding one, and
/// handing the same number to a different window would make it resume a state that belongs
/// to something else. Forgetting them is fine; the counter never goes backwards, so a
/// forgotten handle stays forever unmatched, which is what a module holding it should see.
fn sweep() {
    let started = Instant::now();
    let (before, pids, windows) = TABLE.with(|t| {
        let t = t.borrow();
        (t.len(), t.pids(), t.windows())
    });
    let dead: HashSet<i32> = pids.into_iter().filter(|&p| exited(p)).collect();
    let listed = super::ax::window_numbers();
    let mut gone: HashSet<isize> = HashSet::new();
    let mut silent: HashSet<i32> = HashSet::new();
    let mut asked = 0usize;
    let mut vouched = 0usize;
    for (id, pid, element, window_id) in windows {
        if dead.contains(&pid) {
            continue;
        }
        if !needs_asking(window_id, listed.as_ref()) {
            vouched += 1;
            continue;
        }
        if asked >= WINDOWS_PER_SWEEP || started.elapsed() >= SWEEP_BUDGET {
            continue;
        }
        if silent.contains(&pid) || super::ax::is_busy(pid) {
            continue;
        }
        asked += 1;
        match super::ax::element_gone(&element) {
            Some(true) => {
                gone.insert(id);
            }
            Some(false) => {}
            None => {
                silent.insert(pid);
            }
        }
    }
    let (dropped, after, next) = TABLE.with(|t| {
        let mut t = t.borrow_mut();
        let dropped = t.sweep(|p| dead.contains(&p), &gone);
        (dropped, t.len(), t.next_sweep())
    });
    super::ax::forget_handles(&dropped);
    crate::logging::line(
        "macos",
        &format!(
            "handle table swept: {} of {before} entries dropped ({} process(es) gone, {} window(s) \
             gone of {asked} asked, {vouched} kept by the window list{}), {after} kept, next sweep \
             at {next}, {} ms",
            dropped.len(),
            dead.len(),
            gone.len(),
            if listed.is_none() { ", which could not be read" } else { "" },
            started.elapsed().as_millis()
        ),
    );
}
