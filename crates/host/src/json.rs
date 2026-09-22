//! `host.json`: JSON text to Luau values and back.
//!
//! Modules already had a way to READ a data file (`host.resource.read`) and no way to make
//! sense of one: Luau has no JSON parser, the host exposed none, and the reference's own
//! example called a `parse()` that did not exist. Both halves were already dependencies —
//! `serde_json`, and mlua's `serialize` feature for the legacy data exports — so `decode` is the
//! two joined, with the one choice that matters made on purpose: what `null` becomes.
//!
//! `encode` is the other direction, for structured log lines and for change detection
//! (`encode(state) ~= last`). It does NOT go through mlua's own Lua-to-serde path, which is
//! not safe as an encoder: a table with a sequence part is written as an array and its string
//! keys are dropped without a word, NaN and infinity silently become `null`, and a cycle raises
//! an error that does not say where. The walker below decides every one of those on purpose,
//! and names the place in the value when it refuses.

use std::ffi::c_void;

use mlua::{Lua, LuaSerdeExt, SerializeOptions, Table, Value};
use serde::Serialize;

/// How a parsed document is turned into Luau values.
///
/// JSON `null` becomes `nil`. mlua's default is a `NULL` light userdata instead, which is
/// TRUTHY — `if cfg.optional then` would take the branch for a value that is explicitly absent,
/// the opposite of what anyone reading the JSON means. So an object's `null` member is simply
/// not there, and a `null` in an array leaves a hole (see the docs: `#` is unreliable across
/// it).
///
/// Arrays are plain tables. mlua's default marks them with a PROTECTED metatable, so that they
/// serialise back as arrays; nothing here serialises them back, and a protected metatable
/// would make `setmetatable` on a decoded list raise.
const OPTIONS: SerializeOptions = SerializeOptions::new()
    .serialize_none_to_null(false)
    .serialize_unit_to_null(false)
    .set_array_metatable(false);

/// `host.json.decode(text) -> any`. Raises with the line and column of the first error, so a
/// hand-edited data file names where it went wrong.
///
/// A UTF-8 byte-order mark at the start is skipped. JSON forbids one and `serde_json` refuses
/// it as "expected value at line 1 column 1" — a message that points at nothing visible — while
/// the editors a data file is hand-edited in write it: Notepad before Windows 10 1903, and
/// PowerShell 5's `Out-File -Encoding utf8`. `host.resource.read` hands the file over as it is.
pub(crate) fn decode(lua: &Lua, text: mlua::String) -> mlua::Result<Value> {
    let bytes = text.as_bytes();
    let all: &[u8] = &bytes;
    let body = all.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(all);
    let doc: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| mlua::Error::external(format!("host.json.decode: {e}")))?;
    lua.to_value_with(&doc, OPTIONS)
}

/// The named-registry key of this VM's array marker — see [`array`].
const ARRAY_MARKER: &str = "__json_array_mt";

/// The deepest nesting `encode` writes: 127 containers, which is exactly what `decode` reads
/// back (serde_json stops at a depth of 128).
const MAX_DEPTH: usize = 127;

/// 2^53: the largest magnitude below which every whole double is exact, and the bound for
/// writing a number as an integer.
const SAFE_INTEGER: f64 = 9_007_199_254_740_992.0;

/// This VM's array marker, made on first use.
///
/// One table per VM, and a plain one: FROZEN, so no module can change what it means, and with
/// no `__metatable` field, so it protects nothing. That second half is the point. mlua's own
/// array marker is protected (`__metatable = false`), and Luau's `table.clone` and
/// `table.freeze` raise on any table whose metatable is protected, as `setmetatable` does — a
/// marked list could then be neither copied, frozen nor given a metatable of the module's own.
fn array_marker(lua: &Lua) -> mlua::Result<Table> {
    if let Some(t) = lua.named_registry_value::<Option<Table>>(ARRAY_MARKER)? {
        return Ok(t);
    }
    let mt = lua.create_table()?;
    mt.set_readonly(true);
    lua.set_named_registry_value(ARRAY_MARKER, &mt)?;
    Ok(mt)
}

