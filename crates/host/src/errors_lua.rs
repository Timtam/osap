//! The Luau side of errors.rs: the error value a module gets ([`to_lua`]), `host.errors` with
//! its constructor ([`install`]), and the text argument that takes one ([`Text`]).
//!
//! An error value is a read-only table, `{ code, message, detail? }`, with one metatable per VM:
//! `tostring` and `..` give the message, and a field it does not have raises rather than reading
//! as nil — so `err.cod` and `err:find("x")` say what went wrong where they are written. The
//! metatable is made once per VM, when its host is installed, from a small Luau chunk that keeps
//! its own copies of the globals it uses, so a module that replaces `tostring` later does not
//! change what an error value does. It has no `__metatable` field: Luau's `table.clone` and
//! `table.freeze` raise on a protected metatable, and the value and its metatable are read-only
//! anyway. No `__eq` either: Luau never calls it between a table and a string.
//!
//! `json.encode` reads raw fields only (json.rs), so an error value encodes as `{ "code", "message",
//! "detail" }` with no special case, and `page:post({ ok = false, error = err })` hands a page the
//! shape its `platform.errors` is for (webview.rs).

use mlua::{FromLua, Lua, Table, Value};

use crate::errors::{Code, HostError};
use crate::json::luau_type;
use crate::vm_guard::host_call;

/// The named-registry key of this VM's error-value metatable.
const VALUE_META: &str = "__host_error_meta";

/// The named-registry key of this VM's `host.errors` table: one per VM, shared by the module's
/// host and every code dependency's.
const NAMESPACE: &str = "__host_errors";

/// The named-registry key of `host.errors`'s metatable.
const NAMESPACE_META: &str = "__host_errors_meta";

/// Both metatables, made once per VM: the error value's and `host.errors`'s. `...` is the list of
/// constants `host.errors`'s refusal names.
const METATABLES: &str = r#"
local error, tostring, rawget, type, typeof, getmetatable, freeze = error, tostring, rawget, type, typeof, getmetatable, table.freeze
local codes = ...
local value = {}
local function text(v)
  if type(v) == "string" or type(v) == "number" then return v end
  if getmetatable(v) == value then return rawget(v, "message") end
  error("attempt to concatenate an error value with a " .. typeof(v) .. " value", 3)
end
value.__index = function(_, key)
  if key == "detail" then return nil end
  error("an error value has code, message and detail, not '" .. tostring(key) .. "'", 2)
end
value.__tostring = function(e) return rawget(e, "message") end
value.__concat = function(a, b) return text(a) .. text(b) end
local namespace = {}
namespace.__index = function(_, key)
  error("host.errors has no '" .. tostring(key) .. "'; the codes are " .. codes, 2)
end
return freeze(value), freeze(namespace)
"#;

/// This VM's error-value metatable, and `host.errors`'s, made on first use: at the host's
/// install (`install_host_api`), before any of the module's code runs, so the chunk's copies of
/// the globals are the VM's own. A VM without a host — a test's — makes them at its first error
/// value.
fn metatables(lua: &Lua) -> mlua::Result<(Table, Table)> {
    if let Some(value) = lua.named_registry_value::<Option<Table>>(VALUE_META)? {
        let namespace: Table = lua.named_registry_value(NAMESPACE_META)?;
        return Ok((value, namespace));
    }
    let codes = Code::ALL.iter().map(|c| c.constant()).collect::<Vec<_>>().join(", ");
    let (value, namespace): (Table, Table) = lua.load(METATABLES).set_name("=host.errors").call(codes)?;
    lua.set_named_registry_value(VALUE_META, &value)?;
    lua.set_named_registry_value(NAMESPACE_META, &namespace)?;
    Ok((value, namespace))
}

/// An error value: `{ code, message, detail? }`, read-only, with this VM's metatable. The one
/// place one is made, for the host's own ([`to_lua`]) and for `host.errors.new`.
fn value(lua: &Lua, code: &str, message: &str, detail: Option<&str>) -> mlua::Result<Table> {
    let (meta, _) = metatables(lua)?;
    let t = lua.create_table_with_capacity(0, 3)?;
    t.raw_set("code", code)?;
    t.raw_set("message", message)?;
    if let Some(d) = detail {
        t.raw_set("detail", d)?;
    }
    t.set_metatable(Some(meta))?;
    t.set_readonly(true);
    Ok(t)
}

