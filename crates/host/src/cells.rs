//! `host.screen.cells`: a region reduced to a grid of cells, each cell the share of its pixels
//! that pass a colour test, and the ranking of such a grid against stored ones.
//!
//! Written to reproduce an outside game-menu reader BIT FOR BIT, so that the signatures it
//! recorded can be matched here without converting anything. Every rule below is his, and
//! each is checked against his own test vectors (`cells_golden_tests.rs`):
//!
//! - **Grid:** `cols` x `rows` cells, stored row-major (`cell[row * cols + col]`).
//! - **Blocks:** for a region `L` pixels long cut into `n` blocks, block `s` starts at
//!   `floor(s * L / n)` and ends at `max(start + 1, floor((s + 1) * L / n))`. A block is at
//!   least one pixel, growing right or down, so a region smaller than the grid still has a
//!   value in every cell: neighbouring cells then share pixels.
//! - **Cell value:** `round(255 * matching / total)`, rounding half to even — .NET's
//!   `Math.Round` without a `MidpointRounding` argument — computed exactly as his line does,
//!   `(255.0 * m / t)` in doubles. `round_half_even` is the integer form, tested equal.
//! - **The colour test** is a predicate string: an OR of AND-groups of integer linear
//!   comparisons over the 8-bit r, g and b of the captured pixel. Alpha is ignored.
//! - **Similarity:** `1 - sum|a - b| / (length * 255.0)`, the sum in an integer.
//! - **Decision:** states with the same name form one ITEM, whose score is its best state's
//!   (the first of equals kept); the best item wins (the first of equals), and the best score
//!   among the other items is reported as the runner-up, so a module can apply both of his
//!   thresholds: `similarity >= minimum` and `similarity - runnerUp >= margin`.
//!
//! Hex is the only exchange format for cells: ASCII survives data files, settings, exports,
//! logs and `host.json`, where a raw byte string does not.
//!
//! Pure: no Lua, no OS. Borrowed by `crates/macos-check` so the Mac build type-checks it too.

use std::fmt;
use std::sync::Arc;

use winnow::ascii::multispace0;
use winnow::combinator::{alt, opt, preceded, repeat};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::stream::{LocatingSlice, Location};
use winnow::token::{one_of, take_while};

use crate::backend::CapturedImage;

// ── Limits ─────────────────────────────────────────────────────────────────────────────

/// The longest predicate source, in bytes. Room for every canonical form: multiplying out makes
/// a predicate longer than its source (a 125-byte one prints as 1468 bytes), and the longest
/// canonical form there can be — 8 alternatives of 16 comparisons of 49 bytes each — is 6916
/// bytes, so whatever `host.screen.predicate` returns is accepted again.
pub(crate) const MAX_SOURCE: usize = 8192;
/// How deep parentheses may nest. Each level is a recursion of the parser, about 4 KB of stack
/// in a debug build and 1.6 KB in a release build, and it runs on the event loop, whose stack
/// is 1 MB on Windows: without this, 8192 bytes of `(` need 35 MB in a debug build and 13 MB in
/// a release build, and the overflow is a crash, not an error. With it, the worst source parses
/// in about 130 KB (debug) and 70 KB (release), and a condition pasted from C# uses two or three.
pub(crate) const MAX_NESTING: usize = 32;
/// The most alternatives (OR'd groups) a predicate may have once multiplied out.
pub(crate) const MAX_ALTERNATIVES: usize = 8;
/// The most conditions in one alternative; `==` counts as two.
pub(crate) const MAX_CONDITIONS: usize = 16;
/// Bounds on a condition after its terms are combined. They keep every evaluation inside an
/// i32: 3 * 255 * 10^6 + 10^9 = 1.765 * 10^9 < 2^31 - 1.
pub(crate) const MAX_COEFFICIENT: i64 = 1_000_000;
pub(crate) const MAX_CONSTANT: i64 = 1_000_000_000;
/// The largest number a predicate may write.
const MAX_LITERAL: u64 = i32::MAX as u64;
/// `cols` and `rows` are each 1..=MAX_SIDE, and together at most MAX_CELLS.
pub(crate) const MAX_SIDE: u32 = 256;
pub(crate) const MAX_CELLS: u32 = 4096;
/// The most states one `matchCells` call compares against.
pub(crate) const MAX_STATES: usize = 1024;
/// The largest region, in pixels (an 8K screen is 33 million).
pub(crate) const MAX_REGION_PIXELS: u64 = 40_000_000;

// ── The predicate ──────────────────────────────────────────────────────────────────────

/// One condition, `r*R + g*G + b*B + k >= 0` over a pixel's channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lin {
    pub r: i32,
    pub g: i32,
    pub b: i32,
    pub k: i32,
}

impl Lin {
    #[cfg(test)]
    #[inline]
    fn holds(&self, r: i32, g: i32, b: i32) -> bool {
        self.r * r + self.g * g + self.b * b + self.k >= 0
    }
}

/// A colour test: true when every condition of at least one alternative holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Predicate {
    alternatives: Vec<Vec<Lin>>,
}

/// A predicate that does not parse: where (a 1-based column, in characters) and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PredError {
    pub column: usize,
    pub message: String,
}

impl fmt::Display for PredError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (column {})", self.message, self.column)
    }
}

impl Predicate {
    /// Parses a predicate. The grammar, with `and` binding tighter than `or` as in Luau:
    ///
    /// ```text
    /// expr := and { OR and }       and := atom { AND atom }       atom := cmp | "(" expr ")"
    /// cmp  := sum REL sum          sum := ["-"] term { ("+" | "-") term }
    /// term := INT ["*" CH] | CH ["*" INT]
    /// CH   := r | g | b | red | green | blue        REL := >= | <= | > | < | ==
    /// AND  := and | &&     OR := or | ||
    /// INT  := decimal digits without a leading zero, at most 2147483647
    /// ```
    ///
    /// with parentheses at most `MAX_NESTING` deep, so a C# condition such as
    /// `red*10 >= green*13 && red >= 80` is taken as written. The
    /// expression is multiplied out into alternatives of conditions, each normalised to
    /// `lin >= 0` (`>` is `>= 1`, `<` is `<= -1`, `==` is two conditions), which is exact
    /// because everything is an integer.
    pub(crate) fn parse(src: &str) -> Result<Self, PredError> {
        if src.len() > MAX_SOURCE {
            return Err(PredError {
                column: 1,
                message: format!("the predicate is {} bytes long; the limit is {MAX_SOURCE}", src.len()),
            });
        }
        if src.trim().is_empty() {
            return Err(PredError {
                column: 1,
                message: "the predicate is empty; give at least one comparison, such as \"r >= 80\"".to_string(),
            });
        }
        match (|i: &mut In<'_>| expr(i, 0), end).parse(LocatingSlice::new(src)) {
            Ok((alternatives, ())) => Ok(Predicate { alternatives }),
            Err(e) => {
                let (at, message) = match e.inner().context().next() {
                    Some(w) => (w.at, w.msg.clone()),
                    // Every failure below is raised with a message; this is the net under it.
                    None => (e.offset(), format!("unexpected {}", found(src.get(e.offset()..).unwrap_or("")))),
                };
                Err(PredError { column: column_of(src, at), message })
            }
        }
    }

    /// Whether the pixel passes: the reference path the tests hold `count` to, which evaluates
    /// the same conditions a whole row at a time.
    #[cfg(test)]
    pub(crate) fn test(&self, r: u8, g: u8, b: u8) -> bool {
        let (r, g, b) = (r as i32, g as i32, b as i32);
        self.alternatives.iter().any(|alt| alt.iter().all(|l| l.holds(r, g, b)))
    }

    #[cfg(test)]
    pub(crate) fn alternatives(&self) -> &[Vec<Lin>] {
        &self.alternatives
    }
}

