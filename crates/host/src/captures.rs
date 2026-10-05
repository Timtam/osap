//! Captured keys on the host's side: every registration `host.keys.capture` made, in the order it
//! was made, and what each module set with `host.keys.scope` and `host.keys.menuOpen`.
//!
//! **Each module's own.** The scope and the menu flag were one switch for the whole application,
//! and the last caller won: an overlay that deactivated late set back the scope and the flag of
//! another module's overlay that had activated meanwhile, and which of the two came last depended
//! on the order the modules were loaded in. Each module now sets its own entry here, and the
//! keyboard hook or the event tap decides every key by the entries of the modules that capture it
//! ([`crate::backend::capture_decision`]): the earliest capture whose module is scoped to the
//! window in front, or to everywhere, takes it. This table is the truth; the backend is handed a
//! copy whenever it changes, and a disabled, reloaded or removed module's entry goes with its
//! captures ([`Captures::forget_owner`]).
//!
//! **Who gets a key.** The hook or the tap names the module it took a key for and the window that
//! was in front ([`crate::backend::Taken`]). [`arrive`] hands it to that module's mailbox for that
//! module's registration of the key — never another module's — while the module's scope is still
//! everywhere or that window; otherwise the key is dropped, and the log says why. A key that waited
//! in a busy module's mailbox is looked at again when it runs ([`open_key`]): its registration, or
//! — made again meanwhile, by an overlay that went and came back — the module's new registration of
//! the same key, under the same rule ([`key_target`]). The scope is asked of its own registration
//! too, which b0-final.md's rule 4 hands the key to whenever it is still there: a key pressed in
//! one window does not run on a capture its module has pinned to another since — the rule the hook
//! decided it by. A hotkey, which no scope pins, goes to its own registration unasked. Another
//! module's registration gets such a key only once its own module has let go of it (below).
//!
//! **What the module let go of goes where it would have gone.** A key or a hotkey press that
//! waited in its busy module's mailbox, and whose registration the module released meanwhile
//! without making it again, was kept at the press for nothing: it goes where it would have gone
//! had the module never captured it. First to the next module that captures it in the window it
//! was pressed in — the earliest capture of the combination whose module is scoped to that window
//! or to everywhere, the hook's own rule ([`offered_to`]) — through that module's mailbox, as a
//! key the hook took for it, under that module's rules. With none, it is passed on to the program
//! in front, as it was pressed, while the window it was pressed in is still in front and nothing
//! has reached the program since that it would have come before ([`pass_on`], with
//! [`crate::backend::pass_on_strokes`] deciding). The maintainer's decisions of 2026-10-05: the
//! program, and, asked again the same day, another module's capture first. Otherwise it is
//! dropped, and the log says why. The modules that let go of a key travel with it and are never
//! offered it again, so it goes to one place, and on at most once per module. A key the module's
//! scope has since pinned to another window, or a hotkey another module holds now, is dropped as
//! before.
//!
//! Kept in a file of its own, with the bindings generic over [`KeyHost`], so the rules can be run
//! against real Luau VMs and the stub backend without building the whole host
//! (`key_scope_tests.rs`). The bindings themselves are in lib.rs (`install_key_captures`), where
//! every `host.*` binding is listed.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Value};

use crate::backend::{self, Backend, Captured, KeyOs, NotPassed, OwnerKeys, Pressed};
use crate::logging;
use crate::mailbox::{self, Event, MailHost, Opened};
use crate::task::Ctx;

/// How often a module's dropped keys are said in the log at most: a key a module no longer
/// captures, pressed again and again, is one fact, not a line per press.
const DROPPED_QUIET: Duration = Duration::from_secs(10);

/// One capture: the combination, the module that holds it, its release token, and its callback in
/// the VM it was made from.
pub(crate) struct Reg {
    pub(crate) vk: u32,
    pub(crate) mask: u8,
    pub(crate) owner: usize,
    pub(crate) token: i64,
    pub(crate) lua: Lua,
    pub(crate) cb: RegistryKey,
}

/// What one module set: the window its captures are pinned to (0, everywhere) and whether it says
/// a menu is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Entry {
    scope: isize,
    menu: bool,
}

/// Every module's captures and their scopes and menu flags. Main thread only, like the rest of
/// `Shared`.
#[derive(Default)]
pub(crate) struct Captures {
    /// In registration order — the order that decides between two modules' captures of one key
    /// in one window. One per combination and module: a module's new capture of a combination
    /// replaces its old one and goes to the back.
    regs: RefCell<Vec<Reg>>,
    /// module → its entry. A module that never called `scope` or `menuOpen` has none, and is
    /// everywhere with no menu. Kept, once made, until the module is disabled, reloaded or
    /// removed: going back to everywhere with no menu is an entry like any other, so that the
    /// backend's record of the keys let through for its menu stays its own until it reads it.
    owners: RefCell<BTreeMap<usize, Entry>>,
    /// The last release token handed out. Positive, unique for the life of the process, never
    /// reused, so a stale token cannot release a capture somebody made later.
    next_token: Cell<i64>,
    /// module → when its last dropped key was said (`DROPPED_QUIET`).
    dropped_said: RefCell<HashMap<usize, Instant>>,
    /// module → when its last key that went to its new registration was said.
    moved_said: RefCell<HashMap<usize, Instant>>,
    /// module → when its last key passed on to the program in front was said.
    passed_said: RefCell<HashMap<usize, Instant>>,
    /// module → when its last key that went on to another module's capture was said.
    offered_said: RefCell<HashMap<usize, Instant>>,
}

