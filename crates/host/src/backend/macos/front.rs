//! The front handed to another application's window: `host.window.focus`, and, once the Mac has
//! pages, a page of ours giving it back as it leaves. Done the way the VPS Avenger control tool
//! does it — a tool VoiceOver users run in production with Logic — with three departures, and
//! measured on every Mac runner CI has (`page_check.rs`).
//!
//! **Asked now, focused on the event.** An activation is asynchronous: the request returns before
//! the other application is the active one, and a window raised and focused in the same breath is
//! raised in an application that is not in front yet — with Logic, possibly before its plug-in
//! window is shown again. So the activation is requested at once, and the window is raised and
//! given the focus when the workspace says that application has become the active one
//! ([`activated`], from `watch.rs`) — never after a delay, and never by polling whether it is
//! active yet, which is where the tool waits and settles (the first departure). One such focus waits
//! at a time, and only events end it: its application's activation makes it, unless the event tap
//! saw a key pressed since it was asked for — the user has moved on, and that key went somewhere
//! else; another application's activation gives it up and says so; and so do a new hand-over, this
//! application taking the front itself (`app::activate_self`), and AppKit telling this application
//! that it became the active one again after it had stopped being it — the user came back to it.
//!
//! **This application gets out of the way.** (a), the tool's way: our window ordered out, this
//! application deactivated if it is the active one, then the other application asked to activate.
//! A deactivation hands the front to the application next in line at once, which for a window of
//! ours leaving is normally the one it came from; for `host.window.focus`, with no window of ours
//! leaving, it need not be the one asked for, and its activation, coming first, would give the
//! focus up. So nothing is deactivated then: the request is made with this application as it is,
//! as `host.window.focus` has always made it. (b), macOS 14's cooperative way: this application,
//! still active, yields its activation to the other, which is asked to activate from it, and our
//! window is ordered out last; a request the system refuses is asked again (a), so that this
//! application is not left the active one with no window of its own in front. [`HANDING`] says
//! which the application uses; `page-check` measures both against TextEdit.
//!
//! **An application that is the active one already** announces no activation, so its window is
//! raised and focused at once, with nothing asked. That is also the case while a panel of ours has
//! the keyboard without having activated this application (a page described `activates = false`):
//! the application it took the keyboard from never stopped being the active one, though AppKit says
//! this one is (`own::active`, `backend::active_for_handing`). Should AppKit hand the keyboard to
//! another window of ours as such a panel leaves, this application is deactivated, which asks
//! nothing of the other ([`keyboard_kept`]).
//!
//! **The window it came from, and only while ours had the keyboard.** The front goes to the window
//! and the application the caller names — the tool activates the first DAW of a fixed list (the
//! second departure) — and a window of ours hands it over only while it has the keyboard, Windows'
//! rule for a page (the third): a module that brought the plug-in forward first has already moved
//! on, and a user who went to another application is not pulled back. On a Mac our window hears
//! that it lost the keyboard only after the activation that took it, so a focus still waiting
//! stands as well, and a window of ours leaving meanwhile does not override it. The decision is
//! pure (`backend::hand_plan`, `on_activation`, `on_own_edge`) and tested on Windows.
//!
//! **Where the keyboard went, read last.** The focus chain after a window was focused is read for
//! the log only, so on an activation's path it is read after the activation's window has been read
//! and the application observed ([`report_chain`], from `watch.rs`), never before them.
//!
//! Main thread only, as the rest of the backend: AppKit's calls, the workspace's notices and the
//! pump all run there.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use block2::RcBlock;
use objc2::runtime::NSObjectProtocol;
use objc2::sel;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSRunningApplication, NSWindow,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSProcessInfo};

use crate::backend::{
    hand_plan, on_activation, on_own_edge, HandPlan, HandState, Handing, OnActivation, OwnEdge,
};

/// The way this application hands the front over: (a), the VPS Avenger control tool's, until
/// `page-check`'s data on the CI runners shows (b) handing it over where (a) does not (TODO.md,
/// MW5).
pub(super) const HANDING: Handing = Handing::Deactivate;

