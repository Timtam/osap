//! `host.screen.save` and `saveMarked`: a region of the screen — or of a snapshot — written as a
//! PNG, with a crosshair at every mark for `saveMarked`. The calibration calls: the picture goes to
//! somebody who can look at it.
//!
//! **In a handler they wait** (`task.rs`): the image worker captures the region — on a snapshot,
//! nothing — and its matching pool copies the picture, or the part of the snapshot the region
//! covers, draws the marks, encodes the PNG and writes it (`image_search::Job::Save`); the handler
//! goes on with `true`, or `false` and the reason. A shot nobody waits for any more once its PNG is
//! made — its module disabled, reloaded, stopped or removed, its handler cancelled — is not written
//! ([`write_shot`]'s `wanted`). A file that cannot be written raises at the call, after the wait.
//! **Where it cannot wait** the call raises (`task::wait_message`), and nothing is captured or
//! written. A region with nothing to read this time — a window region with no rectangle, or on a
//! snapshot one with no part inside it — is answered at once wherever it is asked.
//!
//! **With a callback** — `save(path, opts, cb)` — the same shot is filed with the callback as its
//! waiter and nothing waits: `cb(true)`, or `cb(false, reason)`, then `info`, on a later tick; a
//! file that cannot be written is reported as the callback's error, which is not called — the call
//! raises for it in every other form.
//!
//! Its own file, as the profile's bindings are snapshot.rs's: the bindings are registered in lib.rs,
//! by name, where check-docs.ps1 and tools/api-index.py read what exists.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::frame::Frame;
use crate::backend::CapturedImage;
use crate::image_search::{self, BuildAnswer, Haystack, Job, JobKind, Outcome, Plan};
use crate::ocr::lua::ReadHost;
use crate::vm_guard::host_call;
use crate::{describe_value, one_value, snapshot, with_reason, Shared, SHORT_CAPTURE};

/// What a shot is answered when the host's own code failed it: its work panicked, or the worker
/// was gone when it was sent.
pub(crate) const SAVE_FAILED: &str = "internal error: the picture could not be saved";

/// The two calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shot {
    /// `host.screen.save`.
    Plain,
    /// `host.screen.saveMarked`.
    Marked,
}

impl Shot {
    /// The call, as its messages and log lines name it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Shot::Plain => "host.screen.save",
            Shot::Marked => "host.screen.saveMarked",
        }
    }

    /// The call as a file it could not write names it, as it always has — `screen.save 'C:\…':
    /// …` — and as a stop of its VM while its answer is built does.
    pub(crate) fn short(self) -> &'static str {
        match self {
            Shot::Plain => "screen.save",
            Shot::Marked => "screen.saveMarked",
        }
    }
}

/// What the image worker does with a shot's picture once it has it: the marks to draw, and where
/// to write it.
pub(crate) struct SaveJob {
    /// The file: the module's root joined to the path asked, or the path itself when absolute.
    pub(crate) path: PathBuf,
    /// The crosshairs, in screen coordinates; none for `save`.
    pub(crate) marks: Vec<(i32, i32)>,
    /// Which call asked.
    pub(crate) shot: Shot,
}

/// Why a shot was not written.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Unsaved {
    /// There was no picture to write: the reason, answered `false, reason`.
    Unread(String),
    /// The file could not be written: the message the call raises.
    Unwritten(String),
}

/// What a call of `save` or `saveMarked` asks, its arguments read ([`save_ask`]).
pub(crate) struct SaveAsk {
    path: PathBuf,
    /// The rectangle to read — on a snapshot already cut to it — or why there is nothing to read
    /// this time, answered at once.
    region: Result<(i32, i32, i32, i32), String>,
    snap: Option<Arc<Frame>>,
    marks: Vec<(i32, i32)>,
    /// `opts.key`, which only the callback form takes.
    key: Option<String>,
}

/// `save` and `saveMarked` write PNG only, and refuse any other path before anything is read:
/// a wrong extension is the caller's mistake whatever the screen holds, so it raises even when
/// the capture would have failed, as every other argument of theirs does. Matched without
/// regard to case, as the image encoder matched it when it was the one to refuse.
pub(crate) fn png_path_only(fname: &str, full: &Path) -> mlua::Result<()> {
    if full.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Ok(());
    }
    Err(mlua::Error::external(format!(
        "{fname}: '{}' does not end in .png; PNG is the one image format the host writes",
        full.display()
    )))
}