impl Captures {
    /// No module captures anything.
    pub(crate) fn is_empty(&self) -> bool {
        self.regs.borrow().is_empty()
    }

    /// Files module `owner`'s capture of `(vk, mask)`, replacing its own earlier one, and returns
    /// its token.
    pub(crate) fn capture(&self, owner: usize, vk: u32, mask: u8, lua: Lua, cb: RegistryKey) -> i64 {
        let token = self.next_token.get() + 1;
        self.next_token.set(token);
        let mut regs = self.regs.borrow_mut();
        regs.retain(|r| !(r.vk == vk && r.mask == mask && r.owner == owner));
        regs.push(Reg { vk, mask, owner, token, lua, cb });
        token
    }

    /// Takes the capture `token` out; whether there was one.
    pub(crate) fn release(&self, token: i64) -> bool {
        let mut regs = self.regs.borrow_mut();
        let before = regs.len();
        regs.retain(|r| r.token != token);
        regs.len() != before
    }

    /// Takes every capture of module `owner` out.
    pub(crate) fn release_all(&self, owner: usize) {
        self.regs.borrow_mut().retain(|r| r.owner != owner);
    }

    /// Takes the captures of every module `gone` names out — a reload's purge, a rolled-back
    /// hot-load. Their scopes and flags go separately ([`Captures::forget_owner`]), after the
    /// arbiter's re-election, whose `onDeactivate` would otherwise write them again.
    pub(crate) fn drop_registrations(&self, gone: impl Fn(usize) -> bool) {
        self.regs.borrow_mut().retain(|r| !gone(r.owner));
    }

    /// Pins module `owner`'s captures to `window`, 0 for everywhere. Whether that changed
    /// anything, so an unchanged scope is not handed to the backend again.
    pub(crate) fn set_scope(&self, owner: usize, window: isize) -> bool {
        let mut owners = self.owners.borrow_mut();
        let fresh = !owners.contains_key(&owner);
        let e = owners.entry(owner).or_default();
        let changed = e.scope != window;
        e.scope = window;
        changed || fresh
    }

    /// Says module `owner` has a menu open (`true`) or none. Whether that changed anything: the
    /// overlay runtime writes its flag on every tick of its menu timer.
    pub(crate) fn set_menu(&self, owner: usize, open: bool) -> bool {
        let mut owners = self.owners.borrow_mut();
        let fresh = !owners.contains_key(&owner);
        let e = owners.entry(owner).or_default();
        let changed = e.menu != open;
        e.menu = open;
        changed || fresh
    }

    /// The window module `owner`'s captures are pinned to, 0 for everywhere.
    pub(crate) fn scope_of(&self, owner: usize) -> isize {
        self.owners.borrow().get(&owner).map_or(0, |e| e.scope)
    }

