//! What a search's templates are made of, and where that is made: the PNG files the searches
//! name, opened and kept by path and modification time, and the picture of a
//! `host.screen.template` handle, prepared once — both on the image worker, never on the event
//! loop.
//!
//! A search used to open its PNG at the call: a look at the file's modification time on every
//! call, the landmark poll's twice a second included, and its decode the first time and after
//! every re-capture. `template{ rgba | rgb }` converted its bytes at the call, and
//! `template{ file }` decoded its file there. Now the call checks only what it can check without
//! that — a path's PNG header the first time the path is seen ([`locate`]), a handle's sizes and
//! its byte count — and hands the image worker a [`Needle`]. The worker opens every template of a
//! batch before it matches the batch ([`resolve`]): a path through the cache ([`open`]), a
//! handle's picture by making it ([`Prepared::make`]). A handle's preparation is also queued
//! when the handle is made, ahead of any search that uses it; whichever comes first — that task,
//! or a search that needs the picture — makes it, and nobody makes it twice. The task holds the
//! picture weakly: a handle the module has let go, with no search holding it, is gone with its
//! bytes, and its task makes nothing.
//!
//! The event loop opens no template: a search that cannot wait raises (task.rs).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::SystemTime;

use crate::image_search::TEMPLATE_FAILED;
use crate::logging;
use crate::template::Decoded;

/// Distinct PNGs kept decoded before the least recently used one is let go. Far above any real
/// module's fixed template count: a cap on growth, nothing more.
const CACHE_CAP: usize = 64;

/// Paths [`locate`] remembers having checked, before it forgets all of them and checks each again
/// the next time it is named: a cap on growth for a module that makes up a path per call.
const SEEN_CAP: usize = 1024;

/// What a search answers, and a handle from a file is left with, when the PNG cannot be read: a
/// path's, given the reason.
pub(crate) fn cannot_open(path: &Path, why: impl std::fmt::Display) -> String {
    format!("cannot open template '{}': {why}", path.display())
}

// ── The PNG files ─────────────────────────────────────────────────────────────────────────

/// One decoded PNG: the modification time it was decoded at, the picture, and when it was last
/// asked for.
struct Entry {
    modified: SystemTime,
    dec: Arc<Decoded>,
    used: u64,
}

#[derive(Default)]
struct Cache {
    files: HashMap<PathBuf, Entry>,
    /// The stamp of the latest lookup, for the least-recently-used eviction.
    stamp: u64,
}

/// The image worker's decoded PNGs. A lock rather than the worker's own map because the threads of
/// its matching pool open templates through it too, as they make a handle's picture from a file.
/// Held only for a lookup or an insert, never for a decode.
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(|| Mutex::new(Cache::default()));

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    // A plain map that is whole between any two statements: a panic elsewhere while it was held
    // leaves nothing to repair.
    CACHE.lock().unwrap_or_else(|p| p.into_inner())
}

/// The PNG at `path`, decoded: the cached picture while the file has the modification time it was
/// decoded at, and otherwise the file decoded again and kept — so a template that calibration
/// captures again is picked up by the next search, without a restart. A file the system gives no
/// modification time is decoded every time and never kept. `Err` is what the search answers:
/// `cannot open template '<path>': <why>`, for a file gone since its path was checked or a body
/// that cannot be decoded.
pub(crate) fn open(path: &Path) -> Result<Arc<Decoded>, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some(at) = modified {
        let mut c = cache();
        let Cache { files, stamp } = &mut *c;
        if let Some(e) = files.get_mut(path).filter(|e| e.modified == at) {
            *stamp += 1;
            e.used = *stamp;
            return Ok(e.dec.clone());
        }
    }
    let dec = decode(path)?;
    if let Some(at) = modified {
        let mut c = cache();
        c.stamp += 1;
        let used = c.stamp;
        c.files.insert(path.to_path_buf(), Entry { modified: at, dec: dec.clone(), used });
        if c.files.len() > CACHE_CAP {
            let oldest = c.files.iter().min_by_key(|(_, e)| e.used).map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                c.files.remove(&k);
            }
        }
    }
    Ok(dec)
}

