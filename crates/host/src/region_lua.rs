//! The one reader of a Region argument: every call that takes a region reads it here.
//!
//! ```luau
//! { x1, y1, x2, y2 }  or  { x1 = …, y1 = …, x2 = …, y2 = … }                -- corners
//! { window = w, fraction = { x1, y1, x2, y2 } }                             -- window fractions
//! ```
//!
//! **Two readings of corners, one of the window form.** `read` is the strict one, for the calls
//! built with it — `host.screen.cells`, `matchCells`, `matchCellsAsync` and `host.ocr.read`: a
//! mistake in the call raises, at the call: a value that is not a table, a key the form does not
//! take, named and positional corners mixed, a corner missing, a corner that is not a whole
//! number or lies outside the screen's coordinate range, corners that are empty or turned
//! around, a window table without a `client`, a fraction that is not a finite number. There is
//! no default there: `{}` raises like any other region with a corner missing.
//!
//! `read_loose` is for the older calls — `profile`, `template{ capture }`, the image searches and
//! an `imageSearchEach` entry's `within`, `save`, `saveMarked`, `host.ocr.recognize` and each
//! `recognizeMany` region. Their corners are read exactly as those calls always read them (a
//! corner missing or not a number takes its default, a fraction of a pixel is cut toward zero,
//! other keys are ignored), because every module written against them relies on it. But the
//! window form goes through `read`, strictly, as everywhere; and a table that is NEITHER form —
//! a window's `bounds` `{ x, y, w, h }`, an empty `{}` — raises, where it used to become the
//! whole screen without a word. Only a missing region (`nil`) is the whole primary screen.
//!
//! What a window does at run time is not a mistake: a window region whose client area is empty
//! resolves to `Unresolved`, and each call answers that its own way (`nil, reason`, `false,
//! reason`, a `"failed"` reading, an `error` field) — never an error dialog over the application
//! being read.
//!
//! `read_point` is `host.screen.pixel`'s window form, `{ window = w, fraction = { x, y } }`.
//!
//! Host only: it reads Luau values. The rules that need no Luau — resolving a window region or
//! point, checking corners, the loose corners' arithmetic — are in `region.rs`, which the macOS
//! check borrows.

use mlua::{Table, Value};

use crate::describe_value;
use crate::region::{self, Client, Fraction, Region};

const CORNERS: [&str; 4] = ["x1", "y1", "x2", "y2"];

/// The names a table of numbers takes, and how its messages speak of them.
struct Shape<const N: usize> {
    names: [&'static str; N],
    /// "x1, y1, x2 and y2"
    listed: &'static str,
    /// "the four numbers"
    numbers: &'static str,
    /// "corners"
    kind: &'static str,
    /// "all four corners are needed"
    all: &'static str,
}

const CORNER_SHAPE: Shape<4> = Shape {
    names: CORNERS,
    listed: "x1, y1, x2 and y2",
    numbers: "the four numbers",
    kind: "corners",
    all: "all four corners are needed",
};

const POINT_SHAPE: Shape<2> = Shape {
    names: ["x", "y"],
    listed: "x and y",
    numbers: "the two numbers",
    kind: "coordinates",
    all: "both are needed",
};

/// A whole number as Luau holds one (mlua hands integral numbers over as `Integer`).
fn whole_value(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
        _ => None,
    }
}

