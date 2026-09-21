//! `host.json.decode`: JSON text to a Luau value.
//!
//! Modules already had a way to READ a data file (`host.resource.read`) and no way to make
//! sense of one: Luau has no JSON parser, the host exposed none, and the reference's own
//! example called a `parse()` that did not exist. Both halves were already dependencies —
//! `serde_json`, and mlua's `serialize` feature for the legacy data exports — so this is the
//! two joined, with the one choice that matters made on purpose: what `null` becomes.

use mlua::{Lua, LuaSerdeExt, SerializeOptions, Value};

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
}
