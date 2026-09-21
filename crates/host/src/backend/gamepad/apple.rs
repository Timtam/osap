//! The macOS source: the GameController framework. Written blind — no Mac with a pad has run
//! it; `crates/macos-check` type-checks it and TODO.md lists what a session has to measure.
//!
//! **Background delivery is opt-in, and the opt-in is dangerous early.** GameController
//! forwards nothing to an application that is not frontmost unless
//! `+[GCController setShouldMonitorBackgroundEvents:YES]` — and ours is never frontmost while a
//! game is being played. SDL found that setting it before the first controller has connected
//! crashes on macOS, and now sets it while adding a device. So does this file: on the FIRST
//! controller, never at start, and the value is read back and logged, because macOS 15.4 read
//! back `NO` after being told `YES` (fixed in 15.5 beta 3) and only the log can say which
//! kind of system a tester is on.
//!
//! **Every block is caught.** Foundation and GameController call these blocks from frames that
//! cannot unwind, so a panic escaping one would take the process with nothing in the log.
//!
//! **Threads.** The connect and disconnect observers are registered with queue nil, so they run
//! on whatever thread posts the notification — not necessarily the main one, and nothing here
//! assumes it is. Value changes arrive on a private serial dispatch queue rather than on the
//! main queue, so an event's timestamp is taken when the pad reported it and not when the main
//! thread got round to it. Both only touch the hub, which is a `Mutex`.
//!
//! **Identity.** Each controller gets a monotonic id, and the table holds the controller itself
//! (retained) under it. An id derived from the object's address would be reused by the next
//! controller allocated there, and a late value change for the old one would update the new.

use core::ptr::NonNull;
use std::cell::RefCell;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{sel, ClassType};
use objc2_app_kit::NSRunningApplication;
use objc2_foundation::{NSActivityOptions, NSNotification, NSNotificationCenter, NSObjectProtocol, NSProcessInfo, NSString};
use objc2_game_controller::{
    GCController, GCControllerButtonInput, GCControllerDidConnectNotification, GCControllerDidDisconnectNotification,
    GCControllerElement, GCDevice, GCDualSenseGamepad, GCDualShockGamepad, GCExtendedGamepad, GCXboxGamepad,
};

use super::{names, Axis, Button, Control, DeviceKey, Family, Hub, PadDesc, Snapshot, Source};

/// A controller the table holds, and the id it was given.
struct Held {
    id: u64,
    controller: Retained<GCController>,
}

// SAFETY: a `Held` crosses threads only to be compared by address and released — the table is
// written from the notification observers, which run on whatever thread posts. Releasing an
// Objective-C object is thread-safe, and nothing here calls a method on a `Held`'s controller
// from a thread other than the one that is handling that controller's own notification.
unsafe impl Send for Held {}

static TABLE: Mutex<Vec<Held>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// Whether the background flag has been dealt with — see the module note.
static BACKGROUND_DONE: AtomicBool = AtomicBool::new(false);
/// The serial queue every controller's value changes are delivered on.
static QUEUE: OnceLock<DispatchRetained<DispatchQueue>> = OnceLock::new();

thread_local! {
    /// The activity that keeps App Nap away while somebody listens. Main thread only: the
    /// demand that decides it is set from the pump.
    static ACTIVITY: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(None) };
}

fn table() -> MutexGuard<'static, Vec<Held>> {
    TABLE.lock().unwrap_or_else(|e| e.into_inner())
}

fn queue() -> &'static DispatchQueue {
    QUEUE.get_or_init(|| DispatchQueue::new("automation-platform.gamepad", None))
}