/// The entries of a table of `shape`'s numbers, named or positional: those and nothing else,
/// all present, written one way.
fn entries<const N: usize>(t: &Table, what: &str, shape: &Shape<N>) -> Result<[Value; N], String> {
    let mut named: [Option<Value>; N] = std::array::from_fn(|_| None);
    let mut positional: [Option<Value>; N] = std::array::from_fn(|_| None);
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        match &k {
            Value::String(s) => {
                let name = s.to_string_lossy();
                match shape.names.iter().position(|c| *c == name) {
                    Some(i) => named[i] = Some(v),
                    None => {
                        return Err(format!(
                            "{what} has a key '{name}'; it takes {}, or {} in that order",
                            shape.listed, shape.numbers
                        ))
                    }
                }
            }
            Value::Integer(n) if (1..=N as i64).contains(n) => positional[*n as usize - 1] = Some(v),
            other => {
                return Err(format!(
                    "{what} has an entry at {}; it takes {}, or {} in that order",
                    describe_value(other),
                    shape.listed,
                    shape.numbers
                ))
            }
        }
    }
    let any_named = named.iter().any(Option::is_some);
    if any_named && positional.iter().any(Option::is_some) {
        return Err(format!("{what} mixes named and positional {}; give {} one way", shape.kind, shape.listed));
    }
    let given = if any_named { named } else { positional };
    let mut out: [Value; N] = std::array::from_fn(|_| Value::Nil);
    for ((v, name), slot) in given.into_iter().zip(shape.names).zip(out.iter_mut()) {
        *slot = v.ok_or_else(|| format!("{what}.{name} is missing; {}, {}", shape.all, shape.listed))?;
    }
    Ok(out)
}

/// The numbers of a fraction table of `shape`, for the region's and the point's window form.
/// Finiteness is the caller's to check (`Fraction::new` does it for a region).
fn fractions<const N: usize>(v: &Value, what: &str, shape: &Shape<N>) -> Result<[f64; N], String> {
    let Value::Table(t) = v else {
        return Err(format!(
            "{what} must be a table {{ {} }} of fractions of the client area, got {}",
            shape.names.join(", "),
            describe_value(v)
        ));
    };
    let q = entries(t, what, shape)?;
    let mut f = [0f64; N];
    for ((v, name), slot) in q.iter().zip(shape.names).zip(f.iter_mut()) {
        *slot = match v {
            Value::Integer(i) => *i as f64,
            Value::Number(n) => *n,
            other => return Err(format!("{what}.{name} must be a number, got {}", describe_value(other))),
        };
    }
    Ok(f)
}

/// Whether a table is written in the window form: it has `window` or `fraction`.
fn window_form(t: &Table) -> bool {
    ["window", "fraction"].iter().any(|k| t.contains_key(*k).unwrap_or(false))
}

/// `{ window = w, fraction = … }`: nothing beside those two keys, and the window's `client` as
/// four whole numbers. The fraction is the caller's to read.
fn window_client(t: &Table, what: &str) -> Result<Client, String> {
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
    Ok(Client { x: c[0], y: c[1], w: c[2], h: c[3] })
}

fn not_a_table(what: &str, v: &Value) -> String {
    format!(
        "{what} must be a table, {{ x1, y1, x2, y2 }} or {{ window = w, fraction = {{ x1, y1, x2, y2 }} }}, got {}",
        describe_value(v)
    )
}

/// Reads a region strictly: `{ x1, y1, x2, y2 }` (named or positional, whole numbers, not
/// empty) or `{ window = w, fraction = { x1, y1, x2, y2 } }` (any finite numbers — the host
/// clamps them the way `region.rs` explains). `what` names the value in every message
/// (`opts.region`, `the region`, `entry 2.region`); the caller puts its function's name first.
pub(crate) fn read(v: &Value, what: &str) -> Result<Region, String> {
    let Value::Table(t) = v else {
        return Err(not_a_table(what, v));
    };
    if !window_form(t) {
        let c = entries(t, what, &CORNER_SHAPE)?;
        let mut n = [0i64; 4];
        for ((v, name), slot) in c.iter().zip(CORNERS).zip(n.iter_mut()) {
            *slot = whole_value(v)
                .ok_or_else(|| format!("{what}.{name} must be a whole number, got {}", describe_value(v)))?;
        }
        let [x1, y1, x2, y2] = n;
        return region::corners(what, x1, y1, x2, y2).map(Region::Rect);
    }
    let client = window_client(t, what)?;
    let fraction: Value = t.get("fraction").map_err(|e| e.to_string())?;
    let [x1, y1, x2, y2] = fractions(&fraction, &format!("{what}.fraction"), &CORNER_SHAPE)?;
    let fraction = Fraction::new(x1, y1, x2, y2).map_err(|e| format!("{what}.{e}"))?;
    Ok(Region::Window(client, fraction))
}

