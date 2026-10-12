//! `host.element`'s bindings: the questions a module asks of an accessibility tree, each with an
//! optional `opts` and an optional callback last, after host.ocr.recognize's model, and
//! `host.element.pending(key)`. `element.rs` asks them, off the event loop where the platform
//! allows; docs/api/element.md says what each one answers.
//!
//! **One shape for every call.** `host.element.<call>(…its arguments…, opts?, cb?)`: `opts` is
//! `{ key = string }`, or `nil`, or left out before the callback; `cb`, when given, goes last.
//! What follows the call's own arguments is counted, as recognize counts it: one value is `opts` or
//! the callback, two are `opts` and the callback — which must be a function then, a `nil` raising —
//! and a third raises. With a callback the call returns nothing, at once, and `cb` is called with the
//! answer on a later turn of the event loop, as a handler of the module; without one the call
//! returns the answer itself. The answer is the same either way: the call's value, or `nil` and why
//! when the program did not answer — never a "no" made of that. A press answers `true`, or `false`
//! and why. A dump answers its list, and why it is not all of the tree when it is not.
//!
//! **`opts.key`.** A question with a key takes out the module's questions with that key that have
//! not been asked yet — whether it is answered from the epoch's store, waited for, or asked with a
//! callback — and `host.element.pending(key)` says whether one asked with a callback is out.
//!
//! **The epoch's answers.** `find`, `findAny` and `pluginLocate` are asked once per epoch: a second
//! identical call in the same epoch gets the first one's answer — a "not known" of the program's
//! too, for the rest of that epoch — without the program being asked again. With a callback that
//! answer comes on the next turn, as any other. A callback's answer is kept for the epoch only when
//! it is the program's and comes while the epoch and the input epoch it was asked in still stand;
//! what the host told without asking the program (element.rs, `Due::program`) never is. The other
//! calls are asked every time.

use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::{ElementAnswer, ElementAsk, ElementOutcome};
use crate::element::{Pending, SLOW};
use crate::logging;
use crate::ocr::lua::owner_of;
use crate::ocr::types::current_priority;
use crate::vm_guard::host_call;
use crate::{acting, describe_value, dump_node_to_table, focus_step_table, Shared};

/// What follows a call's own arguments: the question's `opts.key`, and the callback.
pub(crate) struct Tail {
    pub key: Option<String>,
    pub cb: Option<Function>,
}

/// What a call raises for an explicit `nil` where the callback goes: almost always a callback that
/// is missing. Recognize's rule (`task::NIL_CB`).
pub(crate) const NIL_CB: &str =
    "the callback is nil — pass a function to be called back with the answer, or leave the argument out to wait for it";

/// A value as a message names it: `nil` by its name.
fn shown(v: &Value) -> String {
    if v.is_nil() {
        "nil".to_string()
    } else {
        describe_value(v)
    }
}

/// Reads `opts?, cb?` after the arguments of call `f`: `rest`, every value that follows them,
/// counted. None: neither. One: a function is the callback, anything else `opts`. Two: `opts` — a
/// table or `nil` — and the callback, a function. A value of the wrong kind raises, naming the call;
/// so does a third value, an option other than `key`, and a key that is not a non-empty string.
pub(crate) fn tail(f: &str, rest: MultiValue) -> mlua::Result<Tail> {
    let bad = |msg: String| mlua::Error::external(format!("{f}: {msg}"));
    let mut rest = rest.into_iter();
    let (a, b, c) = (rest.next(), rest.next(), rest.next());
    let after = |v: &Value| bad(format!("nothing goes after the callback, got {}", shown(v)));
    let key_of = |opts: Value| -> mlua::Result<Option<String>> {
        match opts {
            Value::Nil => Ok(None),
            Value::Table(opts) => {
                let mut key = None;
                for pair in opts.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    match &k {
                        Value::String(s) if &*s.to_str()? == "key" => match v {
                            Value::String(s) if !s.as_bytes().is_empty() => key = Some(s.to_str()?.to_string()),
                            other => return Err(bad(format!("opts.key must be a non-empty string, got {}", shown(&other)))),
                        },
                        other => {
                            let name = match other {
                                Value::String(s) => format!("'{}'", s.to_string_lossy()),
                                v => describe_value(v),
                            };
                            return Err(bad(format!("opts has no option {name}; the one option is key")));
                        }
                    }
                }
                Ok(key)
            }
            other => Err(bad(format!("opts must be a table — {{ key = … }} — or the callback a function, got {}", shown(&other)))),
        }
    };
    match (a, b) {
        (None, _) => Ok(Tail { key: None, cb: None }),
        (Some(Value::Function(_)), Some(v)) => Err(after(&v)),
        (Some(Value::Function(cb)), None) => Ok(Tail { key: None, cb: Some(cb) }),
        (Some(opts), None) => Ok(Tail { key: key_of(opts)?, cb: None }),
        (Some(opts), Some(cb)) => {
            if let Some(v) = &c {
                return Err(after(v));
            }
            let key = key_of(opts)?;
            match cb {
                Value::Function(cb) => Ok(Tail { key, cb: Some(cb) }),
                Value::Nil => Err(bad(NIL_CB.to_string())),
                other => Err(bad(format!("the callback (last argument) must be a function, got {}", shown(&other)))),
            }
        }
    }
}