/// A window of another application, to be raised and focused once its application is the active
/// one.
struct Pending {
    pid: i32,
    /// Its handle; 0 for none, when the application's own choice of window stands.
    window: isize,
    asked: Instant,
    how: Handing,
    /// The event tap's count of keys pressed when it was asked for (`tap::pressed`).
    pressed: u32,
    /// This application has stopped being the active one since it was asked for, as AppKit told it
    /// (`backend::on_own_edge`).
    resigned: bool,
}

/// How the last focus that waited for an activation came to its end, for `page-check`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ended {
    /// Made, at its application's activation.
    Made,
    /// Given up at the activation of this other process.
    WentTo(i32),
    /// Given up at its application's activation: a key was pressed after it was asked for.
    Typed,
    /// Given up before an activation came: a new hand-over, this application taking the front, or
    /// becoming the active one again.
    GivenUp,
}

thread_local! {
    /// The one focus waiting for its application's activation.
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
    /// How the last one ended ([`take_ended`]).
    static ENDED: Cell<Option<Ended>> = const { Cell::new(None) };
    /// Whether AppKit gave the keyboard to another window of ours as the last panel that had it only
    /// lent left, so that this application was deactivated ([`keyboard_kept`], [`take_kept`]).
    static KEPT: Cell<Option<bool>> = const { Cell::new(None) };
    /// A window focused whose focus chain the log is still owed ([`report_chain`]): its process
    /// and its handle.
    static CHAIN_OWED: Cell<Option<(i32, isize)>> = const { Cell::new(None) };
}

/// AppKit's in-process notices of this application becoming and stopping being the active one are
/// subscribed to ([`hook_own`]). One-way, like the subscription.
static HOOKED_OWN: AtomicBool = AtomicBool::new(false);