/// The canonical form: channels in r, g, b order, a coefficient of 1 left out, `>=` with the
/// constant on the right — or, when every coefficient is negative, the condition turned
/// around and written with `<=`, so `b <= 150` stays `b <= 150`. Parses back to the same
/// conditions.
impl fmt::Display for Lin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let flip = self.r <= 0 && self.g <= 0 && self.b <= 0;
        let sign = if flip { -1 } else { 1 };
        let mut first = true;
        for (c, ch) in [(self.r, "r"), (self.g, "g"), (self.b, "b")] {
            let c = c * sign;
            if c == 0 {
                continue;
            }
            if first {
                match c {
                    1 => write!(f, "{ch}")?,
                    -1 => write!(f, "-{ch}")?,
                    _ => write!(f, "{c}*{ch}")?,
                }
            } else {
                let (op, m) = if c < 0 { ('-', -c) } else { ('+', c) };
                if m == 1 {
                    write!(f, " {op} {ch}")?;
                } else {
                    write!(f, " {op} {m}*{ch}")?;
                }
            }
            first = false;
        }
        if flip {
            write!(f, " <= {}", self.k)
        } else {
            write!(f, " >= {}", -self.k)
        }
    }
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let several = self.alternatives.len() > 1;
        for (n, alt) in self.alternatives.iter().enumerate() {
            if n > 0 {
                f.write_str(" or ")?;
            }
            let paren = several && alt.len() > 1;
            if paren {
                f.write_str("(")?;
            }
            for (m, l) in alt.iter().enumerate() {
                if m > 0 {
                    f.write_str(" and ")?;
                }
                write!(f, "{l}")?;
            }
            if paren {
                f.write_str(")")?;
            }
        }
        Ok(())
    }
}

/// The 1-based column, in characters, of byte offset `at`.
fn column_of(src: &str, at: usize) -> usize {
    src.get(..at).map(|s| s.chars().count()).unwrap_or_else(|| src.chars().count()) + 1
}

// The parser, on winnow. Every failure is a CUT carrying its own message and the offset it
// belongs to (`Why`), so the error names the place that is wrong rather than wherever the
// input happened to stop — a term's start for `r*g`, a comparison's start for a constant that
// is out of range.

type In<'a> = LocatingSlice<&'a str>;

#[derive(Clone, Debug)]
struct Why {
    at: usize,
    msg: String,
}

type R<T> = Result<T, ErrMode<ContextError<Why>>>;

/// Alternatives, each a list of conditions.
type Dnf = Vec<Vec<Lin>>;

fn refuse<T>(at: usize, msg: impl Into<String>) -> R<T> {
    let mut e = ContextError::new();
    e.push(Why { at, msg: msg.into() });
    Err(ErrMode::Cut(e))
}

/// The next token of `rest`, quoted, for "found …" in a message.
fn found(rest: &str) -> String {
    let s = rest.trim_start();
    let Some(c) = s.chars().next() else { return "the end".to_string() };
    const OPS: &str = "<>=!~&|";
    let n = if c.is_ascii_alphanumeric() || c == '_' {
        s.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).unwrap_or(s.len())
    } else if OPS.contains(c) {
        s.find(|c: char| !OPS.contains(c)).unwrap_or(s.len())
    } else {
        c.len_utf8()
    };
    format!("'{}'", &s[..n])
}

/// What is left of the input, as plain text.
fn rest<'a>(i: &In<'a>) -> &'a str {
    **i
}

fn ws(i: &mut In<'_>) -> R<()> {
    multispace0.void().parse_next(i)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn word<'a>(i: &mut In<'a>) -> R<&'a str> {
    (one_of(|c: char| c.is_ascii_alphabetic() || c == '_'), take_while(0.., is_word_char))
        .take()
        .parse_next(i)
}

fn keyword(kw: &'static str) -> impl FnMut(&mut In<'_>) -> R<()> {
    move |i: &mut In<'_>| word.verify(|w: &str| w == kw).void().parse_next(i)
}

fn and_op(i: &mut In<'_>) -> R<()> {
    preceded(ws, alt(("&&".void(), keyword("and")))).parse_next(i)
}

fn or_op(i: &mut In<'_>) -> R<()> {
    preceded(ws, alt(("||".void(), keyword("or")))).parse_next(i)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ch {
    R,
    G,
    B,
}

#[derive(Clone, Copy, Debug)]
enum Operand {
    Int(i64),
    Ch(Ch),
}

/// `r*R + g*G + b*B + k`, folded in i64. Nothing here can overflow: a term is below 2^31 and
/// every term after the first takes at least two bytes with its sign, so an 8192-byte source
/// sums at most 4097 of them, below 2^44.
#[derive(Clone, Copy, Debug, Default)]
struct Form {
    r: i64,
    g: i64,
    b: i64,
    k: i64,
}

impl Form {
    fn channel(c: Ch, n: i64) -> Form {
        match c {
            Ch::R => Form { r: n, ..Form::default() },
            Ch::G => Form { g: n, ..Form::default() },
            Ch::B => Form { b: n, ..Form::default() },
        }
    }
    fn plus(self, o: Form) -> Form {
        Form { r: self.r + o.r, g: self.g + o.g, b: self.b + o.b, k: self.k + o.k }
    }
    fn neg(self) -> Form {
        Form { r: -self.r, g: -self.g, b: -self.b, k: -self.k }
    }
}

/// Decimal digits, taken whole: a number that is refused is refused with all of its digits in
/// the message, not cut where a number parser would stop.
fn number(i: &mut In<'_>) -> R<i64> {
    let at = i.current_token_start();
    let digits: &str = take_while(1.., |c: char| c.is_ascii_digit()).parse_next(i)?;
    // A decimal point says what it is, rather than "expected a comparison, found '.'".
    if rest(i).starts_with('.') {
        let rest: &str = preceded('.', take_while(0.., |c: char| c.is_ascii_digit())).parse_next(i)?;
        return refuse(
            at,
            format!("'{digits}.{rest}' is not a whole number; scale both sides instead: 10*r >= 13*g, not r >= 1.3*g"),
        );
    }
    // `010` is ten in C# and Luau and eight in C and JavaScript; a pasted condition is taken as
    // written only where every one of them agrees.
    if digits.len() > 1 && digits.starts_with('0') {
        let plain = match digits.trim_start_matches('0') {
            "" => "0",
            p => p,
        };
        return refuse(
            at,
            format!("'{digits}' has a leading zero, which C and JavaScript can read as octal; write {plain}"),
        );
    }
    match digits.parse::<u64>() {
        Ok(n) if n <= MAX_LITERAL => Ok(n as i64),
        // Too large for a u64 as well, or merely for the limit: the same answer either way.
        _ => refuse(at, format!("'{digits}' is too large (the limit is {MAX_LITERAL})")),
    }
}

fn operand(i: &mut In<'_>) -> R<Operand> {
    ws(i)?;
    let at = i.current_token_start();
    match rest(i).chars().next() {
        Some(c) if c.is_ascii_digit() => Ok(Operand::Int(number(i)?)),
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            let w = word(i)?;
            match w {
                "r" | "red" => Ok(Operand::Ch(Ch::R)),
                "g" | "green" => Ok(Operand::Ch(Ch::G)),
                "b" | "blue" => Ok(Operand::Ch(Ch::B)),
                "not" => refuse(at, "'not' is not supported; turn the comparison around (not r >= 80 is r < 80)"),
                "and" | "or" => refuse(at, format!("expected r, g, b or a whole number, found '{w}'")),
                _ if matches!(w.to_ascii_lowercase().as_str(), "r" | "g" | "b" | "red" | "green" | "blue") => {
                    refuse(at, format!("'{w}' is not a channel; channel names are lower-case: r, g, b"))
                }
                _ => refuse(at, format!("'{w}' is not a channel; use r, g or b (or red, green, blue)")),
            }
        }
        _ => refuse(at, format!("expected r, g, b or a whole number, found {}", found(rest(i)))),
    }
}

fn term(i: &mut In<'_>) -> R<Form> {
    ws(i)?;
    let at = i.current_token_start();
    let ((a, b), text) = (operand, opt(preceded((ws, '*'), operand))).with_taken().parse_next(i)?;
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    match (a, b) {
        (Operand::Int(n), None) => Ok(Form { k: n, ..Form::default() }),
        (Operand::Ch(c), None) => Ok(Form::channel(c, 1)),
        (Operand::Int(n), Some(Operand::Ch(c))) | (Operand::Ch(c), Some(Operand::Int(n))) => Ok(Form::channel(c, n)),
        (Operand::Ch(_), Some(Operand::Ch(_))) => {
            refuse(at, format!("'{text}' multiplies two channels; a term is a whole number times one channel"))
        }
        (Operand::Int(x), Some(Operand::Int(y))) => {
            refuse(at, format!("'{text}' multiplies two numbers; write the product ({})", x * y))
        }
    }
}

fn sum(i: &mut In<'_>) -> R<Form> {
    ws(i)?;
    let neg = opt('-').parse_next(i)?.is_some();
    let first = term(i)?;
    let rest: Vec<(char, Form)> = repeat(0.., (preceded(ws, one_of(['+', '-'])), term)).parse_next(i)?;
    let mut acc = if neg { first.neg() } else { first };
    for (op, f) in rest {
        acc = acc.plus(if op == '+' { f } else { f.neg() });
    }
    Ok(acc)
}