/// Reads a region for a call that has always read its corners loosely, given the primary
/// screen's size for the defaults (see the file's comment):
/// - `nil`: the whole primary screen;
/// - the window form: exactly as `read` reads it;
/// - a table with at least one of `x1, y1, x2, y2` or the entries 1 to 4: corners, each as the
///   older calls always read it — the named one if it converts to a number, else the
///   positional one, else `0` for `x1` and `y1` and the screen's width and height for `x2` and
///   `y2` — and `x2 - x1` wide, a negative size read as none (`region::loose_corners`);
/// - anything else raises: a table with none of those keys, and a value that is not a table.
pub(crate) fn read_loose(v: &Value, what: &str, (sw, sh): (i32, i32)) -> Result<Region, String> {
    let t = match v {
        Value::Nil => return Ok(Region::Rect(region::loose_corners(0, 0, sw, sh))),
        Value::Table(t) => t,
        other => return Err(not_a_table(what, other)),
    };
    if window_form(t) {
        return read(v, what);
    }
    let present = |got: mlua::Result<Value>| got.map(|v| !v.is_nil()).unwrap_or(false);
    let named = CORNERS.iter().any(|n| present(t.get::<Value>(*n)));
    let positional = (1..=4i64).any(|i| present(t.get::<Value>(i)));
    if !(named || positional) {
        let mut keys: Vec<String> = t
            .clone()
            .pairs::<Value, Value>()
            .flatten()
            .map(|(k, _)| match &k {
                Value::String(s) => s.to_string_lossy(),
                other => format!("[{}]", describe_value(other)),
            })
            .collect();
        keys.sort();
        let bounds = ["x", "y", "w", "h"].iter().all(|k| keys.iter().any(|g| g == k));
        return Err(format!(
            "{what} is neither {{ x1, y1, x2, y2 }} nor {{ window = w, fraction = {{ x1, y1, x2, y2 }} }}: {}{}",
            if keys.is_empty() {
                "it is empty".to_string()
            } else {
                format!("it has none of x1, y1, x2, y2 (its keys: {})", keys.join(", "))
            },
            if bounds {
                "; a rectangle { x, y, w, h }, such as a window's bounds, is written { b.x, b.y, b.x + b.w, b.y + b.h }"
            } else {
                ""
            }
        ));
    }
    // Exactly as the older calls have always read a corner: `get` converts as Luau would (a
    // numeral string counts, a fraction is cut toward zero), and a corner that does not convert
    // takes its default without a word.
    let corner = |name: &str, at: i64, default: i32| -> i32 {
        t.get::<i32>(name).or_else(|_| t.get::<i32>(at)).unwrap_or(default)
    };
    Ok(Region::Rect(region::loose_corners(
        corner("x1", 1, 0),
        corner("y1", 2, 0),
        corner("x2", 3, sw),
        corner("y2", 4, sh),
    )))
}