/// Whether the answers to questions of `ask`'s kind are kept for the epoch: `find`, `findAny`,
/// `pluginLocate` — the identity and location questions every evaluation of an overlay asks again.
pub(crate) fn kept(ask: &ElementAsk) -> bool {
    matches!(ask, ElementAsk::Find { .. } | ElementAsk::FindAny { .. } | ElementAsk::PluginLocate { .. })
}

/// The name a callback's errors are reported under: `"element.find"`.
fn what_of(ask: &ElementAsk) -> &'static str {
    match ask {
        ElementAsk::Find { .. } => "element.find",
        ElementAsk::FindAny { .. } => "element.findAny",
        ElementAsk::Locate { .. } => "element.locate",
        ElementAsk::LocateVia { .. } => "element.locateVia",
        ElementAsk::PluginLocate { .. } => "element.pluginLocate",
        ElementAsk::StateProbe { .. } => "element.stateProbe",
        ElementAsk::Dump { .. } => "element.dump",
        ElementAsk::RawDump { .. } => "element.rawDump",
        ElementAsk::ClassNavPoint { .. } => "element.classNavPoint",
        ElementAsk::FocusStep { .. } => "element.focusStep",
        ElementAsk::FocusWithin { .. } => "element.focusWithin",
        ElementAsk::Press { .. } => "element.press",
        ElementAsk::FocusPut { .. } => "window.focus",
    }
}

/// `{ x, y }`.
fn point(lua: &Lua, (x, y): (i32, i32)) -> mlua::Result<Value> {
    let t = lua.create_table()?;
    t.set("x", x)?;
    t.set("y", y)?;
    Ok(Value::Table(t))
}

/// What a call returns, and its callback is called with, for `outcome`: the answer — `nil` for
/// "nothing there" — or, when the program did not answer, `nil` and why. A press: `true`, or
/// `false` and why. A dump: its list, and why it is not all of the tree when it is not.
pub(crate) fn values(lua: &Lua, ask: &ElementAsk, outcome: &ElementOutcome) -> mlua::Result<MultiValue> {
    let why = |s: &str| -> mlua::Result<Value> { Ok(Value::String(lua.create_string(s)?)) };
    let one = |v: Value| MultiValue::from_vec(vec![v]);
    let answer = match outcome {
        Err(f) => {
            let first = match ask {
                ElementAsk::Press { .. } => Value::Boolean(false),
                ElementAsk::Dump { .. } | ElementAsk::RawDump { .. } => Value::Table(lua.create_table()?),
                _ => Value::Nil,
            };
            return Ok(MultiValue::from_vec(vec![first, why(&f.why)?]));
        }
        Ok(a) => a,
    };
    Ok(match answer {
        ElementAnswer::Found(b) => one(Value::Boolean(*b)),
        ElementAnswer::Index(i) => one(i.map_or(Value::Nil, |i| Value::Integer(i as i64))),
        ElementAnswer::Point(p) => one(match p {
            Some(p) => point(lua, *p)?,
            None => Value::Nil,
        }),
        ElementAnswer::State(s) => one(match s {
            None => Value::Nil,
            Some((toggle, legacy)) => {
                let t = lua.create_table()?;
                if *toggle >= 0 {
                    t.set("toggle", *toggle)?;
                }
                t.set("legacyState", *legacy)?;
                t.set("checked", legacy & 0x10 != 0)?;
                Value::Table(t)
            }
        }),
        ElementAnswer::Nodes(nodes, stopped) => {
            let arr = lua.create_table()?;
            for (i, n) in nodes.iter().enumerate() {
                arr.set(i + 1, dump_node_to_table(lua, n.clone())?)?;
            }
            match stopped {
                Some(s) => MultiValue::from_vec(vec![Value::Table(arr), why(s)?]),
                None => one(Value::Table(arr)),
            }
        }
        ElementAnswer::Step(s) => one(match s {
            Some(step) => Value::Table(focus_step_table(lua, step.clone())?),
            None => Value::Nil,
        }),
        ElementAnswer::Within(w) => one(match w {
            Some((name, ctype)) => {
                let t = lua.create_table()?;
                t.set("name", name.as_str())?;
                t.set("ctype", *ctype)?;
                Value::Table(t)
            }
            None => Value::Nil,
        }),
        ElementAnswer::Pressed => one(Value::Boolean(true)),
        ElementAnswer::Put(took) => one(Value::Boolean(*took)),
    })
}