/// A shot's arguments, read as the calls have always read them and before anything is read of the
/// screen: the path — a string, joined to `root` — and `opts`, optional for `save` and a table for
/// `saveMarked`; the path's extension; `opts.snapshot`; `opts.region`, resolved now against a primary
/// screen of `screen`'s size, or with the snapshot's edges as its defaults and cut to the snapshot;
/// and `saveMarked`'s marks, read loosely as they always were: an entry that is not a table is
/// passed over, and a coordinate that is not a whole number is 0; then `opts.key`, which only the
/// callback form (`cb`) takes. A mistake raises; a region with nothing to read this time is
/// answered.
fn save_ask(lua: &Lua, shot: Shot, root: &Path, screen: (i32, i32), path: Value, opts: Value, cb: bool) -> mlua::Result<SaveAsk> {
    let fname = shot.name();
    let err = |m: String| mlua::Error::external(format!("{fname}: {m}"));
    let path = <String as mlua::FromLua>::from_lua(path.clone(), lua)
        .map_err(|_| err(format!("the path must be a string, got {}", describe_value(&path))))?;
    let opts = match (shot, opts) {
        (_, Value::Table(t)) => Some(t),
        (Shot::Plain, Value::Nil) => None,
        (Shot::Plain, other) => return Err(err(format!("opts must be a table, got {}", describe_value(&other)))),
        (Shot::Marked, other) => {
            return Err(err(format!("opts must be a table — pass at least {{}} — got {}", describe_value(&other))))
        }
    };
    let full = root.join(&path);
    png_path_only(fname, &full)?;
    let o = opts.as_ref();
    let snap = snapshot::from_opts(o, fname)?;
    // A window region is resolved to its screen rectangle first; the marks stay in screen
    // coordinates whichever form the region takes. On a snapshot, the part of the region inside
    // it: the marks are then placed relative to that part's corner.
    let region = match &snap {
        Some(f) => snapshot::opts_region_in(o, fname, f)?.and_then(|r| snapshot::clip(f, r)).map(|r| r.tuple()),
        None => {
            let v = match o {
                Some(t) => t.get::<Value>("region")?,
                None => Value::Nil,
            };
            crate::region_arg_on(screen, &v, fname, "opts.region")?.map(|r| (r.x, r.y, r.w, r.h))
        }
    };
    let marks = match (shot, o) {
        (Shot::Marked, Some(t)) => read_marks(lua, t)?,
        _ => Vec::new(),
    };
    let key = snapshot::opts_key(o, fname, cb, "the third argument")?;
    Ok(SaveAsk { path: full, region, snap, marks, key })
}

/// `opts.marks` of `saveMarked`, as it has always been read: by name, `{ x = 100, y = 200 }`, an
/// entry that is not a table passed over and a coordinate that is not a whole number read as 0.
fn read_marks(lua: &Lua, opts: &Table) -> mlua::Result<Vec<(i32, i32)>> {
    let marks: Table = opts.get("marks").unwrap_or(lua.create_table()?);
    let mut out = Vec::new();
    for m in marks.sequence_values::<Table>() {
        let Ok(m) = m else { continue };
        out.push((m.get("x").unwrap_or(0), m.get("y").unwrap_or(0)));
    }
    Ok(out)
}

/// What a shot nobody waits for any more answers, to nobody: the answer is let go when it comes.
pub(crate) const NOT_WAITED: &str = "nobody waits for the shot any more";