/// What handing the front over came to, at the call.
pub(super) enum Handed {
    /// Nothing was asked: why.
    Kept(&'static str),
    /// The application was the active one already, and its window was raised or focused (`true`),
    /// or neither.
    AtOnce(bool),
    /// Its activation was asked for, `how`, and the system took the request (`taken`) or not;
    /// `how` is (a) where (b) was asked first and refused (`yield_refused`).
    Asked { how: Handing, taken: bool, yield_refused: bool },
}

impl Handed {
    /// `host.window.focus`'s answer: the request taken, or the window raised or focused at once.
    pub(super) fn accepted(&self) -> bool {
        match *self {
            Handed::Kept(_) => false,
            Handed::AtOnce(ok) => ok,
            Handed::Asked { taken, .. } => taken,
        }
    }
}

/// Hands the front to window `window` of process `pid` (0: whichever window the application
/// chooses), `how` being the way to ask; `leaving` is the window of ours giving up the keyboard,
/// which is ordered out whatever comes of it. Main thread only.
///
/// What is asked follows `backend::hand_plan`: nothing, when ours did not have the keyboard, when a
/// focus asked for before still waits and a window of ours is leaving, or when the front would go
/// to this application; the window raised and focused now, when its application is the active one
/// already, since no activation will be announced; otherwise the activation asked for, and the
/// window raised and focused when the workspace announces it ([`activated`]). A focus still waiting
/// is given up only when something is asked: this request is the newer one, and for the same reason
/// a page's request for the front whose page left before it was answered is forgotten
/// (`page::superseded`). The notices it waits on are subscribed to first, also when no module
/// watches windows.
pub(super) fn hand_front_to(pid: i32, window: isize, leaving: Option<&NSWindow>, how: Handing) -> Handed {
    let name = super::ax::exe_for_pid(pid);
    let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        if let Some(w) = leaving {
            w.orderOut(None);
        }
        crate::logging::line("macos", &format!("the front is not handed to pid {pid}: no such application"));
        return Handed::Kept("no such application");
    };
    let app = super::own::app();
    let front = super::ax::frontmost_pid();
    // AppKit's word, but not while a panel of ours has the keyboard only lent and the workspace
    // names another application (`own::active`).
    let active = super::own::active(front);
    let state = HandState {
        leaving_key: leaving.map(|w| w.isKeyWindow() && (super::own::nonactivating(w) || active)),
        target_ours: pid == std::process::id() as i32,
        target_front: front == Some(pid),
        ours_active: active,
        cooperative: app.as_deref().is_some_and(|a| cooperative(a, &target)),
        pending: PENDING.with(|p| p.borrow().is_some()),
    };
    match hand_plan(state, how) {
        HandPlan::Keep(why) => {
            if let Some(w) = leaving {
                w.orderOut(None);
            }
            crate::logging::line("macos", &format!("the front is not handed to {name} (pid {pid}): {}", why.words()));
            Handed::Kept(why.words())
        }
        HandPlan::AtOnce => {
            cancel(&format!("the front is handed to {name} (pid {pid}) now"));
            super::page::superseded();
            if let Some(w) = leaving {
                w.orderOut(None);
                keyboard_kept(app.as_deref(), w, &name);
            }
            let ok = focus_now(pid, window, None);
            // Nobody waits on an activation's path here: the read is made at the call.
            report_chain();
            Handed::AtOnce(ok)
        }
        HandPlan::Ask(planned) => {
            cancel(&format!("the front is handed to {name} (pid {pid}) now"));
            super::page::superseded();
            super::watch::hook_activations();
            hook_own();
            // Waited for from before the request, so that no notice of the activation can come
            // before it is waited for — and AppKit can say this application resigned inside the
            // request's own deactivation; taken back if the system did not take the request.
            PENDING.with(|p| {
                *p.borrow_mut() = Some(Pending {
                    pid,
                    window,
                    asked: Instant::now(),
                    how: planned,
                    pressed: super::tap::pressed(),
                    resigned: false,
                });
            });
            // What (a) does with a window of ours leaving, also after (b) was refused.
            let deactivates = leaving.is_some() && active;
            let (how, taken) = request(app.as_deref(), active, &target, planned, leaving);
            PENDING.with(|p| {
                let mut p = p.borrow_mut();
                match p.as_mut() {
                    Some(w) if w.pid == pid && taken => w.how = how,
                    Some(w) if w.pid == pid => *p = None,
                    _ => {}
                }
            });
            crate::logging::line(
                "macos",
                &format!(
                    "the front is handed to {name} (pid {pid}), window {window}: its activation asked for, \
                     {}{} — {}",
                    how.word(),
                    match (how, deactivates) {
                        (Handing::Yield, _) => "",
                        (Handing::Deactivate, true) => ", this application deactivated first",
                        (Handing::Deactivate, false) => ", this application not deactivated",
                    },
                    if taken {
                        "taken; the window is raised and focused when the workspace says it is the active \
                         application"
                    } else {
                        "REFUSED, so nothing waits for it"
                    }
                ),
            );
            Handed::Asked { how, taken, yield_refused: planned == Handing::Yield && how == Handing::Deactivate }
        }
    }
}

/// A window of ours that had the keyboard, `left`, has been ordered out, with the application the
/// front goes to (`name`) the active one already ([`HandPlan::AtOnce`]): a panel that had the
/// keyboard only lent, a page described `activates = false`. AppKit calls this application the
/// active one while such a panel has the keyboard, and may then hand the keyboard to another window
/// of ours that can take it — a page shown without the keyboard, the module manager — which would
/// keep it in this application with the other one in front. So where a window of ours is the key
/// window straight after the order out, this application is deactivated: that asks no application
/// to activate, and the one in front is already. Read at the call, as AppKit's word on its key
/// window; whether AppKit chooses another window then or only later, and where the keyboard goes
/// after the deactivation, is what `page-check` shows (TODO.md, MW1b).
fn keyboard_kept(app: Option<&NSApplication>, left: &NSWindow, name: &str) {
    let Some(app) = app else { return };
    let kept = super::own::key_now(app);
    KEPT.with(|k| k.set(Some(kept.is_some())));
    let Some(kept) = kept else { return };
    app.deactivate();
    crate::logging::line(
        "macos",
        &format!(
            "AppKit gave the keyboard to our window '{}' as '{}', which had it without activating this application, \
             left, with {name} the active application already: this application deactivated, so that it does not keep \
             the keyboard",
            kept.title(),
            left.title()
        ),
    );
}

