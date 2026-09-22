//! The one strict reader of a Region argument, for every call that reads regions strictly:
//! `host.screen.cells`, `matchCells` and `matchCellsAsync`, and `host.ocr.read`.
//!
//! ```luau
//! { x1, y1, x2, y2 }  or  { x1 = …, y1 = …, x2 = …, y2 = … }                -- corners
//! { window = w, fraction = { x1, y1, x2, y2 } }                             -- window fractions
//! ```
//!
//! **One rule for both kinds of call.** A mistake in the call raises, at the call: a value that
//! is not a table, a key the form does not take, named and positional corners mixed, a corner
//! missing, a corner that is not a whole number or lies outside the screen's coordinate range,
//! corners that are empty or turned around, a window table without a `client`, a fraction that
//! is not a finite number. What a window does at run time is not a mistake: a window region
//! whose client area is empty resolves to `Unresolved`, and each call answers that its own way
//! (`nil, reason` for the cells calls, a `"failed"` reading for `read`) — never an error dialog
//! over the application being read.
//!
//! There is no default: `{}` raises like any other region with a corner missing. A region that
//! quietly became the whole screen is exactly the mistake a strict reader is there to catch.
//!
//! The older calls (`imageSearch`, `profile`, `recognize`, …) still read corners loosely
//! (`read_region` in `lib.rs`); see TODO.md for adopting this reader there.
//!
//! Host only: it reads Luau values. The rules that need no Luau — resolving a window region,
//! checking corners — are in `region.rs`, which the macOS check borrows.

use mlua::{Table, Value};

use crate::describe_value;
use crate::region::{self, Client, Fraction, Region};

const CORNERS: [&str; 4] = ["x1", "y1", "x2", "y2"];

/// A whole number as Luau holds one (mlua hands integral numbers over as `Integer`).
fn whole_value(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
        _ => None,
    }
}

/// The four entries of an `{ x1, y1, x2, y2 }` table, named or positional: those four and
/// nothing else, all present, written one way.
fn four(t: &Table, what: &str) -> Result<[Value; 4], String> {
    let mut named: [Option<Value>; 4] = [None, None, None, None];
    let mut positional: [Option<Value>; 4] = [None, None, None, None];
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        match &k {
            Value::String(s) => {
                let name = s.to_string_lossy();
                match CORNERS.iter().position(|c| *c == name) {
                    Some(i) => named[i] = Some(v),
                    None => {
                        return Err(format!(
                            "{what} has a key '{name}'; it takes x1, y1, x2 and y2, or the four numbers in that order"
                        ))
                    }
                }
            }
            Value::Integer(n) if (1..=4).contains(n) => positional[*n as usize - 1] = Some(v),
            other => {
                return Err(format!(
                    "{what} has an entry at {}; it takes x1, y1, x2 and y2, or the four numbers in that order",
                    describe_value(other)
                ))
            }
        }
    }
    let any_named = named.iter().any(Option::is_some);
    if any_named && positional.iter().any(Option::is_some) {
        return Err(format!("{what} mixes named and positional corners; give x1, y1, x2 and y2 one way"));
    }
    let given = if any_named { named } else { positional };
    let mut out = Vec::with_capacity(4);
    for (v, name) in given.into_iter().zip(CORNERS) {
        out.push(v.ok_or_else(|| format!("{what}.{name} is missing; all four corners are needed, x1, y1, x2 and y2"))?);
    }
    Ok(out.try_into().expect("four corners"))
}