/// Reads `host.screen.pixel`'s window form strictly: `{ window = w, fraction = { x, y } }` (or
/// `fraction = { x = …, y = … }`), finite numbers, nothing else. `region::resolve_point` turns it
/// into a pixel.
pub(crate) fn read_point(v: &Value, what: &str) -> Result<(Client, f64, f64), String> {
    let Value::Table(t) = v else {
        return Err(format!("{what} must be a table {{ window = w, fraction = {{ x, y }} }}, got {}", describe_value(v)));
    };
    if !window_form(t) {
        return Err(format!(
            "{what} is not {{ window = w, fraction = {{ x, y }} }}; a point in screen coordinates is given as two numbers, pixel(x, y)"
        ));
    }
    let client = window_client(t, what)?;
    let fraction: Value = t.get("fraction").map_err(|e| e.to_string())?;
    let [x, y] = fractions(&fraction, &format!("{what}.fraction"), &POINT_SHAPE)?;
    for (name, f) in [("x", x), ("y", y)] {
        if !f.is_finite() {
            return Err(format!("{what}.fraction.{name} is {f}, not a finite number"));
        }
    }
    Ok((client, x, y))
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

    const SCREEN: (i32, i32) = (1920, 1080);

    fn loose_src(lua: &Lua, src: &str) -> Result<Region, String> {
        let v: Value = lua.load(src).eval().unwrap();
        read_loose(&v, "opts.region", SCREEN)
    }

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Result<Region, String> {
        Ok(Region::Rect(ScreenRect { x, y, w, h }))
    }

    /// Every corner shape the modules and tools in this repository pass today reads exactly as
    /// it did: named, positional, a fraction cut toward zero, a numeral string, a corner left
    /// out or misspelt taking its default, other keys ignored, an empty or turned-around
    /// rectangle read as empty rather than raised.
    #[test]
    fn the_older_calls_keep_their_corners() {
        let lua = Lua::new();
        assert_eq!(loose_src(&lua, "return nil"), rect(0, 0, 1920, 1080), "no region is the whole primary screen");
        assert_eq!(loose_src(&lua, "return { 10, 20, 110, 70 }"), rect(10, 20, 100, 50));
        assert_eq!(loose_src(&lua, "return { x1 = 10, y1 = 20, x2 = 110, y2 = 70 }"), rect(10, 20, 100, 50));
        assert_eq!(loose_src(&lua, "return { 10.7, 20.2, 110.9, 70 }"), rect(10, 20, 100, 50), "cut toward zero");
        assert_eq!(loose_src(&lua, "return { -10.7, 0, 5, 5 }"), rect(-10, 0, 15, 5), "toward zero, not down");
        assert_eq!(loose_src(&lua, "return { '10', 20, 110, 70 }"), rect(10, 20, 100, 50), "a numeral string converts");
        assert_eq!(loose_src(&lua, "return { x1 = 100 }"), rect(100, 0, 1820, 1080), "missing corners default");
        assert_eq!(loose_src(&lua, "return { x1 = 0, y1 = 0, x2 = 100, yy2 = 5 }"), rect(0, 0, 100, 1080), "a misspelt corner defaults");
        assert_eq!(loose_src(&lua, "return { x1 = 5, 1, 2, 30, 40 }"), rect(5, 2, 25, 38), "named first, then positional");
        assert_eq!(loose_src(&lua, "return { 1, 2, 3, 4, colour = 'red' }"), rect(1, 2, 2, 2), "other keys are ignored");
        assert_eq!(loose_src(&lua, "return { 50, 50, 10, 10 }"), rect(50, 50, 0, 0), "turned around is empty, not an error");
        assert_eq!(loose_src(&lua, "return { 7, 7, 7, 7 }"), rect(7, 7, 0, 0));
        assert_eq!(loose_src(&lua, "return { -2000000000, 0, 2000000000, 10 }"), rect(-2_000_000_000, 0, 0, 10), "no overflow");
    }

    /// The window form, through `read` itself, in a loosely read call.
    #[test]
    fn the_older_calls_take_the_window_form_strictly() {
        let lua = Lua::new();
        let w = loose_src(&lua, "return { window = { client = { x = 100, y = 50, w = 1280, h = 1024 } }, fraction = { 0.02, 0.33, 0.09, 0.86 } }")
            .unwrap();
        assert_eq!(w.resolve(), Ok(ScreenRect { x: 125, y: 387, w: 91, h: 544 }));
        let named = loose_src(&lua, "return { window = { client = { x = 0, y = 0, w = 100, h = 100 } }, fraction = { x1 = 0.5, y1 = 0, x2 = 1, y2 = 1 } }")
            .unwrap();
        assert_eq!(named.resolve(), Ok(ScreenRect { x: 50, y: 0, w: 50, h: 100 }));
        let minimised = loose_src(&lua, "return { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } }")
            .expect("a minimised window is answered, not raised");
        assert_eq!(minimised.resolve().unwrap_err().to_string(), "the window's client area is empty (0x0)");
        // Strict in the window form, exactly as the strict calls are.
        for (src, want) in [
            ("return { window = { client = { x = 0, y = 0, w = 10, h = 10 } }, fraction = { 0, 0, 1, 1 }, x1 = 3 }", "opts.region has a key 'x1' beside window and fraction"),
            ("return { window = {}, fraction = { 0, 0, 1, 1 } }", "opts.region.window has no client table"),
            ("return { window = { client = { x = 0, y = 0, w = 10, h = 10 } }, fraction = { 0, 0, 1 } }", "opts.region.fraction.y2 is missing"),
            ("return { window = { client = { x = 0, y = 0, w = 10, h = 10 } }, fraction = { 0, 0, 1/0, 1 } }", "opts.region.fraction.x2 is inf"),
            ("return { fraction = { 0, 0, 1, 1 } }", "opts.region.window must be a window table"),
        ] {
            let e = loose_src(&lua, src).unwrap_err();
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
    }

    /// What used to become the whole screen without a word now raises, naming the argument.
    #[test]
    fn a_table_that_is_neither_form_raises() {
        let lua = Lua::new();
        for (src, want) in [
            ("return {}", "opts.region is neither { x1, y1, x2, y2 } nor { window = w, fraction = { x1, y1, x2, y2 } }: it is empty"),
            ("return { x = 1, y = 2, w = 3, h = 4 }", "it has none of x1, y1, x2, y2 (its keys: h, w, x, y); a rectangle { x, y, w, h }, such as a window's bounds, is written { b.x, b.y, b.x + b.w, b.y + b.h }"),
            ("return { left = 1, top = 2 }", "(its keys: left, top)"),
            ("return { [5] = 1 }", "(its keys: [5])"),
            ("return 'whole'", "opts.region must be a table"),
            ("return 5", "opts.region must be a table"),
            ("return false", "opts.region must be a table"),
        ] {
            let e = loose_src(&lua, src).unwrap_err();
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
        // A table with only a corner of the wrong type is still corners: that corner defaults,
        // as it always has.
        assert_eq!(loose_src(&lua, "return { x1 = 'left' }"), rect(0, 0, 1920, 1080));
    }

    fn point_src(lua: &Lua, src: &str) -> Result<(Client, f64, f64), String> {
        let v: Value = lua.load(src).eval().unwrap();
        read_point(&v, "point")
    }

    /// `pixel`'s window form: two fractions, positional or named, strictly.
    #[test]
    fn a_point_is_a_window_and_two_fractions() {
        let lua = Lua::new();
        let c = Client { x: 10, y: 20, w: 200, h: 100 };
        let win = "window = { client = { x = 10, y = 20, w = 200, h = 100 } }";
        assert_eq!(point_src(&lua, &format!("return {{ {win}, fraction = {{ 0.5, 0.25 }} }}")), Ok((c, 0.5, 0.25)));
        assert_eq!(point_src(&lua, &format!("return {{ {win}, fraction = {{ x = 1, y = 0 }} }}")), Ok((c, 1.0, 0.0)));
        for (src, want) in [
            (format!("return {{ {win}, fraction = {{ 0.5 }} }}"), "point.fraction.y is missing; both are needed, x and y"),
            (format!("return {{ {win}, fraction = {{ 0.5, 0.5, 0.5 }} }}"), "point.fraction has an entry at 3; it takes x and y"),
            (format!("return {{ {win}, fraction = {{ x = 0.5, 0.5 }} }}"), "mixes named and positional coordinates"),
            (format!("return {{ {win}, fraction = {{ 0/0, 0.5 }} }}"), "point.fraction.x is NaN, not a finite number"),
            (format!("return {{ {win}, fraction = {{ 'a', 0.5 }} }}"), "point.fraction.x must be a number"),
            (format!("return {{ {win}, fraction = {{ 0, 0 }}, name = 1 }}"), "a key 'name' beside window and fraction"),
            ("return { 100, 200 }".to_string(), "a point in screen coordinates is given as two numbers, pixel(x, y)"),
            ("return { window = {}, fraction = { 0, 0 } }".to_string(), "point.window has no client table"),
        ] {
            let e = point_src(&lua, &src).unwrap_err();
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
    }
}