/// Registers the observers, adds the controllers already known, and logs what the run can be
/// judged by. Main thread.
pub fn start(hub: &'static Hub) -> Result<(), String> {
    // An empty bundle identifier left `[GCController controllers]` empty (Apple forum thread
    // 667832). The shipped build runs from the .app and has one; `run-dev.sh` runs the bare
    // binary out of target/, which may not — and "no controllers" there would otherwise look
    // exactly like a bug in this file.
    let bundle = NSRunningApplication::currentApplication()
        .bundleIdentifier()
        .map(|s| s.to_string())
        .unwrap_or_default();
    if bundle.is_empty() {
        crate::logging::line(
            "gamepad",
            "this process has no bundle identifier — GameController has been seen to list no \
             controllers at all in that case; run the .app to rule that out",
        );
        hub.set_status("bundle", "none (GameController may see nothing; run the .app)");
    } else {
        crate::logging::line("gamepad", &format!("bundle identifier {bundle}"));
        hub.set_status("bundle", bundle);
    }

    // Before anything that can add a controller: the observers below may fire on another
    // thread at once, and every add can set this to what macOS answered. Written after them,
    // it overwrote that answer with "not requested yet" whenever a pad was already there at
    // start — the one number a Mac session with a pad exists to read.
    hub.set_status("background", "not requested yet — it is set when the first controller connects");

    hub.set_control(|what| {
        if what == Control::Demand {
            // Only for buttons and axes: a `connected` listener — the probe has one in every
            // session — is no reason to hold a latency-critical activity against App Nap.
            hold_activity(super::demand::wants_values(super::hub().demand()));
        }
    });

    let centre = NSNotificationCenter::defaultCenter();
    let connected = RcBlock::new(|note: NonNull<NSNotification>| {
        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: a live notification for the duration of the call.
            let note = unsafe { note.as_ref() };
            if let Some(c) = note.object().and_then(|o| o.downcast::<GCController>().ok()) {
                add(super::hub(), c);
            }
        }));
        if caught.is_err() {
            crate::logging::line("gamepad", "panic while adding a game controller");
        }
    });
    let disconnected = RcBlock::new(|note: NonNull<NSNotification>| {
        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: as above.
            let note = unsafe { note.as_ref() };
            if let Some(c) = note.object().and_then(|o| o.downcast::<GCController>().ok()) {
                remove(super::hub(), &c);
            }
        }));
        if caught.is_err() {
            crate::logging::line("gamepad", "panic while removing a game controller");
        }
    });
    // SAFETY: framework-owned notification names; blocks the centre copies; queue nil, so the
    // blocks run on the posting thread, which they are written for.
    unsafe {
        let a = centre.addObserverForName_object_queue_usingBlock(
            Some(GCControllerDidConnectNotification),
            None,
            None,
            &connected,
        );
        let b = centre.addObserverForName_object_queue_usingBlock(
            Some(GCControllerDidDisconnectNotification),
            None,
            None,
            &disconnected,
        );
        // Leaked deliberately, like every other subscription in this backend: they last as
        // long as the process.
        core::mem::forget(a);
        core::mem::forget(b);
    }

    // What the framework already knows. Discovery is asynchronous, so this is often empty at
    // launch even with a pad attached; the connect notification brings the rest.
    // SAFETY: a class method with no preconditions.
    let known = unsafe { GCController::controllers() };
    let n = known.len();
    for c in known.iter() {
        add(hub, c);
    }
    hub.set_status("gamecontroller", format!("observing; {n} controller(s) known at start"));
    crate::logging::line("gamepad", &format!("GameController observers registered; {n} controller(s) known at start"));
    Ok(())
}

/// Holds an activity against App Nap while anybody listens for buttons or axes, and lets it
/// go when nobody does.
///
/// `UserInitiatedAllowingIdleSystemSleep | LatencyCritical`, not plain `UserInitiated`, which
/// also keeps the Mac from sleeping — a module listening to a pad is no reason to stop the
/// machine sleeping when its owner walks away.
fn hold_activity(want: bool) {
    ACTIVITY.with(|a| {
        let mut a = a.borrow_mut();
        match (want, a.is_some()) {
            (true, false) => {
                let reason = NSString::from_str("Watching game controllers for an accessibility module");
                let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
                    NSActivityOptions::UserInitiatedAllowingIdleSystemSleep | NSActivityOptions::LatencyCritical,
                    &reason,
                );
                *a = Some(token);
                super::hub().set_status("activity", "held (latency-critical, while a button or axis listener exists)");
            }
            (false, true) => {
                if let Some(token) = a.take() {
                    // SAFETY: the token beginActivity returned, ended once.
                    unsafe { NSProcessInfo::processInfo().endActivity(&token) };
                }
                super::hub().set_status("activity", "not held");
            }
            _ => {}
        }
    });
}

