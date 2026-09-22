//! What a read is made of, with nothing in it but the standard library: the rectangle, the
//! priority it runs at, the engine's answer per region, and the `Reading` a module is handed.
//!
//! Kept free of `mlua`, `image` and every platform crate on purpose. `crates/macos-check`
//! borrows this file (and the other pure ones beside it) so that the macOS backend, which
//! names these types, is type-checked against the same definitions the real build uses.

use std::cell::Cell;

/// A screen rectangle: top-left corner and size, in the platform's screen units (physical
/// pixels on Windows, points on macOS). `w` or `h` of 0 is an empty region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Whether there is anything inside it to read.
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Its area in pixels, in 64 bits: two 40 000-pixel sides do not fit in an `i32`.
    pub fn area(&self) -> i64 {
        if self.is_empty() {
            0
        } else {
            self.w as i64 * self.h as i64
        }
    }

    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.h)
    }

    /// The smallest rectangle holding both. An empty one does not widen the other.
    ///
    /// Worked out in 64 bits, with the size capped at `i32::MAX`: two small regions two
    /// billion pixels either side of zero pass every check a read makes, and their box is
    /// wider than an `i32` holds. Capped, such a box is simply far too large to capture as one,
    /// which is the right answer about it (`plan::capture_plan`).
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x as i64 + self.w as i64).max(other.x as i64 + other.w as i64);
        let bottom = (self.y as i64 + self.h as i64).max(other.y as i64 + other.h as i64);
        let capped = |len: i64| len.min(i32::MAX as i64) as i32;
        Rect::new(x, y, capped(right - x as i64), capped(bottom - y as i64))
    }

    /// The tuple the backend's capture and OCR calls take.
    pub fn tuple(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.w, self.h)
    }

    pub fn from_tuple((x, y, w, h): (i32, i32, i32, i32)) -> Rect {
        Rect::new(x, y, w, h)
    }
}

/// Which lane a read waits in. Inferred by the host from what was being dispatched when the
/// read was asked for — never chosen by a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    /// Polls, and everything a module does while loading.
    #[default]
    Background,
    /// A key, a hotkey, a game-controller press, a window coming forward or the focus moving:
    /// somebody is waiting for the answer.
    Interactive,
}

thread_local! {
    /// What the host is dispatching on this thread right now. Only the main thread ever sets
    /// it; every other thread reads the default.
    static DISPATCH: Cell<Priority> = const { Cell::new(Priority::Background) };
}

/// The priority a read, a timer or an image search asked for NOW would inherit.
pub fn current_priority() -> Priority {
    DISPATCH.with(|p| p.get())
}

/// Sets the priority for as long as the returned value lives, and puts the previous one back
/// when it is dropped — unwinding included, so a callback that raises cannot leave the host
/// dispatching everything after it as interactive.
#[must_use = "the priority is restored when this is dropped"]
pub struct PriorityScope {
    previous: Priority,
}

pub fn enter_priority(p: Priority) -> PriorityScope {
    PriorityScope { previous: DISPATCH.with(|c| c.replace(p)) }
}

impl Drop for PriorityScope {
    fn drop(&mut self) {
        let previous = self.previous;
        DISPATCH.with(|c| c.set(previous));
    }
}

/// How a read ended, per region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Something was read.
    Text,
    /// The region has nothing drawn in it; no recogniser was asked.
    Blank,
    /// The recognisers ran and read nothing.
    None,
    /// It could not be read at all; `error` says why.
    Failed,
    /// A newer read with the same key was asked for before this one was recognised, so it was
    /// never recognised.
    Stale,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Text => "text",
            Status::Blank => "blank",
            Status::None => "none",
            Status::Failed => "failed",
            Status::Stale => "stale",
        }
    }
}

/// A word as a module sees it: absolute screen coordinates. `approx` marks a box the host
/// worked out from the text's length rather than one an engine located.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub approx: bool,
}

/// One row of text: the engine lines that share a baseline, left to right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub rect: Rect,
    pub words: Vec<Word>,
}

/// One region's answer, the shape `host.ocr.read` hands a callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The region as it was read, absolute.
    pub rect: Rect,
    pub status: Status,
    /// The rows joined with "\n"; empty unless `status` is `Text`.
    pub text: String,
    pub rows: Vec<Row>,
    /// Every row's words in reading order. Non-empty exactly when `text` has something other
    /// than white space in it.
    pub words: Vec<Word>,
    /// The language tag the engine was asked for, as the platform names it; empty when the read
    /// never reached an engine.
    pub lang: String,
    /// Why, when `status` is `Failed`.
    pub error: Option<String>,
}