/// Draws the marks on a shot of the region whose top-left is `(rx, ry)` on screen, encodes it as a
/// PNG and writes it to `path`, its folder made first — unless `wanted` says, once the PNG is made,
/// that nobody waits for it any more: then neither the folder nor the file is made. The work of a
/// shot, on the image worker's matching pool. `false`'s reason for a
/// picture shorter than its own size; the message `shot` raises for a file it could not write.
///
/// Each mark a magenta crosshair 19 pixels across — not a colour these dark plug-in interfaces use,
/// so it cannot be mistaken for part of them — and the nth mark n dots on a row 12 pixels below its
/// centre, so marks stay tellable apart without text. A mark outside the shot is passed over.
pub(crate) fn write_shot(
    cap: CapturedImage,
    (rx, ry): (i32, i32),
    marks: &[(i32, i32)],
    path: &Path,
    shot: Shot,
    wanted: &dyn Fn() -> bool,
) -> Result<(), Unsaved> {
    let (w, h) = (cap.w as i32, cap.h as i32);
    let Some(mut img) = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba) else {
        return Err(Unsaved::Unread(SHORT_CAPTURE.to_string()));
    };
    let ink = image::Rgba([255u8, 0, 255, 255]);
    for (n, &(mx, my)) in marks.iter().enumerate() {
        // Region-relative, and only what is actually inside the shot.
        let (cx, cy) = (mx - rx, my - ry);
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            continue;
        }
        for d in -9i32..=9 {
            for (px, py) in [(cx + d, cy), (cx, cy + d)] {
                if px >= 0 && py >= 0 && px < w && py < h {
                    img.put_pixel(px as u32, py as u32, ink);
                }
            }
        }
        for t in 0..=(n as i32) {
            let (px, py) = (cx - (n as i32) + 2 * t, cy + 12);
            if px >= 0 && py >= 0 && px < w && py < h {
                img.put_pixel(px as u32, py as u32, ink);
            }
        }
    }
    let unwritten = |e: &dyn std::fmt::Display| Unsaved::Unwritten(format!("{} '{}': {e}", shot.short(), path.display()));
    // Encoded before anything is put on disk, and asked again after: the encode is the slow part,
    // and a module disabled, reloaded or removed meanwhile — its folder deleted, perhaps — gets no
    // file and no folder made again behind it.
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).map_err(|e| unwritten(&e))?;
    if !wanted() {
        return Err(Unsaved::Unread(NOT_WAITED.to_string()));
    }
    // Create the folder: writing the file does not, and "the system cannot find the path" is a poor
    // answer to "put a screenshot here".
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, &png).map_err(|e| unwritten(&e))
}

/// What a shot answers — to a handler that waited and to a callback alike: `true`, one value;
/// `false` and the reason when there was nothing to write; and a file that could not be written
/// raises, so a typo in a folder's name is not mistaken for a failed capture.
pub(crate) fn save_values(lua: &Lua, outcome: Outcome) -> mlua::Result<MultiValue> {
    match outcome {
        Outcome::Save(Ok(())) => one_value(Value::Boolean(true)),
        Outcome::Save(Err(Unsaved::Unread(why))) => with_reason(lua, Value::Boolean(false), why),
        Outcome::Save(Err(Unsaved::Unwritten(message))) => Err(mlua::Error::external(message)),
        // Never: the worker answers a shot with a shot.
        _ => with_reason(lua, Value::Boolean(false), SAVE_FAILED.to_string()),
    }
}

/// The work of a shot of `region`: the region of the screen captured — or the part of its snapshot
/// it covers — marked and written, the module's first-read comparison with it when it is owed.
fn save_work(lua: &Lua, shot: Shot, ask: SaveAsk, region: (i32, i32, i32, i32)) -> (Haystack, Job) {
    let SaveAsk { path, snap, marks, .. } = ask;
    let (hay, _) = image_search::haystack(lua, snap, Ok(region));
    (hay, Job::Save(Arc::new(SaveJob { path, marks, shot })))
}

/// A shot's plan: answered at once when there is nothing to read this time; otherwise a shot of
/// the image worker, answered by [`save_values`].
fn save_plan(lua: &Lua, shot: Shot, ask: SaveAsk) -> mlua::Result<Plan> {
    let region = match &ask.region {
        Ok(r) => *r,
        // Nothing to read this time: answered at once, wherever the call is made.
        Err(none) => return Ok(Plan::Now(with_reason(lua, Value::Boolean(false), none.clone())?, Box::new(save_values))),
    };
    let (hay, job) = save_work(lua, shot, ask, region);
    let answer: BuildAnswer = Box::new(save_values);
    Ok(Plan::Work { hay, job, answer })
}

/// A waited `save` or `saveMarked`: its arguments read by `parse` — a mistake raises at the call —
/// and answered at once when there is nothing to read this time; otherwise a shot of the image
/// worker filed for the handler that waits.
fn start_save<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    shot: Shot,
    nonce: Table,
    parse: impl FnOnce() -> mlua::Result<SaveAsk>,
) -> mlua::Result<MultiValue> {
    image_search::start_plan(h, lua, scope, shot.name(), shot.short(), nonce, || save_plan(lua, shot, parse()?))
}

/// `save(path, opts, cb)` or `saveMarked(path, opts, cb)`: the same shot with `cb` as its waiter,
/// under `opts.key` — it never waits — answered on a later tick.
fn submit_save<H: ReadHost>(h: &H, lua: &Lua, scope: usize, shot: Shot, cb: Value, parse: impl FnOnce() -> mlua::Result<SaveAsk>) -> mlua::Result<()> {
    let cb = image_search::callback_arg(shot.name(), cb, "third argument")?;
    let mut ask = parse()?;
    let key = ask.key.take();
    let plan = save_plan(lua, shot, ask)?;
    image_search::submit_plan(h, lua, scope, cb, key, shot.short(), JobKind::Save, plan)
}

