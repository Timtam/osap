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

use std::cell::RefCell;
use std::collections::HashMap;

use objc2_application_services::AXUIElement;
use objc2_core_foundation::CFRetained;

/// One interned thing: the element, who owns it, and its window id when it has one.
///
/// The window id is carried alongside because capture needs it and accessibility cannot
/// provide it: the two halves of "a window" on this platform come from different APIs and
/// only this table holds both. Zero means "not a window, or we could not find out".
#[derive(Clone)]
pub struct Entry {
    pub element: CFRetained<AXUIElement>,
    pub pid: i32,
    pub window_id: u32,
}

#[derive(Default)]
struct Table {
    next: isize,
    by_element: HashMap<CFRetained<AXUIElement>, isize>,
    by_id: HashMap<isize, Entry>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// Above this, the table is swept for entries whose process has exited.
///
/// It grows by one per newly seen control, and a plugin's tree is hundreds of them, so
/// without a sweep a long session in a DAW would accumulate steadily. The sweep is not on a
/// timer: it costs a `kill(pid, 0)` per distinct process and only runs when the table has
/// actually grown, which in practice means a few times an hour.
const SWEEP_AT: usize = 4096;

/// The handle for this element — the same one as last time, if it has been seen before.
///
/// Never returns 0. The host uses 0 for "no window" in the key-scope logic and Lua treats a
/// falsy id as "no origin", so a real handle of 0 would read as absence.
pub fn intern(element: CFRetained<AXUIElement>, pid: i32, window_id: u32) -> isize {
    TABLE.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(&id) = t.by_element.get(&element) {
            // Learned late: an element first seen as a control can turn out to be a window
            // once something asks for its window id. Fill it in rather than re-interning,
            // or the same window would end up with two handles.
            if window_id != 0 {
                if let Some(e) = t.by_id.get_mut(&id) {
                    e.window_id = window_id;
                }
            }
            return id;
        }
        if t.by_id.len() >= SWEEP_AT {
            sweep(&mut t);
        }
        t.next += 1;
        let id = t.next;
        t.by_element.insert(element.clone(), id);
        t.by_id.insert(id, Entry { element, pid, window_id });
        id
    })
}

/// What is behind a handle, or `None` if the handle is not one of ours.
pub fn get(id: isize) -> Option<Entry> {
    TABLE.with(|t| t.borrow().by_id.get(&id).cloned())
}

/// Just the element — the common case.
pub fn element(id: isize) -> Option<CFRetained<AXUIElement>> {
    get(id).map(|e| e.element)
}

/// Drops everything owned by processes that have gone away.
///
/// Handles of dead windows must not be *reused* — a module could still be holding one, and
/// handing the same number to a different window would make it resume a state that belongs
/// to something else. Forgetting them is fine; the counter never goes backwards, so a
/// forgotten handle stays forever unmatched, which is what a module holding it should see.
fn sweep(t: &mut Table) {
    let before = t.by_id.len();
    let mut alive: HashMap<i32, bool> = HashMap::new();
    t.by_id.retain(|_, e| {
        *alive.entry(e.pid).or_insert_with(|| unsafe { libc::kill(e.pid, 0) } == 0)
    });
    let live_ids: std::collections::HashSet<isize> = t.by_id.keys().copied().collect();
    t.by_element.retain(|_, id| live_ids.contains(id));
    crate::logging::line(
        "macos",
        &format!("handle table swept: {} of {before} entries dropped", before - t.by_id.len()),
    );
}