/// `host.json.array(t?) -> table`: marks `t`, or a new empty table, to be written as a JSON
/// array, and returns it.
///
/// Needed for exactly one case — an EMPTY list, which is otherwise indistinguishable from an
/// empty object and is written as `{}`. A table with entries 1..n is an array already.
pub(crate) fn array(lua: &Lua, t: Value) -> mlua::Result<Table> {
    let fail = |why: String| mlua::Error::external(format!("host.json.array: {why}"));
    let t = match t {
        Value::Nil => lua.create_table()?,
        Value::Table(t) => t,
        other => return Err(fail(format!("expected a table or nothing, not a {}", luau_type(&other)))),
    };
    let marker = array_marker(lua)?;
    if let Some(mt) = t.metatable() {
        let p = mt.to_pointer();
        if p == marker.to_pointer() || p == lua.array_metatable().to_pointer() {
            return Ok(t); // already marked, by us or by mlua
        }
        return Err(fail(
            "the table already has a metatable, and marking it would replace that one".into(),
        ));
    }
    if t.is_readonly() {
        return Err(fail(
            "the table is frozen, so it cannot be marked; mark it before table.freeze".into(),
        ));
    }
    t.set_metatable(Some(marker))?;
    Ok(t)
}

/// One step of the way from the value handed to `encode` down to the part being written.
enum Step {
    Key(String),
    Index(i64),
}

/// `value.states[2].cels`, Luau-style, for an error message.
fn render(path: &[Step]) -> String {
    let mut out = String::from("value");
    for step in path {
        match step {
            Step::Index(i) => out.push_str(&format!("[{i}]")),
            Step::Key(k) => {
                let ident = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if ident {
                    out.push('.');
                    out.push_str(k);
                } else {
                    out.push_str(&format!("[{k:?}]"));
                }
            }
        }
    }
    out
}

/// What a key is, for the table rules.
enum KeyKind {
    Text(String),
    /// A whole number of at least 1.
    Position(i64),
}

/// A value's type as Luau's `type()` names it, for an error. mlua keeps a whole number apart
/// as an "integer" and a light userdata as a "lightuserdata"; Luau has neither type, and a
/// message saying "not a integer" names nothing the author wrote.
fn luau_type(v: &Value) -> &'static str {
    match v {
        Value::Integer(_) | Value::Number(_) => "number",
        Value::LightUserData(_) => "userdata",
        other => other.type_name(),
    }
}

/// A value's name in an error, spelt the way a module author would say it.
fn a_value(v: &Value) -> String {
    match v {
        Value::Integer(i) => format!("the number {i}"),
        Value::Number(n) => format!("the number {n}"),
        Value::String(s) => format!("the string {:?}", s.to_string_lossy()),
        other => format!("a {}", luau_type(other)),
    }
}

/// The suggestion every numeric-key refusal ends with, because the likely cause is always the
/// same: an id used as a key.
const USE_TOSTRING: &str =
    "if the numbers are ids, key the table by tostring(id), which JSON keeps as an object";

/// Lua values to a `serde_json::Value`, refusing — with the place — whatever JSON cannot hold.
struct Walker {
    /// This VM's marker and mlua's: a table carrying either is an array whatever its keys say.
    markers: [*const c_void; 2],
    path: Vec<Step>,
    /// The tables being written, outermost first, with the path length each was reached at —
    /// a table met again while it is still open is a cycle, and both places are named.
    open: Vec<(*const c_void, usize)>,
}

impl Walker {
    fn fail(&self, why: impl std::fmt::Display) -> String {
        format!("host.json.encode: {why} at {}", render(&self.path))
    }

    fn value(&mut self, v: &Value) -> Result<serde_json::Value, String> {
        use serde_json::Value as J;
        match v {
            Value::Nil => Ok(J::Null),
            Value::Boolean(b) => Ok(J::Bool(*b)),
            // A Luau number that mlua saw was a whole number fitting an i64.
            Value::Integer(i) => {
                if i.unsigned_abs() <= SAFE_INTEGER as u64 {
                    Ok(J::from(*i))
                } else {
                    Ok(float(*i as f64))
                }
            }
            Value::Number(n) => {
                if !n.is_finite() {
                    Err(self.fail(format!("{n} has no JSON form (JSON has no NaN or infinity)")))
                } else if n.trunc() == *n && n.abs() <= SAFE_INTEGER {
                    // `-0` included: as an integer it is simply 0.
                    Ok(J::from(*n as i64))
                } else {
                    Ok(float(*n))
                }
            }
            Value::String(s) => match s.to_str() {
                Ok(text) => Ok(J::String(text.to_string())),
                Err(_) => Err(self.fail("a string that is not valid UTF-8")),
            },
            Value::Table(t) => self.table(t),
            other => Err(self.fail(format!("a {} has no JSON form", luau_type(other)))),
        }
    }