/// Asks for `target`'s activation the way `how` says, with `leaving` ordered out where that way
/// puts it: the way that was asked in the end, and whether the system took the request. (b) is
/// planned only where both of its calls exist and this application is the active one
/// (`backend::hand_plan`), which needs the application object; without it, (a). `active`: this
/// application is the active one, as `own::active` has it.
///
/// (a), the tool's order: our window out, this application deactivated, then the request, with the
/// options the rest of the backend activates with. On 14 and later, where the request's option to
/// take the front regardless has no effect, this likely works because an application that
/// deactivates hands the activation on, normally to the one before it — which is also how it would
/// fail, when that order is another one; `page-check` measures it. Deactivated only when a window
/// of ours is leaving: for `host.window.focus` the application next in line need not be the one
/// asked for, and its activation would come first and give the focus up.
///
/// (b): the yield, then the request from this application while it is still the active one —
/// answered at once with whether the system allowed it — and our window out last. A request
/// refused is asked again (a): our window ordered out after a refusal would leave this application
/// the active one with nothing of its own in front, where VoiceOver users are left in no
/// application at all (`app.rs`).
fn request(
    app: Option<&NSApplication>,
    active: bool,
    target: &NSRunningApplication,
    how: Handing,
    leaving: Option<&NSWindow>,
) -> (Handing, bool) {
    if let (Handing::Yield, Some(app)) = (how, app) {
        app.yieldActivationToApplication(target);
        let taken = target.activateFromApplication_options(
            &NSRunningApplication::currentApplication(),
            NSApplicationActivationOptions::ActivateAllWindows,
        );
        if taken {
            if let Some(w) = leaving {
                w.orderOut(None);
            }
            return (Handing::Yield, true);
        }
        crate::logging::line(
            "macos",
            &format!(
                "the front handed to pid {} {}: REFUSED, so it is asked again, {}",
                target.processIdentifier(),
                Handing::Yield.word(),
                Handing::Deactivate.word()
            ),
        );
    }
    if let Some(w) = leaving {
        w.orderOut(None);
    }
    if leaving.is_some() && active {
        if let Some(app) = app {
            app.deactivate();
        }
    }
    (Handing::Deactivate, target.activateWithOptions(super::app::activation_options()))
}

/// Whether (b)'s two calls exist here: macOS 14 and later, and asked of the runtime as well — an
/// unrecognised selector is not an error on a Mac, it ends the process, and objc2 binds both with
/// no availability gating.
fn cooperative(app: &NSApplication, target: &NSRunningApplication) -> bool {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion >= 14
        && app.respondsToSelector(sel!(yieldActivationToApplication:))
        && target.respondsToSelector(sel!(activateFromApplication:options:))
}

/// Subscribes, once for the life of the process, to AppKit's in-process notices of this application
/// stopping being and becoming the active one ([`own_edge`]). With no queue, so they run where
/// AppKit posts them: on the main thread, in the order they happened, inside a deactivation this
/// application asks for as well. Main thread only; also a page's show, whose request can outlive
/// the page (`page.rs`).
pub(super) fn hook_own() {
    if objc2::MainThreadMarker::new().is_none() || HOOKED_OWN.swap(true, Ordering::SeqCst) {
        return;
    }
    let centre = NSNotificationCenter::defaultCenter();
    // SAFETY: framework-owned constants that live for the process.
    let names = unsafe {
        [(NSApplicationDidResignActiveNotification, OwnEdge::Resigned), (NSApplicationDidBecomeActiveNotification, OwnEdge::Became)]
    };
    for (name, edge) in names {
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            // An unwind out of a block AppKit calls would tear through frames that cannot handle it.
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| own_edge(edge)));
            if caught.is_err() {
                crate::logging::line("macos", "panic while noting this application becoming or stopping being the active one");
            }
        });
        // SAFETY: the block is copied by the centre; no queue, so it runs on the posting thread.
        let token = unsafe { centre.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
        // Leaked deliberately, like every subscription in this backend: it lasts as long as the
        // process.
        core::mem::forget(token);
    }
}