/// The PNG at `path`, read and decoded to RGBA. A panic in the decoder is answered for this PNG
/// alone, as the host's own failure (`TEMPLATE_FAILED`), and said within the image worker's
/// throttle: the rest of its batch — other modules' searches, cells, profiles and shots — reads as
/// usual. Not kept, so the next search that names it decodes it again.
fn decode(path: &Path) -> Result<Arc<Decoded>, String> {
    #[cfg(test)]
    note(path.display().to_string());
    match logging::contain(|| read_png(path)) {
        Ok(got) => got,
        Err(report) => {
            crate::image_search::contained("a template's decode", "its searches are answered nil", &report);
            Err(TEMPLATE_FAILED.to_string())
        }
    }
}

/// The PNG at `path`, decoded.
fn read_png(path: &Path) -> Result<Arc<Decoded>, String> {
    #[cfg(test)]
    if path.file_name().is_some_and(|n| n == TEST_PANICS) {
        panic!("a decoder that panics, in a test");
    }
    let img = image::open(path).map_err(|e| cannot_open(path, e))?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    Ok(Arc::new(Decoded::from_png_rgba(w, h, img.into_raw())))
}

/// The paths [`locate`] has checked.
static SEEN: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

fn seen() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    SEEN.lock().unwrap_or_else(|p| p.into_inner())
}

/// The size of the PNG at `path`, read from its header — a few bytes at the start of the file,
/// never its pixels — and the path remembered as checked. `Err`: `cannot open template '<path>':
/// <why>`, for a file that is not there, cannot be opened, or is not a PNG.
pub(crate) fn header(path: &Path) -> Result<(u32, u32), String> {
    let size = image::image_dimensions(path).map_err(|e| cannot_open(path, e))?;
    let mut s = seen();
    if s.len() >= SEEN_CAP {
        s.clear();
    }
    s.insert(path.to_path_buf());
    Ok(size)
}

/// The PNG a search names at `path`, checked the first time it is named ([`header`]): so a path
/// that is not there, or not a PNG, raises at the call that names it, wherever it sits in a list
/// of templates. After that the path is taken as it is, and the image worker opens it; a file gone
/// since, or one whose body cannot be decoded, is answered by the search that finds it so.
pub(crate) fn locate(path: PathBuf) -> Result<PathBuf, String> {
    if !seen().contains(&path) {
        header(&path)?;
    }
    Ok(path)
}

// ── A handle's picture ────────────────────────────────────────────────────────────────────

/// What a handle's picture is made from, until it is made.
pub(crate) enum Raw {
    /// RGBA bytes the module gave, exactly `w * h * 4` of them: checked at the call.
    Rgba { w: u32, h: u32, bytes: Vec<u8> },
    /// RGB bytes the module gave, exactly `w * h * 3` of them.
    Rgb { w: u32, h: u32, bytes: Vec<u8> },
    /// A PNG whose header was read at the call: opened through the cache ([`open`]), and that
    /// decode kept for as long as the handle lives.
    File(PathBuf),
}

/// The picture of a `host.screen.template` handle, and how many of its pixels a search compares:
/// made on the image worker from what the call was given, once — by the preparation the call
/// queued, or by the first search that needs it, whichever comes first. A `capture` handle's
/// picture is made before the handle is.
pub(crate) struct Prepared {
    /// What the picture is made from, until it is made. Its lock is held by the thread making it,
    /// so a thread that asks meanwhile waits for that one; the event loop's looks
    /// ([`get`](Prepared::get), [`failed`](Prepared::failed)) never take it.
    raw: Mutex<Option<Raw>>,
    made: OnceLock<Result<(Arc<Decoded>, u32), String>>,
    /// Why a file's picture could not be made the last time it was tried, while it is still to be
    /// made: `tostring` says it.
    failed: Mutex<Option<String>>,
}

impl Prepared {
    /// A picture still to be made, from `raw`.
    pub(crate) fn of(raw: Raw) -> Arc<Prepared> {
        Arc::new(Prepared { raw: Mutex::new(Some(raw)), made: OnceLock::new(), failed: Mutex::new(None) })
    }

    /// A picture made already: a `capture` handle's.
    pub(crate) fn made(dec: Arc<Decoded>, count: u32) -> Arc<Prepared> {
        let made = OnceLock::new();
        let _ = made.set(Ok((dec, count)));
        Arc::new(Prepared { raw: Mutex::new(None), made, failed: Mutex::new(None) })
    }

    /// The picture and its count, or why it could not be made — `None` while it is still to be
    /// made. Never makes it: the event loop asks this.
    pub(crate) fn get(&self) -> Option<&Result<(Arc<Decoded>, u32), String>> {
        self.made.get()
    }