    fn table(&mut self, t: &Table) -> Result<serde_json::Value, String> {
        let me = t.to_pointer();
        if let Some(&(_, at)) = self.open.iter().find(|(p, _)| *p == me) {
            return Err(format!(
                "host.json.encode: a cycle — {} is {} again",
                render(&self.path),
                render(&self.path[..at])
            ));
        }
        if self.open.len() >= MAX_DEPTH {
            return Err(self.fail(format!(
                "tables nested more than {MAX_DEPTH} deep, which host.json.decode could not read back"
            )));
        }
        let marked = t.metatable().is_some_and(|mt| self.markers.contains(&mt.to_pointer()));

        // Raw entries only — `__index`, `__iter` and every other metamethod are ignored; the
        // array marker is the one metatable that counts. Collected first, so nothing is
        // iterated while the walk goes deeper.
        let mut entries: Vec<(Value, Value)> = Vec::new();
        t.for_each::<Value, Value>(|k, v| {
            entries.push((k, v));
            Ok(())
        })
        .map_err(|e| self.fail(e))?;
        if entries.is_empty() {
            return Ok(if marked {
                serde_json::Value::Array(Vec::new())
            } else {
                serde_json::Value::Object(serde_json::Map::new())
            });
        }

        let mut keyed: Vec<(KeyKind, Value)> = Vec::with_capacity(entries.len());
        for (k, v) in entries {
            let kind = match &k {
                Value::String(s) => match s.to_str() {
                    Ok(text) => KeyKind::Text(text.to_string()),
                    Err(_) => return Err(self.fail("a key that is not valid UTF-8")),
                },
                Value::Integer(i) if *i >= 1 => KeyKind::Position(*i),
                Value::Integer(_) | Value::Number(_) => {
                    return Err(self.fail(format!(
                        "{} as a key, which neither a JSON array (indices from 1) nor a JSON \
                         object (string keys) can hold — {USE_TOSTRING}",
                        a_value(&k)
                    )))
                }
                other => {
                    return Err(self.fail(format!(
                        "a {} as a key; JSON keys are strings",
                        luau_type(other)
                    )))
                }
            };
            keyed.push((kind, v));
        }
        let texts = keyed.iter().filter(|(k, _)| matches!(k, KeyKind::Text(_))).count();
        if texts > 0 && texts < keyed.len() {
            let text = keyed.iter().find_map(|(k, _)| match k {
                KeyKind::Text(s) => Some(s.clone()),
                KeyKind::Position(_) => None,
            });
            let pos = keyed.iter().find_map(|(k, _)| match k {
                KeyKind::Position(i) => Some(*i),
                KeyKind::Text(_) => None,
            });
            return Err(self.fail(format!(
                "a table with both string keys ({:?}) and number keys ({}), which JSON cannot \
                 mix — {USE_TOSTRING}",
                text.unwrap_or_default(),
                pos.unwrap_or_default()
            )));
        }

        self.open.push((me, self.path.len()));
        let out = if texts == 0 {
            self.array(keyed)
        } else if marked {
            Err(self.fail("a table marked by host.json.array has string keys"))
        } else {
            self.object(keyed)
        };
        self.open.pop();
        out
    }

    /// Keys 1..max, holes as `null` — unless the holes would outnumber what is there.
    ///
    /// The sparse rule is Lua CJSON's default: refused when the largest index is above 10 AND
    /// more than twice the number of entries. Writing it as an object instead would turn the
    /// integer keys into string keys when read back, which is a different table.
    fn array(&mut self, mut keyed: Vec<(KeyKind, Value)>) -> Result<serde_json::Value, String> {
        let pos = |k: &KeyKind| match k {
            KeyKind::Position(i) => *i,
            KeyKind::Text(_) => unreachable!("only called with number keys"),
        };
        keyed.sort_by_key(|(k, _)| pos(k));
        let max = pos(&keyed[keyed.len() - 1].0);
        let count = keyed.len() as i64;
        if max > 10 && max > 2 * count {
            return Err(self.fail(format!(
                "a list with {count} entr{} whose largest index is {max}, too sparse to write \
                 as a JSON array (it would be padded with {} nulls) — {USE_TOSTRING}",
                if count == 1 { "y" } else { "ies" },
                max - count
            )));
        }
        let mut items = vec![serde_json::Value::Null; max as usize];
        for (k, v) in keyed {
            let i = pos(&k);
            self.path.push(Step::Index(i));
            let item = self.value(&v);
            self.path.pop();
            items[(i - 1) as usize] = item?;
        }
        Ok(serde_json::Value::Array(items))
    }

    /// String keys, written in byte order.
    ///
    /// Sorted HERE rather than left to `serde_json::Map`: that map is a BTreeMap today, but any
    /// crate in the build that switches on serde_json's `preserve_order` feature turns it into
    /// an insertion-ordered one for every user at once. Inserting in sorted order is right
    /// under both.
    fn object(&mut self, keyed: Vec<(KeyKind, Value)>) -> Result<serde_json::Value, String> {
        let mut pairs: Vec<(String, Value)> = keyed
            .into_iter()
            .map(|(k, v)| match k {
                KeyKind::Text(s) => (s, v),
                KeyKind::Position(_) => unreachable!("only called with string keys"),
            })
            .collect();
        pairs.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let mut map = serde_json::Map::new();
        for (k, v) in pairs {
            self.path.push(Step::Key(k.clone()));
            let item = self.value(&v);
            self.path.pop();
            map.insert(k, item?);
        }
        Ok(serde_json::Value::Object(map))
    }
}

