//! The window-relative Region form: a rectangle given as fractions of a window's client area,
//! which the host turns into pixels (Windows) or points (macOS) at the moment of the read.
//!
//! ```luau
//! { window = w, fraction = { x1 = 0.02, y1 = 0.33, x2 = 0.09, y2 = 0.86 } }   -- or { 0.02, 0.33, 0.09, 0.86 }
//! ```
//!
//! Why it exists: a region a module writes as pixels is right for one window size and one
//! display scaling, and wrong for every other. A game module that records its menu region as
//! fractions of the client area keeps working when the window is resized, when the display
//! scaling changes, and on a Retina Mac, where the same window is measured in points — and the
//! module never has to know which of those it is running under. That is the platform's rule:
//! the host resolves platform differences, the module does not.
//!
//! **The formula is an outside reader's, reproduced exactly**, because signatures recorded
//! there must be read from the same pixels here. It stores a region as four C# `double`s and
//! resolves it per captured frame of width `W` and height `H`:
//!
//! ```text
//! x0 = clamp(floor(W * x1), 0, W - 1)        x1' = clamp(ceil(W * x2), x0 + 1, W)
//! y0 = clamp(floor(H * y1), 0, H - 1)        y1' = clamp(ceil(H * y2), y0 + 1, H)
//! clamp(v, lo, hi) = min(hi, max(lo, v))
//! ```
//!
//! in double-precision arithmetic, literally: `W * x1` is one IEEE multiplication of an integer
//! converted to f64 by an f64. Exact rational arithmetic would be WRONG here — `800 * 0.035` is
//! 28.000000000000004 in f64, so ceil gives 29 where the exact product gives 28, and his reader
//! says 29. The clamps mean the result is never empty and never leaves the client area: a
//! fraction outside 0..1, or an end before its start, still gives at least one column and one
//! row. So this raises for nothing a module computes. The Lua reader (`region_lua.rs`) raises
//! for a window region's shape, never for its numbers' range or order: a fraction that is not a
//! finite number, a wrong type, an unknown key, a fraction corner missing or corners mixed, a
//! window table without a `client`.
//!
//! The other form, corners in screen coordinates, is checked here too (`corners`), so that the
//! one rule for both forms — what raises, what is answered — lives in one file with its tests.
//!
//! Pure: no Lua, no OS. Borrowed by `crates/macos-check` so the Mac build type-checks it too.

/// A window's client area in screen coordinates, as `host.window` reports it in `client`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Client {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// A region as fractions of the client area. `x2` and `y2` are the right and bottom edges
/// (an outside reader that stores `XStart, XEnd, YStart, YEnd` has x1 = XStart, x2 = XEnd,
/// y1 = YStart, y2 = YEnd).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fraction {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// A resolved rectangle relative to the client area's origin, in the reader's own terms:
/// columns `x0..x1` and rows `y0..y1`, the ends exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub x0: i64,
    pub x1: i64,
    pub y0: i64,
    pub y1: i64,
}

/// A rectangle in screen coordinates, `(x, y, w, h)` as every capture takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Why a window region has no rectangle this time. Runtime conditions, not mistakes: a module
/// gets `nil, reason` for them, never an error dialog over the game it is reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// The client area has no width or no height: a minimised window, or one not yet laid out.
    EmptyClient { w: i64, h: i64 },
    /// The rectangle does not fit the screen's coordinate range. Cannot happen for a window
    /// the platform reported; kept so no arithmetic here can wrap.
    OutOfRange,
}

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unresolved::EmptyClient { w, h } => write!(f, "the window's client area is empty ({w}x{h})"),
            Unresolved::OutOfRange => write!(f, "the region lies outside the screen's coordinate range"),
        }
    }
}