    /// Whether module `owner` says a menu is open.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn menu_of(&self, owner: usize) -> bool {
        self.owners.borrow().get(&owner).is_some_and(|e| e.menu)
    }

    /// Drops module `owner`'s scope and flag: it captures everywhere again, with no menu — what a
    /// disable, a reload and a removal do, beside dropping its captures. Whether it had an entry.
    pub(crate) fn forget_owner(&self, owner: usize) -> bool {
        self.dropped_said.borrow_mut().remove(&owner);
        self.moved_said.borrow_mut().remove(&owner);
        self.passed_said.borrow_mut().remove(&owner);
        self.offered_said.borrow_mut().remove(&owner);
        self.owners.borrow_mut().remove(&owner).is_some()
    }

    /// [`Captures::forget_owner`] for every module from index `n` on: a rolled-back hot-load.
    pub(crate) fn forget_owners_from(&self, n: usize) -> bool {
        self.dropped_said.borrow_mut().retain(|i, _| *i < n);
        self.moved_said.borrow_mut().retain(|i, _| *i < n);
        self.passed_said.borrow_mut().retain(|i, _| *i < n);
        self.offered_said.borrow_mut().retain(|i, _| *i < n);
        let mut owners = self.owners.borrow_mut();
        let before = owners.len();
        owners.retain(|i, _| *i < n);
        owners.len() != before
    }

    /// The captures of the modules `enabled` says are on, in registration order: what the hook
    /// or the tap decides by.
    pub(crate) fn set(&self, enabled: impl Fn(usize) -> bool) -> Vec<Captured> {
        self.regs
            .borrow()
            .iter()
            .filter(|r| enabled(r.owner))
            .map(|r| Captured { vk: r.vk, mask: r.mask, owner: r.owner as u32 })
            .collect()
    }

    /// The entries of the modules `enabled` says are on.
    pub(crate) fn owner_entries(&self, enabled: impl Fn(usize) -> bool) -> Vec<OwnerKeys> {
        self.owners
            .borrow()
            .iter()
            .filter(|(i, _)| enabled(**i))
            .map(|(i, e)| OwnerKeys { owner: *i as u32, scope: e.scope, menu: e.menu })
            .collect()
    }

    /// The line the trace switch writes for the captured set: each combination once, in the order
    /// it was first captured, with the modules that hold it in the order that decides — each with
    /// its scope when it has one (`@0x…`) and `+menu` while it says a menu is open.
    pub(crate) fn captured_line(&self, set: &[Captured], id_of: impl Fn(usize) -> String) -> String {
        let owners = self.owners.borrow();
        let mut keys: Vec<(u32, u8)> = Vec::new();
        for c in set {
            if !keys.contains(&(c.vk, c.mask)) {
                keys.push((c.vk, c.mask));
            }
        }
        let each: Vec<String> = keys
            .iter()
            .map(|&(vk, m)| {
                let holders: Vec<String> = set
                    .iter()
                    .filter(|c| c.vk == vk && c.mask == m)
                    .map(|c| {
                        let idx = c.owner as usize;
                        let e = owners.get(&idx).copied().unwrap_or_default();
                        let scope = if e.scope == 0 { String::new() } else { format!("@{:#x}", e.scope) };
                        let menu = if e.menu { "+menu" } else { "" };
                        format!("{}{scope}{menu}", id_of(idx))
                    })
                    .collect();
                format!("vk 0x{vk:02X}/m{m}[{}]", holders.join(","))
            })
            .collect();
        format!("captured set: {}", each.join(" "))
    }

    /// Whether module `owner`'s dropped key — or hotkey — is to be said at `now`: the first of
    /// [`DROPPED_QUIET`], per module.
    pub(crate) fn say_dropped(&self, owner: usize, now: Instant) -> bool {
        once_per_quiet(&mut self.dropped_said.borrow_mut(), owner, now)
    }

    /// Whether module `owner`'s key or hotkey that went to its new registration is to be said at
    /// `now`: the first of [`DROPPED_QUIET`], per module, on a clock of its own.
    pub(crate) fn say_moved(&self, owner: usize, now: Instant) -> bool {
        once_per_quiet(&mut self.moved_said.borrow_mut(), owner, now)
    }

    /// Whether module `owner`'s key or hotkey passed on to the program in front is to be said at
    /// `now`: the first of [`DROPPED_QUIET`], per module, on a clock of its own.
    pub(crate) fn say_passed(&self, owner: usize, now: Instant) -> bool {
        once_per_quiet(&mut self.passed_said.borrow_mut(), owner, now)
    }

    /// Whether module `owner`'s key or hotkey that went on to another module's capture is to be
    /// said at `now`: the first of [`DROPPED_QUIET`], per module, on a clock of its own.
    pub(crate) fn say_offered(&self, owner: usize, now: Instant) -> bool {
        once_per_quiet(&mut self.offered_said.borrow_mut(), owner, now)
    }

    /// Module `owner`'s capture of `(vk, mask)`: its token and the VM it was made from.
    fn reg_for(&self, owner: usize, vk: u32, mask: u8) -> Option<(i64, Lua)> {
        self.regs.borrow().iter().find(|r| r.owner == owner && r.vk == vk && r.mask == mask).map(|r| (r.token, r.lua.clone()))
    }
}

fn once_per_quiet(said: &mut HashMap<usize, Instant>, owner: usize, now: Instant) -> bool {
    match said.get(&owner) {
        Some(at) if now.saturating_duration_since(*at) < DROPPED_QUIET => false,
        _ => {
            said.insert(owner, now);
            true
        }
    }
}

/// What the key bindings and the delivery need of the host: the captures, the backend the hook or
/// the tap is in, which modules are on, and their names. The host's `Shared`, or a test's holder.
pub(crate) trait KeyHost: 'static {
    fn captures(&self) -> &Captures;
    fn key_backend(&self) -> &dyn Backend;
    /// Whether module `idx` is enabled: a disabled module's captures, scope and flag are not
    /// handed to the backend, and a key taken for it is not run.
    fn key_module_enabled(&self, idx: usize) -> bool;
    /// The module's id, for the log.
    fn key_module_id(&self, idx: usize) -> String;
    /// Whether a hotkey of this application holds the combination `(vk, mask)` at the system now,
    /// enabled module or not: one that would take a key passed on to the program ([`pass_on`]).
    fn key_hotkey_holds(&self, vk: u32, mask: u8) -> bool;
    /// The captures changed, or which modules are on: the backend gets the set and the owners
    /// again. The host's own also writes the trace line.
    fn refresh_captured(&self) {
        send_set(self);
        send_owners(self);
    }
    /// A module's scope or flag changed: the backend gets the owners again.
    fn refresh_key_owners(&self) {
        send_owners(self);
    }
}

/// Hands the backend the captures of the enabled modules.
pub(crate) fn send_set<H: KeyHost + ?Sized>(h: &H) {
    let set = h.captures().set(|i| h.key_module_enabled(i));
    h.key_backend().set_captured_keys(&set);
}

/// Hands the backend the scopes and flags of the enabled modules.
pub(crate) fn send_owners<H: KeyHost + ?Sized>(h: &H) {
    let owners = h.captures().owner_entries(|i| h.key_module_enabled(i));
    h.key_backend().set_key_owners(&owners);
}

/// Why a key the hook or the tap took for a module was not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gone {
    /// The module no longer captures the combination.
    Released,
    /// The module's scope is a window other than the one the key was pressed in now.
    ScopeMoved,
    /// The module was disabled. Not said: the user switched it off.
    Disabled,
    /// Another module holds the hotkey's combination now (hotkeys only).
    TakenOver,
}