/// AppKit told this application that it stopped being or became the active one: what that does to
/// a focus waiting is `backend::on_own_edge`'s; becoming it answers a page's request for the front
/// whose page left first (`page::became_active`). One borrow each and nothing else when nothing
/// waits.
fn own_edge(edge: OwnEdge) {
    let gives_up = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let w = p.as_mut()?;
        let (resigned, gives_up) = on_own_edge(w.resigned, edge);
        w.resigned = resigned;
        Some(gives_up)
    });
    if gives_up == Some(true) {
        cancel("this application became the active one again after it had stopped being it");
    }
    if edge == OwnEdge::Became {
        super::page::became_active();
    }
}

/// The workspace named process `pid` the active application (`watch.rs`, also while nobody watches
/// windows): the focus waiting for it is made now, before the activation's window is read, unless a
/// key was pressed since it was asked for; one waiting for another application is given up; this
/// application's own decides nothing (`backend::on_activation`). One borrow and an atomic read, and
/// nothing else, when no focus waits. Where a window was focused, [`report_chain`] follows on the
/// same path, last.
pub(super) fn activated(pid: i32) {
    let ours = std::process::id() as i32;
    let pressed = super::tap::pressed();
    let step = PENDING.with(|p| p.borrow().as_ref().map(|w| on_activation(w.pid, pid, ours, pressed != w.pressed)));
    match step {
        None => {}
        Some(OnActivation::Wait) => crate::logging::trace("macos", || {
            "this application's own activation announced while a hand-over of the front waits; it waits on"
                .to_string()
        }),
        Some(OnActivation::Drop) => {
            if let Some(p) = PENDING.with(|c| c.borrow_mut().take()) {
                ENDED.with(|e| e.set(Some(Ended::WentTo(pid))));
                crate::logging::line(
                    "macos",
                    &format!(
                        "the front went to {} (pid {pid}) instead of {} (pid {}), {:.0} ms after it was asked \
                         for, {}: window {} is not raised or focused",
                        super::ax::exe_for_pid(pid),
                        super::ax::exe_for_pid(p.pid),
                        p.pid,
                        p.asked.elapsed().as_secs_f64() * 1000.0,
                        p.how.word(),
                        p.window
                    ),
                );
            }
        }
        Some(OnActivation::Typed) => {
            if let Some(p) = PENDING.with(|c| c.borrow_mut().take()) {
                ENDED.with(|e| e.set(Some(Ended::Typed)));
                crate::logging::line(
                    "macos",
                    &format!(
                        "{} (pid {pid}) is the active application {:.0} ms after it was asked for, {}, but a \
                         key was pressed since: window {} is not raised or focused, and the application's \
                         own choice stands",
                        super::ax::exe_for_pid(pid),
                        p.asked.elapsed().as_secs_f64() * 1000.0,
                        p.how.word(),
                        p.window
                    ),
                );
            }
        }
        Some(OnActivation::Focus) => {
            if let Some(p) = PENDING.with(|c| c.borrow_mut().take()) {
                ENDED.with(|e| e.set(Some(Ended::Made)));
                focus_now(p.pid, p.window, Some((p.asked, p.how)));
            }
        }
    }
}