/// The `[observe]` line for a call the event loop waited `took` for, `behind` of it before the
/// question was taken — behind questions put before it — from [`SLOW`] on, as `slow_observation`'s
/// lines are: the pump was held for all of it, and the part behind the others is said.
pub(crate) fn blocked_line(ask: &ElementAsk, took: Duration, behind: Duration) -> Option<String> {
    if took < SLOW {
        return None;
    }
    let mut line = format!("uia.{}({}) blocked the pump for {} ms", ask.call_name(), ask.detail(), took.as_millis());
    if behind.as_millis() > 0 {
        line += &format!(", {} ms of it behind questions asked before it", behind.as_millis());
    }
    Some(line)
}

/// Asks `ask` now and waits for the answer — on the accessibility thread where the platform has
/// one, else on the event loop — and keeps the program's answer for the epoch where its kind is
/// kept there. A wait of [`SLOW`] or more is named in the log ([`blocked_line`]).
fn ask_here(sh: &Shared, ask: &ElementAsk, input_epoch: u64) -> ElementOutcome {
    let t = Instant::now();
    let waited = sh.elements.ask(&*sh.backend, ask.clone(), input_epoch, Some(&sh.guard));
    if let Some(line) = blocked_line(ask, t.elapsed(), waited.behind) {
        logging::line("observe", &line);
    }
    if waited.program && kept(ask) {
        sh.observations().elements.insert(ask.clone(), waited.outcome.clone());
    }
    waited.outcome
}

/// One question of module `idx`: answered from the epoch's store where its kind is kept there and
/// it was asked this epoch; asked otherwise — waited for, or handed on with the callback.
fn put(sh: &Rc<Shared>, lua: &Lua, idx: usize, ask: ElementAsk, tail: Tail) -> mlua::Result<MultiValue> {
    let cached = if kept(&ask) {
        let mut obs = sh.observations();
        obs.asked += 1;
        let hit = obs.elements.get(&ask).cloned();
        if hit.is_some() {
            obs.served += 1;
        }
        hit
    } else {
        None
    };
    // No borrow of the store is held past this point: the wait below answers what is sent to this
    // thread's windows (element.rs, `Signal`).
    let input_epoch = sh.input_epoch.get();
    let owner = owner_of(lua, &sh.vm_gens.borrow(), idx);
    if let Some(key) = &tail.key {
        sh.elements.replace(owner, key);
    }
    match tail.cb {
        Some(cb) => {
            let pending = Pending {
                answer: acting::Answer { idx, lua: lua.clone(), cb, what: what_of(&ask) },
                prio: current_priority(),
                ask: ask.clone(),
                asked_in: (sh.epoch.get(), input_epoch),
                owner,
                key: tail.key,
            };
            match (cached, sh.backend.element_runner()) {
                (Some(outcome), _) => sh.elements.answer_now(pending, outcome),
                (None, Some(runner)) => sh.elements.ask_for(runner, ask, input_epoch, pending),
                // No thread: asked here, now, and answered on the next turn all the same.
                (None, None) => {
                    let outcome = ask_here(sh, &ask, input_epoch);
                    sh.elements.answer_now(pending, outcome);
                }
            }
            Ok(MultiValue::new())
        }
        None => {
            let outcome = match cached {
                Some(outcome) => outcome,
                None => ask_here(sh, &ask, input_epoch),
            };
            values(lua, &ask, &outcome)
        }
    }
}