impl Fraction {
    /// The four numbers, refused only when one is not finite: NaN and the infinities have no
    /// floor, and the reader's C# would turn them into an arbitrary integer.
    pub(crate) fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, String> {
        for (name, v) in [("x1", x1), ("y1", y1), ("x2", x2), ("y2", y2)] {
            if !v.is_finite() {
                return Err(format!("fraction.{name} is {v}, not a finite number"));
            }
        }
        Ok(Fraction { x1, y1, x2, y2 })
    }
}

/// One axis: `(start, end)` for a frame of `len` pixels, `len >= 1`.
///
/// `as i64` saturates, so a product beyond the integer range (a fraction of 1e300) lands on
/// the clamp's bound instead of wrapping. The reader's `(int)` cast is undefined there; for
/// every product inside the i32 range the two agree exactly.
fn axis(len: i64, start: f64, end: f64) -> (i64, i64) {
    let l = len as f64;
    let clamp = |v: i64, lo: i64, hi: i64| hi.min(lo.max(v));
    let a = clamp((l * start).floor() as i64, 0, len - 1);
    let b = clamp((l * end).ceil() as i64, a + 1, len);
    (a, b)
}

/// The reader's pixel bounds of `f` in a frame of `w` x `h`, or `None` for an empty frame.
pub(crate) fn frame_bounds(w: i64, h: i64, f: &Fraction) -> Option<Bounds> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let (x0, x1) = axis(w, f.x1, f.x2);
    let (y0, y1) = axis(h, f.y1, f.y2);
    Some(Bounds { x0, x1, y0, y1 })
}

/// `f` of `client`, in screen coordinates. No OS call: the client rectangle is the one the
/// window table carries, so the region is exactly as current as that table.
pub(crate) fn resolve(client: Client, f: &Fraction) -> Result<ScreenRect, Unresolved> {
    let b = frame_bounds(client.w, client.h, f).ok_or(Unresolved::EmptyClient { w: client.w, h: client.h })?;
    let fit = |v: i64| i32::try_from(v).map_err(|_| Unresolved::OutOfRange);
    Ok(ScreenRect {
        x: fit(client.x.checked_add(b.x0).ok_or(Unresolved::OutOfRange)?)?,
        y: fit(client.y.checked_add(b.y0).ok_or(Unresolved::OutOfRange)?)?,
        w: fit(b.x1 - b.x0)?,
        h: fit(b.y1 - b.y0)?,
    })
}

/// A region as a module gives one, once `region_lua::read` has checked it: corners in screen
/// coordinates, or fractions of a window's client area, resolved when the read is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Region {
    Rect(ScreenRect),
    Window(Client, Fraction),
}

impl Region {
    /// The rectangle to read now. Corners are that rectangle already; a window region is
    /// resolved against the client area its table carries, and may have none this time.
    pub(crate) fn resolve(&self) -> Result<ScreenRect, Unresolved> {
        match self {
            Region::Rect(r) => Ok(*r),
            Region::Window(client, f) => resolve(*client, f),
        }
    }
}