/// `host.screen.save(path, opts?, cb?) -> (boolean, string?)`. In a handler it waits while the
/// image worker captures, encodes and writes ([`start_save`]); where it cannot wait it raises,
/// unless there is nothing to read this time (`image_search::now_plan`); with a callback nothing
/// waits ([`submit_save`]).
pub(crate) fn save(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    let start = lua.create_function(move |lua, (nonce, path, opts): (Table, Value, Value)| {
        host_call!("host.screen.save");
        let root = sh.root(idx);
        start_save(&*sh, lua, idx, Shot::Plain, nonce, || save_ask(lua, Shot::Plain, &root, sh.backend.screen_size(), path, opts, false))
    })?;
    let sh = shared.clone();
    let now = lua.create_function(move |lua, (path, opts): (Value, Value)| {
        host_call!("host.screen.save");
        let ask = save_ask(lua, Shot::Plain, &sh.root(idx), sh.backend.screen_size(), path, opts, false)?;
        Ok(image_search::now_plan(save_plan(lua, Shot::Plain, ask)?))
    })?;
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (path, opts, cb): (Value, Value, Value)| {
        host_call!("host.screen.save");
        let root = sh.root(idx);
        submit_save(&*sh, lua, idx, Shot::Plain, cb, || save_ask(lua, Shot::Plain, &root, sh.backend.screen_size(), path, opts, true))
    })?;
    crate::task::points(lua, idx, shared.clone())?.call((Shot::Plain.name(), start, now, submit, 3))
}

/// `host.screen.saveMarked(path, opts) -> (boolean, string?)`: `save`, with a crosshair drawn at every
/// point in `opts.marks`, in screen coordinates. It waits in a handler and raises where it cannot
/// wait, as `save` does.
///
/// This is the calibration question in one picture: "is my control on its button?" Reading a
/// control's coordinates out of a log and finding that spot in a screenshot by hand is the step that
/// hides errors — a set of toggles in this repo sat 16 px off, on the caption row under the
/// buttons, for as long as it existed, because the colours sampled there happened to look
/// plausible. A crosshair drawn where the click will actually land makes that a glance instead of
/// an arithmetic exercise.
pub(crate) fn save_marked(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    let start = lua.create_function(move |lua, (nonce, path, opts): (Table, Value, Value)| {
        host_call!("host.screen.saveMarked");
        let root = sh.root(idx);
        start_save(&*sh, lua, idx, Shot::Marked, nonce, || save_ask(lua, Shot::Marked, &root, sh.backend.screen_size(), path, opts, false))
    })?;
    let sh = shared.clone();
    let now = lua.create_function(move |lua, (path, opts): (Value, Value)| {
        host_call!("host.screen.saveMarked");
        let ask = save_ask(lua, Shot::Marked, &sh.root(idx), sh.backend.screen_size(), path, opts, false)?;
        Ok(image_search::now_plan(save_plan(lua, Shot::Marked, ask)?))
    })?;
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (path, opts, cb): (Value, Value, Value)| {
        host_call!("host.screen.saveMarked");
        let root = sh.root(idx);
        submit_save(&*sh, lua, idx, Shot::Marked, cb, || save_ask(lua, Shot::Marked, &root, sh.backend.screen_size(), path, opts, true))
    })?;
    crate::task::points(lua, idx, shared.clone())?.call((Shot::Marked.name(), start, now, submit, 3))
}