#[derive(Clone, Copy, Debug)]
enum Rel {
    Ge,
    Le,
    Gt,
    Lt,
    Eq,
}

const REL_CHARS: [char; 5] = ['<', '>', '=', '!', '~'];

fn rel(i: &mut In<'_>, left: &str) -> R<Rel> {
    ws(i)?;
    let at = i.current_token_start();
    let op: &str = take_while(0.., REL_CHARS).parse_next(i)?;
    match op {
        ">=" => Ok(Rel::Ge),
        "<=" => Ok(Rel::Le),
        ">" => Ok(Rel::Gt),
        "<" => Ok(Rel::Lt),
        "==" => Ok(Rel::Eq),
        "" => refuse(
            at,
            format!("expected a comparison (>=, <=, >, <, ==) after \"{}\", found {}", left.trim(), found(rest(i))),
        ),
        "!=" | "~=" => refuse(at, format!("'{op}' cannot be one condition; write \"x < y or x > y\" as two")),
        "=" => refuse(at, "'=' is not a comparison; use == (or >=, <=)"),
        "=>" => refuse(at, "'=>' is written '>='"),
        "=<" => refuse(at, "'=<' is written '<='"),
        other => refuse(at, format!("'{other}' is not a comparison; use >=, <=, >, < or ==")),
    }
}

/// `sum REL sum`, as its alternatives: one, with one condition — or two for `==`.
fn cmp(i: &mut In<'_>) -> R<Dnf> {
    ws(i)?;
    let at = i.current_token_start();
    let ((l, op, r), text) = (|i: &mut In<'_>| -> R<(Form, Rel, Form)> {
        let (l, left) = sum.with_taken().parse_next(i)?;
        let op = rel(i, left)?;
        let r = sum(i)?;
        Ok((l, op, r))
    })
    .with_taken()
    .parse_next(i)?;
    // `80 <= r <= 120` would silently mean something else in C, and nothing at all here.
    ws(i)?;
    if rest(i).starts_with(REL_CHARS) {
        return refuse(
            i.current_token_start(),
            "a comparison cannot be chained ('80 <= r <= 120'); write 80 <= r and r <= 120",
        );
    }
    let text = text.trim();
    let d = l.plus(r.neg());
    let minus_one = |f: Form| Form { k: f.k - 1, ..f };
    let rows: Vec<Form> = match op {
        Rel::Ge => vec![d],
        Rel::Gt => vec![minus_one(d)],
        Rel::Le => vec![d.neg()],
        Rel::Lt => vec![minus_one(d.neg())],
        Rel::Eq => vec![d, d.neg()],
    };
    let mut out = Vec::with_capacity(rows.len());
    for f in rows {
        if f.r == 0 && f.g == 0 && f.b == 0 {
            return refuse(at, format!("'{text}' does not depend on r, g or b, so it never looks at the pixel"));
        }
        for (c, name) in [(f.r, "r"), (f.g, "g"), (f.b, "b")] {
            if c.abs() > MAX_COEFFICIENT {
                return refuse(
                    at,
                    format!("the coefficient of {name} in '{text}' is {c} once its terms are combined; the limit is ±{MAX_COEFFICIENT}"),
                );
            }
        }
        if f.k.abs() > MAX_CONSTANT {
            return refuse(
                at,
                format!("the constant in '{text}' is {} once its terms are combined; the limit is ±{MAX_CONSTANT}", f.k),
            );
        }
        out.push(Lin { r: f.r as i32, g: f.g as i32, b: f.b as i32, k: f.k as i32 });
    }
    Ok(vec![out])
}

/// A comparison, or a parenthesised expression `depth` parentheses deep.
fn atom(i: &mut In<'_>, depth: usize) -> R<Dnf> {
    ws(i)?;
    if !rest(i).starts_with('(') {
        return cmp(i);
    }
    let open = i.current_token_start();
    if depth >= MAX_NESTING {
        return refuse(open, format!("parentheses are nested more than {MAX_NESTING} deep here"));
    }
    '('.parse_next(i)?;
    let inner = expr(i, depth + 1)?;
    ws(i)?;
    let at = i.current_token_start();
    match rest(i).chars().next() {
        Some(')') => {
            ')'.parse_next(i)?;
            Ok(inner)
        }
        None => refuse(open, "this '(' is never closed"),
        _ => refuse(at, format!("expected 'and', 'or' or ')', found {}", found(rest(i)))),
    }
}

/// Every alternative of `a` joined with every alternative of `b`.
fn product(a: Dnf, b: Dnf, at: usize) -> R<Dnf> {
    let n = a.len().saturating_mul(b.len());
    if n > MAX_ALTERNATIVES {
        return refuse(at, format!("expands to at least {n} alternatives; the limit is {MAX_ALTERNATIVES}"));
    }
    let mut out = Vec::with_capacity(n);
    for x in &a {
        for y in &b {
            let mut g = x.clone();
            g.extend_from_slice(y);
            if g.len() > MAX_CONDITIONS {
                return refuse(
                    at,
                    format!("an alternative has {} conditions by here (== counts as two); the limit is {MAX_CONDITIONS}", g.len()),
                );
            }
            out.push(g);
        }
    }
    Ok(out)
}

fn and_group(i: &mut In<'_>, depth: usize) -> R<Dnf> {
    let mut acc = atom(i, depth)?;
    while opt(and_op).parse_next(i)?.is_some() {
        ws(i)?;
        let at = i.current_token_start();
        let next = atom(i, depth)?;
        acc = product(acc, next, at)?;
    }
    Ok(acc)
}

/// An expression inside `depth` parentheses; the whole predicate is depth 0.
fn expr(i: &mut In<'_>, depth: usize) -> R<Dnf> {
    let mut acc = and_group(i, depth)?;
    while opt(or_op).parse_next(i)?.is_some() {
        ws(i)?;
        let at = i.current_token_start();
        let next = and_group(i, depth)?;
        let n = acc.len() + next.len();
        if n > MAX_ALTERNATIVES {
            return refuse(at, format!("expands to at least {n} alternatives; the limit is {MAX_ALTERNATIVES}"));
        }
        acc.extend(next);
    }
    Ok(acc)
}

fn end(i: &mut In<'_>) -> R<()> {
    ws(i)?;
    let at = i.current_token_start();
    match rest(i).chars().next() {
        None => Ok(()),
        Some(')') => refuse(at, "this ')' has no '(' to close"),
        Some('&') => refuse(at, "'&' is written '&&' (or 'and')"),
        Some('|') => refuse(at, "'|' is written '||' (or 'or')"),
        _ => refuse(at, format!("expected 'and', 'or' or the end, found {}", found(rest(i)))),
    }
}

// ── The grid ───────────────────────────────────────────────────────────────────────────

/// A grid and the colour test its cells count.
#[derive(Clone, Debug)]
pub(crate) struct CellSpec {
    pub cols: u32,
    pub rows: u32,
    pub pred: Arc<Predicate>,
}

impl CellSpec {
    pub(crate) fn new(cols: u32, rows: u32, pred: Predicate) -> Result<Self, String> {
        for (name, n) in [("cols", cols), ("rows", rows)] {
            if !(1..=MAX_SIDE).contains(&n) {
                return Err(format!("{name} is {n}; it must be from 1 to {MAX_SIDE}"));
            }
        }
        if cols * rows > MAX_CELLS {
            return Err(format!("{cols}x{rows} is {} cells; the limit is {MAX_CELLS}", cols * rows));
        }
        Ok(CellSpec { cols, rows, pred: Arc::new(pred) })
    }

    /// The number of cells, which is also the number of bytes a state holds.
    pub(crate) fn cells(&self) -> usize {
        (self.cols * self.rows) as usize
    }
}

/// The part of an image to reduce, in the image's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sub {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellsError {
    /// The part asked for is not inside the image.
    OutOfImage,
    /// The image's buffer is shorter than its own dimensions say.
    ShortBuffer,
    /// Nothing to reduce: no width or no height.
    Empty,
    /// More pixels than a cell's counters hold. The bindings refuse far smaller regions first.
    TooLarge,
}

impl fmt::Display for CellsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CellsError::OutOfImage => "the region is not inside the captured image",
            CellsError::ShortBuffer => "the capture came back shorter than its own size",
            CellsError::Empty => "the region is empty",
            CellsError::TooLarge => "the region is too large to count",
        })
    }
}