fn add(hub: &Hub, controller: Retained<GCController>) {
    let mut t = table();
    if t.iter().any(|h| Retained::as_ptr(&h.controller) == Retained::as_ptr(&controller)) {
        return;
    }
    // SAFETY: plain property reads on a live controller.
    let Some(gamepad) = (unsafe { controller.extendedGamepad() }) else {
        let name = unsafe { controller.vendorName() }.map(|s| s.to_string()).unwrap_or_default();
        crate::logging::line(
            "gamepad",
            &format!("'{name}' is not an extended gamepad (no two sticks and triggers); it is not watched"),
        );
        return;
    };
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    t.push(Held { id, controller: controller.clone() });
    drop(t);

    enable_background_monitoring(hub);

    let desc = describe(&controller, &gamepad, id);
    let baseline = read(&gamepad);
    // Connected BEFORE the handler is set: a value change that arrived first would be an
    // update for a device the hub does not know yet, and it would be dropped.
    hub.connect(DeviceKey::Apple(id), desc, &baseline, Instant::now());

    let handler = RcBlock::new(move |g: NonNull<GCExtendedGamepad>, _changed: NonNull<GCControllerElement>| {
        let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // SAFETY: the profile is live for the duration of the call.
            let snap = read(unsafe { g.as_ref() });
            super::hub().update(&DeviceKey::Apple(id), &snap, Instant::now());
        }));
        if caught.is_err() {
            crate::logging::line("gamepad", "panic while reading a game controller");
        }
    });
    // SAFETY: our own serial queue; the framework copies the block when it is set.
    unsafe {
        controller.setHandlerQueue(queue());
        gamepad.setValueChangedHandler(RcBlock::as_ptr(&handler));
    }
}

fn remove(hub: &Hub, controller: &GCController) {
    let held = {
        let mut t = table();
        let pos = t.iter().position(|h| Retained::as_ptr(&h.controller) == controller as *const GCController);
        pos.map(|p| t.remove(p))
    };
    let Some(held) = held else { return };
    // A change already queued may still run after this; it names an id the hub no longer
    // knows and is ignored there. Clearing the handler stops any more being queued.
    // SAFETY: a live controller; a null handler is documented as "none".
    unsafe {
        if let Some(g) = held.controller.extendedGamepad() {
            g.setValueChangedHandler(std::ptr::null_mut());
        }
    }
    hub.disconnect(&DeviceKey::Apple(held.id), Instant::now());
}

/// Sets `shouldMonitorBackgroundEvents` once, on the first controller, and says what macOS
/// made of it.
fn enable_background_monitoring(hub: &Hub) {
    if BACKGROUND_DONE.swap(true, Ordering::SeqCst) {
        return;
    }
    // On this objc2 a selector the runtime does not have aborts the process instead of
    // failing, so it is asked first. The property is macOS 11.3+ and the oldest supported
    // macOS is 12, so this is defence rather than expectation.
    let meta = GCController::class().metaclass();
    if !meta.responds_to(sel!(setShouldMonitorBackgroundEvents:)) || !meta.responds_to(sel!(shouldMonitorBackgroundEvents)) {
        hub.set_status("background", "not available on this macOS — pads are heard only while this app is in front");
        crate::logging::line(
            "gamepad",
            "this macOS has no shouldMonitorBackgroundEvents; controller input arrives only while this \
             application is frontmost",
        );
        return;
    }
    // SAFETY: both selectors checked above.
    let before = unsafe { GCController::shouldMonitorBackgroundEvents() };
    if !before {
        unsafe { GCController::setShouldMonitorBackgroundEvents(true) };
    }
    let after = unsafe { GCController::shouldMonitorBackgroundEvents() };
    hub.set_status(
        "background",
        format!("requested on the first controller; macOS reports {}", if after { "on" } else { "OFF" }),
    );
    crate::logging::line(
        "gamepad",
        &format!(
            "background monitoring was {before}, requested, and macOS now reports {after}{}",
            if after {
                ""
            } else {
                " — pads will be heard only while this application is frontmost (macOS 15.4 had this \
                 read-back bug; 15.5 fixed it)"
            }
        ),
    );
}

fn pressed(b: &GCControllerButtonInput) -> bool {
    // SAFETY: a plain property read.
    unsafe { b.isPressed() }
}