/// Where a press goes, decided when it arrives and again when it runs: by a capture's token, or a
/// hotkey's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target<T = i64> {
    /// To the registration it was pressed for, which is still there.
    Same(T),
    /// To the module's new registration of the same key — the one it was pressed for was released
    /// and made again while the press waited in the module's mailbox.
    Moved(T),
    Gone(Gone),
}

/// The registration a key-down of `(vk, mask)` the hook took for module `owner`, in window `front`,
/// goes to: the capture `token`, while it is there — on arrival, with no token yet, the module's
/// capture of the combination — and otherwise the module's new capture of the same combination.
/// Only while the module is on and its scope is everywhere or `front`. Never another module's: the
/// key was decided for this one, by this one's scope; a key it let go of while the key waited goes
/// on to another module only by [`pass_on_released`], as the hook would have sent it there.
///
/// HEAD looked the key up afresh in every module's registrations, earliest first, which handed a
/// key to another module whenever an activation earlier in the same turn of the loop had
/// released the module's own capture.
pub(crate) fn key_target<H: KeyHost + ?Sized>(
    h: &H,
    owner: usize,
    vk: u32,
    mask: u8,
    front: isize,
    token: Option<i64>,
) -> Target {
    if !h.key_module_enabled(owner) {
        return Target::Gone(Gone::Disabled);
    }
    let caps = h.captures();
    let regs = caps.regs.borrow();
    let Some(r) = regs.iter().find(|r| r.owner == owner && r.vk == vk && r.mask == mask) else {
        return Target::Gone(Gone::Released);
    };
    let scope = caps.scope_of(owner);
    if scope != 0 && scope != front {
        return Target::Gone(Gone::ScopeMoved);
    }
    match token {
        Some(t) if t != r.token => Target::Moved(r.token),
        _ => Target::Same(r.token),
    }
}

/// The capture `token`'s VM and callback.
fn reg_of<H: KeyHost + ?Sized>(h: &H, token: i64) -> Option<(Lua, Function)> {
    let regs = h.captures().regs.borrow();
    let r = regs.iter().find(|r| r.token == token)?;
    let f = r.lua.registry_value::<Function>(&r.cb).ok()?;
    Some((r.lua.clone(), f))
}

/// What became of a key the hook or the tap took, as it arrived.
#[derive(Debug, PartialEq)]
pub(crate) enum Arrived {
    /// Handed to its module's mailbox: run at once, or queued while the module is busy.
    Delivered(mailbox::Delivered),
    /// Not handed over — see [`Gone`].
    Dropped(Gone),
}

/// A key-down the hook or the tap took for module `owner` arrives, `pressed` in the window
/// `pressed.front`: handed to the module's mailbox for its capture of the key ([`key_target`]), or
/// dropped and said — at most once every ten seconds per module, and not for a disabled module.
pub(crate) fn arrive<H: KeyHost + MailHost>(h: &H, vk: u32, mods: u8, owner: usize, repeat: bool, pressed: Pressed) -> Arrived {
    match key_target(h, owner, vk, mods, pressed.front, None) {
        Target::Same(token) | Target::Moved(token) => match reg_of(h, token) {
            Some((lua, _)) => {
                let ev = Event::Key { owner, token, vk, mods, repeat, pressed, let_go: Vec::new() };
                Arrived::Delivered(mailbox::deliver(h, owner, &lua, ev))
            }
            None => Arrived::Dropped(Gone::Released),
        },
        Target::Gone(gone) => {
            if gone != Gone::Disabled && h.captures().say_dropped(owner, Instant::now()) {
                let spec = backend::spec_name_for(KeyOs::CURRENT, vk, mods);
                logging::line("keys", &dropped_line(&h.key_module_id(owner), &spec, gone));
            }
            Arrived::Dropped(gone)
        }
    }
}

/// A key as it runs (`Event::Key`): its capture, or the module's new capture of it, with the
/// `mods` table the callback gets — or nothing, said once every ten seconds per module. A key whose
/// capture the module released meanwhile, and did not make again, goes on where it would have gone
/// had the module never captured it: to the next module that captures it in the window it was
/// pressed in, or to the program in front ([`pass_on_released`]); `repeat` and `let_go` go with it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_key<H: KeyHost + MailHost>(
    h: &H,
    owner: usize,
    token: i64,
    vk: u32,
    mods: u8,
    repeat: bool,
    pressed: Pressed,
    let_go: Vec<usize>,
) -> Opened {
    let target = key_target(h, owner, vk, mods, pressed.front, Some(token));
    let spec = || backend::spec_name_for(KeyOs::CURRENT, vk, mods);
    let token = match target {
        Target::Same(t) => t,
        // Only a key that waited in its busy module's mailbox gets here: one run at once was
        // looked up a moment ago, as it arrived.
        Target::Moved(t) => {
            if h.captures().say_moved(owner, Instant::now()) {
                logging::line("keys", &moved_line(&h.key_module_id(owner), &spec(), true));
            }
            t
        }
        // Here too, only a key that waited: the hook kept it from the program for a capture the
        // module has let go of since.
        Target::Gone(Gone::Released) => {
            pass_on_released(h, owner, &spec(), Some((vk, mods)), (repeat, pressed), let_go);
            return Opened::Gone;
        }
        Target::Gone(gone) => {
            if gone != Gone::Disabled && h.captures().say_dropped(owner, Instant::now()) {
                logging::line("keys", &busy_dropped_line(&h.key_module_id(owner), &spec(), gone));
            }
            return Opened::Gone;
        }
    };
    let Some((lua, f)) = reg_of(h, token) else { return Opened::Gone };
    let args = match mods_table(&lua, mods) {
        Ok(t) => MultiValue::from_vec(vec![Value::Table(t)]),
        Err(_) => MultiValue::new(),
    };
    Opened::Run { f, args, ctx: Ctx::new("key", owner) }
}