/// The blocks of an axis `len` pixels long (`len >= 1`) cut into `n`, as `(start, end)`,
/// end exclusive: `start = floor(s * len / n)`, `end = max(start + 1, floor((s + 1) * len / n))`.
///
/// Integer division, where the reader divides in doubles (`floor(s * len / (double)n)`): the
/// two agree whenever the numerator is below 2^53, because a quotient that is not a whole
/// number lies at least 1/n below the next one — far more than a double's error there. Since
/// `start < len`, `end` never passes the end of the axis, which is why the reader's
/// `min(end, regionEnd)` never cuts anything.
pub(crate) fn blocks(len: u32, n: u32) -> Vec<(u32, u32)> {
    (0..n as u64)
        .map(|s| {
            let start = (s * len as u64 / n as u64) as u32;
            let end = ((s + 1) * len as u64 / n as u64) as u32;
            (start, end.max(start + 1))
        })
        .collect()
}

/// `round(num / den)`, half to even, exactly. `den > 0`. The integer form of `cell_value`,
/// which the tests hold it to on every value where the two could differ.
#[cfg(test)]
pub(crate) fn round_half_even(num: u64, den: u64) -> u64 {
    let (q, r) = (num / den, num % den);
    match (2 * r).cmp(&den) {
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Equal => q + (q & 1),
    }
}

/// One cell's value, as the reader computes it: `(byte)Math.Round(255.0 * matching / total)`.
/// The multiplication is exact and the division correctly rounded, so a true half stays a half
/// and every other quotient is at least 1/(2 * total) away from one; `round_half_even` is the
/// same number in integers, which the tests hold it to.
pub(crate) fn cell_value(matching: u32, total: u32) -> u8 {
    (255.0 * matching as f64 / total as f64).round_ties_even() as u8
}

/// Per cell, row-major: how many pixels passed, and how many were looked at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Counts {
    pub matching: Vec<u32>,
    pub total: Vec<u32>,
}

/// Counts every cell of `sub` in `img`. Never panics: a part outside the image, or a buffer
/// shorter than the image, is an error.
///
/// Row by row: an image row's pixels are split into channel lanes, every condition is evaluated
/// over the whole row without a branch per pixel (so the cost does not depend on the picture),
/// and the passing pixels are added up per column block. When the region is lower than the
/// grid, consecutive block rows share an image row; its mask is kept rather than evaluated again.
pub(crate) fn count(img: &CapturedImage, sub: Sub, spec: &CellSpec) -> Result<Counts, CellsError> {
    if sub.w == 0 || sub.h == 0 {
        return Err(CellsError::Empty);
    }
    if sub.w as u64 * sub.h as u64 > u32::MAX as u64 {
        return Err(CellsError::TooLarge);
    }
    let (iw, ih) = (img.w as u64, img.h as u64);
    if sub.x as u64 + sub.w as u64 > iw || sub.y as u64 + sub.h as u64 > ih {
        return Err(CellsError::OutOfImage);
    }
    if (img.rgba.len() as u64) < iw * ih * 4 {
        return Err(CellsError::ShortBuffer);
    }
    let (cols, rows) = (spec.cols as usize, spec.rows as usize);
    let xb = blocks(sub.w, spec.cols);
    let yb = blocks(sub.h, spec.rows);
    let w = sub.w as usize;
    let mut matching = vec![0u32; cols * rows];
    let mut total = vec![0u32; cols * rows];
    let (mut r, mut g, mut b) = (vec![0i32; w], vec![0i32; w], vec![0i32; w]);
    let (mut any, mut all) = (vec![0u8; w], vec![0u8; w]);
    let mut masked: Option<u32> = None;
    for (sy, &(ys, ye)) in yb.iter().enumerate() {
        for y in ys..ye {
            if masked != Some(y) {
                let row0 = ((sub.y + y) as usize * img.w as usize + sub.x as usize) * 4;
                let px = &img.rgba[row0..row0 + w * 4];
                for (((p, r), g), b) in px.chunks_exact(4).zip(r.iter_mut()).zip(g.iter_mut()).zip(b.iter_mut()) {
                    *r = p[0] as i32;
                    *g = p[1] as i32;
                    *b = p[2] as i32;
                }
                any.fill(0);
                for alt in &spec.pred.alternatives {
                    all.fill(1);
                    for l in alt {
                        let (lr, lg, lb, lk) = (l.r, l.g, l.b, l.k);
                        for (((a, r), g), b) in all.iter_mut().zip(&r).zip(&g).zip(&b) {
                            *a &= (lr * r + lg * g + lb * b + lk >= 0) as u8;
                        }
                    }
                    for (a, s) in any.iter_mut().zip(&all) {
                        *a |= *s;
                    }
                }
                masked = Some(y);
            }
            for (sx, &(xs, xe)) in xb.iter().enumerate() {
                let n: u32 = any[xs as usize..xe as usize].iter().map(|&v| v as u32).sum();
                matching[sy * cols + sx] += n;
            }
        }
        for (sx, &(xs, xe)) in xb.iter().enumerate() {
            total[sy * cols + sx] = (xe - xs) * (ye - ys);
        }
    }
    Ok(Counts { matching, total })
}

/// The cells of `sub` in `img`, row-major, one byte each.
pub(crate) fn reduce(img: &CapturedImage, sub: Sub, spec: &CellSpec) -> Result<Vec<u8>, CellsError> {
    let c = count(img, sub, spec)?;
    Ok(c.matching.iter().zip(&c.total).map(|(&m, &t)| cell_value(m, t)).collect())
}

/// The cells of a whole capture that should be `w` x `h`, or why there are none.
pub(crate) fn of_capture(cap: &CapturedImage, w: i32, h: i32, spec: &CellSpec) -> Result<Vec<u8>, String> {
    if cap.w as i64 != w as i64 || cap.h as i64 != h as i64 {
        return Err(format!("the capture came back {}x{}, not the region's {w}x{h}", cap.w, cap.h));
    }
    reduce(cap, Sub { x: 0, y: 0, w: cap.w, h: cap.h }, spec).map_err(|e| e.to_string())
}

// ── Comparison ─────────────────────────────────────────────────────────────────────────

/// `sum |a - b|`.
pub(crate) fn distance(a: &[u8], b: &[u8]) -> u64 {
    a.iter().zip(b).map(|(&x, &y)| (x as i64 - y as i64).unsigned_abs()).sum()
}

/// `1 - distance / (length * 255.0)`: the reader's line, with the same IEEE operations —
/// the integer sum converted once, divided by the length times 255.0. 1.0 is identical; 0.0 is
/// every cell as far apart as it can be. Unequal or empty inputs are 0.0, as in his code.
pub(crate) fn similarity(a: &[u8], b: &[u8]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    1.0 - distance(a, b) as f64 / (a.len() as f64 * 255.0)
}

/// Which item each state belongs to: states that share a name are one item, and a state
/// without a name is an item of its own. Items are numbered in order of first appearance.
pub(crate) fn items_of(names: &[Option<String>]) -> Vec<usize> {
    let mut seen: Vec<&str> = Vec::new();
    let mut next = 0usize;
    let mut named: Vec<usize> = Vec::new();
    names
        .iter()
        .map(|n| match n {
            Some(n) => match seen.iter().position(|s| *s == n.as_str()) {
                Some(p) => named[p],
                None => {
                    seen.push(n.as_str());
                    named.push(next);
                    next += 1;
                    next - 1
                }
            },
            None => {
                next += 1;
                next - 1
            }
        })
        .collect()
}

/// The answer to "which stored state is this?".
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ranked {
    /// The winning state, 0-based: the best state of the best item.
    pub state: usize,
    pub similarity: f64,
    pub distance: u64,
    /// The best score among the OTHER items; 0.0 when there is no other item.
    pub runner_up: f64,
    /// Every state's similarity, in the order given.
    pub all: Vec<f64>,
}

/// Ranks `live` against `states`, grouped into items by `item_of` (from `items_of`).
///
/// Strict `>` everywhere, as in the reader: within an item the first of equal states is kept,
/// and among items the first of equal items wins. `None` only for an empty or inconsistent
/// list, which the bindings refuse before it could get here.
pub(crate) fn rank(live: &[u8], states: &[Box<[u8]>], item_of: &[usize]) -> Option<Ranked> {
    if states.is_empty() || item_of.len() != states.len() {
        return None;
    }
    let all: Vec<f64> = states.iter().map(|s| similarity(live, s)).collect();
    let n_items = item_of.iter().copied().max()? + 1;
    let mut best_of: Vec<Option<usize>> = vec![None; n_items];
    for (s, &it) in item_of.iter().enumerate() {
        match best_of[it] {
            Some(b) if all[s] <= all[b] => {}
            _ => best_of[it] = Some(s),
        }
    }
    let mut winner: Option<(usize, usize)> = None; // (item, state)
    for (it, best) in best_of.iter().enumerate() {
        let Some(s) = *best else { continue };
        match winner {
            Some((_, w)) if all[s] <= all[w] => {}
            _ => winner = Some((it, s)),
        }
    }
    let (wi, ws) = winner?;
    let runner_up = best_of
        .iter()
        .enumerate()
        .filter(|(it, _)| *it != wi)
        .filter_map(|(_, s)| s.map(|s| all[s]))
        .fold(0.0, f64::max);
    Some(Ranked { state: ws, similarity: all[ws], distance: distance(live, &states[ws]), runner_up, all })
}