    /// Why a file's picture could not be made the last time a search or the preparation tried,
    /// while it is still to be made. Never makes it, and never waits for a making.
    pub(crate) fn failed(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// The picture and its count, made now if nobody has made it yet; a thread that asks while
    /// another makes it waits for that one. A panic in the making is answered, as every panic on
    /// the image worker is.
    ///
    /// Kept once made, and kept when it cannot be made from bytes. A file that cannot be opened
    /// this time — gone, held by another program, half written — is not kept: the next search
    /// that needs the picture tries again. Kept, it would leave the handle unusable for as long as
    /// it lives, where the call that built it used to raise and could simply be made again.
    pub(crate) fn make(&self) -> Result<(Arc<Decoded>, u32), String> {
        if let Some(made) = self.made.get() {
            return made.clone();
        }
        let mut raw = self.raw.lock().unwrap_or_else(|p| p.into_inner());
        // Made by the thread this one waited for.
        if let Some(made) = self.made.get() {
            return made.clone();
        }
        let taken = raw.take();
        let file = match &taken {
            Some(Raw::File(path)) => Some(path.clone()),
            _ => None,
        };
        let got = match logging::contain(|| prepare(taken)) {
            Ok(got) => got,
            Err(report) => {
                crate::image_search::contained("a template's preparation", "its searches are answered nil", &report);
                Err(TEMPLATE_FAILED.to_string())
            }
        };
        match (got, file) {
            (Err(why), Some(path)) => {
                *raw = Some(Raw::File(path));
                *self.failed.lock().unwrap_or_else(|p| p.into_inner()) = Some(why.clone());
                Err(why)
            }
            (got, _) => {
                let _ = self.made.set(got.clone());
                *self.failed.lock().unwrap_or_else(|p| p.into_inner()) = None;
                got
            }
        }
    }
}

/// `raw` made a picture, and its pixels counted.
fn prepare(raw: Option<Raw>) -> Result<(Arc<Decoded>, u32), String> {
    #[cfg(test)]
    if let Some(Raw::Rgba { w, h, .. } | Raw::Rgb { w, h, .. }) = &raw {
        note(format!("bytes {w}x{h}"));
    }
    let dec = match raw {
        Some(Raw::Rgba { w, h, bytes }) => Arc::new(Decoded::from_rgba(w, h, bytes).map_err(|e| e.to_string())?),
        Some(Raw::Rgb { w, h, bytes }) => Arc::new(Decoded::from_rgb(w, h, &bytes).map_err(|e| e.to_string())?),
        Some(Raw::File(path)) => open(&path)?,
        // Taken by a making that panicked: answered as the host's own failure.
        None => return Err(TEMPLATE_FAILED.to_string()),
    };
    let count = dec.count();
    Ok((dec, count))
}

// ── What a search hands the worker ─────────────────────────────────────────────────────────

/// A template as a search hands it to the image worker.
#[derive(Clone)]
pub(crate) enum Needle {
    /// A path the search was given, checked at the call ([`locate`]): opened on the worker.
    File(PathBuf),
    /// A handle's picture.
    Handle(Arc<Prepared>),
}

/// The PNGs one batch has opened, by path: a path two of its searches share is opened once.
#[derive(Default)]
pub(crate) struct Opened(HashMap<PathBuf, Result<Arc<Decoded>, String>>);

/// Each needle's picture, in order — or why one of them has none, which is what the search
/// answers: `cannot open template '<path>': <why>` for a PNG that cannot be opened or decoded, and
/// a handle's own reason for one whose picture could not be made.
pub(crate) fn resolve<'a>(needles: impl IntoIterator<Item = &'a Needle>, opened: &mut Opened) -> Result<Vec<Arc<Decoded>>, String> {
    needles
        .into_iter()
        .map(|n| match n {
            Needle::File(path) => opened.0.entry(path.clone()).or_insert_with(|| open(path)).clone(),
            Needle::Handle(p) => p.make().map(|(dec, _)| dec),
        })
        .collect()
}

// ── For the tests ──────────────────────────────────────────────────────────────────────────

/// Every decode of a PNG and every preparation of a handle from bytes, with the thread it was
/// made on: for the tests that keep them off the event loop. A file is named by its path, bytes
/// by their size, `bytes 7x3`.
#[cfg(test)]
static MADE: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