/// What became of a key or a hotkey press that waited in its busy module's mailbox, whose
/// registration the module released meanwhile and did not make again ([`pass_on_released`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PassedOn {
    /// Handed to the mailbox of the next module that captures it in the window it was pressed in,
    /// named by its id ([`offered_to`]); what that module does with it is that module's.
    Offered(String),
    /// Sent to the program in front, as it was pressed.
    Sent,
    /// Not sent, for this reason.
    NotSent(NotPassed),
    /// The backend could not send it, for this reason; nothing it sent is left down.
    Failed(String),
    /// A hotkey whose combination only the system read (`HotkeyReg::binding` is `None`): there is
    /// no key to send.
    NoKey,
}

/// Passes key `vk`, pressed with the roles in `mask` as `pressed` says, on to the program in front
/// — the key a busy module's registration took at the press, and so kept from that program, and
/// that the module no longer holds as its mailbox delivers it: the same key with the same
/// modifiers, while the window it was pressed in is still in front, the key itself is not held
/// down, no modifier and no screen reader's key is held that the press was made without, and no
/// key-down has reached the program since the press ([`backend::pass_on_strokes`]); and not while
/// a hotkey of this application holds the combination, which the system would hand the key to
/// instead ([`NotPassed::HotkeyHolds`], asked after the others). Sent by the backend, marked so
/// that the hook or the tap lets it through and no capture takes it again
/// ([`Backend::pass_on_key`]).
pub(crate) fn pass_on<H: KeyHost + ?Sized>(h: &H, vk: u32, mask: u8, pressed: Pressed) -> PassedOn {
    let b = h.key_backend();
    match backend::pass_on_strokes(mask, pressed, b.keyboard_now(vk, mask, pressed.phys)) {
        Ok(_) if h.key_hotkey_holds(vk, mask) => PassedOn::NotSent(NotPassed::HotkeyHolds),
        Ok(strokes) => match b.pass_on_key(vk, mask, pressed.phys, &strokes) {
            Ok(()) => PassedOn::Sent,
            Err(e) => PassedOn::Failed(e),
        },
        Err(why) => PassedOn::NotSent(why),
    }
}

/// The module a key of `(vk, mask)` pressed in window `front` goes on to once the modules in
/// `let_go` have let go of it: of the captures in `set`, in registration order, the earliest of a
/// module not in `let_go` that is scoped to `front` or to everywhere — the rule the hook takes a
/// key by ([`backend::in_scope`], [`backend::capture_decision`]), so the module the hook would have
/// taken it for had those never captured it. A menu is not asked: none was open there at the
/// press, or the hook would not have taken the key, and a key that waited is not looked at for one
/// since. `None`: no other module captures it there. `set` and `owners` are the enabled modules',
/// as the hook is handed them ([`Captures::set`], [`Captures::owner_entries`]).
pub(crate) fn offered_to(
    set: &[Captured],
    owners: &[OwnerKeys],
    vk: u32,
    mask: u8,
    front: isize,
    let_go: &[usize],
) -> Option<usize> {
    set.iter()
        .filter(|c| c.vk == vk && c.mask == mask && !let_go.contains(&(c.owner as usize)))
        .find(|c| backend::in_scope(owners, c.owner, front))
        .map(|c| c.owner as usize)
}

/// A key or a hotkey press, written `spec`, that waited in module `owner`'s mailbox and whose
/// registration the module released meanwhile without making it again — `key` is its `(vk, mask)`,
/// `None` for a hotkey only the system read; `held` its repeat flag and its press; `let_go` the
/// modules that let go of it before it came to `owner`. It goes where it would have gone had the
/// modules that let go of it never captured it: to the next module that captures it in the window
/// it was pressed in ([`offered_to`]), through that module's mailbox as a key the hook took for it,
/// with `owner` added to `let_go` so that it never comes back to a module that let it go and
/// moves on at most once per module; with none, to the program in front ([`pass_on`]). Said at most
/// once every ten seconds per module: that it went to another module and that it was passed on,
/// each on a clock of its own, and why it went nowhere on the clock of the dropped keys.
pub(crate) fn pass_on_released<H: KeyHost + MailHost>(
    h: &H,
    owner: usize,
    spec: &str,
    key: Option<(u32, u8)>,
    held: (bool, Pressed),
    mut let_go: Vec<usize>,
) -> PassedOn {
    let (repeat, pressed) = held;
    let say = |outcome: &PassedOn| {
        let (caps, now) = (h.captures(), Instant::now());
        let due = match outcome {
            PassedOn::Offered(_) => caps.say_offered(owner, now),
            PassedOn::Sent => caps.say_passed(owner, now),
            _ => caps.say_dropped(owner, now),
        };
        if due {
            logging::line("keys", &released_line(&h.key_module_id(owner), spec, outcome));
        }
    };
    let Some((vk, mask)) = key else {
        say(&PassedOn::NoKey);
        return PassedOn::NoKey;
    };
    let_go.push(owner);
    let caps = h.captures();
    let enabled = |i| h.key_module_enabled(i);
    let next = offered_to(&caps.set(enabled), &caps.owner_entries(enabled), vk, mask, pressed.front, &let_go)
        .and_then(|to| caps.reg_for(to, vk, mask).map(|(token, lua)| (to, token, lua)));
    if let Some((to, token, lua)) = next {
        // Said before it is handed over: the module it goes to may run it at once.
        let outcome = PassedOn::Offered(h.key_module_id(to));
        say(&outcome);
        mailbox::deliver(h, to, &lua, Event::Key { owner: to, token, vk, mods: mask, repeat, pressed, let_go });
        return outcome;
    }
    let outcome = pass_on(h, vk, mask, pressed);
    say(&outcome);
    outcome
}