/// A grid, its colour test and the states to rank against: one `matchCells` call's work, as
/// the image worker carries it.
#[derive(Debug)]
pub(crate) struct Matcher {
    pub spec: CellSpec,
    pub states: Vec<Box<[u8]>>,
    pub item_of: Vec<usize>,
}

impl Matcher {
    /// The live cells of a capture that should be `w` x `h`, and their ranking.
    pub(crate) fn answer(&self, cap: &CapturedImage, w: i32, h: i32) -> Result<(Vec<u8>, Ranked), String> {
        let live = of_capture(cap, w, h, &self.spec)?;
        let ranked = rank(&live, &self.states, &self.item_of).ok_or_else(|| "no states to compare".to_string())?;
        Ok((live, ranked))
    }
}

// ── Hex ────────────────────────────────────────────────────────────────────────────────

/// Lower-case hex, two digits a cell.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Why a stored state is not `cells` cells of hex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HexError {
    /// Exactly `cells` bytes that are not all hex digits: raw cell bytes, not hex.
    RawBytes { want: usize },
    /// The wrong number of characters. Only for ASCII, where characters and bytes are the same.
    Length { got: usize, want: usize },
    /// A character that is not a hex digit, at a 1-based position counted in characters.
    Digit { at: usize, ch: char },
    /// Bytes that are not UTF-8 text: the first that is not a hex digit, 1-based.
    Byte { at: usize, byte: u8 },
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HexError::RawBytes { want } => {
                write!(f, "states are hex ({want} characters), not the cells' raw bytes")
            }
            HexError::Length { got, want } => {
                write!(f, "{got} characters; this grid's states are {want} hex digits, two per cell")
            }
            HexError::Digit { at, ch } => write!(f, "{ch:?} at character {at} is not a hex digit"),
            HexError::Byte { at, byte } => {
                write!(f, "byte {at} (0x{byte:02X}) is not a hex digit, and the state is not text")
            }
        }
    }
}

/// `cells` bytes from hex of exactly `2 * cells` digits, either case.
///
/// Anything that is not ASCII is named by its first character that is not a hex digit, counted
/// in characters — a length or a position counted in bytes would not match what the author
/// sees — and by its byte when the state is not UTF-8 text at all.
pub(crate) fn from_hex(s: &[u8], cells: usize) -> Result<Vec<u8>, HexError> {
    let want = 2 * cells;
    if s.len() != want && s.len() == cells && !s.iter().all(u8::is_ascii_hexdigit) {
        return Err(HexError::RawBytes { want });
    }
    if !s.is_ascii() {
        if let Some(e) = first_non_hex(s) {
            return Err(e);
        }
    }
    if s.len() != want {
        return Err(HexError::Length { got: s.len(), want });
    }
    hex::decode(s).map_err(|e| match e {
        hex::FromHexError::InvalidHexCharacter { c, index } => HexError::Digit { at: index + 1, ch: c },
        // Cannot happen after the length check; answered rather than unwrapped.
        _ => HexError::Length { got: s.len(), want },
    })
}