/// Reads a region strictly: `{ x1, y1, x2, y2 }` (named or positional, whole numbers, not
/// empty) or `{ window = w, fraction = { x1, y1, x2, y2 } }` (any finite numbers — the host
/// clamps them the way `region.rs` explains). `what` names the value in every message
/// (`opts.region`, `the region`, `entry 2.region`); the caller puts its function's name first.
pub(crate) fn read(v: &Value, what: &str) -> Result<Region, String> {
    let Value::Table(t) = v else {
        return Err(format!(
            "{what} must be a table, {{ x1, y1, x2, y2 }} or {{ window = w, fraction = {{ x1, y1, x2, y2 }} }}, got {}",
            describe_value(v)
        ));
    };
    let window_form = ["window", "fraction"].iter().any(|k| t.contains_key(*k).unwrap_or(false));
    if !window_form {
        let c = four(t, what)?;
        let mut n = [0i64; 4];
        for ((v, name), slot) in c.iter().zip(CORNERS).zip(n.iter_mut()) {
            *slot = whole_value(v)
                .ok_or_else(|| format!("{what}.{name} must be a whole number, got {}", describe_value(v)))?;
        }
        let [x1, y1, x2, y2] = n;
        return region::corners(what, x1, y1, x2, y2).map(Region::Rect);
    }
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair.map_err(|e| e.to_string())?;
        let ok = matches!(&k, Value::String(s) if matches!(s.as_bytes().as_ref(), b"window" | b"fraction"));
        if !ok {
            return Err(format!(
                "{what} has {} beside window and fraction; a region is either {{ x1, y1, x2, y2 }} or {{ window, fraction }}",
                match &k {
                    Value::String(s) => format!("a key '{}'", s.to_string_lossy()),
                    other => format!("an entry at {}", describe_value(other)),
                }
            ));
        }
    }
    let window: Value = t.get("window").map_err(|e| e.to_string())?;
    let Value::Table(window) = window else {
        return Err(format!(
            "{what}.window must be a window table, as host.window.active() returns, got {}",
            describe_value(&window)
        ));
    };
    let client: Value = window.get("client").map_err(|e| e.to_string())?;
    let Value::Table(client) = client else {
        return Err(format!("{what}.window has no client table; give it the table host.window returns"));
    };
    let mut c = [0i64; 4];
    for (name, slot) in ["x", "y", "w", "h"].iter().zip(c.iter_mut()) {
        let v: Value = client.get(*name).map_err(|e| e.to_string())?;
        *slot = whole_value(&v)
            .ok_or_else(|| format!("{what}.window.client.{name} must be a whole number, got {}", describe_value(&v)))?;
    }
    let fraction: Value = t.get("fraction").map_err(|e| e.to_string())?;
    let Value::Table(fraction) = fraction else {
        return Err(format!(
            "{what}.fraction must be a table {{ x1, y1, x2, y2 }} of fractions of the client area, got {}",
            describe_value(&fraction)
        ));
    };
    let q = four(&fraction, &format!("{what}.fraction"))?;
    let mut f = [0f64; 4];
    for ((v, name), slot) in q.iter().zip(CORNERS).zip(f.iter_mut()) {
        *slot = match v {
            Value::Integer(i) => *i as f64,
            Value::Number(n) => *n,
            other => return Err(format!("{what}.fraction.{name} must be a number, got {}", describe_value(other))),
        };
    }
    let fraction = Fraction::new(f[0], f[1], f[2], f[3]).map_err(|e| format!("{what}.{e}"))?;
    Ok(Region::Window(Client { x: c[0], y: c[1], w: c[2], h: c[3] }, fraction))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::ScreenRect;
    use mlua::Lua;

    fn read_src(lua: &Lua, src: &str) -> Result<Region, String> {
        let v: Value = lua.load(src).eval().unwrap();
        read(&v, "region")
    }

    /// Both forms, and the one rule for what raises — the same for every strict call, because
    /// there is only this reader.
    #[test]
    fn one_reader_one_rule() {
        let lua = Lua::new();
        assert_eq!(read_src(&lua, "return { 10, 20, 110, 70 }"), Ok(Region::Rect(ScreenRect { x: 10, y: 20, w: 100, h: 50 })));
        assert_eq!(
            read_src(&lua, "return { x1 = 10.0, y1 = 20, x2 = 110, y2 = 70 }"),
            Ok(Region::Rect(ScreenRect { x: 10, y: 20, w: 100, h: 50 })),
            "a float without a fraction is a whole number"
        );
        let w = read_src(&lua, "return { window = { client = { x = 100, y = 50, w = 1280, h = 1024 } }, fraction = { 0.02, 0.33, 0.09, 0.86 } }")
            .unwrap();
        assert_eq!(w.resolve(), Ok(ScreenRect { x: 125, y: 387, w: 91, h: 544 }));
        let empty = read_src(&lua, "return { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } }")
            .expect("an empty client area is not a mistake in the call");
        assert_eq!(empty.resolve().unwrap_err().to_string(), "the window's client area is empty (0x0)");
        for (src, want) in [
            ("return 'x'", "region must be a table"),
            ("return {}", "region.x1 is missing; all four corners are needed"),
            ("return { 1, 2, 3 }", "region.y2 is missing"),
            ("return { x1 = 10.9, y1 = 20, x2 = 110, y2 = 40 }", "region.x1 must be a whole number, got 10.9"),
            ("return { 1, 2, '3', 4 }", "region.x2 must be a whole number, got the string \"3\""),
            ("return { 1, 2, 0/0, 4 }", "region.x2 must be a whole number, got NaN"),
            ("return { 1, 2, 3, 4, 5 }", "region has an entry at 5"),
            ("return { x1 = 1, 2, 3, 4, 5 }", "mixes named and positional"),
            ("return { x1 = 0, y1 = 0, x2 = 1, y2 = 1, colour = 3 }", "region has a key 'colour'"),
            ("return { 50, 50, 10, 10 }", "is empty or turned around"),
            ("return { window = { client = { x = 0, y = 0, w = 10, h = 10 } }, fraction = { 0, 0, 1, 1 }, name = 'x' }", "a key 'name' beside window and fraction"),
            ("return { window = { client = { x = 0, y = 0, w = 10, h = 10 } } }", "region.fraction must be a table"),
            ("return { window = {}, fraction = { 0, 0, 1, 1 } }", "region.window has no client table"),
        ] {
            let e = read_src(&lua, src).unwrap_err();
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
    }
}