/// `e` as an error value in `lua`: made in the VM that receives it, at delivery, so an answer that
/// crosses threads stays a [`HostError`] until then. Only the tests make one until the producers
/// answer with HostError (errors.rs), and the allowance goes with that step.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn to_lua(lua: &Lua, e: &HostError) -> mlua::Result<Table> {
    value(lua, e.code().as_str(), e.message(), e.detail())
}

/// `v`'s message when it is an error value of this VM; `None` for anything else.
pub(crate) fn message_of(lua: &Lua, v: &Value) -> mlua::Result<Option<String>> {
    let Value::Table(t) = v else { return Ok(None) };
    let Some(meta) = t.metatable() else { return Ok(None) };
    let Some(ours) = lua.named_registry_value::<Option<Table>>(VALUE_META)? else { return Ok(None) };
    if meta.to_pointer() != ours.to_pointer() {
        return Ok(None);
    }
    t.raw_get::<String>("message").map(Some)
}

/// A text argument that also takes an error value, as its message: `host.log.info(err)` and
/// `host.speech.output(err)`. Anything else converts as a `String` argument does — a string or a
/// number — and raises with the same words when it cannot.
pub(crate) struct Text(pub String);

impl FromLua for Text {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        if let Some(message) = message_of(lua, &value)? {
            return Ok(Text(message));
        }
        String::from_lua(value, lua).map(Text)
    }
}