/// A finite double that is not a safe integer, in the shortest form that reads back exactly.
fn float(n: f64) -> serde_json::Value {
    serde_json::Number::from_f64(n)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}

/// Reads `{ pretty = true | 1..8 }`: `None` for compact output, else the indent width.
fn indent_from(opts: &Value) -> mlua::Result<Option<usize>> {
    let fail = |why: String| mlua::Error::external(format!("host.json.encode: {why}"));
    let t = match opts {
        Value::Nil => return Ok(None),
        Value::Table(t) => t,
        other => return Err(fail(format!("options are a table, not a {}", luau_type(other)))),
    };
    let mut indent = None;
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair?;
        let key = match &k {
            Value::String(s) => s.to_string_lossy(),
            other => {
                return Err(fail(format!("option names are strings, not a {}", luau_type(other))))
            }
        };
        if key != "pretty" {
            return Err(fail(format!("'{key}' is not an option; the only one is pretty")));
        }
        indent = match v {
            Value::Nil | Value::Boolean(false) => None,
            Value::Boolean(true) => Some(2),
            Value::Integer(n) if (1..=8).contains(&n) => Some(n as usize),
            Value::Number(n) if n.trunc() == n && (1.0..=8.0).contains(&n) => Some(n as usize),
            other => {
                return Err(fail(format!(
                    "pretty is true, false or an indent width from 1 to 8, not {}",
                    a_value(&other)
                )))
            }
        };
    }
    Ok(indent)
}