#[cfg(test)]
fn note(what: String) {
    let thread = std::thread::current().name().unwrap_or("?").to_string();
    MADE.lock().unwrap_or_else(|p| p.into_inner()).push((what, thread));
}

/// The file name whose decode panics, in the tests: for the ones that keep such a panic to its own
/// search.
#[cfg(test)]
pub(crate) const TEST_PANICS: &str = "decode-panics.png";

/// The threads `what` was made on — a path as `Path::display` writes it, or `bytes 7x3` — in
/// order.
#[cfg(test)]
pub(crate) fn test_made_on(what: &str) -> Vec<String> {
    MADE.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|(w, _)| w == what).map(|(_, t)| t.clone()).collect()
}

/// A 1x1 PNG of `rgb` written to `path`, with a modification time `secs` seconds from now, so two
/// writes in a row are told apart on any file system.
#[cfg(test)]
pub(crate) fn test_write_dot(path: &Path, rgb: [u8; 3], secs: u64) {
    image::RgbaImage::from_pixel(1, 1, image::Rgba([rgb[0], rgb[1], rgb[2], 255])).save(path).expect("the PNG is written");
    let at = SystemTime::now() + std::time::Duration::from_secs(secs);
    std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(at)).expect("its time is set");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of the test's own, empty.
    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("host-template-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rgba(d: &Decoded) -> Vec<u8> {
        let crate::template::Body::Dense { rgba, .. } = &d.body;
        rgba.clone()
    }

    /// A PNG is decoded once while its modification time stays, and again when it changes — the
    /// picture then the one the file holds now.
    #[test]
    fn a_png_is_decoded_once_and_again_when_it_is_captured_again() {
        let d = dir("again");
        let path = d.join("dot.png");
        test_write_dot(&path, [10, 20, 30], 0);
        let first = open(&path).unwrap();
        let second = open(&path).unwrap();
        assert!(Arc::ptr_eq(&first, &second), "the cached picture");
        assert_eq!(test_made_on(&path.display().to_string()).len(), 1, "decoded once");
        test_write_dot(&path, [40, 50, 60], 10);
        let third = open(&path).unwrap();
        assert_eq!(rgba(&third), [40, 50, 60, 255], "the picture the file holds now");
        assert_eq!(rgba(&first), [10, 20, 30, 255], "a picture already handed out stays as it was");
        assert_eq!(test_made_on(&path.display().to_string()).len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A path is checked the first time it is named — a file that is not there, or is not a PNG,
    /// raises there — and taken as it is after that: a file gone since is the search's to answer.
    #[test]
    fn a_path_is_checked_the_first_time_it_is_named() {
        let d = dir("locate");
        let missing = d.join("missing.png");
        let e = locate(missing.clone()).unwrap_err();
        assert!(e.starts_with(&format!("cannot open template '{}': ", missing.display())), "{e}");
        let bmp = d.join("dot.bmp");
        std::fs::write(&bmp, b"BM not really").unwrap();
        assert!(locate(bmp).unwrap_err().starts_with("cannot open template '"), "not a PNG");
        let garbage = d.join("garbage.png");
        std::fs::write(&garbage, b"not a PNG at all").unwrap();
        assert!(locate(garbage).is_err(), "no PNG header");
        let path = d.join("dot.png");
        test_write_dot(&path, [1, 2, 3], 0);
        assert_eq!(header(&path).unwrap(), (1, 1));
        assert_eq!(locate(path.clone()).unwrap(), path);
        assert!(test_made_on(&path.display().to_string()).is_empty(), "nothing decoded at the call");
        std::fs::remove_file(&path).unwrap();
        assert_eq!(locate(path.clone()).unwrap(), path, "seen: taken as it is");
        let e = open(&path).err().unwrap();
        assert!(e.starts_with(&format!("cannot open template '{}': ", path.display())), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A PNG whose header reads and whose body does not is found out by the open, not the check.
    #[test]
    fn a_body_that_cannot_be_decoded_is_found_by_the_open() {
        let d = dir("corrupt");
        let path = d.join("corrupt.png");
        test_write_dot(&path, [1, 2, 3], 0);
        let mut bytes = std::fs::read(&path).unwrap();
        // Everything up to the image data stays, and two bytes of that data: the rest is cut away.
        let at = bytes.windows(4).position(|w| w == b"IDAT").expect("an IDAT chunk");
        bytes.truncate(at + 4 + 2);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(locate(path.clone()).unwrap(), path, "the header reads");
        assert!(open(&path).err().unwrap().starts_with(&format!("cannot open template '{}': ", path.display())));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A handle's picture is made once, by whoever asks first, and kept; before that the event
    /// loop's look finds nothing. A file's shares the cache's decode.
    #[test]
    fn a_handles_picture_is_made_once() {
        let p = Prepared::of(Raw::Rgb { w: 2, h: 1, bytes: vec![1, 2, 3, 4, 5, 6] });
        assert!(p.get().is_none(), "still to be made");
        let first = p.make().unwrap().0;
        assert!(Arc::ptr_eq(&first, &p.make().unwrap().0), "made once");
        assert_eq!(p.get().unwrap().as_ref().unwrap().1, 2, "every pixel compared");
        assert_eq!(rgba(&first), [1, 2, 3, 255, 4, 5, 6, 255]);
        let d = dir("handle");
        let path = d.join("dot.png");
        test_write_dot(&path, [7, 8, 9], 0);
        let f = Prepared::of(Raw::File(path.clone()));
        let made = f.make().unwrap().0;
        assert!(Arc::ptr_eq(&made, &open(&path).unwrap()), "the cache's decode, shared");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A file that cannot be opened when its picture is first made is tried again by the next
    /// making, not kept: once it is there the handle has its picture. Until then `failed` says
    /// why, and the picture is still to be made. Bytes that cannot be made are kept.
    #[test]
    fn a_files_failure_is_tried_again_and_a_made_picture_kept() {
        let d = dir("again-later");
        let path = d.join("later.png");
        let f = Prepared::of(Raw::File(path.clone()));
        let e = f.make().err().unwrap();
        assert!(e.starts_with(&format!("cannot open template '{}': ", path.display())), "{e}");
        assert!(f.get().is_none(), "not kept");
        assert_eq!(f.failed().as_deref(), Some(e.as_str()));
        test_write_dot(&path, [3, 4, 5], 0);
        let (dec, count) = f.make().unwrap();
        assert_eq!((rgba(&dec), count), (vec![3, 4, 5, 255], 1), "made once it is there");
        assert!(f.failed().is_none(), "nothing left to say once it is made");
        std::fs::remove_file(&path).unwrap();
        assert!(Arc::ptr_eq(&f.make().unwrap().0, &dec), "and kept: the file gone since changes nothing");
        let wrong = Prepared::of(Raw::Rgb { w: 2, h: 1, bytes: vec![1, 2, 3] });
        let e = wrong.make().err().unwrap();
        assert_eq!(wrong.get().and_then(|r| r.as_ref().err()), Some(&e), "bytes that cannot be made are kept");
        assert!(wrong.failed().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A decoder that panics is answered for its own PNG, the host's own failure, and not kept.
    #[test]
    fn a_decode_that_panics_is_answered_for_its_png_alone() {
        let d = dir("panics");
        let path = d.join(TEST_PANICS);
        test_write_dot(&path, [1, 1, 1], 0);
        assert_eq!(open(&path).err().as_deref(), Some(TEMPLATE_FAILED));
        assert_eq!(open(&path).err().as_deref(), Some(TEMPLATE_FAILED), "tried again");
        assert_eq!(test_made_on(&path.display().to_string()).len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A batch's needles, in order: a path two of them share opened once, and the first that has
    /// no picture is the answer.
    #[test]
    fn a_batch_opens_each_path_once() {
        let d = dir("batch");
        let path = d.join("dot.png");
        test_write_dot(&path, [9, 9, 9], 0);
        let what = path.display().to_string();
        let before = test_made_on(&what).len();
        let mut opened = Opened::default();
        let handle = Needle::Handle(Prepared::of(Raw::Rgba { w: 1, h: 1, bytes: vec![1, 1, 1, 255] }));
        let got = resolve([&Needle::File(path.clone()), &handle, &Needle::File(path.clone())], &mut opened).unwrap();
        assert_eq!(got.len(), 3);
        assert!(Arc::ptr_eq(&got[0], &got[2]));
        assert_eq!(test_made_on(&what).len(), before + 1);
        let e = resolve([&handle, &Needle::File(d.join("gone.png"))], &mut opened).err().unwrap();
        assert!(e.contains("gone.png"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