/// The line for a key or hotkey that waited in its busy module's mailbox and whose registration was
/// released meanwhile: gone on to another module's capture, passed on to the program in front, or
/// dropped and why.
pub(crate) fn released_line(module: &str, spec: &str, outcome: &PassedOn) -> String {
    let why = match outcome {
        PassedOn::Offered(to) => {
            return format!(
                "[{module}] {spec}, pressed while the module was busy, went to {to}, the next module that captures it \
                 in the window it was pressed in: its registration was released meanwhile"
            )
        }
        PassedOn::Sent => {
            return format!(
                "[{module}] {spec}, pressed while the module was busy, was passed on to the program in front: its \
                 registration was released meanwhile"
            )
        }
        PassedOn::NotSent(NotPassed::Tap) => {
            return format!(
                "[{module}] {spec}, pressed while the module was busy, was not run: its registration was released \
                 meanwhile, and the program in front had it already, as a modifier tap is never kept from it"
            )
        }
        PassedOn::NotSent(NotPassed::OtherWindow) => "the window it was pressed in is no longer in front".to_string(),
        PassedOn::NotSent(NotPassed::NoWindow) => "no window was known to be in front when it was pressed".to_string(),
        PassedOn::NotSent(NotPassed::KeyHeld) => "the key is still held down".to_string(),
        PassedOn::NotSent(NotPassed::OtherModifiers(m)) => {
            format!("{} is held down now, which the press was made without", modifier_words(*m))
        }
        PassedOn::NotSent(NotPassed::FrontUnknown) => "nobody could say which window is in front now".to_string(),
        PassedOn::NotSent(NotPassed::ReaderModifier) => {
            "the screen reader's key is held down now, which the press was made without".to_string()
        }
        PassedOn::NotSent(NotPassed::Overtaken) => "a key typed after it has reached the program first".to_string(),
        PassedOn::NotSent(NotPassed::HotkeyHolds) => {
            "a hotkey holds its combination now, which would take it instead of the program".to_string()
        }
        PassedOn::Failed(e) => format!("passing it on to the program in front failed: {e}"),
        PassedOn::NoKey => "its combination is one only the system could read, so it cannot be sent again".to_string(),
    };
    format!(
        "[{module}] {spec}, pressed while the module was busy, was dropped: its registration was released meanwhile, \
         and {why}"
    )
}

/// The modifier roles in `mask` as this platform writes them, joined with `+`: `Ctrl+Shift`, or on a
/// Mac `Shift+Cmd`.
fn modifier_words(mask: u8) -> String {
    let words: Vec<&str> =
        backend::role_words(KeyOs::CURRENT).iter().filter(|w| mask & w.role != 0).map(|w| w.short).collect();
    words.join("+")
}

/// The table a captured key's callback gets: one field per modifier role — on a Mac `ctrl` is
/// Command held, `win` Control.
fn mods_table(lua: &Lua, mods: u8) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (name, held) in backend::capture_mods_fields(mods) {
        t.set(name, held)?;
    }
    Ok(t)
}

/// What a dropped key or hotkey's line says it lost.
fn why_words(gone: Gone) -> &'static str {
    match gone {
        Gone::Released => "its capture was released",
        Gone::ScopeMoved => "the module's scope moved to another window",
        Gone::Disabled => "the module was disabled",
        Gone::TakenOver => "the hotkey went to another module",
    }
}

/// The line for a key a module's capture took and that was not run.
pub(crate) fn dropped_line(module: &str, spec: &str, gone: Gone) -> String {
    format!(
        "[{module}] {spec} was dropped: {} between the press and its delivery. Said at most once \
         every {} s per module",
        why_words(gone),
        DROPPED_QUIET.as_secs()
    )
}

/// The line for a key or hotkey that waited in its busy module's mailbox and was not run: its
/// module's scope moved to another window, or another module holds the hotkey now. One whose
/// registration was released is [`released_line`]'s.
pub(crate) fn busy_dropped_line(module: &str, spec: &str, gone: Gone) -> String {
    format!("[{module}] {spec}, pressed while the module was busy, was dropped: {} meanwhile", registration_words(gone))
}