/// `host.json.encode(value, { pretty = true | 1..8 }?) -> string`.
///
/// Compact by default. Keys are always sorted, so equal tables give equal strings and
/// `encode(t) ~= last` is a real change test. Raises, naming the place, for anything JSON
/// cannot hold — see [`Walker`].
pub(crate) fn encode(lua: &Lua, (value, opts): (Value, Value)) -> mlua::Result<String> {
    let indent = indent_from(&opts)?;
    let marker = array_marker(lua)?;
    let mut walker = Walker {
        markers: [marker.to_pointer(), lua.array_metatable().to_pointer()],
        path: Vec::new(),
        open: Vec::new(),
    };
    let doc = walker.value(&value).map_err(mlua::Error::external)?;
    let text = match indent {
        None => serde_json::to_string(&doc),
        Some(n) => {
            let spaces = vec![b' '; n];
            let mut out = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut out,
                serde_json::ser::PrettyFormatter::with_indent(&spaces),
            );
            doc.serialize(&mut ser).map(|()| String::from_utf8(out).unwrap_or_default())
        }
    };
    text.map_err(|e| mlua::Error::external(format!("host.json.encode: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lua: &Lua, json: &str, check: &str) -> bool {
        lua.globals().set("decode", lua.create_function(decode).unwrap()).unwrap();
        lua.globals().set("text", json).unwrap();
        lua.load(format!("local v = decode(text) return {check}")).eval().unwrap()
    }

    #[test]
    fn documents_become_plain_luau_values() {
        let lua = Lua::new();
        let doc = r#"{ "name": "WarmSelection", "w": 10, "cells": [1, 2.5, -3],
                       "on": true, "nested": { "a": "b" } }"#;
        assert!(run(&lua, doc, "v.name == 'WarmSelection' and v.w == 10 and v.on == true"));
        assert!(run(&lua, doc, "#v.cells == 3 and v.cells[2] == 2.5 and v.cells[3] == -3"));
        assert!(run(&lua, doc, "v.nested.a == 'b'"));
        // A plain table: no protected metatable in the way of a module's own.
        assert!(run(&lua, doc, "getmetatable(v.cells) == nil and pcall(setmetatable, v.cells, {})"));
    }

    #[test]
    fn null_is_nil_not_a_truthy_placeholder() {
        let lua = Lua::new();
        assert!(run(&lua, r#"{ "a": null, "b": 1 }"#, "v.a == nil and v.b == 1"));
        assert!(run(&lua, "null", "v == nil"));
        assert!(run(&lua, "[1, null, 3]", "v[1] == 1 and v[2] == nil and v[3] == 3"));
        assert!(run(&lua, "[]", "type(v) == 'table' and next(v) == nil"));
    }

    #[test]
    fn a_byte_order_mark_is_skipped() {
        let lua = Lua::new();
        lua.globals().set("decode", lua.create_function(decode).unwrap()).unwrap();
        let text = lua.create_string(b"\xEF\xBB\xBF{ \"a\": [1, 2] }").unwrap();
        lua.globals().set("text", text).unwrap();
        assert!(lua.load("local v = decode(text) return v.a[2] == 2").eval::<bool>().unwrap());
        // Only at the start: anywhere else it is still an error.
        let text = lua.create_string(b"{ \"a\": \xEF\xBB\xBF1 }").unwrap();
        lua.globals().set("text", text).unwrap();
        assert!(lua.load("return decode(text)").eval::<Value>().is_err());
    }

    #[test]
    fn a_bad_document_names_where() {
        let lua = Lua::new();
        lua.globals().set("decode", lua.create_function(decode).unwrap()).unwrap();
        let e = lua
            .load("return decode('{\\n  \"a\": 1,\\n  \"b\": ]\\n}')")
            .eval::<Value>()
            .unwrap_err()
            .to_string();
        assert!(e.contains("host.json.decode") && e.contains("line 3"), "{e}");
    }

    // ---- encode and array ---------------------------------------------------------------

    /// A VM with the three functions as the global `json`, the way modules reach them.
    fn json_vm() -> Lua {
        let lua = Lua::new();
        let json = lua.create_table().unwrap();
        json.set("decode", lua.create_function(decode).unwrap()).unwrap();
        json.set("encode", lua.create_function(encode).unwrap()).unwrap();
        json.set("array", lua.create_function(array).unwrap()).unwrap();
        lua.globals().set("json", json).unwrap();
        lua
    }

    fn enc(lua: &Lua, expr: &str) -> String {
        lua.load(format!("return json.encode({expr})")).eval::<String>().unwrap_or_else(|e| panic!("{expr}: {e}"))
    }

    /// The error `json.encode(<expr>)` raises, as the module would see it.
    fn enc_err(lua: &Lua, expr: &str) -> String {
        match lua.load(format!("return json.encode({expr})")).eval::<String>() {
            Ok(s) => panic!("{expr} encoded as {s} instead of raising"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn scalars() {
        let lua = json_vm();
        assert_eq!(enc(&lua, "nil"), "null");
        assert_eq!(enc(&lua, "true"), "true");
        assert_eq!(enc(&lua, "3"), "3");
        assert_eq!(enc(&lua, "3.0"), "3", "a whole number has no fraction");
        assert_eq!(enc(&lua, "-7"), "-7");
        assert_eq!(enc(&lua, "2^53"), "9007199254740992");
        assert_eq!(enc(&lua, "-2^53"), "-9007199254740992");
        assert_eq!(enc(&lua, "0.1"), "0.1");
        assert_eq!(enc(&lua, "-0"), "0");
        assert_eq!(enc(&lua, "-0.0"), "0");
        // serde_json's shortest exact form, exponent sign included.
        assert_eq!(enc(&lua, "1.5e300"), "1.5e+300");
        // Past 2^53 a whole number is written as a double that reads back exactly.
        let big: bool = lua.load("return json.decode(json.encode(2^60)) == 2^60").eval().unwrap();
        assert!(big);
        for bad in ["0/0", "math.huge", "-math.huge"] {
            let e = enc_err(&lua, bad);
            assert!(e.contains("host.json.encode") && e.contains("no JSON form"), "{bad}: {e}");
        }
    }

    #[test]
    fn strings() {
        let lua = json_vm();
        assert_eq!(enc(&lua, r#""a\"b\\c\n\t""#), r#""a\"b\\c\n\t""#);
        assert_eq!(enc(&lua, r#""nul\0end""#), r#""nul\u0000end""#);
        // Non-ASCII stays UTF-8, and `/` is not escaped.
        assert_eq!(enc(&lua, r#""Grüße/größer""#), "\"Grüße/größer\"");
        let e = enc_err(&lua, r#"{ name = "\xff" }"#);
        assert!(e.contains("not valid UTF-8") && e.contains("value.name"), "{e}");
        let e = enc_err(&lua, r#"{ ["\xff"] = 1 }"#);
        assert!(e.contains("key that is not valid UTF-8"), "{e}");
    }

    #[test]
    fn tables_arrays_and_objects() {
        let lua = json_vm();
        assert_eq!(enc(&lua, "{}"), "{}", "an empty unmarked table is an object");
        assert_eq!(enc(&lua, "json.array()"), "[]");
        assert_eq!(enc(&lua, "json.array({})"), "[]");
        assert_eq!(enc(&lua, "json.array({ 1, 2 })"), "[1,2]");
        // What `[]` decodes to is plain, so it comes back as `{}`: the one documented loss.
        assert_eq!(enc(&lua, "json.decode('[]')"), "{}");
        assert_eq!(enc(&lua, "{ 1, nil, 3 }"), "[1,null,3]");
        assert_eq!(enc(&lua, "{ 'a', { 'b' } }"), r#"["a",["b"]]"#);
        assert_eq!(enc(&lua, "{ b = 1, a = { x = true }, B = 3 }"), r#"{"B":3,"a":{"x":true},"b":1}"#);
        // mlua's own array marker counts too (the legacy data exports carry it).
        let lua2 = json_vm();
        let marked = lua2.array_metatable();
        let t = lua2.create_table().unwrap();
        t.set_metatable(Some(marked)).unwrap();
        lua2.globals().set("mluaArray", t).unwrap();
        assert_eq!(enc(&lua2, "mluaArray"), "[]");
    }

    #[test]
    fn sparse_lists_at_the_limit() {
        let lua = json_vm();
        assert_eq!(enc(&lua, "{ [1] = 1, [10] = 2 }"), "[1,null,null,null,null,null,null,null,null,2]");
        let e = enc_err(&lua, "{ seen = { [1] = 1, [21] = 2 } }");
        assert!(e.contains("too sparse") && e.contains("tostring(id)") && e.contains("value.seen"), "{e}");
        // The same rule for a marked table: a marker is not a licence to pad a million nulls.
        let e = enc_err(&lua, "json.array({ [1000000] = true })");
        assert!(e.contains("too sparse"), "{e}");
    }

    #[test]
    fn keys_json_cannot_hold_raise() {
        let lua = json_vm();
        for (expr, want) in [
            ("{ 1, a = 2 }", "both string keys"),
            ("{ [0] = 1 }", "the number 0 as a key"),
            ("{ [-1] = 1 }", "the number -1 as a key"),
            ("{ [1.5] = 1 }", "the number 1.5 as a key"),
            ("{ [true] = 1 }", "a boolean as a key"),
            ("{ [{}] = 1 }", "a table as a key"),
            ("{ [print] = 1 }", "a function as a key"),
        ] {
            let e = enc_err(&lua, expr);
            assert!(e.contains(want), "{expr}: {e}");
        }
        // Every numeric-key refusal says what to do about it.
        for expr in ["{ 1, a = 2 }", "{ [0] = 1 }", "{ [1.5] = 1 }"] {
            assert!(enc_err(&lua, expr).contains("tostring(id)"), "{expr}");
        }
        let e = enc_err(&lua, "json.array({ a = 1 })");
        assert!(e.contains("marked by host.json.array has string keys"), "{e}");
    }

    #[test]
    fn metatables_are_not_consulted() {
        let lua = json_vm();
        let s = enc(
            &lua,
            "setmetatable({ x = 1 }, { __index = function() return 5 end, \
             __iter = function() return next, { y = 2 } end, __len = function() return 9 end })",
        );
        assert_eq!(s, r#"{"x":1}"#);
        assert_eq!(enc(&lua, "setmetatable({}, { __index = { a = 1 } })"), "{}");
    }

    #[test]
    fn cycles_and_shared_subtables() {
        let lua = json_vm();
        let e = lua
            .load("local t = { a = {} }; t.a.b = t; return json.encode(t)")
            .eval::<String>()
            .unwrap_err()
            .to_string();
        assert!(e.contains("a cycle") && e.contains("value.a.b is value again"), "{e}");
        let e = lua
            .load("local t = { list = { {}, {} } }; t.list[2].back = t.list; return json.encode(t)")
            .eval::<String>()
            .unwrap_err()
            .to_string();
        assert!(e.contains("value.list[2].back is value.list again"), "{e}");
        // Not a cycle: the same table in two places is written twice.
        let s: String = lua.load("local s = { 1 }; return json.encode({ x = s, y = s })").eval().unwrap();
        assert_eq!(s, r#"{"x":[1],"y":[1]}"#);
    }

    /// 127 levels encode and decode; 128 raise both ways.
    #[test]
    fn nesting_limit_matches_what_decode_reads() {
        let lua = json_vm();
        let ok: bool = lua
            .load(
                r#"
                local t = json.array()
                for _ = 2, 127 do t = { t } end
                local s = json.encode(t)
                return #s == 254 and json.encode(json.decode(s)) ~= nil
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
        let e = lua
            .load("local t = {}; for _ = 2, 128 do t = { t } end; return json.encode(t)")
            .eval::<String>()
            .unwrap_err()
            .to_string();
        assert!(e.contains("nested more than 127"), "{e}");
        let decoded: bool =
            lua.load("return pcall(json.decode, string.rep('[', 127) .. string.rep(']', 127))").eval().unwrap();
        assert!(decoded, "decode reads 127 levels");
        let decoded: bool =
            lua.load("return pcall(json.decode, string.rep('[', 128) .. string.rep(']', 128))").eval().unwrap();
        assert!(!decoded, "decode stops at 128, which is why encode does");
    }

    struct Handle;
    impl mlua::UserData for Handle {}

    #[test]
    fn values_json_cannot_hold_raise_with_the_path() {
        let lua = json_vm();
        let e = enc_err(&lua, "{ states = { {}, { cels = { print } } } }");
        assert!(e.contains("a function has no JSON form at value.states[2].cels[1]"), "{e}");
        let e = enc_err(&lua, "{ co = coroutine.create(function() end) }");
        assert!(e.contains("a thread has no JSON form at value.co"), "{e}");
        let e = enc_err(&lua, r#"{ ["two words"] = print }"#);
        assert!(e.contains(r#"at value["two words"]"#), "{e}");

        let encode_fn = lua.create_function(encode).unwrap();
        let odd = [
            ("userdata", Value::UserData(lua.create_userdata(Handle).unwrap())),
            // Luau's `type()` calls a light userdata "userdata", and so does the message.
            ("userdata", Value::LightUserData(mlua::LightUserData(std::ptr::null_mut()))),
            ("vector", Value::Vector(mlua::Vector::new(1.0, 2.0, 3.0))),
            ("buffer", Value::Buffer(lua.create_buffer(b"ab").unwrap())),
        ];
        for (name, v) in odd {
            let e = encode_fn.call::<String>((v, Value::Nil)).unwrap_err().to_string();
            assert!(e.contains(&format!("a {name} has no JSON form at value")), "{name}: {e}");
        }
    }

    /// A type in a message is named as Luau's `type()` names it: mlua's "integer" is a
    /// number to the author who wrote `5`.
    #[test]
    fn errors_name_types_as_luau_does() {
        let lua = json_vm();
        let array_err = |expr: &str| {
            lua.load(format!("return json.array({expr})")).eval::<Table>().unwrap_err().to_string()
        };
        for e in [
            array_err("5"),
            array_err("1.5"),
            enc_err(&lua, "{}, 5"),
            enc_err(&lua, "{}, { [1] = true }"),
        ] {
            assert!(e.contains("not a number") && !e.contains("integer"), "{e}");
        }
        assert!(array_err("'x'").contains("not a string"));
    }

    #[test]
    fn pretty_output() {
        let lua = json_vm();
        let v = "{ a = { 1, 2 }, b = {}, c = json.array() }";
        assert_eq!(
            enc(&lua, &format!("{v}, {{ pretty = true }}")),
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {},\n  \"c\": []\n}"
        );
        assert_eq!(
            enc(&lua, &format!("{v}, {{ pretty = 4 }}")),
            "{\n    \"a\": [\n        1,\n        2\n    ],\n    \"b\": {},\n    \"c\": []\n}"
        );
        assert_eq!(enc(&lua, &format!("{v}, {{ pretty = false }}")), r#"{"a":[1,2],"b":{},"c":[]}"#);
        assert_eq!(enc(&lua, "{ 1 }, {}"), "[1]");
        for bad in ["{ pretty = 0 }", "{ pretty = 9 }", "{ pretty = 2.5 }", "{ pretty = 'yes' }", "{ prety = true }", "'pretty'"] {
            let e = enc_err(&lua, &format!("{{}}, {bad}"));
            assert!(e.contains("host.json.encode"), "{bad}: {e}");
        }
    }

    /// The marker is frozen and unprotected, so the Luau standard library still works on a
    /// marked table — the reason it is not mlua's own protected one.
    #[test]
    fn the_marker_leaves_the_standard_library_working() {
        let lua = json_vm();
        let ok: bool = lua
            .load(
                r#"
                local t = json.array()
                local copy = table.clone(t)
                assert(json.encode(copy) == "[]", "the marker survives table.clone")
                assert(json.array(t) == t, "marking twice is harmless and returns the table")
                local frozen = table.freeze(json.array({ 1 }))
                assert(json.encode(frozen) == "[1]", "table.freeze works on a marked table")
                local own = setmetatable(json.array(), { __index = {} })
                assert(json.encode(own) == "{}", "setmetatable works, and replaces the marker")
                local mt = getmetatable(t)
                assert(not pcall(function() mt.extra = 1 end), "the marker itself is frozen")
                assert(getmetatable(json.array()) == mt, "one marker per VM")
                return true
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
        for (expr, want) in [
            ("setmetatable({}, {})", "already has a metatable"),
            ("table.freeze({})", "frozen"),
            ("5", "expected a table or nothing"),
        ] {
            let e = lua
                .load(format!("return json.array({expr})"))
                .eval::<Value>()
                .unwrap_err()
                .to_string();
            assert!(e.contains("host.json.array") && e.contains(want), "{expr}: {e}");
        }
    }

    #[test]
    fn decode_stays_plain() {
        let lua = json_vm();
        let plain: bool = lua
            .load("local v = json.decode('[1, [2], {\"a\": []}]') return getmetatable(v) == nil and getmetatable(v[2]) == nil and getmetatable(v[3].a) == nil")
            .eval()
            .unwrap();
        assert!(plain);
    }

    /// Equal tables give equal strings, whatever order they were built in.
    #[test]
    fn equal_tables_encode_equally() {
        let lua = json_vm();
        let same: bool = lua
            .load(
                r#"
                local a = {}; a.z = 1; a.y = { 3, 4 }; a.x = "s"
                local b = { x = "s", y = { 3, 4 }, z = 1 }
                for i = 1, 50 do a["k" .. i] = i end
                for i = 50, 1, -1 do b["k" .. i] = i end
                return json.encode(a) == json.encode(b)
                "#,
            )
            .eval()
            .unwrap();
        assert!(same);
    }

    /// A tiny deterministic generator, so the round trips cover shapes nobody wrote by hand.
    struct Gen(u64);
    impl Gen {
        fn next(&mut self) -> u64 {
            // xorshift64*
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn text(&mut self) -> String {
            const PARTS: [&str; 8] = ["a", "B", "ü", "\"", "\\", "\n", "/", " x"];
            (0..self.below(4)).map(|_| PARTS[self.below(8) as usize]).collect()
        }
        /// A JSON value with no `null` and no empty array — the two things `decode` does not
        /// bring back as themselves — and whole numbers only when `whole`.
        fn value(&mut self, depth: u32, whole: bool) -> serde_json::Value {
            use serde_json::Value as J;
            let pick = if depth >= 4 { self.below(4) } else { self.below(6) };
            match pick {
                0 => J::Bool(self.below(2) == 0),
                1 => J::from(self.below(2_000_000) as i64 - 1_000_000),
                2 if whole => J::from((self.next() >> 11) as i64),
                // An odd number of 64ths: never whole, and exact in binary.
                2 => J::from((2 * self.below(1_000_000) + 1) as f64 / 64.0 - 7777.0),
                3 => J::String(self.text()),
                4 => J::Array((0..1 + self.below(4)).map(|_| self.value(depth + 1, whole)).collect()),
                _ => J::Object((0..self.below(4)).map(|_| (self.text(), self.value(depth + 1, whole))).collect()),
            }
        }
    }

    #[test]
    fn round_trips() {
        let lua = json_vm();
        let check: mlua::Function = lua
            .load(
                r#"
                return function(text)
                  local v = json.decode(text)
                  local once = json.encode(v)
                  local twice = json.encode(json.decode(once))
                  return once, twice
                end
                "#,
            )
            .eval()
            .unwrap();
        let mut g = Gen(0x9E37_79B9_7F4A_7C15);
        for i in 0..400 {
            let whole = i % 2 == 0;
            let doc = g.value(0, whole);
            let text = serde_json::to_string_pretty(&doc).unwrap();
            let (once, twice): (String, String) = check.call(text.as_str()).unwrap();
            // decode(encode(decode(s))) is decode(s): encoding what was decoded is stable.
            assert_eq!(once, twice, "{text}");
            // And it is serde_json's own canonical form of the input: compact, keys sorted,
            // whole numbers without a fraction, other numbers in the shortest exact form.
            assert_eq!(once, serde_json::to_string(&doc).unwrap(), "{text}");
        }
    }

    /// `decode(encode(x))` deep-equals `x` for Luau values nobody decoded first, holes included.
    #[test]
    fn luau_values_survive_the_round_trip() {
        let lua = json_vm();
        let ok: bool = lua
            .load(
                r#"
                local function same(a, b)
                  if type(a) ~= type(b) then return false end
                  if type(a) ~= "table" then return a == b end
                  for k, v in pairs(a) do if not same(v, b[k]) then return false end end
                  for k in pairs(b) do if a[k] == nil then return false end end
                  return true
                end
                local cases = {
                  { 1, nil, 3 },
                  { name = "Arcade", menu = { "Start", "Options" }, cursor = { x = 12.5, y = -3 } },
                  { [1] = true, [10] = false },
                  { deep = { { { "x" } } }, n = 2^53, f = 0.1, neg = -0.25 },
                  "just a string", 42, false,
                }
                for i, v in ipairs(cases) do
                  if not same(v, json.decode(json.encode(v))) then error("case " .. i) end
                end
                return true
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
    }
}