/// Everything the profile says, as one report. Y is negated: GameController has up positive,
/// the hub has down positive, like SDL and like every screen.
fn read(g: &GCExtendedGamepad) -> Snapshot {
    let mut s = Snapshot::default();
    // SAFETY: property reads on a live profile, all present on macOS 12, which is the oldest
    // this application supports; the optional ones are `Option` for exactly that reason.
    unsafe {
        let mut put = |b: Button, input: Option<Retained<GCControllerButtonInput>>| {
            if input.is_some_and(|i| pressed(&i)) {
                s.pressed.insert(b);
            }
        };
        put(Button::South, Some(g.buttonA()));
        put(Button::East, Some(g.buttonB()));
        put(Button::West, Some(g.buttonX()));
        put(Button::North, Some(g.buttonY()));
        put(Button::Start, Some(g.buttonMenu()));
        put(Button::Back, g.buttonOptions());
        put(Button::Guide, g.buttonHome());
        put(Button::LeftShoulder, Some(g.leftShoulder()));
        put(Button::RightShoulder, Some(g.rightShoulder()));
        put(Button::LeftStick, g.leftThumbstickButton());
        put(Button::RightStick, g.rightThumbstickButton());
        let dpad = g.dpad();
        put(Button::DpadUp, Some(dpad.up()));
        put(Button::DpadDown, Some(dpad.down()));
        put(Button::DpadLeft, Some(dpad.left()));
        put(Button::DpadRight, Some(dpad.right()));
        if let Some(ds) = g.downcast_ref::<GCDualShockGamepad>() {
            put(Button::Touchpad, ds.touchpadButton());
        }
        if let Some(ds) = g.downcast_ref::<GCDualSenseGamepad>() {
            put(Button::Touchpad, Some(ds.touchpadButton()));
        }
        if let Some(x) = g.downcast_ref::<GCXboxGamepad>() {
            // P1/P2 are the right-hand paddles and P3/P4 the left, upper before lower, which is
            // SDL's right_paddle1/right_paddle2/left_paddle1/left_paddle2. Unmeasured on a Mac.
            put(Button::RightPaddle1, x.paddleButton1());
            put(Button::RightPaddle2, x.paddleButton2());
            put(Button::LeftPaddle1, x.paddleButton3());
            put(Button::LeftPaddle2, x.paddleButton4());
            put(Button::Misc1, x.buttonShare());
        }
        let (ls, rs) = (g.leftThumbstick(), g.rightThumbstick());
        s.axes = vec![
            (Axis::LeftX, ls.xAxis().value()),
            (Axis::LeftY, -ls.yAxis().value()),
            (Axis::RightX, rs.xAxis().value()),
            (Axis::RightY, -rs.yAxis().value()),
            (Axis::LeftTrigger, g.leftTrigger().value()),
            (Axis::RightTrigger, g.rightTrigger().value()),
        ];
    }
    s
}

fn describe(c: &GCController, g: &GCExtendedGamepad, id: u64) -> PadDesc {
    // SAFETY: property reads on live objects; see `read`.
    unsafe {
        let category = c.productCategory().to_string();
        let name = c.vendorName().map(|s| s.to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| category.clone());
        let playstation = g.downcast_ref::<GCDualShockGamepad>().is_some() || g.downcast_ref::<GCDualSenseGamepad>().is_some();
        let xbox = g.downcast_ref::<GCXboxGamepad>().is_some();
        let lower = category.to_lowercase();
        let family = if playstation || lower.contains("dualshock") || lower.contains("dualsense") {
            Family::PlayStation
        } else if xbox || lower.contains("xbox") {
            Family::Xbox
        } else if lower.contains("switch") || lower.contains("joy-con") || lower.contains("nintendo") {
            Family::Nintendo
        } else {
            Family::Generic
        };

        let mut buttons = vec![
            Button::South,
            Button::East,
            Button::West,
            Button::North,
            Button::Start,
            Button::LeftShoulder,
            Button::RightShoulder,
            Button::DpadUp,
            Button::DpadDown,
            Button::DpadLeft,
            Button::DpadRight,
        ];
        let mut maybe = |b: Button, present: bool| {
            if present {
                buttons.push(b);
            }
        };
        maybe(Button::Back, g.buttonOptions().is_some());
        maybe(Button::Guide, g.buttonHome().is_some());
        maybe(Button::LeftStick, g.leftThumbstickButton().is_some());
        maybe(Button::RightStick, g.rightThumbstickButton().is_some());
        if let Some(ds) = g.downcast_ref::<GCDualShockGamepad>() {
            maybe(Button::Touchpad, ds.touchpadButton().is_some());
        }
        maybe(Button::Touchpad, g.downcast_ref::<GCDualSenseGamepad>().is_some());
        if let Some(x) = g.downcast_ref::<GCXboxGamepad>() {
            maybe(Button::RightPaddle1, x.paddleButton1().is_some());
            maybe(Button::RightPaddle2, x.paddleButton2().is_some());
            maybe(Button::LeftPaddle1, x.paddleButton3().is_some());
            maybe(Button::LeftPaddle2, x.paddleButton4().is_some());
            maybe(Button::Misc1, x.buttonShare().is_some());
        }
        // In SDL's order, the order everything else lists buttons in.
        buttons.sort();
        buttons.dedup();

        PadDesc {
            id: format!("gc:{id}"),
            name: if name.is_empty() { "Game controller".to_string() } else { name },
            family,
            source: Source::GameController,
            mapped: true,
            vendor: None,
            product: None,
            buttons,
            axes: names::AXES.to_vec(),
        }
    }
}