/// The line for a hotkey its free module did not run: its registration was released, or handed to
/// another module, between the press and the turn of the loop that delivered it — a `WM_HOTKEY`
/// already on its way when the combination changed hands.
pub(crate) fn hotkey_dropped_line(module: &str, spec: &str, gone: Gone) -> String {
    format!(
        "[{module}] {spec} was dropped: {} between the press and its delivery. Said at most once every {} s \
         per module",
        registration_words(gone),
        DROPPED_QUIET.as_secs()
    )
}

/// What a dropped key or hotkey's line says it lost, a capture or a hotkey alike.
fn registration_words(gone: Gone) -> &'static str {
    match gone {
        Gone::Released => "its registration was released",
        other => why_words(other),
    }
}

/// The line for a key or hotkey that went to the module's new registration of it: one that waited
/// in its busy module's mailbox (`queued`), or a hotkey whose registration was made again between
/// the press and its delivery while its module was free.
pub(crate) fn moved_line(module: &str, spec: &str, queued: bool) -> String {
    if queued {
        format!("[{module}] {spec}, pressed while the module was busy, went to the module's current registration of it")
    } else {
        format!(
            "[{module}] {spec} went to the module's current registration of it, made again between the press \
             and its delivery"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(c: &Captures, lua: &Lua, owner: usize, vk: u32, mask: u8) -> i64 {
        let cb = lua.create_registry_value(lua.create_function(|_, ()| Ok(())).unwrap()).unwrap();
        c.capture(owner, vk, mask, lua.clone(), cb)
    }

    /// Registration order across modules, one capture per combination and module (a module's
    /// new one goes to the back), and the tokens: new each time, a stale one releases nothing.
    #[test]
    fn captures_keep_their_order_and_one_per_combination_and_module() {
        let c = Captures::default();
        let lua = Lua::new();
        let a_tab = reg(&c, &lua, 1, 0x09, 0);
        reg(&c, &lua, 2, 0x09, 0);
        reg(&c, &lua, 1, 0x20, 0);
        let all = |_| true;
        let order = |c: &Captures| c.set(all).iter().map(|k| (k.owner, k.vk)).collect::<Vec<_>>();
        assert_eq!(order(&c), vec![(1, 0x09), (2, 0x09), (1, 0x20)]);
        let again = reg(&c, &lua, 1, 0x09, 0);
        assert!(again > a_tab);
        assert_eq!(order(&c), vec![(2, 0x09), (1, 0x20), (1, 0x09)], "module 1's Tab, captured again, goes last");
        assert!(!c.release(a_tab), "a stale token releases nothing");
        assert!(c.release(again));
        assert_eq!(order(&c), vec![(2, 0x09), (1, 0x20)]);
        assert_eq!(c.set(|i| i != 2).len(), 1, "a disabled module's captures are not handed over");
        c.release_all(1);
        c.drop_registrations(|i| i == 2);
        assert!(c.is_empty());
    }

    /// Each module's own scope and flag; a change says it changed, a repeat does not, a first
    /// write always does; forgetting drops the entry, a rolled-back hot-load every one from `n`.
    #[test]
    fn each_module_sets_its_own_scope_and_flag() {
        let c = Captures::default();
        assert!(c.set_scope(1, 0x111), "a first scope");
        assert!(!c.set_scope(1, 0x111), "the same again");
        assert!(c.set_menu(2, false), "a first flag is an entry, even false");
        assert!(!c.set_menu(2, false));
        assert!(c.set_menu(1, true));
        assert!(c.set_scope(2, 0x222));
        assert_eq!((c.scope_of(1), c.menu_of(1)), (0x111, true));
        assert_eq!((c.scope_of(2), c.menu_of(2)), (0x222, false));
        assert_eq!((c.scope_of(3), c.menu_of(3)), (0, false), "never set: everywhere, no menu");
        // Module 1 sets back its own; module 2's stays.
        assert!(c.set_scope(1, 0));
        assert!(c.set_menu(1, false));
        assert_eq!((c.scope_of(2), c.menu_of(2)), (0x222, false));
        let entries = c.owner_entries(|_| true);
        assert_eq!(
            entries,
            vec![OwnerKeys { owner: 1, scope: 0, menu: false }, OwnerKeys { owner: 2, scope: 0x222, menu: false }],
            "kept once made, everywhere with no menu included"
        );
        assert_eq!(c.owner_entries(|i| i != 1).len(), 1, "a disabled module's entry is not handed over");
        assert!(c.forget_owner(2));
        assert!(!c.forget_owner(2));
        assert_eq!(c.scope_of(2), 0);
        c.set_scope(5, 0x555);
        c.set_scope(7, 0x777);
        assert!(c.forget_owners_from(5));
        assert_eq!(c.owner_entries(|_| true).len(), 1);
        assert!(!c.forget_owners_from(5));
    }

    /// The trace line: each key once, its modules in deciding order, with scope and flag.
    #[test]
    fn the_captured_line_names_each_key_s_modules_in_order() {
        let c = Captures::default();
        let lua = Lua::new();
        reg(&c, &lua, 1, 0x09, 0);
        reg(&c, &lua, 2, 0x09, 0);
        reg(&c, &lua, 2, 0x20, 1);
        c.set_scope(2, 0x1a2b);
        c.set_menu(2, true);
        let set = c.set(|_| true);
        let line = c.captured_line(&set, |i| format!("m{i}"));
        assert_eq!(line, "captured set: vk 0x09/m0[m1,m2@0x1a2b+menu] vk 0x20/m1[m2@0x1a2b+menu]");
    }

    /// Where a key goes on once modules let go of it: the earliest capture of the combination, in
    /// registration order, of a module that did not let go of it and is scoped to the window of the
    /// press or to everywhere — none when there is no such capture. The same module the hook takes
    /// the key for with the modules that let go of it taken out and no menu open
    /// ([`backend::capture_decision`]), for every window and every set of modules let go.
    #[test]
    fn a_key_let_go_of_goes_on_to_the_earliest_capture_in_scope_of_another_module() {
        const W1: isize = 0x111;
        const W2: isize = 0x222;
        let cap = |owner, vk, mask| Captured { vk, mask, owner };
        let own = |owner, scope, menu| OwnerKeys { owner, scope, menu };
        // Module 1 pinned to W1, 2 to W2, 3 everywhere, 4 never scoped; 4 captures Space only.
        let set = [cap(1, 0x09, 0), cap(2, 0x09, 0), cap(3, 0x09, 0), cap(4, 0x20, 0), cap(2, 0x09, backend::MASK_SHIFT)];
        let owners = [own(1, W1, false), own(2, W2, false), own(3, 0, false)];
        assert_eq!(offered_to(&set, &owners, 0x09, 0, W2, &[1]), Some(2), "2 is scoped to W2, before 3");
        assert_eq!(offered_to(&set, &owners, 0x09, 0, W1, &[1]), Some(3), "2 is pinned to W2; 3 is everywhere");
        assert_eq!(offered_to(&set, &owners, 0x09, 0, W2, &[1, 2]), Some(3));
        assert_eq!(offered_to(&set, &owners, 0x09, 0, W2, &[1, 2, 3]), None, "nobody else: the program's");
        assert_eq!(offered_to(&set, &owners, 0x09, 0, W1, &[3]), Some(1), "a module that did not let go of it");
        assert_eq!(offered_to(&set, &owners, 0x09, backend::MASK_SHIFT, W1, &[1]), None, "Shift+Tab is 2's, in W2");
        assert_eq!(offered_to(&set, &owners, 0x20, 0, W1, &[4]), None, "a key nobody else captures");
        assert_eq!(offered_to(&set, &owners, 0x20, 0, 0, &[]), Some(4), "no window known: an unscoped module's");
        // A menu flag is not asked: none was open at the press, or the hook would not have taken it.
        let menu = [own(1, W1, false), own(2, W2, true), own(3, 0, false)];
        assert_eq!(offered_to(&set, &menu, 0x09, 0, W2, &[1]), Some(2));
        // The hook's rule, for every window and every set of modules let go.
        for front in [0, W1, W2, 0x333] {
            for gone in 0u32..16 {
                let let_go: Vec<usize> = (1..=4).filter(|i| gone & (1 << (i - 1)) != 0).collect();
                let kept: Vec<Captured> = set.iter().copied().filter(|c| !let_go.contains(&(c.owner as usize))).collect();
                for (vk, mask) in [(0x09, 0), (0x20, 0), (0x09, backend::MASK_SHIFT)] {
                    let hook = match backend::capture_decision(&kept, &owners, vk, mask, front, || false) {
                        Some(backend::Capture::Take { owner }) => Some(owner as usize),
                        _ => None,
                    };
                    assert_eq!(offered_to(&set, &owners, vk, mask, front, &let_go), hook, "{front:#x} {let_go:?} {vk:#x}/{mask}");
                }
            }
        }
    }

    /// A module's dropped keys are said once per ten seconds, each module on its own clock, and
    /// afresh once its entry is forgotten.
    #[test]
    fn dropped_keys_are_said_once_per_ten_seconds_per_module() {
        let c = Captures::default();
        let t0 = Instant::now();
        assert!(c.say_dropped(1, t0));
        assert!(!c.say_dropped(1, t0 + Duration::from_secs(9)));
        assert!(c.say_dropped(2, t0 + Duration::from_secs(9)), "its own clock");
        assert!(c.say_dropped(1, t0 + Duration::from_secs(10)));
        c.forget_owner(1);
        assert!(c.say_dropped(1, t0 + Duration::from_secs(11)), "forgotten with the module");
        // A key gone on to another module and a key passed on to the program: a clock each.
        let t1 = t0 + Duration::from_secs(30);
        assert!(c.say_offered(1, t1));
        assert!(!c.say_offered(1, t1 + Duration::from_secs(9)));
        assert!(c.say_passed(1, t1 + Duration::from_secs(9)), "not the offered keys' clock");
        assert!(c.say_dropped(1, t1 + Duration::from_secs(9)), "nor the dropped keys'");
        c.forget_owner(1);
        assert!(c.say_offered(1, t1 + Duration::from_secs(9)), "forgotten with the module");
        c.say_offered(7, t1);
        c.forget_owners_from(5);
        assert!(c.say_offered(7, t1 + Duration::from_secs(1)), "and with a rolled-back hot-load");
        let l = dropped_line("kontakt", "Tab", Gone::ScopeMoved);
        assert_eq!(
            l,
            "[kontakt] Tab was dropped: the module's scope moved to another window between the press and \
             its delivery. Said at most once every 10 s per module"
        );
        assert!(dropped_line("m", "Space", Gone::Released).contains("its capture was released"));
    }
}