/// `save` and `saveMarked` as the host builds them, over `holder`, their waits and where they cannot
/// wait the host's own ([`start_save`], `image_search::now_plan`), and every relative path is under
/// `root`: for the tests' holders, which have no backend. The primary screen is 1920x1080. The
/// pair, `save` first.
#[cfg(test)]
pub(crate) fn test_saves<H: ReadHost>(lua: &Lua, idx: usize, holder: Rc<H>, root: PathBuf) -> mlua::Result<(Function, Function)> {
    let build = |shot: Shot| -> mlua::Result<Function> {
        let (h, r) = (holder.clone(), root.clone());
        let start = lua.create_function(move |lua, (nonce, path, opts): (Table, Value, Value)| {
            start_save(&*h, lua, idx, shot, nonce, || save_ask(lua, shot, &r, (1920, 1080), path, opts, false))
        })?;
        let r = root.clone();
        let now = lua.create_function(move |lua, (path, opts): (Value, Value)| {
            let ask = save_ask(lua, shot, &r, (1920, 1080), path, opts, false)?;
            Ok(image_search::now_plan(save_plan(lua, shot, ask)?))
        })?;
        let (h, r) = (holder.clone(), root.clone());
        let submit = lua.create_function(move |lua, (path, opts, cb): (Value, Value, Value)| {
            submit_save(&*h, lua, idx, shot, cb, || save_ask(lua, shot, &r, (1920, 1080), path, opts, true))
        })?;
        crate::task::points(lua, idx, holder.clone())?.call((shot.name(), start, now, submit, 3))
    };
    Ok((build(Shot::Plain)?, build(Shot::Marked)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("host-save-tests-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn grey(w: u32, h: u32) -> CapturedImage {
        CapturedImage { w, h, rgba: [40u8, 40, 40, 255].repeat((w * h) as usize) }
    }

    /// A shot is written as a PNG of the picture, its folder made first; each mark a crosshair at
    /// its screen point taken from the region's corner, and its number of dots below it; a mark
    /// outside the picture passed over.
    #[test]
    fn a_shot_is_written_with_its_marks_where_their_screen_points_fall() {
        let dir = scratch("marks");
        let path = dir.join("deeper").join("shot.png");
        let marks = [(110, 60), (5000, 5000), (100, 50)];
        write_shot(grey(40, 30), (100, 50), &marks, &path, Shot::Marked, &|| true).expect("written");
        let img = image::open(&path).expect("a PNG").to_rgba8();
        assert_eq!((img.width(), img.height()), (40, 30));
        let ink = image::Rgba([255u8, 0, 255, 255]);
        let plain = image::Rgba([40u8, 40, 40, 255]);
        assert_eq!(*img.get_pixel(10, 10), ink, "the first mark's centre");
        assert_eq!(*img.get_pixel(19, 10), ink, "its arm, 9 to the right");
        assert_eq!(*img.get_pixel(20, 10), plain, "and no further");
        assert_eq!(*img.get_pixel(10, 22), ink, "its one dot, 12 below");
        assert_eq!(*img.get_pixel(0, 0), ink, "the third mark, at the corner");
        assert_eq!((*img.get_pixel(0, 12), *img.get_pixel(2, 12)), (ink, ink), "its three dots, the first two inside");
        assert_eq!(*img.get_pixel(1, 12), plain, "between them");
        assert_eq!(*img.get_pixel(30, 25), plain);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A picture shorter than its own size is `false` and the reason; a file that cannot be written
    /// is the message the call raises, naming the call and the path.
    #[test]
    fn a_short_picture_is_unread_and_a_file_that_cannot_be_written_is_unwritten() {
        let dir = scratch("unwritten");
        let short = CapturedImage { w: 4, h: 4, rgba: vec![0; 10] };
        assert_eq!(write_shot(short, (0, 0), &[], &dir.join("a.png"), Shot::Plain, &|| true), Err(Unsaved::Unread(SHORT_CAPTURE.to_string())));
        // A folder where the file goes: the write fails, whatever the platform.
        let path = dir.join("taken.png");
        std::fs::create_dir_all(&path).unwrap();
        match write_shot(grey(2, 2), (0, 0), &[], &path, Shot::Marked, &|| true) {
            Err(Unsaved::Unwritten(m)) => assert!(m.starts_with(&format!("screen.saveMarked '{}': ", path.display())), "{m}"),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A shot nobody waits for any more once its PNG is made makes neither its file nor its folder;
    /// the question is asked after the encode, the slow part, not before.
    #[test]
    fn a_shot_nobody_waits_for_makes_no_file_and_no_folder() {
        let dir = scratch("unwanted");
        let path = dir.join("deeper").join("shot.png");
        let asked = std::cell::Cell::new(0);
        let wanted = || {
            asked.set(asked.get() + 1);
            false
        };
        assert_eq!(write_shot(grey(4, 4), (0, 0), &[(1, 1)], &path, Shot::Marked, &wanted), Err(Unsaved::Unread(NOT_WAITED.to_string())));
        assert_eq!(asked.get(), 1);
        assert!(!dir.exists(), "neither the file nor its folder");
        // A picture with nothing to encode is answered before the question.
        let short = CapturedImage { w: 4, h: 4, rgba: vec![0; 10] };
        assert_eq!(write_shot(short, (0, 0), &[], &path, Shot::Plain, &wanted), Err(Unsaved::Unread(SHORT_CAPTURE.to_string())));
        assert_eq!(asked.get(), 1);
    }
}