/// Gives up the focus still waiting for an application's activation, saying `why`; nothing when none
/// waits.
pub(super) fn cancel(why: &str) {
    if let Some(p) = PENDING.with(|c| c.borrow_mut().take()) {
        ENDED.with(|e| e.set(Some(Ended::GivenUp)));
        crate::logging::line(
            "macos",
            &format!(
                "the focus waiting for {} (pid {}) to become the active application is given up, {:.0} ms \
                 after it was asked for: {why}",
                super::ax::exe_for_pid(p.pid),
                p.pid,
                p.asked.elapsed().as_secs_f64() * 1000.0
            ),
        );
    }
}

/// The application a focus is waiting for, while one is: `page-check` reads it after the
/// activation it measures.
pub(super) fn waiting_for() -> Option<i32> {
    PENDING.with(|p| p.borrow().as_ref().map(|w| w.pid))
}

/// How the last focus that waited ended, once; `page-check` reads it after each hand-over.
pub(super) fn take_ended() -> Option<Ended> {
    ENDED.with(Cell::take)
}

/// Whether AppKit gave the keyboard to another window of ours as the last panel that had it only
/// lent left, once — `None` when no such panel has left since the last read ([`keyboard_kept`]);
/// `page-check` reads it after hiding one.
pub(super) fn take_kept() -> Option<bool> {
    KEPT.with(Cell::take)
}

/// Raises and focuses window `window` of process `pid`, its application the active one: at once
/// (`asked` is `None`), or at the workspace's notice of an activation asked for then, that way.
/// Whether the window took the raise or the focus; with no window named, `true` — the
/// application's own choice stands. The focus chain is owed to the log after it ([`report_chain`]).
///
/// A raise or a focus refused is said, and is no failure of the hand-over: the application is in
/// front either way, and a window of Logic's that it ordered out while another application was
/// active may be one it has not shown again yet.
fn focus_now(pid: i32, window: isize, asked: Option<(Instant, Handing)>) -> bool {
    let name = super::ax::exe_for_pid(pid);
    let when = match asked {
        Some((t, how)) => format!(
            "the active application {:.0} ms after it was asked for, {}",
            t.elapsed().as_secs_f64() * 1000.0,
            how.word()
        ),
        None => "the active application already".to_string(),
    };
    if window == 0 {
        crate::logging::line("macos", &format!("{name} (pid {pid}) is {when}; no window was named, so its own choice stands"));
        return true;
    }
    let Some((raised, focused)) = super::ax::raise_and_focus(window) else {
        crate::logging::line(
            "macos",
            &format!("{name} (pid {pid}) is {when}; window {window} is no longer known, so its own choice stands"),
        );
        return false;
    };
    crate::logging::line(
        "macos",
        &format!(
            "{name} (pid {pid}) is {when}: focusing window {window}, raise {}, focused-attribute {}{}",
            if raised { "yes" } else { "REFUSED" },
            if focused { "yes" } else { "REFUSED" },
            if raised && focused {
                ""
            } else {
                " (the window may be gone or not shown again yet; the application's own choice stands)"
            }
        ),
    );
    CHAIN_OWED.with(|c| c.set(Some((pid, window))));
    raised || focused
}

/// The focus chain after a window was focused ([`focus_now`]), read and said in the log; nothing
/// when no read is owed, which is every activation but one that made a focus. Called last on an
/// activation's path (`watch.rs`), after the window in front is read and the application observed:
/// it is a system-wide read into the application that has just become the active one, at its
/// busiest, and only this log line waits for it. Read at the call where the window was focused at
/// once.
pub(super) fn report_chain() {
    let Some((pid, window)) = CHAIN_OWED.with(Cell::take) else {
        return;
    };
    let depth = super::ax::window_focus_chain().len();
    crate::logging::line(
        "macos",
        &format!(
            "the focus chain after focusing window {window} of {} (pid {pid}) is {depth} deep{}",
            super::ax::exe_for_pid(pid),
            // Depth only, and said as depth: the caller that needs to know checks the last link's
            // identity against the window it asked for, and this log line is not that caller.
            match depth {
                0 => ", and where the keyboard is could not be read",
                1 => ", and the focused element is a window",
                _ => ", so the focus is on something inside whatever is in front",
            }
        ),
    );
}