/// Corners `{ x1, y1, x2, y2 }`, `x2` and `y2` exclusive, as a rectangle — or the mistake in
/// them. Strict, for every call that takes the Region form strictly: each corner inside the
/// screen's coordinate range, and `x2` greater than `x1` and `y2` greater than `y1`. Corners
/// are written by the module, so an empty or turned-around rectangle is a mistake in the call
/// (usually two corners swapped) and raises; a region that is empty because of what a window
/// does at run time is a window region, and `Unresolved` answers it. `what` names the region
/// in the message (`opts.region`, `entry 2`).
pub(crate) fn corners(what: &str, x1: i64, y1: i64, x2: i64, y2: i64) -> Result<ScreenRect, String> {
    for (name, v) in [("x1", x1), ("y1", y1), ("x2", x2), ("y2", y2)] {
        if i32::try_from(v).is_err() {
            return Err(format!("{what}.{name} is {v}, outside the screen's coordinate range"));
        }
    }
    if x2 <= x1 || y2 <= y1 {
        return Err(format!(
            "{what} {{ {x1}, {y1}, {x2}, {y2} }} is empty or turned around: x2 must be greater than x1, and y2 than y1"
        ));
    }
    let (Ok(w), Ok(h)) = (i32::try_from(x2 - x1), i32::try_from(y2 - y1)) else {
        return Err(format!(
            "{what} {{ {x1}, {y1}, {x2}, {y2} }} is wider or higher than the screen's coordinate range"
        ));
    };
    Ok(ScreenRect { x: x1 as i32, y: y1 as i32, w, h })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frac(x1: f64, y1: f64, x2: f64, y2: f64) -> Fraction {
        Fraction::new(x1, y1, x2, y2).unwrap()
    }

    /// The outside reader's own bounds, from the numbers of his test package: four regions,
    /// each at two frame sizes, as (x0, x1, y0, y1). All eight must come out exactly.
    #[test]
    fn the_readers_region_golden() {
        // Regions as (XStart, XEnd, YStart, YEnd), the order he stores them in.
        let cases: [((f64, f64, f64, f64), [(i64, i64, (i64, i64, i64, i64)); 2]); 4] = [
            ((0.02, 0.09, 0.33, 0.86), [(1280, 1024, (25, 116, 337, 881)), (1024, 768, (20, 93, 253, 661))]),
            ((0.34, 0.415, 0.64, 0.86), [(1280, 1024, (435, 532, 655, 881)), (1024, 768, (348, 425, 491, 661))]),
            ((0.025, 0.09, 0.42, 0.69), [(1280, 1024, (32, 116, 430, 707)), (1024, 768, (25, 93, 322, 530))]),
            ((0.02, 0.09, 0.34, 0.36), [(1280, 1024, (25, 116, 348, 369)), (1024, 768, (20, 93, 261, 277))]),
        ];
        for ((xs, xe, ys, ye), sizes) in cases {
            for (w, h, (x0, x1, y0, y1)) in sizes {
                let f = frac(xs, ys, xe, ye);
                assert_eq!(
                    frame_bounds(w, h, &f),
                    Some(Bounds { x0, x1, y0, y1 }),
                    "({xs}, {xe}, {ys}, {ye}) at {w}x{h}"
                );
            }
        }
    }

    /// f64, literally, not exact arithmetic: 800 * 0.035 is 28.000000000000004 in f64.
    #[test]
    fn the_arithmetic_is_doubles_not_exact_fractions() {
        let b = frame_bounds(800, 600, &frac(0.0, 0.0, 0.035, 1.0)).unwrap();
        assert_eq!((b.x0, b.x1), (0, 29), "ceil(28.000000000000004) is 29");
        let r = resolve(Client { x: 100, y: 50, w: 800, h: 600 }, &frac(0.0, 0.0, 0.035, 1.0)).unwrap();
        assert_eq!(r, ScreenRect { x: 100, y: 50, w: 29, h: 600 });
    }

    /// His clamps: never empty, never outside the client area, never a raise.
    #[test]
    fn the_clamps_keep_one_pixel_inside_the_client() {
        // An end before its start: one column, at the start.
        assert_eq!(frame_bounds(100, 100, &frac(0.5, 0.5, 0.2, 0.2)), Some(Bounds { x0: 50, x1: 51, y0: 50, y1: 51 }));
        // Outside 0..1 on both sides: the whole axis.
        assert_eq!(frame_bounds(100, 10, &frac(-0.5, -1.0, 1.5, 2.0)), Some(Bounds { x0: 0, x1: 100, y0: 0, y1: 10 }));
        // A start at or past the right edge: the last column.
        assert_eq!(frame_bounds(100, 10, &frac(1.0, 0.0, 1.0, 1.0)), Some(Bounds { x0: 99, x1: 100, y0: 0, y1: 10 }));
        // Zero width: one column.
        assert_eq!(frame_bounds(100, 10, &frac(0.25, 0.0, 0.25, 1.0)), Some(Bounds { x0: 25, x1: 26, y0: 0, y1: 10 }));
        // A one-pixel frame.
        assert_eq!(frame_bounds(1, 1, &frac(0.0, 0.0, 1.0, 1.0)), Some(Bounds { x0: 0, x1: 1, y0: 0, y1: 1 }));
        // Absurd but finite: saturates onto the bounds instead of wrapping.
        assert_eq!(frame_bounds(100, 10, &frac(1e300, -1e300, 1e300, 1e300)), Some(Bounds { x0: 99, x1: 100, y0: 0, y1: 10 }));
    }

    #[test]
    fn an_empty_client_and_a_non_finite_fraction_are_refused() {
        let f = frac(0.0, 0.0, 1.0, 1.0);
        assert_eq!(resolve(Client { x: 0, y: 0, w: 0, h: 600 }, &f), Err(Unresolved::EmptyClient { w: 0, h: 600 }));
        assert_eq!(resolve(Client { x: 0, y: 0, w: 800, h: -1 }, &f), Err(Unresolved::EmptyClient { w: 800, h: -1 }));
        assert!(Unresolved::EmptyClient { w: 0, h: 600 }.to_string().contains("empty (0x600)"));
        let e = Fraction::new(0.0, f64::NAN, 1.0, 1.0).unwrap_err();
        assert!(e.contains("fraction.y1") && e.contains("NaN"), "{e}");
        assert!(Fraction::new(0.0, 0.0, f64::INFINITY, 1.0).is_err());
        // A client on a monitor left of the primary: negative origins are ordinary.
        let r = resolve(Client { x: -1920, y: -200, w: 1280, h: 1024 }, &frac(0.02, 0.33, 0.09, 0.86)).unwrap();
        assert_eq!(r, ScreenRect { x: -1920 + 25, y: -200 + 337, w: 91, h: 544 });
        assert_eq!(
            resolve(Client { x: i32::MAX as i64, y: 0, w: 10, h: 10 }, &frac(0.5, 0.0, 1.0, 1.0)),
            Err(Unresolved::OutOfRange)
        );
    }

    /// Corners: a rectangle when it is one, a message naming the region when it is not.
    #[test]
    fn corners_are_a_rectangle_or_a_mistake() {
        assert_eq!(corners("region", 10, 20, 110, 70), Ok(ScreenRect { x: 10, y: 20, w: 100, h: 50 }));
        // Left of and above the primary monitor.
        assert_eq!(corners("region", -10, -5, 0, 5), Ok(ScreenRect { x: -10, y: -5, w: 10, h: 10 }));
        for (c, want) in [
            ((10, 0, 10, 10), "region { 10, 0, 10, 10 } is empty or turned around"),
            ((50, 50, 10, 10), "region { 50, 50, 10, 10 } is empty or turned around"),
            ((0, 10, 10, 9), "is empty or turned around"),
            ((0, 0, 1 << 31, 10), "region.x2 is 2147483648, outside the screen's coordinate range"),
            ((0, -(1 << 31) - 1, 10, 10), "region.y1 is -2147483649, outside"),
            // Each corner fits, the width does not: two billion either side of zero.
            ((-2_000_000_000, 0, 2_000_000_000, 10), "is wider or higher than the screen's coordinate range"),
        ] {
            let e = corners("region", c.0, c.1, c.2, c.3).unwrap_err();
            assert!(e.contains(want), "{c:?}: {e}");
        }
        // The window form never needs this: it is resolved, and its only empty case is answered.
        let w = Region::Window(Client { x: 0, y: 0, w: 0, h: 0 }, frac(0.0, 0.0, 1.0, 1.0));
        assert_eq!(w.resolve(), Err(Unresolved::EmptyClient { w: 0, h: 0 }));
        let r = Region::Rect(ScreenRect { x: 1, y: 2, w: 3, h: 4 });
        assert_eq!(r.resolve(), Ok(ScreenRect { x: 1, y: 2, w: 3, h: 4 }));
    }
}