impl Reading {
    /// A reading that says nothing but `status` and why.
    pub fn outcome(rect: Rect, status: Status, error: Option<String>) -> Reading {
        Reading {
            rect,
            status,
            text: String::new(),
            rows: Vec::new(),
            words: Vec::new(),
            lang: String::new(),
            error,
        }
    }

    pub fn failed(rect: Rect, error: impl Into<String>) -> Reading {
        Reading::outcome(rect, Status::Failed, Some(error.into()))
    }
}

/// A word as an engine located it, relative to the region it read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WordBox {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One line as an engine returned it: `OcrLine` on Windows, one observation on macOS. What a
/// "line" is differs between the two — Vision returns a row with a gap in it as two — which
/// is why the host groups them into rows itself (`pipeline::rows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineLine {
    pub text: String,
    pub words: Vec<WordBox>,
}

/// What the platform's recognisers made of one region, before the host normalises it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineOut {
    /// The blank guard answered: nothing drawn, no engine asked.
    Blank,
    /// The system recogniser's lines, region-relative.
    Lines(Vec<EngineLine>),
    /// The Windows fallback recogniser read `text` and located nothing; `content` is the
    /// content crop it read, region-relative.
    Fallback { text: String, content: Rect },
    /// It could not be read.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_union_ignores_empty_rectangles() {
        let a = Rect::new(10, 10, 5, 5);
        let b = Rect::new(0, 12, 3, 10);
        assert_eq!(a.union(&b), Rect::new(0, 10, 15, 12));
        assert_eq!(a.union(&Rect::default()), a);
        assert_eq!(Rect::default().union(&b), b);
        assert_eq!(Rect::new(0, 0, 40_000, 40_000).area(), 1_600_000_000);
        assert_eq!(Rect::new(5, 5, -1, 4).area(), 0);
    }

    /// Two 10x10 regions two billion pixels either side of zero: 200 pixels, which every check
    /// a read makes lets through. Their box used to overflow an `i32` — a panic in a debug
    /// build, a wrapped negative width in a release one.
    #[test]
    fn a_union_of_regions_far_apart_is_capped_rather_than_overflowing() {
        let a = Rect::new(-2_000_000_000, 0, 10, 10);
        let b = Rect::new(2_000_000_000, -2_000_000_000, 10, 10);
        let u = a.union(&b);
        assert_eq!((u.x, u.y), (-2_000_000_000, -2_000_000_000));
        assert_eq!((u.w, u.h), (i32::MAX, 2_000_000_010), "four billion wide, capped; two billion tall");
        assert!(!u.is_empty());
        let edge = Rect::new(i32::MAX - 5, 0, 5, 5).union(&Rect::new(0, 0, 1, 1));
        assert_eq!(edge, Rect::new(0, 0, i32::MAX, 5));
    }

    #[test]
    fn a_priority_scope_restores_what_it_found_even_when_nested_and_unwound() {
        assert_eq!(current_priority(), Priority::Background);
        {
            let _a = enter_priority(Priority::Interactive);
            assert_eq!(current_priority(), Priority::Interactive);
            {
                let _b = enter_priority(Priority::Background);
                assert_eq!(current_priority(), Priority::Background);
            }
            assert_eq!(current_priority(), Priority::Interactive);
        }
        assert_eq!(current_priority(), Priority::Background);
        // `resume_unwind` unwinds without running the panic hook, so nothing is printed.
        let unwound = std::panic::catch_unwind(|| {
            let _a = enter_priority(Priority::Interactive);
            std::panic::resume_unwind(Box::new("inside a callback"));
        });
        assert!(unwound.is_err());
        assert_eq!(current_priority(), Priority::Background, "a panic did not leave it set");
        assert!(Priority::Interactive > Priority::Background);
    }

    #[test]
    fn statuses_have_the_names_the_documentation_uses() {
        let all = [Status::Text, Status::Blank, Status::None, Status::Failed, Status::Stale];
        let names: Vec<&str> = all.iter().map(|s| s.as_str()).collect();
        assert_eq!(names, ["text", "blank", "none", "failed", "stale"]);
    }
}