/// Puts module `idx`'s `host.element` on `host`.
pub(crate) fn install(lua: &Lua, host: &Table, shared: &Rc<Shared>, idx: usize) -> mlua::Result<()> {
    let uia = lua.create_table()?;
    // host.element.find(hwnd, name, controlType, opts?, cb?) -> true | false | nil, why — does the
    // window's tree hold an element with that Name and ControlType? (Plugin identity.)
    let sh = shared.clone();
    uia.set(
        "find",
        lua.create_function(move |lua, (hwnd, name, ctype, rest): (isize, String, i32, MultiValue)| {
            host_call!("host.element.find");
            let tail = tail("host.element.find", rest)?;
            put(&sh, lua, idx, ElementAsk::Find { hwnd, name, ctype }, tail)
        })?,
    )?;
    // host.element.type — the UIA ControlType ids, by name. Modules were declaring these as
    // local constants (and writing bare integers where they hadn't), which reads as a
    // magic number at every call site: `host.element.find(id, "Kontakt 8", 50033)`.
    let types = lua.create_table()?;
    for (name, id) in [
        ("Button", 50000),
        ("Calendar", 50001),
        ("CheckBox", 50002),
        ("ComboBox", 50003),
        ("Edit", 50004),
        ("Hyperlink", 50005),
        ("Image", 50006),
        ("ListItem", 50007),
        ("List", 50008),
        ("Menu", 50009),
        ("MenuBar", 50010),
        ("MenuItem", 50011),
        ("ProgressBar", 50012),
        ("RadioButton", 50013),
        ("ScrollBar", 50014),
        ("Slider", 50015),
        ("Spinner", 50016),
        ("StatusBar", 50017),
        ("Tab", 50018),
        ("TabItem", 50019),
        ("Text", 50020),
        ("ToolBar", 50021),
        ("ToolTip", 50022),
        ("Tree", 50023),
        ("TreeItem", 50024),
        ("Custom", 50025),
        ("Group", 50026),
        ("Thumb", 50027),
        ("DataGrid", 50028),
        ("DataItem", 50029),
        ("Document", 50030),
        ("SplitButton", 50031),
        ("Window", 50032),
        ("Pane", 50033),
        ("Header", 50034),
        ("HeaderItem", 50035),
        ("Table", 50036),
        ("TitleBar", 50037),
        ("Separator", 50038),
    ] {
        types.set(name, id)?;
    }
    uia.set("type", types)?;
    // host.element.findAny(hwnd, names, types, opts?, cb?) -> index | nil | nil, why — "is any of
    // these names present as any of these control types?", the shape a plugin-identity check
    // takes ("Kontakt 8" as Window OR Pane). One tree traversal per NAME rather than one per
    // name×type pair, and it returns WHICH name matched (1-based), so a caller can read the
    // plugin's version straight out of the answer instead of asking once per version.
    let sh = shared.clone();
    uia.set(
        "findAny",
        lua.create_function(move |lua, (hwnd, names, types, rest): (isize, Vec<String>, Vec<i32>, MultiValue)| {
            host_call!("host.element.findAny");
            let tail = tail("host.element.findAny", rest)?;
            put(&sh, lua, idx, ElementAsk::FindAny { hwnd, names, types }, tail)
        })?,
    )?;
    // host.element.locate(hwnd, name, controlType, opts?, cb?) -> { x, y } | nil | nil, why —
    // the screen centre of the matching element, to click it. For driving plugin UI via UIA.
    let sh = shared.clone();
    uia.set(
        "locate",
        lua.create_function(move |lua, (hwnd, name, ctype, rest): (isize, String, i32, MultiValue)| {
            host_call!("host.element.locate");
            let tail = tail("host.element.locate", rest)?;
            put(&sh, lua, idx, ElementAsk::Locate { hwnd, name, ctype }, tail)
        })?,
    )?;
    // host.element.locateVia(hwnd, viaName, viaType, name, controlType, opts?, cb?) -> { x, y } |
    // nil | nil, why. Like locate, but descends into a container element (viaName/viaType) first
    // and searches within it — reaches a plugin's UI that hangs off an identity pane as a nested
    // UIA fragment (a DAW-embedded Kontakt). Ports ReaHotkey's two-level find.
    let sh = shared.clone();
    uia.set(
        "locateVia",
        lua.create_function(
            move |lua, (hwnd, via_name, via_type, name, ctype, rest): (isize, String, i32, String, i32, MultiValue)| {
                host_call!("host.element.locateVia");
                let tail = tail("host.element.locateVia", rest)?;
                put(&sh, lua, idx, ElementAsk::LocateVia { hwnd, via_name, via_type, name, ctype }, tail)
            },
        )?,
    )?;
    // host.element.pluginLocate(hwnd, containerName, name, controlType, opts?, cb?) -> { x, y } |
    // nil | nil, why. ReaHotkey's GetPluginUIAElement + FindElement: find the element that IS the
    // plugin (containerName, Window/Pane, QuickWindow class preferred) and search within it over
    // the RAW tree — the only view that reaches a DAW-embedded plugin's hosted Qt UI.
    let sh = shared.clone();
    uia.set(
        "pluginLocate",
        lua.create_function(
            move |lua, (hwnd, container, name, ctype, rest): (isize, String, String, i32, MultiValue)| {
                host_call!("host.element.pluginLocate");
                let tail = tail("host.element.pluginLocate", rest)?;
                put(&sh, lua, idx, ElementAsk::PluginLocate { hwnd, container, name, ctype }, tail)
            },
        )?,
    )?;
    // host.element.stateProbe(hwnd, container, name, ctype, opts?, cb?) -> table | nil | nil, why
    // — what a named element says about its OWN state, rather than what its control type implies.
    let sh = shared.clone();
    uia.set(
        "stateProbe",
        lua.create_function(
            move |lua, (hwnd, container, name, ctype, rest): (isize, String, String, i32, MultiValue)| {
                host_call!("host.element.stateProbe");
                let tail = tail("host.element.stateProbe", rest)?;
                put(&sh, lua, idx, ElementAsk::StateProbe { hwnd, container, name, ctype }, tail)
            },
        )?,
    )?;
    // host.element.dump(hwnd, opts?, cb?) -> { {depth, name, class, ctype, bounds}, … }, why? —
    // diagnostic condition-based dump, for discovering plugin identity properties.
    let sh = shared.clone();
    uia.set(
        "dump",
        lua.create_function(move |lua, (hwnd, rest): (isize, MultiValue)| {
            host_call!("host.element.dump");
            let tail = tail("host.element.dump", rest)?;
            put(&sh, lua, idx, ElementAsk::Dump { hwnd }, tail)
        })?,
    )?;
    // host.element.rawDump(hwnd, opts?, cb?) — like dump, but over the RAW TreeWalker, which
    // crosses into a hosted Qt fragment (a DAW-embedded plugin's real UI).
    let sh = shared.clone();
    uia.set(
        "rawDump",
        lua.create_function(move |lua, (hwnd, rest): (isize, MultiValue)| {
            host_call!("host.element.rawDump");
            let tail = tail("host.element.rawDump", rest)?;
            put(&sh, lua, idx, ElementAsk::RawDump { hwnd }, tail)
        })?,
    )?;
    // host.element.classNavPoint(hwnd, className, ctype, child, sibling, opts?, cb?) -> {x,y} |
    // nil | nil, why — click-point of the element reached from the first ClassName-contains +
    // ControlType match by walking `child` (nth child) then `sibling` siblings. Ports
    // ReaHotkey's FindElement(ClassName) + WalkTree(path).Click.
    let sh = shared.clone();
    uia.set(
        "classNavPoint",
        lua.create_function(
            move |lua, (hwnd, class, ctype, child, sibling, rest): (isize, String, i32, i32, i32, MultiValue)| {
                host_call!("host.element.classNavPoint");
                let tail = tail("host.element.classNavPoint", rest)?;
                put(&sh, lua, idx, ElementAsk::ClassNavPoint { hwnd, class, ctype, child, sibling }, tail)
            },
        )?,
    )?;
    // host.element.focusStep(hwnd, direction, opts?, cb?) -> { name, ctype, index, count,
    // bounds? } | nil | nil, why — Tab pass-through for a standalone plugin window: SetFocus the
    // next (direction>=0) / previous keyboard-focusable descendant relative to the current focus,
    // wrapping at the ends, and return the newly focused element — where it is included, so a
    // caller can read what a stop with no name shows (`focus_step_table`, lib.rs).
    let sh = shared.clone();
    uia.set(
        "focusStep",
        lua.create_function(move |lua, (hwnd, direction, rest): (isize, i32, MultiValue)| {
            host_call!("host.element.focusStep");
            let tail = tail("host.element.focusStep", rest)?;
            put(&sh, lua, idx, ElementAsk::FocusStep { hwnd, direction }, tail)
        })?,
    )?;
    // host.element.focusWithin(hwnd, { x, y, w, h }, opts?, cb?) -> { name, ctype } | nil | nil,
    // why. Gives the keyboard to the first focusable element inside the rectangle — the way into a
    // plugin that publishes elements, without pressing anything at its corner.
    let sh = shared.clone();
    uia.set(
        "focusWithin",
        lua.create_function(move |lua, (hwnd, rect, rest): (isize, Table, MultiValue)| {
            host_call!("host.element.focusWithin");
            let (x, y, w, h): (i32, i32, i32, i32) = (rect.get("x")?, rect.get("y")?, rect.get("w")?, rect.get("h")?);
            let tail = tail("host.element.focusWithin", rest)?;
            put(&sh, lua, idx, ElementAsk::FocusWithin { hwnd, x, y, w, h }, tail)
        })?,
    )?;
    // host.element.press(hwnd, name, controlType, opts?, cb?) -> true | false, why — or nothing
    // with a callback. The element's own action (Invoke, Toggle, the default action), the keyboard
    // focus left where it is (backend/uia.rs). Noted as acting, and the input epoch turned over,
    // before it is asked, as a click is.
    let sh = shared.clone();
    uia.set(
        "press",
        lua.create_function(move |lua, (hwnd, name, ctype, rest): (isize, String, i32, MultiValue)| {
            host_call!("host.element.press");
            let tail = tail("host.element.press", rest)?;
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.element.press");
            sh.bump_input_epoch();
            put(&sh, lua, idx, ElementAsk::Press { hwnd, name, ctype }, tail)
        })?,
    )?;
    // host.element.pending(key) -> boolean — whether a question of this module asked with `key` and
    // a callback is out: not answered to it yet — on its way, or its answer waiting in the module's
    // mailbox — nor dropped (element.rs, `Elements::pending_with`). Recognize's `pending`.
    let sh = shared.clone();
    uia.set(
        "pending",
        lua.create_function(move |lua, key: Value| {
            host_call!("host.element.pending");
            let key = match key {
                Value::String(s) if !s.as_bytes().is_empty() => s.to_str()?.to_string(),
                Value::String(_) => return Err(mlua::Error::external("host.element.pending: the key is empty")),
                other => {
                    return Err(mlua::Error::external(format!(
                        "host.element.pending: the key must be a non-empty string, got {}",
                        shown(&other)
                    )))
                }
            };
            Ok(sh.elements.pending_with(owner_of(lua, &sh.vm_gens.borrow(), idx), &key))
        })?,
    )?;
    host.set("element", uia)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{DumpNode, ElementFailure, FocusBounds, FocusStep};

    fn shown(lua: &Lua, mv: MultiValue) -> String {
        lua.globals().set("v", lua.create_sequence_from(mv.clone()).unwrap()).unwrap();
        lua.globals().set("n", mv.len()).unwrap();
        lua.load(
            r#"
            local function show(x)
              if type(x) ~= "table" then return tostring(x) end
              local keys = {}
              for k in pairs(x) do keys[#keys + 1] = tostring(k) end
              table.sort(keys)
              local out = {}
              for _, k in ipairs(keys) do out[#out + 1] = k .. "=" .. show(x[tonumber(k) or k]) end
              return "{" .. table.concat(out, ",") .. "}"
            end
            local out = {}
            for i = 1, n do out[i] = show(v[i]) end
            return table.concat(out, " ")
            "#,
        )
        .eval()
        .unwrap()
    }

    fn late() -> ElementOutcome {
        Err(ElementFailure { why: "the program did not answer within 1000 ms".into(), unanswered: true })
    }

    /// What a module is handed, call by call: the answer, `nil` for nothing there — and `nil` and
    /// why where the program did not answer, which is never `false`; a press's `false` and why; a
    /// dump's list, and why it is not all of the tree.
    #[test]
    fn an_answer_is_its_value_and_not_known_is_nil_and_why() {
        let lua = Lua::new();
        let find = ElementAsk::Find { hwnd: 1, name: "x".into(), ctype: 50033 };
        let show = |ask: &ElementAsk, o: ElementOutcome| shown(&lua, values(&lua, ask, &o).unwrap());
        assert_eq!(show(&find, Ok(ElementAnswer::Found(true))), "true");
        assert_eq!(show(&find, Ok(ElementAnswer::Found(false))), "false");
        assert_eq!(show(&find, late()), "nil the program did not answer within 1000 ms");
        let any = ElementAsk::FindAny { hwnd: 1, names: vec!["a".into()], types: vec![50032] };
        assert_eq!(show(&any, Ok(ElementAnswer::Index(Some(2)))), "2");
        assert_eq!(show(&any, Ok(ElementAnswer::Index(None))), "nil");
        assert_eq!(show(&any, late()), "nil the program did not answer within 1000 ms");
        let locate = ElementAsk::Locate { hwnd: 1, name: "Play".into(), ctype: 50000 };
        assert_eq!(show(&locate, Ok(ElementAnswer::Point(Some((10, 20))))), "{x=10,y=20}");
        assert_eq!(show(&locate, Ok(ElementAnswer::Point(None))), "nil");
        let probe = ElementAsk::StateProbe { hwnd: 1, container: "Kontakt 8".into(), name: "Keyboard".into(), ctype: 50000 };
        assert_eq!(show(&probe, Ok(ElementAnswer::State(Some((1, 0x10))))), "{checked=true,legacyState=16,toggle=1}");
        assert_eq!(show(&probe, Ok(ElementAnswer::State(Some((-1, 0x100000))))), "{checked=false,legacyState=1048576}");
        let dump = ElementAsk::RawDump { hwnd: 1 };
        let node = DumpNode { depth: 2, name: "FILE".into(), class: "".into(), ctype: 50000, x: 1, y: 2, w: 3, h: 4 };
        let one = "{1={bounds={h=4,w=3,x=1,y=2},class=,ctype=50000,depth=2,name=FILE}}";
        assert_eq!(show(&dump, Ok(ElementAnswer::Nodes(vec![node.clone()], None))), one);
        assert_eq!(
            show(&dump, Ok(ElementAnswer::Nodes(vec![node], Some("the program is gone".into())))),
            format!("{one} the program is gone")
        );
        assert_eq!(show(&dump, late()), "{} the program did not answer within 1000 ms", "a list, empty, and why");
        let step = ElementAsk::FocusStep { hwnd: 1, direction: 1 };
        let landed = FocusStep { name: "FILE".into(), ctype: 50000, index: 1, count: 3, bounds: FocusBounds::new(1.0, 2.0, 3.0, 4.0) };
        assert_eq!(show(&step, Ok(ElementAnswer::Step(Some(landed)))), "{bounds={h=4,w=3,x=1,y=2},count=3,ctype=50000,index=1,name=FILE}");
        assert_eq!(show(&step, late()), "nil the program did not answer within 1000 ms");
        let within = ElementAsk::FocusWithin { hwnd: 1, x: 0, y: 0, w: 1, h: 1 };
        assert_eq!(show(&within, Ok(ElementAnswer::Within(Some(("Search".into(), 50004))))), "{ctype=50004,name=Search}");
        assert_eq!(show(&within, late()), "nil the program did not answer within 1000 ms");
        let press = ElementAsk::Press { hwnd: 1, name: "Bypass".into(), ctype: 50000 };
        assert_eq!(show(&press, Ok(ElementAnswer::Pressed)), "true");
        assert_eq!(show(&press, late()), "false the program did not answer within 1000 ms");
    }

    /// `opts?, cb?`, counted as recognize counts them: neither; `opts` alone, `nil` included; the
    /// callback alone; both, `opts` `nil` or a table. An explicit `nil` callback, anything after the
    /// callback, and every value of the wrong kind raise, naming the call.
    #[test]
    fn opts_and_the_callback_are_read_strictly() {
        let lua = Lua::new();
        let f = lua.create_function(|_, ()| Ok(())).unwrap();
        let read = |src: &str| -> mlua::Result<Tail> {
            let rest: MultiValue = lua.load(src).eval().unwrap();
            tail("host.element.find", rest)
        };
        lua.globals().set("f", f).unwrap();
        let t = read("return").unwrap();
        assert!(t.key.is_none() && t.cb.is_none());
        let t = read("return nil").unwrap();
        assert!(t.key.is_none() && t.cb.is_none(), "opts left as nil");
        let t = read("return f").unwrap();
        assert!(t.key.is_none() && t.cb.is_some());
        let t = read("return nil, f").unwrap();
        assert!(t.cb.is_some());
        let t = read("return { key = 'identity' }, f").unwrap();
        assert_eq!((t.key.as_deref(), t.cb.is_some()), (Some("identity"), true));
        let t = read("return { key = 'identity' }").unwrap();
        assert_eq!((t.key.as_deref(), t.cb.is_none()), (Some("identity"), true), "a key without a callback");
        let t = read("return {}").unwrap();
        assert!(t.key.is_none() && t.cb.is_none());
        for (src, want) in [
            ("return f, 5", "host.element.find: nothing goes after the callback, got 5"),
            ("return f, nil", "host.element.find: nothing goes after the callback, got nil"),
            ("return {}, f, nil", "host.element.find: nothing goes after the callback, got nil"),
            ("return nil, f, 3", "host.element.find: nothing goes after the callback, got 3"),
            ("return { key = 'k' }, 5", "host.element.find: the callback (last argument) must be a function, got 5"),
            ("return nil, true", "the callback (last argument) must be a function, got a boolean"),
            ("return nil, nil", "host.element.find: the callback is nil — pass a function to be called back with the answer"),
            ("return { key = 'k' }, nil", "host.element.find: the callback is nil"),
            ("return 'k', f", "host.element.find: opts must be a table — { key = … } — or the callback a function, got the string \"k\""),
            ("return 3", "host.element.find: opts must be a table — { key = … } — or the callback a function, got 3"),
            ("return { key = '' }, f", "host.element.find: opts.key must be a non-empty string, got the string \"\""),
            ("return { key = 3 }, f", "opts.key must be a non-empty string, got 3"),
            ("return { lang = 'en' }, f", "host.element.find: opts has no option 'lang'; the one option is key"),
        ] {
            let e = read(src).err().unwrap_or_else(|| panic!("{src} did not raise")).to_string();
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
    }

    /// Only the identity and location questions every evaluation asks again are kept for the epoch.
    #[test]
    fn find_find_any_and_plugin_locate_are_kept_for_the_epoch() {
        assert!(kept(&ElementAsk::Find { hwnd: 1, name: String::new(), ctype: 50009 }));
        assert!(kept(&ElementAsk::FindAny { hwnd: 1, names: vec![], types: vec![] }));
        assert!(kept(&ElementAsk::PluginLocate { hwnd: 1, container: "K".into(), name: "VIEW".into(), ctype: 50000 }));
        assert!(!kept(&ElementAsk::Locate { hwnd: 1, name: "x".into(), ctype: 50000 }));
        assert!(!kept(&ElementAsk::Press { hwnd: 1, name: "x".into(), ctype: 50000 }));
        assert!(!kept(&ElementAsk::FocusStep { hwnd: 1, direction: 1 }));
        assert!(!kept(&ElementAsk::RawDump { hwnd: 1 }));
    }

    /// The event loop's line for a long wait: from 50 ms, what was asked and how long the pump was
    /// held, and how much of that was behind questions asked before it.
    #[test]
    fn a_long_wait_says_how_much_of_it_was_behind_other_questions() {
        let any = ElementAsk::FindAny { hwnd: 4242, names: vec!["Komplete Kontrol".into()], types: vec![50032, 50033] };
        assert_eq!(blocked_line(&any, Duration::from_millis(49), Duration::ZERO), None);
        assert_eq!(
            blocked_line(&any, Duration::from_millis(1459), Duration::ZERO).as_deref(),
            Some("uia.findAny(Komplete Kontrol) blocked the pump for 1459 ms")
        );
        assert_eq!(
            blocked_line(&any, Duration::from_millis(5300), Duration::from_millis(5004)).as_deref(),
            Some("uia.findAny(Komplete Kontrol) blocked the pump for 5300 ms, 5004 ms of it behind questions asked before it")
        );
    }
}