/// Whether `code` is a code `host.errors.new` takes: lower-case snake_case, starting with a letter.
fn snake_case(code: &str) -> bool {
    code.starts_with(|c: char| c.is_ascii_lowercase()) && code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `host.errors.new(code, message, detail?)`: an error value of the module's own, for a library
/// or the overlay runtime to answer as the host does. Any snake_case code, not only the host's.
fn new_error(lua: &Lua, (code, message, detail): (Value, Value, Value)) -> mlua::Result<Table> {
    let fail = |why: String| mlua::Error::external(format!("host.errors.new: {why}"));
    let Value::String(code) = code else {
        return Err(fail(format!("the code must be a string, not a {}", luau_type(&code))));
    };
    let code = code.to_str()?.to_string();
    if !snake_case(&code) {
        return Err(fail(format!("the code '{code}' is not lower-case snake_case: a letter, then letters, digits and _")));
    }
    let Value::String(message) = message else {
        return Err(fail(format!("the message must be a string, not a {}", luau_type(&message))));
    };
    let message = message.to_str()?.to_string();
    if message.is_empty() {
        return Err(fail("the message is empty".to_string()));
    }
    let detail = match detail {
        Value::Nil => None,
        Value::String(d) => Some(d.to_str()?.to_string()),
        other => return Err(fail(format!("the detail must be a string or nil, not a {}", luau_type(&other)))),
    };
    value(lua, &code, &message, detail.as_deref())
}

/// `host.errors`: the 19 constants, `NOT_READY = "not_ready"` and the rest, and `new`. Read-only;
/// a name it does not have raises and lists the codes. One table per VM, so a code dependency's
/// host has the module's own. Ungated, like `host.os`: every module compares codes.
pub(crate) fn install(lua: &Lua, host: &Table) -> mlua::Result<()> {
    let codes = match lua.named_registry_value::<Option<Table>>(NAMESPACE)? {
        Some(codes) => codes,
        None => {
            let (_, meta) = metatables(lua)?;
            let codes = lua.create_table_with_capacity(0, Code::ALL.len() + 1)?;
            for code in Code::ALL {
                codes.raw_set(code.constant(), code.as_str())?;
            }
            codes.set(
                "new",
                lua.create_function(|lua, args: (Value, Value, Value)| {
                    host_call!("host.errors.new");
                    new_error(lua, args)
                })?,
            )?;
            codes.set_metatable(Some(meta))?;
            codes.set_readonly(true);
            lua.set_named_registry_value(NAMESPACE, &codes)?;
            codes
        }
    };
    host.set("errors", codes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::{input, screen, HostError, REASONS};
    use mlua::Function;

    /// A VM with `host.errors`, `host.json.encode`, and `E(reason, message?, detail?)` making the
    /// host's error value of a Reason by its index in [`REASONS`].
    fn vm() -> Lua {
        let lua = Lua::new();
        let host = lua.create_table().unwrap();
        install(&lua, &host).unwrap();
        let json = lua.create_table().unwrap();
        json.set("encode", lua.create_function(|lua, args: (Value, Value)| crate::json::encode(lua, args)).unwrap()).unwrap();
        host.set("json", json).unwrap();
        lua.globals().set("host", host).unwrap();
        lua
    }

    fn yes(lua: &Lua, code: &str) {
        match lua.load(code).set_name("=check").eval::<Value>() {
            Ok(Value::Boolean(true)) => {}
            other => panic!("{code}\n→ {other:?}"),
        }
    }

    /// The error `code` raises, as text.
    fn raised(lua: &Lua, code: &str) -> String {
        match lua.load(code).set_name("=mod.luau").exec() {
            Err(e) => e.to_string(),
            Ok(()) => panic!("{code} did not raise"),
        }
    }

    /// The same checks for any error value `err` with `message` and `detail`: what a module can
    /// do with one.
    fn behaves(lua: &Lua, message: &str, detail: Option<&str>) {
        lua.globals().set("want", message).unwrap();
        lua.globals().set("wantDetail", detail).unwrap();
        yes(lua, "return type(err) == 'table' and typeof(err) == 'table'");
        yes(lua, "return tostring(err) == want");
        yes(lua, "return ('a: ' .. err) == 'a: ' .. want and (err .. ' b') == want .. ' b'");
        yes(lua, "return (err .. err) == want .. want and (1 .. err) == '1' .. want");
        // Luau's `%s` takes strings only, unlike Lua 5.4's, and raises for any table; `%*` and
        // `tostring` give the message.
        yes(lua, "return string.format('%*', err) == want and string.format('[%s]', tostring(err)) == '[' .. want .. ']'");
        assert!(raised(lua, "local x = string.format('%s', err)").contains("string expected, got table"));
        // The rest of the string library, utf8 and table.concat take strings too; `#` is a
        // table's, 0.
        assert!(raised(lua, "local x = string.len(err)").contains("invalid argument #1 to 'len' (string expected, got table)"));
        for call in ["string.upper(err)", "utf8.len(err)"] {
            assert!(raised(lua, &format!("local x = {call}")).contains("string expected, got table"), "{call}");
        }
        assert!(raised(lua, "local x = table.concat({ 'a', err })").contains("invalid value (table) at index 2 in table for 'concat'"));
        yes(lua, "return #err == 0 and table.concat({ 'a', tostring(err) }) == 'a' .. want");
        // `error` raises the value itself, and its message is what is reported (mailbox_tests.rs);
        // Luau's `assert` takes a string message only, so `assert(ok, err)` raises about its own
        // argument instead — `assert(ok, tostring(err))` keeps the message.
        assert!(raised(lua, "error(err)").contains(&format!("{message}\nstack traceback")));
        assert!(raised(lua, "assert(false, err)").contains("invalid argument #2 to 'assert' (string expected, got table)"));
        assert!(raised(lua, "assert(false, tostring(err))").contains(message));
        yes(lua, "return err.message == want and err.detail == wantDetail");
        yes(lua, "local n = 0; for k in pairs(err) do n += 1 end; return n == (wantDetail and 3 or 2)");
        let e = raised(lua, "local x = err.cod");
        assert!(e.contains("an error value has code, message and detail, not 'cod'") && e.contains("mod.luau:1"), "{e}");
        let e = raised(lua, "local x = err:find('x')");
        assert!(e.contains("an error value has code, message and detail, not 'find'"), "{e}");
        for change in ["err.code = 'other'", "err.extra = 1", "setmetatable(err, nil)", "getmetatable(err).__index = nil"] {
            let e = raised(lua, change);
            assert!(e.to_lowercase().contains("readonly"), "{change}: {e}");
        }
        let e = raised(lua, "local x = err .. {}");
        assert!(e.contains("attempt to concatenate an error value with a table value") && e.contains("mod.luau:1"), "{e}");
        assert!(raised(lua, "local x = nil .. err").contains("attempt to concatenate an error value with a nil value"));
        // No protected metatable stands in the way of a copy, which keeps it and can be written.
        yes(lua, "local c = table.clone(err); c.extra = 1; return c.message == want and getmetatable(c) == getmetatable(err)");
    }

    #[test]
    fn host_errors_holds_the_codes_and_new_and_nothing_else() {
        let lua = vm();
        for code in Code::ALL {
            yes(&lua, &format!("return host.errors.{} == '{}'", code.constant(), code.as_str()));
        }
        yes(&lua, "local n, f = 0, 0; for k, v in pairs(host.errors) do if type(v) == 'string' then n += 1; assert(k == string.upper(v)) else f += 1; assert(k == 'new') end end; return n == 19 and f == 1");
        let e = raised(&lua, "local x = host.errors.NOT_REDY");
        let list = Code::ALL.iter().map(|c| c.constant()).collect::<Vec<_>>().join(", ");
        assert!(e.contains(&format!("host.errors has no 'NOT_REDY'; the codes are {list}")) && e.contains("mod.luau:1"), "{e}");
        for change in ["host.errors.NOT_READY = 'x'", "host.errors.MINE = 'x'", "getmetatable(host.errors).__index = nil"] {
            let e = raised(&lua, change);
            assert!(e.to_lowercase().contains("readonly"), "{change}: {e}");
        }
        // A second host of the same VM — a code dependency's — has the same table.
        let other = lua.create_table().unwrap();
        install(&lua, &other).unwrap();
        lua.globals().set("other", other).unwrap();
        yes(&lua, "return rawequal(other.errors, host.errors)");
    }

    #[test]
    fn an_error_value_of_the_host_reads_and_says_its_message() {
        let lua = vm();
        let e = HostError::with(&screen::CLIENT_EMPTY, "the window's client area is empty (0x0)".into());
        lua.globals().set("err", to_lua(&lua, &e).unwrap()).unwrap();
        behaves(&lua, "the window's client area is empty (0x0)", None);
        yes(&lua, "return err.code == host.errors.HIDDEN");
        let e = HostError::of(&input::COVERED).with_detail("the window above: Notepad".into());
        lua.globals().set("err", to_lua(&lua, &e).unwrap()).unwrap();
        behaves(&lua, "something else is drawn over the point", Some("the window above: Notepad"));
        yes(&lua, "return err.code == host.errors.COVERED");
    }

    #[test]
    fn an_error_value_of_new_is_the_same_and_new_checks_what_it_is_given() {
        let lua = vm();
        lua.load("err = host.errors.new('not_in_front', 'the overlay is no longer active')").exec().unwrap();
        behaves(&lua, "the overlay is no longer active", None);
        yes(&lua, "return err.code == 'not_in_front'");
        lua.load("err = host.errors.new(host.errors.BUSY, 'one at a time', 'the first began 3 s ago')").exec().unwrap();
        behaves(&lua, "one at a time", Some("the first began 3 s ago"));
        yes(&lua, "return getmetatable(err) == getmetatable(host.errors.new('x', 'y'))");
        for (call, says) in [
            ("host.errors.new('Not_Ready', 'm')", "the code 'Not_Ready' is not lower-case snake_case"),
            ("host.errors.new('not-ready', 'm')", "the code 'not-ready' is not lower-case snake_case"),
            ("host.errors.new('9lives', 'm')", "the code '9lives' is not lower-case snake_case"),
            ("host.errors.new('', 'm')", "the code '' is not lower-case snake_case"),
            ("host.errors.new(nil, 'm')", "the code must be a string, not a nil"),
            ("host.errors.new(1, 'm')", "the code must be a string, not a number"),
            ("host.errors.new('busy', '')", "the message is empty"),
            ("host.errors.new('busy')", "the message must be a string, not a nil"),
            ("host.errors.new('busy', 12)", "the message must be a string, not a number"),
            ("host.errors.new('busy', 'm', 5)", "the detail must be a string or nil, not a number"),
            ("host.errors.new('busy', 'm', 0.5)", "the detail must be a string or nil, not a number"),
            ("host.errors.new('busy', 'm', {})", "the detail must be a string or nil, not a table"),
        ] {
            let e = raised(&lua, call);
            assert!(e.contains(&format!("host.errors.new: {says}")), "{call}: {e}");
        }
    }

    /// json.encode reads raw fields: the code, the message and the detail, as a page gets them.
    #[test]
    fn json_encode_gives_the_three_fields() {
        let lua = vm();
        let e = HostError::of(&screen::CAPTURE_FAILED).with_detail("BitBlt failed".into());
        lua.globals().set("err", to_lua(&lua, &e).unwrap()).unwrap();
        let text: String = lua.load("return host.json.encode({ ok = false, error = err })").eval().unwrap();
        let back: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(back, serde_json::json!({ "ok": false, "error": { "code": "failed", "message": "screen capture failed", "detail": "BitBlt failed" } }));
        lua.load("err = host.errors.new('busy', 'one at a time')").exec().unwrap();
        let text: String = lua.load("return host.json.encode(err)").eval().unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&text).unwrap(), serde_json::json!({ "code": "busy", "message": "one at a time" }));
    }

    /// A HostError made on one thread and handed to a VM on another, as an answer crosses from a
    /// worker to the event loop, is the same error value there as one made in place.
    #[test]
    fn an_error_value_delivered_to_another_vm_behaves_the_same() {
        let e = HostError::with(&screen::OWNER_REQUESTS_ENDED, screen::OWNER_REQUESTS_ENDED.message.to_string()).with_detail("x".into());
        let crossed = std::thread::spawn(move || e).join().unwrap();
        for _ in 0..2 {
            let lua = vm();
            let cb: Function = lua.load("return function(value, err) got = err; return value == nil end").eval().unwrap();
            let first: bool = cb.call((Value::Nil, to_lua(&lua, &crossed).unwrap())).unwrap();
            assert!(first);
            lua.load("err = got").exec().unwrap();
            behaves(&lua, "too many snapshot requests waiting for this module (16); the oldest was ended", Some("x"));
            yes(&lua, "return err.code == host.errors.TOO_MANY");
        }
    }

    /// `host.log.info` and `host.speech.output` take their text as [`Text`]: an error value's
    /// message, a string or a number, and anything else refused in the words a `String` argument
    /// is refused with.
    #[test]
    fn a_text_argument_takes_an_error_value_as_its_message() {
        let lua = vm();
        let said = lua.create_function(|_, (t, _opts): (Text, Option<Table>)| Ok(t.0)).unwrap();
        let plain = lua.create_function(|_, (t, _opts): (String, Option<Table>)| Ok(t)).unwrap();
        lua.globals().set("said", said).unwrap();
        lua.globals().set("plain", plain).unwrap();
        lua.load("err = host.errors.new('busy', 'one at a time', 'never said')").exec().unwrap();
        yes(&lua, "return said(err) == 'one at a time' and said(err, { interrupt = false }) == 'one at a time'");
        yes(&lua, "return said('words') == 'words' and said(12) == '12'");
        let e = HostError::of(&screen::CAPTURE_FAILED);
        lua.globals().set("host_err", to_lua(&lua, &e).unwrap()).unwrap();
        yes(&lua, "return said(host_err) == 'screen capture failed'");
        // A table that only looks like one is not one, and is refused as before, in log.md's words.
        let refused = raised(&lua, "said({ code = 'busy', message = 'm' })");
        assert!(refused.starts_with("bad argument #1: error converting Lua table to String (expected string or number)"), "{refused}");
        for arg in ["{ code = 'busy', message = 'm' }", "setmetatable({ message = 'm' }, { __tostring = function() return 'm' end })", "nil", "true"] {
            let ours = raised(&lua, &format!("said({arg})"));
            let before = raised(&lua, &format!("plain({arg})"));
            assert_eq!(ours.lines().next(), before.lines().next(), "{arg}");
        }
    }

    /// `host.log.info` and `host.speech.output` are the two text calls that take an error value.
    #[test]
    fn the_log_and_speech_read_their_text_as_text() {
        const LIB: &str = include_str!("lib.rs");
        let api = &LIB[LIB.find("fn install_host_api(").unwrap()..];
        // The binding's own lines: from its name to its first statement.
        let binding = |name: &str| {
            let at = api.find(&format!("\"{name}\",")).unwrap();
            &api[at..at + api[at..].find("host_call!").unwrap()]
        };
        assert!(binding("info").contains("|_, msg: errors_lua::Text|"), "host.log.info");
        assert!(binding("output").contains("(text, opts): (errors_lua::Text, Option<Table>)"), "host.speech.output");
        assert!(api.contains("errors_lua::install(lua, &host)?;"), "host.errors");
    }

    /// Every Reason makes an error value whose code is its own and whose message is its pattern.
    #[test]
    fn every_reason_makes_its_error_value() {
        let lua = vm();
        for r in REASONS {
            let t = to_lua(&lua, &HostError::of(r)).unwrap();
            assert_eq!(t.raw_get::<String>("code").unwrap(), r.code.as_str());
            assert_eq!(t.raw_get::<String>("message").unwrap(), r.message);
        }
    }
}