/// The first character of `s` that is not a hex digit, counted in characters; or, when `s` is
/// not UTF-8, its first such byte. `None` only for all-hex input.
fn first_non_hex(s: &[u8]) -> Option<HexError> {
    match std::str::from_utf8(s) {
        Ok(text) => text
            .chars()
            .enumerate()
            .find(|(_, c)| !c.is_ascii_hexdigit())
            .map(|(n, ch)| HexError::Digit { at: n + 1, ch }),
        Err(_) => s
            .iter()
            .enumerate()
            .find(|(_, b)| !b.is_ascii_hexdigit())
            .map(|(n, &byte)| HexError::Byte { at: n + 1, byte }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outside reader's condition, as he wrote it in C#.
    const HIS: &str = "(red >= 80 && red*10 >= green*13 && red*10 >= blue*12) || \
                       (red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue)";

    /// The same condition written the Luau way, relying on `and` binding tighter than `or`.
    const LUAU: &str = "r >= 80 and 10*r >= 13*g and 10*r >= 12*b or \
                        r >= 100 and g >= 45 and b <= 150 and r >= g and g >= b";

    fn his(r: i32, g: i32, b: i32) -> bool {
        (r >= 80 && r * 10 >= g * 13 && r * 10 >= b * 12)
            || (r >= 100 && g >= 45 && b <= 150 && r >= g && g >= b)
    }

    fn lin(r: i32, g: i32, b: i32, k: i32) -> Lin {
        Lin { r, g, b, k }
    }

    fn eight_rows() -> Vec<Vec<Lin>> {
        vec![
            vec![lin(1, 0, 0, -80), lin(10, -13, 0, 0), lin(10, 0, -12, 0)],
            vec![lin(1, 0, 0, -100), lin(0, 1, 0, -45), lin(0, 0, -1, 150), lin(1, -1, 0, 0), lin(0, 1, -1, 0)],
        ]
    }

    fn err(src: &str) -> PredError {
        Predicate::parse(src).expect_err(src)
    }

    // ── 1. parsing his string, verbatim ────────────────────────────────────────────────

    #[test]
    fn his_csharp_and_the_luau_form_give_the_same_eight_rows() {
        let a = Predicate::parse(HIS).unwrap();
        let b = Predicate::parse(LUAU).unwrap();
        assert_eq!(a.alternatives(), eight_rows().as_slice());
        assert_eq!(b.alternatives(), eight_rows().as_slice());
        assert_eq!(
            a.to_string(),
            "(r >= 80 and 10*r - 13*g >= 0 and 10*r - 12*b >= 0) or \
             (r >= 100 and g >= 45 and b <= 150 and r - g >= 0 and g - b >= 0)"
        );
        // The canonical form parses back to the same thing, and prints the same.
        let again = Predicate::parse(&a.to_string()).unwrap();
        assert_eq!(again, a);
        assert_eq!(again.to_string(), a.to_string());
    }

    #[test]
    fn parentheses_precedence_and_spelling() {
        // Parentheses around single comparisons, as pasted C# often has them.
        let p = Predicate::parse("((r >= 80) && (g <= 3)) || (b == 7)").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(1, 0, 0, -80), lin(0, -1, 0, 3)], vec![lin(0, 0, 1, -7), lin(0, 0, -1, 7)]]);
        // `and` binds tighter than `or`...
        let p = Predicate::parse("r >= 1 or g >= 2 and b >= 3").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(1, 0, 0, -1)], vec![lin(0, 1, 0, -2), lin(0, 0, 1, -3)]]);
        // ...and parentheses change that, multiplied out.
        let p = Predicate::parse("(r >= 1 or g >= 2) and b >= 3").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(1, 0, 0, -1), lin(0, 0, 1, -3)], vec![lin(0, 1, 0, -2), lin(0, 0, 1, -3)]]);
        assert_eq!(p.to_string(), "(r >= 1 and b >= 3) or (g >= 2 and b >= 3)");
        // Whitespace anywhere, newlines included, and none at all.
        let p = Predicate::parse("\n  red*10>=green*13\t&&\r\nr>=80 ").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(10, -13, 0, 0), lin(1, 0, 0, -80)]]);
        // Terms on both sides, a leading minus and a constant on the left.
        let p = Predicate::parse("-r + 2*g - 3 >= b - 10").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(-1, 2, -1, 7)]]);
        assert_eq!(p.to_string(), "-r + 2*g - b >= -7");
        // All coefficients negative: printed turned around.
        let p = Predicate::parse("r + 2*g <= 5").unwrap();
        assert_eq!(p.alternatives(), &[vec![lin(-1, -2, 0, 5)]]);
        assert_eq!(p.to_string(), "r + 2*g <= 5");
        // A single alternative is printed without parentheses.
        assert_eq!(Predicate::parse("(r >= 1 && g >= 2)").unwrap().to_string(), "r >= 1 and g >= 2");
    }

    // ── 2–5. what the predicate means ──────────────────────────────────────────────────

    /// Every colour there is, against a Rust copy of his condition.
    #[test]
    fn every_colour_agrees_with_his_condition() {
        let p = Predicate::parse(HIS).unwrap();
        let mut warm = 0u32;
        for r in 0..=255u8 {
            for g in 0..=255u8 {
                for b in 0..=255u8 {
                    let want = his(r as i32, g as i32, b as i32);
                    assert_eq!(p.test(r, g, b), want, "({r}, {g}, {b})");
                    warm += want as u32;
                }
            }
        }
        assert_eq!(warm, 4_393_872);
    }

    #[test]
    fn the_boundaries() {
        let p = Predicate::parse(HIS).unwrap();
        for ((r, g, b), want) in [
            ((80, 0, 0), true),
            ((79, 0, 0), false),
            ((80, 61, 0), true),
            ((80, 62, 0), false),
            ((80, 0, 66), true),
            ((80, 0, 67), false),
            ((91, 70, 0), true), // 10r = 13g exactly
            ((91, 71, 0), false),
            ((90, 0, 75), true), // 10r = 12b exactly
            ((90, 0, 76), false),
            ((100, 90, 50), true), // clause 2 only
            ((99, 90, 50), false),
            ((170, 160, 150), true),
            ((170, 160, 151), false),
            ((160, 160, 100), true), // r = g
            ((159, 160, 100), false),
            ((170, 150, 150), true), // g = b
            ((170, 149, 150), false),
            ((255, 255, 0), true), // clause 2 only
            ((255, 255, 255), false),
            ((0, 0, 0), false),
        ] {
            assert_eq!(p.test(r, g, b), want, "({r}, {g}, {b})");
            assert_eq!(his(r as i32, g as i32, b as i32), want, "the table itself, ({r}, {g}, {b})");
        }
    }

    /// `g >= 45` never decides his combined condition, so it is checked on its own clause.
    #[test]
    fn the_green_floor_of_clause_two() {
        let p = Predicate::parse("red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue").unwrap();
        assert!(p.test(100, 45, 0));
        assert!(!p.test(100, 44, 0));
    }

    #[test]
    fn strict_and_equal_comparisons() {
        let gt = Predicate::parse("r > 79").unwrap();
        let ge = Predicate::parse("r >= 80").unwrap();
        let lt = Predicate::parse("r < 80").unwrap();
        for r in 0..=255u8 {
            assert_eq!(gt.test(r, 0, 0), ge.test(r, 0, 0));
            assert_eq!(lt.test(r, 0, 0), !ge.test(r, 0, 0));
        }
        assert_eq!(gt, ge, "normalised to the same condition");
        let eq = Predicate::parse("2*g == b").unwrap();
        assert_eq!(eq.alternatives(), &[vec![lin(0, 2, -1, 0), lin(0, -2, 1, 0)]]);
        assert!(eq.test(0, 10, 20) && !eq.test(0, 10, 21) && !eq.test(0, 10, 19));
        assert_eq!(Predicate::parse(&eq.to_string()).unwrap(), eq);
    }

    // ── 6. errors, with their columns ──────────────────────────────────────────────────

    #[test]
    fn mistakes_are_named_where_they_are() {
        let cases: &[(&str, usize, &str)] = &[
            ("", 1, "empty"),
            ("   ", 1, "empty"),
            ("r >= 80 and x > 3", 13, "'x' is not a channel; use r, g or b"),
            ("R >= 80", 1, "'R' is not a channel; channel names are lower-case"),
            ("r >= 1.3*g", 6, "'1.3' is not a whole number"),
            ("r >= 99999999999", 6, "'99999999999' is too large (the limit is 2147483647)"),
            ("r >= 99999999999999999999999", 6, "is too large"),
            // A leading zero: ten in C#, eight in C. The whole number is named, not its tail.
            ("r >= 080", 6, "'080' has a leading zero, which C and JavaScript can read as octal; write 80"),
            ("r >= 010 and g >= 1", 6, "'010' has a leading zero"),
            ("r >= 00", 6, "write 0"),
            ("r >= 01.5", 6, "'01.5' is not a whole number"),
            ("r*g >= 3", 1, "'r*g' multiplies two channels"),
            ("2 * 3 >= r", 1, "'2*3' multiplies two numbers; write the product (6)"),
            ("10*r and g >= 3", 6, "expected a comparison (>=, <=, >, <, ==) after \"10*r\", found 'and'"),
            ("r != 3", 3, "'!=' cannot be one condition"),
            ("r ~= 3", 3, "'~=' cannot be one condition"),
            ("r = 3", 3, "'=' is not a comparison"),
            ("r => 3", 3, "'=>' is written '>='"),
            ("not r >= 80", 1, "'not' is not supported"),
            ("(r >= 80 and g >= 3", 1, "this '(' is never closed"),
            ("r >= 80)", 8, "this ')' has no '(' to close"),
            ("80 <= r <= 120", 9, "cannot be chained"),
            ("80 >= 3", 1, "'80 >= 3' does not depend on r, g or b"),
            ("r - r >= 0", 1, "does not depend on r, g or b"),
            ("2000000*g >= 1", 1, "the coefficient of g in '2000000*g >= 1' is 2000000"),
            ("r >= 2000000000", 1, "the constant in 'r >= 2000000000' is -2000000000"),
            ("r >= 80 g >= 3", 9, "expected 'and', 'or' or the end, found 'g'"),
            ("r >= 80 and", 12, "expected r, g, b or a whole number, found the end"),
            ("r + >= 3", 5, "expected r, g, b or a whole number, found '>='"),
            ("r >= 3 & g >= 1", 8, "'&' is written '&&'"),
            ("r >= #", 6, "expected r, g, b or a whole number, found '#'"),
            ("(r >= 1 g)", 9, "expected 'and', 'or' or ')', found 'g'"),
            // Columns count characters, not bytes: 'ä' is two bytes.
            ("r >= 1 and ä >= 2", 12, "expected r, g, b or a whole number, found 'ä'"),
        ];
        for (src, col, text) in cases {
            let e = err(src);
            assert!(e.message.contains(text), "{src:?}: {}", e.message);
            assert_eq!(e.column, *col, "{src:?}: {}", e.message);
        }
        let long = format!("r >= 1{}", " ".repeat(MAX_SOURCE));
        assert!(err(&long).message.contains("the limit is 8192"));
        // Zero itself, and a zero coefficient, are fine.
        assert_eq!(Predicate::parse("r >= 0 and 0*g + b >= 0").unwrap().alternatives(), &[vec![lin(1, 0, 0, 0), lin(0, 0, 1, 0)]]);
    }

    /// Parentheses are the parser's only recursion, one level per `(`. Before the limit, 508 of
    /// them around one comparison fit in 1024 bytes and needed 2.1 MB of stack in a debug build,
    /// twice the event loop's 1 MB on Windows, and a stack overflow ends the process. Now the
    /// 33rd level is refused, so the deepest source there can be parses in a fraction of that —
    /// here on a thread with half of it.
    #[test]
    fn deep_parentheses_are_refused_before_they_use_the_stack() {
        let nested = |d: usize| format!("{}r >= 1{}", "(".repeat(d), ")".repeat(d));
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                let p = Predicate::parse(&nested(MAX_NESTING)).unwrap();
                assert_eq!(p.alternatives(), &[vec![lin(1, 0, 0, -1)]]);
                let e = err(&nested(MAX_NESTING + 1));
                assert_eq!(e.column, MAX_NESTING + 1, "at the parenthesis one too deep");
                assert!(e.message.contains("nested more than 32 deep"), "{}", e.message);
                // As deep as the length limit allows, and nothing but parentheses.
                let fill = (MAX_SOURCE - 6) / 2;
                assert_eq!(err(&nested(fill)).column, MAX_NESTING + 1);
                assert_eq!(err(&"(".repeat(MAX_SOURCE)).column, MAX_NESTING + 1);
                // Depth is nesting, not the number of groups: side by side is any number.
                let wide = vec!["(r >= 1)"; 8].join(" or ");
                assert_eq!(Predicate::parse(&wide).unwrap().alternatives().len(), 8);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn the_expansion_limits() {
        // Eight alternatives are fine, nine are not.
        let or = |n: usize| (0..n).map(|i| format!("r >= {i}")).collect::<Vec<_>>().join(" or ");
        assert_eq!(Predicate::parse(&or(8)).unwrap().alternatives().len(), 8);
        let e = err(&or(9));
        assert!(e.message.contains("expands to at least 9 alternatives; the limit is 8"), "{}", e.message);
        // Multiplied out: (2 alternatives) and (2) and (2) is 8; one more factor is 16.
        let f = "(r >= 1 or g >= 1)";
        assert_eq!(Predicate::parse(&[f; 3].join(" and ")).unwrap().alternatives().len(), 8);
        let e = err(&[f; 4].join(" and "));
        assert!(e.message.contains("expands to at least 16 alternatives"), "{}", e.message);
        assert_eq!(e.column, 3 * (f.len() + 5) + 1, "at the factor that tipped it");
        // Sixteen conditions in one alternative are fine, seventeen are not; == counts as two.
        let and = |n: usize| (0..n).map(|i| format!("r >= {i}")).collect::<Vec<_>>().join(" and ");
        assert_eq!(Predicate::parse(&and(16)).unwrap().alternatives()[0].len(), 16);
        assert!(err(&and(17)).message.contains("17 conditions"));
        assert!(err(&format!("{} and g == 3", and(15))).message.contains("17 conditions"));
        // The bounds themselves are allowed.
        assert!(Predicate::parse("1000000*r - 1000000*g >= 1000000000").is_ok());
        assert!(Predicate::parse("1000001*r >= 0").is_err());
    }

    /// Multiplying out makes the canonical form longer than its source; it must still be
    /// accepted, or `host.screen.predicate` would hand back a string the cells calls refuse.
    #[test]
    fn every_canonical_form_parses_back() {
        let back = |src: &str| {
            let p = Predicate::parse(src).unwrap();
            let canonical = p.to_string();
            let again = Predicate::parse(&canonical).unwrap_or_else(|e| panic!("{canonical}: {e}"));
            assert_eq!(again, p);
            assert_eq!(again.to_string(), canonical);
            (p, canonical)
        };
        // Found in review: 125 bytes of source, 1468 of canonical form.
        let src = format!(
            "(r>=1||g>=1)&&(r>=2||g>=2)&&(r>=3||g>=3)&&{}",
            (4..=16).map(|k| format!("b>={k}")).collect::<Vec<_>>().join("&&")
        );
        assert_eq!(src.len(), 125);
        let (p, canonical) = back(&src);
        assert_eq!((p.alternatives().len(), p.alternatives()[0].len()), (8, 16));
        assert_eq!(canonical.len(), 1468);
        // The longest there can be: 8 alternatives of 16 comparisons, each at the coefficient
        // and constant limits with every sign printed, from a source of 1024 bytes.
        let c = "-1000000*r + 1000000*g - 1000000*b >= -1000000000";
        assert_eq!(c.len(), 49);
        let factor = format!("({c} or {c})");
        let src = [vec![factor.as_str(); 3], vec![c; 13]].concat().join(" and ");
        assert_eq!(src.len(), 1024);
        let (p, canonical) = back(&src);
        assert_eq!((p.alternatives().len(), p.alternatives()[0].len()), (8, 16));
        assert_eq!(canonical.len(), 8 * (16 * 49 + 15 * 5 + 2) + 7 * 4);
        assert_eq!(canonical.len(), 6916);
        assert!(canonical.len() <= MAX_SOURCE);
    }

    // ── 7. rounding ────────────────────────────────────────────────────────────────────

    #[test]
    fn half_to_even_and_the_double_line_agree() {
        for ((m, t), want) in [
            ((1, 6), 42),
            ((1, 2), 128),
            ((3, 6), 128),
            ((24, 720), 8),
            ((5, 6), 212),
            ((1, 7), 36),
            ((2, 7), 73),
            ((0, 5), 0),
            ((5, 5), 255),
        ] {
            assert_eq!(round_half_even(255 * m as u64, t as u64), want, "{m}/{t}");
            assert_eq!(cell_value(m, t), want as u8, "{m}/{t}");
        }
        // Every m for every total up to 2000.
        for t in 1..=2000u32 {
            for m in 0..=t {
                assert_eq!(cell_value(m, t) as u64, round_half_even(255 * m as u64, t as u64), "{m}/{t}");
            }
        }
    }

    /// Every exact half (255*m mod t == t/2) for totals up to 200,000 — the only places the
    /// two could differ. 582,266 of them.
    #[test]
    fn every_midpoint_up_to_200000() {
        fn inverse(a: i64, m: i64) -> i64 {
            let (mut old_r, mut r) = (a, m);
            let (mut old_s, mut s) = (1i64, 0i64);
            while r != 0 {
                let q = old_r / r;
                (old_r, r) = (r, old_r - q * r);
                (old_s, s) = (s, old_s - q * s);
            }
            old_s.rem_euclid(m)
        }
        fn gcd(a: u64, b: u64) -> u64 {
            if b == 0 {
                a
            } else {
                gcd(b, a % b)
            }
        }
        let mut n = 0u32;
        for t in (2..=200_000u64).step_by(2) {
            let half = t / 2;
            let g = gcd(255, t);
            if half % g != 0 {
                continue;
            }
            let tp = t / g;
            let m0 = if tp == 1 { 0 } else { ((half / g) % tp * inverse(((255 / g) % tp) as i64, tp as i64) as u64) % tp };
            let mut m = m0;
            while m <= t {
                assert_eq!((255 * m) % t, half);
                assert_eq!(cell_value(m as u32, t as u32) as u64, round_half_even(255 * m, t), "{m}/{t}");
                n += 1;
                m += tp;
            }
        }
        assert_eq!(n, 582_266);
    }

    // ── 8. blocks ──────────────────────────────────────────────────────────────────────

    #[test]
    fn blocks_are_floor_edges_at_least_one_pixel_wide() {
        let edges = |b: Vec<(u32, u32)>| -> Vec<u32> {
            let mut e: Vec<u32> = b.iter().map(|p| p.0).collect();
            e.push(b.last().unwrap().1);
            e
        };
        assert_eq!(edges(blocks(103, 10)), vec![0, 10, 20, 30, 41, 51, 61, 72, 82, 92, 103]);
        assert_eq!(edges(blocks(25, 10)), vec![0, 2, 5, 7, 10, 12, 15, 17, 20, 22, 25]);
        assert!(blocks(360, 10).iter().all(|&(s, e)| e - s == 36));
        // Smaller than the grid: every block one pixel, neighbours sharing it.
        assert_eq!(blocks(5, 10), vec![(0, 1), (0, 1), (1, 2), (1, 2), (2, 3), (2, 3), (3, 4), (3, 4), (4, 5), (4, 5)]);
        assert_eq!(blocks(3, 10), vec![(0, 1), (0, 1), (0, 1), (0, 1), (1, 2), (1, 2), (1, 2), (2, 3), (2, 3), (2, 3)]);
        assert_eq!(blocks(1, 4), vec![(0, 1); 4]);
        for len in 1..300u32 {
            for n in 1..=40u32 {
                let b = blocks(len, n);
                assert_eq!(b.len(), n as usize);
                assert!(b.iter().all(|&(s, e)| s < e && e <= len), "{len}/{n}");
                if len >= n {
                    let (lo, hi) = (len / n, len.div_ceil(n));
                    assert!(b.iter().all(|&(s, e)| e - s == lo || e - s == hi), "{len}/{n}");
                    assert!(b.windows(2).all(|w| w[0].1 == w[1].0), "no gaps and no overlap, {len}/{n}");
                    assert_eq!((b[0].0, b[n as usize - 1].1), (0, len));
                } else {
                    assert!(b.iter().all(|&(s, e)| e - s == 1), "{len}/{n}");
                }
            }
        }
    }

    // ── 9. reduce against a naive reference ────────────────────────────────────────────

    fn xorshift(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    fn image(w: u32, h: u32, seed: &mut u64) -> CapturedImage {
        // Channels skewed warm, so the cells are neither all 0 nor all 255.
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            let v = xorshift(seed);
            rgba.extend_from_slice(&[(v >> 8) as u8 | 0x40, (v >> 20) as u8 >> 1, (v >> 32) as u8 >> 1, (v >> 44) as u8]);
        }
        CapturedImage { w, h, rgba }
    }

    /// His loop, pixel by pixel.
    fn naive(img: &CapturedImage, sub: Sub, spec: &CellSpec) -> (Vec<u8>, Counts) {
        let (mut out, mut matching, mut total) = (Vec::new(), Vec::new(), Vec::new());
        for sy in 0..spec.rows as u64 {
            let ys = sy * sub.h as u64 / spec.rows as u64;
            let ye = ((sy + 1) * sub.h as u64 / spec.rows as u64).max(ys + 1).min(sub.h as u64);
            for sx in 0..spec.cols as u64 {
                let xs = sx * sub.w as u64 / spec.cols as u64;
                let xe = ((sx + 1) * sub.w as u64 / spec.cols as u64).max(xs + 1).min(sub.w as u64);
                let (mut m, mut t) = (0u32, 0u32);
                for y in ys..ye {
                    for x in xs..xe {
                        let o = (((sub.y as u64 + y) * img.w as u64 + sub.x as u64 + x) * 4) as usize;
                        t += 1;
                        m += spec.pred.test(img.rgba[o], img.rgba[o + 1], img.rgba[o + 2]) as u32;
                    }
                }
                out.push(round_half_even(255 * m as u64, t as u64) as u8);
                matching.push(m);
                total.push(t);
            }
        }
        (out, Counts { matching, total })
    }

    #[test]
    fn reduce_equals_the_readers_loop() {
        let spec = CellSpec::new(10, 36, Predicate::parse(HIS).unwrap()).unwrap();
        let odd = CellSpec::new(7, 3, Predicate::parse("r > g or b == 0").unwrap()).unwrap();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        for w in (1..=40).step_by(3) {
            for h in (1..=40).step_by(4) {
                let img = image(w, h, &mut seed);
                let all = Sub { x: 0, y: 0, w, h };
                for s in [&spec, &odd] {
                    let (want, counts) = naive(&img, all, s);
                    assert_eq!(count(&img, all, s).unwrap(), counts, "{w}x{h}");
                    assert_eq!(reduce(&img, all, s).unwrap(), want, "{w}x{h}");
                }
            }
        }
    }

    /// The same pixels inside a larger image give the same cells: what a region cut from a
    /// held frame will rely on.
    #[test]
    fn a_sub_rectangle_reads_the_same_as_its_own_image() {
        let spec = CellSpec::new(10, 36, Predicate::parse(HIS).unwrap()).unwrap();
        let mut seed = 42u64;
        let small = image(23, 17, &mut seed);
        let mut big = image(64, 64, &mut seed);
        for y in 0..17usize {
            let from = y * 23 * 4;
            let to = ((5 + y) * 64 + 7) * 4;
            big.rgba[to..to + 23 * 4].copy_from_slice(&small.rgba[from..from + 23 * 4]);
        }
        assert_eq!(
            reduce(&big, Sub { x: 7, y: 5, w: 23, h: 17 }, &spec).unwrap(),
            reduce(&small, Sub { x: 0, y: 0, w: 23, h: 17 }, &spec).unwrap()
        );
    }

    #[test]
    fn a_bad_image_is_an_error_not_a_panic() {
        let spec = CellSpec::new(2, 2, Predicate::parse("r >= 1").unwrap()).unwrap();
        let img = CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
        assert_eq!(reduce(&img, Sub { x: 2, y: 0, w: 3, h: 4 }, &spec), Err(CellsError::OutOfImage));
        assert_eq!(reduce(&img, Sub { x: 0, y: 0, w: 0, h: 4 }, &spec), Err(CellsError::Empty));
        let short = CapturedImage { w: 4, h: 4, rgba: vec![0; 63] };
        assert_eq!(reduce(&short, Sub { x: 0, y: 0, w: 4, h: 4 }, &spec), Err(CellsError::ShortBuffer));
        let e = of_capture(&img, 4, 5, &spec).unwrap_err();
        assert!(e.contains("came back 4x4, not the region's 4x5"), "{e}");
        assert_eq!(of_capture(&img, 4, 4, &spec).unwrap(), vec![0; 4]);
        assert!(CellSpec::new(0, 3, Predicate::parse("r >= 1").unwrap()).unwrap_err().contains("cols is 0"));
        assert!(CellSpec::new(3, 257, Predicate::parse("r >= 1").unwrap()).unwrap_err().contains("rows is 257"));
        assert!(CellSpec::new(100, 41, Predicate::parse("r >= 1").unwrap()).unwrap_err().contains("4100 cells"));
    }

    // ── 10. ranking ────────────────────────────────────────────────────────────────────

    fn boxed(v: &[&[u8]]) -> Vec<Box<[u8]>> {
        v.iter().map(|s| s.to_vec().into_boxed_slice()).collect()
    }

    #[test]
    fn similarity_is_his_line() {
        let zero = vec![0u8; 360];
        let full = vec![255u8; 360];
        assert_eq!(similarity(&zero, &zero), 1.0);
        assert_eq!(similarity(&zero, &full), 0.0);
        let mut one = zero.clone();
        one[17] = 1;
        assert_eq!(similarity(&zero, &one), 1.0 - 1.0 / 91800.0);
        assert_eq!(distance(&zero, &one), 1);
        assert_eq!(similarity(&zero, &zero[..10]), 0.0, "unequal lengths");
        assert_eq!(similarity(&[], &[]), 0.0, "empty");
    }

    #[test]
    fn items_group_states_by_name() {
        let n = |s: &str| Some(s.to_string());
        assert_eq!(items_of(&[n("Start"), None, n("Options"), n("Start"), None]), vec![0, 1, 2, 0, 3]);
        assert_eq!(items_of(&[]), Vec::<usize>::new());
    }

    #[test]
    fn the_winner_the_runner_up_and_ties() {
        let live: &[u8] = &[10, 10, 10, 10];
        // Item 0 = states 0 and 2, item 1 = state 1, item 2 = state 3.
        let states = boxed(&[&[10, 10, 10, 14], &[10, 10, 10, 12], &[10, 10, 10, 10], &[0, 10, 10, 10]]);
        let r = rank(live, &states, &[0, 1, 0, 2]).unwrap();
        assert_eq!((r.state, r.similarity, r.distance), (2, 1.0, 0));
        assert_eq!(r.runner_up, 1.0 - 2.0 / 1020.0, "item 1's score");
        assert_eq!(r.all.len(), 4);
        // Equal items: the first wins, and the runner-up equals the winner.
        let same = boxed(&[&[10, 10, 10, 11], &[10, 10, 10, 9]]);
        let r = rank(live, &same, &[0, 1]).unwrap();
        assert_eq!((r.state, r.runner_up), (0, r.similarity));
        // Equal states inside one item: the first is kept.
        let r = rank(live, &same, &[0, 0]).unwrap();
        assert_eq!((r.state, r.runner_up), (0, 0.0), "no other item: runner-up 0");
        // Refused shapes answer None.
        assert!(rank(live, &[], &[]).is_none());
        assert!(rank(live, &same, &[0]).is_none());
    }

    // ── 11. hex ────────────────────────────────────────────────────────────────────────

    #[test]
    fn hex_round_trips_and_names_its_mistakes() {
        let bytes: Vec<u8> = (0..=255u8).collect();
        let h = to_hex(&bytes);
        assert_eq!(&h[..8], "00010203");
        assert_eq!(from_hex(h.as_bytes(), 256).unwrap(), bytes);
        assert_eq!(from_hex(h.to_uppercase().as_bytes(), 256).unwrap(), bytes, "either case");
        assert_eq!(from_hex(b"0a0", 2), Err(HexError::Length { got: 3, want: 4 }));
        assert_eq!(from_hex(b"0g00", 2), Err(HexError::Digit { at: 2, ch: 'g' }));
        assert_eq!(from_hex(&[0x80, 0x01], 2), Err(HexError::RawBytes { want: 4 }));
        assert!(HexError::RawBytes { want: 720 }.to_string().contains("states are hex (720 characters)"));
        // Two hex digits a cell: 360 digits of hex is a length error, not raw bytes.
        assert_eq!(from_hex("ab".repeat(180).as_bytes(), 360), Err(HexError::Length { got: 360, want: 720 }));
        // Text that is not ASCII is named by its character, counted in characters, whatever
        // its length in bytes — not as the first byte of it read as Latin-1, 'Ã'.
        let e = from_hex("é".as_bytes(), 1).unwrap_err();
        assert_eq!(e, HexError::Digit { at: 1, ch: 'é' });
        assert_eq!(e.to_string(), "'é' at character 1 is not a hex digit");
        assert_eq!(from_hex("é0".as_bytes(), 1), Err(HexError::Digit { at: 1, ch: 'é' }), "not '3 characters'");
        assert_eq!(from_hex("0aé".as_bytes(), 2), Err(HexError::Digit { at: 3, ch: 'é' }));
        assert_eq!(from_hex("äg00".as_bytes(), 2), Err(HexError::Digit { at: 1, ch: 'ä' }));
        assert_eq!(from_hex("0gä0".as_bytes(), 2), Err(HexError::Digit { at: 2, ch: 'g' }), "the first, ASCII or not");
        // Bytes that are not text at all are named by the byte.
        let e = from_hex(&[b'0', b'a', 0xFF, b'1', b'2'], 2).unwrap_err();
        assert_eq!(e, HexError::Byte { at: 3, byte: 0xFF });
        assert_eq!(e.to_string(), "byte 3 (0xFF) is not a hex digit, and the state is not text");
    }
}
