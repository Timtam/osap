//! A picture of the screen as a value: what `host.screen.snapshot` hands a module, and what
//! every read on a snapshot reads.
//!
//! **What it is.** One capture of a rectangle, kept: the rectangle in screen coordinates, the
//! pixels as the image matcher and the cells reduction read them (tightly packed top-down RGBA,
//! exactly `w * h` of them, alpha forced opaque), when it was taken, and which path took it. On
//! macOS it also keeps the backing-store image the window server returned, which the point-sized
//! pixels were drawn from — the sharp picture the text recogniser wants (`NativeImage`); nowhere
//! else is there such a thing, and the type is empty there.
//!
//! **Why it is a value and not a cache.** `docs/screen-frame-sharing-design.md` rejected a frame
//! the host keeps and serves reads from implicitly: a read answered from a picture the module
//! did not ask for is a read of the past. A snapshot is taken because a module asked for one,
//! and read because the module passes it; nothing else ever sees it. What it buys is several
//! reads of ONE picture — a menu's cursor, its help bubble and its page title from the same
//! moment — at the cost of one capture.
//!
//! **The two geometry rules** the calls apply live here as questions a frame answers: `clip`
//! (the part of a region inside the picture) for the calls that read a region, `contains` and
//! `area` (the region wholly inside, or no answer) for the calls whose answer would change if a
//! region were cut — a point, a template, a grid of cells.
//!
//! Pure: data and geometry, no Lua, no OS call. Borrowed by `crates/macos-check` through
//! `backend/mod.rs`, so it is type-checked for the Mac, where `NativeImage` is real, and its
//! tests build there too.

use std::collections::BTreeMap;
use std::time::Instant;

use super::{CapturedImage, CAPTURE_FAILED};
use crate::ocr::types::Rect;

/// The most pixels one frame may hold: 40 million, the limit every capture path already has
/// (`MAX_POINTS` on macOS, `dxgi::MAX_PIXELS` on Windows) — past any real display, and still
/// only 160 MB of pixels.
pub const MAX_FRAME_PIXELS: i64 = 40_000_000;

/// Points that lie within a box of at most this many pixels are read with one capture of the
/// box: on the standard Windows path a capture costs about one compositor frame whatever its
/// size up to about 1028x666 and not quite two for a whole 1080p screen, so reading close points
/// one by one would pay a frame for each.
pub const POINTS_BOX_PIXELS: i64 = 2_000_000;

/// The tiles points spread wider than [`POINTS_BOX_PIXELS`] are grouped by: fixed, 2048 x 976,
/// so the box of the points in one tile is never larger than `POINTS_BOX_PIXELS` either.
const TILE_W: i64 = 2048;
const TILE_H: i64 = 976;
const _: () = assert!(TILE_W * TILE_H <= POINTS_BOX_PIXELS);

/// Which way a frame was taken, for `s.via`: the answer to "which picture is this?" that a
/// module otherwise cannot get, since a read through desktop duplication falls back to the
/// standard path without a word under the default fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameVia {
    /// Windows: the screen device context, `BitBlt`.
    #[cfg_attr(not(windows), allow(dead_code))]
    Gdi,
    /// Windows: DXGI Desktop Duplication (`backend/dxgi.rs`).
    #[cfg_attr(not(windows), allow(dead_code))]
    Duplication,
    /// macOS: `SCScreenshotManager captureImageInRect`.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    ScreenCaptureKit,
    /// macOS: one of the two older Core Graphics capture functions, looked up at run time.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    CoreGraphics,
}

impl FrameVia {
    /// The word a module reads in `s.via`.
    pub fn word(self) -> &'static str {
        match self {
            FrameVia::Gdi => "gdi",
            FrameVia::Duplication => "duplication",
            FrameVia::ScreenCaptureKit => "screencapturekit",
            FrameVia::CoreGraphics => "coregraphics",
        }
    }
}

/// The macOS backing-store image a frame keeps — see `backend/macos/capture.rs`.
#[cfg(target_os = "macos")]
pub use super::macos::NativeImage;

/// Nothing: only macOS has a picture beside the pixels.
#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub enum NativeImage {}

#[cfg(not(target_os = "macos"))]
impl NativeImage {
    pub fn bytes(&self) -> usize {
        match *self {}
    }

    pub fn crop(&self, _r: Rect) -> Option<NativeImage> {
        match *self {}
    }

    pub fn covers(&self) -> Rect {
        match *self {}
    }
}

/// A part of a frame in the frame's own pixels: what the matcher, the profile and the cells
/// reduction are handed to read a region of a picture without copying it out first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Area {
    /// The whole of a `w` x `h` image.
    pub fn full(w: u32, h: u32) -> Area {
        Area { x: 0, y: 0, w, h }
    }
}

/// One capture, kept. See the file's comment.
pub struct Frame {
    /// The rectangle captured, in screen coordinates (device pixels on Windows, points on
    /// macOS). `img` is exactly its size.
    pub rect: Rect,
    pub img: CapturedImage,
    /// When the capture came back — what `s.time` reports, on `host.now()`'s clock.
    pub taken: Instant,
    pub via: FrameVia,
    /// macOS: the backing-store image of the part of `rect` that was on the desktop.
    pub native: Option<NativeImage>,
    /// `host.inputEpoch()` once the capture had come back — what `s.inputEpoch` reports. 0 from
    /// [`Frame::from_image`]; stamped by whoever took the picture: the event loop for
    /// `host.screen.snapshot`, the `screen-capture` thread for `snapshotAsync` (`Round::run`).
    /// A copy keeps it.
    pub input_epoch: u64,
}

// A frame is handed to the image worker and the rayon pool behind an `Arc`, so it has to be
// both. Checked here, where a field that is not would be added.
const _: () = {
    const fn ok<T: Send + Sync>() {}
    ok::<Frame>()
};

impl Frame {
    /// A frame of `img` taken of `rect`, or `None` when the image is not exactly that rectangle's
    /// size — a capture that came back clipped or short, which no read may index into. Alpha is
    /// forced opaque, so a template cut from it is never all wildcards and a PNG saved from it is
    /// never invisible. Every capture path already makes it opaque, so this is a second pass over
    /// the pixels on the event loop; it is kept as the one place that guarantees it, since a
    /// path that forgot would make both mistakes without a word.
    pub fn from_image(
        rect: Rect,
        mut img: CapturedImage,
        taken: Instant,
        via: FrameVia,
        native: Option<NativeImage>,
    ) -> Option<Frame> {
        if rect.is_empty() || rect.area() > MAX_FRAME_PIXELS {
            return None;
        }
        if img.w as i64 != rect.w as i64 || img.h as i64 != rect.h as i64 {
            return None;
        }
        let need = (img.w as usize).checked_mul(img.h as usize)?.checked_mul(4)?;
        if img.rgba.len() != need {
            return None;
        }
        for px in img.rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Some(Frame { rect, img, taken, via, native, input_epoch: 0 })
    }

    /// Whether `r` lies wholly inside the picture.
    pub fn contains(&self, r: Rect) -> bool {
        self.rect.contains(&r)
    }

    /// Whether the pixel at `(x, y)` is in the picture.
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        self.contains(Rect::new(x, y, 1, 1))
    }

    /// The part of `r` inside the picture, or `None` when they do not meet.
    pub fn clip(&self, r: Rect) -> Option<Rect> {
        self.rect.intersect(&r)
    }

    /// `r` in the picture's own pixels, when it lies wholly inside.
    pub fn area(&self, r: Rect) -> Option<Area> {
        if !self.contains(r) {
            return None;
        }
        Some(Area {
            x: (r.x as i64 - self.rect.x as i64) as u32,
            y: (r.y as i64 - self.rect.y as i64) as u32,
            w: r.w as u32,
            h: r.h as u32,
        })
    }

    /// The colour at the screen point `(x, y)`, when it is in the picture.
    pub fn sample(&self, x: i32, y: i32) -> Option<(u8, u8, u8)> {
        if !self.contains_point(x, y) {
            return None;
        }
        let dx = (x as i64 - self.rect.x as i64) as usize;
        let dy = (y as i64 - self.rect.y as i64) as usize;
        let i = (dy * self.img.w as usize + dx) * 4;
        let px = self.img.rgba.get(i..i + 3)?;
        Some((px[0], px[1], px[2]))
    }

    /// The pixels of `r`, copied out, when it lies wholly inside.
    pub fn crop_image(&self, r: Rect) -> Option<CapturedImage> {
        let a = self.area(r)?;
        let row = self.img.w as usize * 4;
        let (x0, len) = (a.x as usize * 4, a.w as usize * 4);
        let mut rgba = Vec::with_capacity(len * a.h as usize);
        for y in a.y as usize..(a.y + a.h) as usize {
            let start = y * row + x0;
            rgba.extend_from_slice(self.img.rgba.get(start..start + len)?);
        }
        Some(CapturedImage { w: a.w, h: a.h, rgba })
    }

    /// A frame of its own of `r`, when it lies wholly inside: the pixels copied, the same moment
    /// and path, and on macOS the matching part of the backing image, drawn anew.
    pub fn crop(&self, r: Rect) -> Option<Frame> {
        let img = self.crop_image(r)?;
        let native = self.native.as_ref().and_then(|n| n.crop(r));
        Some(Frame { rect: r, img, taken: self.taken, via: self.via, native, input_epoch: self.input_epoch })
    }

    /// [`crop`](Frame::crop) without the backing image: the point-sized pixels of `r` only, for
    /// a picture that is only ever compared with, never read or recognised — a change wait's
    /// baseline. On macOS a fifth of what `crop` keeps, and no redrawing.
    pub fn pixels_only(&self, r: Rect) -> Option<Frame> {
        let img = self.crop_image(r)?;
        Some(Frame { rect: r, img, taken: self.taken, via: self.via, native: None, input_epoch: self.input_epoch })
    }

    /// The part of the picture a text recogniser can read: all of it, except on macOS, where
    /// that is the part the kept backing image covers — the part that was on the desktop. A read
    /// of a snapshot cuts its regions to this.
    pub fn ocr_rect(&self) -> Rect {
        self.native.as_ref().map_or(self.rect, NativeImage::covers)
    }

    /// Roughly what it holds in memory: its pixels, and on macOS the backing image beside them.
    /// What a snapshot is charged against its module's budget once it exists.
    pub fn bytes(&self) -> usize {
        self.img.rgba.len() + self.native.as_ref().map_or(0, NativeImage::bytes)
    }
}

/// The smallest rectangle holding every point, or `None` when there are none or it does not fit
/// the coordinate range (two points four billion pixels apart). In 64 bits.
pub fn bbox(pts: &[(i32, i32)]) -> Option<Rect> {
    let x0 = pts.iter().map(|p| p.0 as i64).min()?;
    let y0 = pts.iter().map(|p| p.1 as i64).min()?;
    let x1 = pts.iter().map(|p| p.0 as i64).max()? + 1;
    let y1 = pts.iter().map(|p| p.1 as i64).max()? + 1;
    let w = i32::try_from(x1 - x0).ok()?;
    let h = i32::try_from(y1 - y0).ok()?;
    Some(Rect::new(x0 as i32, y0 as i32, w, h))
}

/// What to capture to read `pts`: their box when it holds at most [`POINTS_BOX_PIXELS`];
/// otherwise the points grouped by the fixed 2048 x 976 tile each lies in, and the box of each
/// group — never more than `POINTS_BOX_PIXELS` a capture (8 MB), and never more captures than
/// there are distinct points or tiles they occupy. Two points far apart are two 1x1 captures,
/// not one of everything between them; points spread over a whole desktop are one capture per
/// tile they reach, not one per point nor one of the desktop.
pub fn plan_points(pts: &[(i32, i32)]) -> Vec<Rect> {
    if let Some(b) = bbox(pts) {
        if b.area() <= POINTS_BOX_PIXELS {
            return vec![b];
        }
    }
    let mut tiles: BTreeMap<(i64, i64), Vec<(i32, i32)>> = BTreeMap::new();
    for &(x, y) in pts {
        tiles.entry(((x as i64).div_euclid(TILE_W), (y as i64).div_euclid(TILE_H))).or_default().push((x, y));
    }
    tiles.values().filter_map(|p| bbox(p)).collect()
}

/// The colours at `pts`, in order, from the captures `plan_points` plans, each taken through
/// `capture` — or why one of them failed, which fails the call: a module reading several points
/// is answered for all of them or for none. Each capture is sampled for the points it holds and
/// dropped before the next is taken, so no more than one is held at a time.
pub fn read_points(
    pts: &[(i32, i32)],
    mut capture: impl FnMut(Rect) -> Result<CapturedImage, String>,
) -> Result<Vec<(u8, u8, u8)>, String> {
    let mut out: Vec<Option<(u8, u8, u8)>> = vec![None; pts.len()];
    for r in plan_points(pts) {
        let img = capture(r)?;
        let whole = img.w as i64 == r.w as i64
            && img.h as i64 == r.h as i64
            && img.rgba.len() as u64 >= img.w as u64 * img.h as u64 * 4;
        if !whole {
            return Err(CAPTURE_FAILED.to_string());
        }
        for (slot, &(x, y)) in out.iter_mut().zip(pts) {
            if slot.is_none() && r.contains(&Rect::new(x, y, 1, 1)) {
                let dx = (x as i64 - r.x as i64) as usize;
                let dy = (y as i64 - r.y as i64) as usize;
                let i = (dy * img.w as usize + dx) * 4;
                *slot = Some((img.rgba[i], img.rgba[i + 1], img.rgba[i + 2]));
            }
        }
    }
    out.into_iter().map(|c| c.ok_or_else(|| CAPTURE_FAILED.to_string())).collect()
}

/// The colours at `pts`, in order, where a point that `off` says lies on no display reads black
/// (0, 0, 0) — nothing captured or asked for it — and the rest are read through `read`, handed
/// them in the order asked. `read` must answer each of them; an answer of another length fails
/// the call. The backends pass their own test of "on no display", which on macOS counts a point
/// in the gap between two displays of different sizes as off.
pub fn black_where_off(
    pts: &[(i32, i32)],
    off: impl Fn(i32, i32) -> bool,
    read: impl FnOnce(&[(i32, i32)]) -> Result<Vec<(u8, u8, u8)>, String>,
) -> Result<Vec<(u8, u8, u8)>, String> {
    let away: Vec<bool> = pts.iter().map(|&(x, y)| off(x, y)).collect();
    let on: Vec<(i32, i32)> = pts.iter().zip(&away).filter(|(_, a)| !**a).map(|(p, _)| *p).collect();
    let read = if on.is_empty() { Vec::new() } else { read(&on)? };
    if read.len() != on.len() {
        return Err(CAPTURE_FAILED.to_string());
    }
    let mut read = read.into_iter();
    away.iter()
        .map(|a| if *a { Ok((0, 0, 0)) } else { read.next().ok_or_else(|| CAPTURE_FAILED.to_string()) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w` x `h` image whose pixel at (x, y) is (x, y, 7) with a stray alpha.
    fn ramp(w: u32, h: u32) -> CapturedImage {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[x as u8, y as u8, 7, 3]);
            }
        }
        CapturedImage { w, h, rgba }
    }

    fn frame(x: i32, y: i32, w: u32, h: u32) -> Frame {
        Frame::from_image(Rect::new(x, y, w as i32, h as i32), ramp(w, h), Instant::now(), FrameVia::Gdi, None)
            .expect("a whole image")
    }

    #[test]
    fn from_image_forces_alpha() {
        let f = frame(0, 0, 4, 3);
        assert!(f.img.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert_eq!(f.bytes(), 4 * 3 * 4);
    }

    #[test]
    fn from_image_rejects_short_or_mismatched_buffer() {
        let r = Rect::new(0, 0, 4, 3);
        let short = CapturedImage { w: 4, h: 3, rgba: vec![0; 40] };
        assert!(Frame::from_image(r, short, Instant::now(), FrameVia::Gdi, None).is_none());
        let long = CapturedImage { w: 4, h: 3, rgba: vec![0; 52] };
        assert!(Frame::from_image(r, long, Instant::now(), FrameVia::Gdi, None).is_none());
        assert!(Frame::from_image(r, ramp(3, 4), Instant::now(), FrameVia::Gdi, None).is_none(), "the other way round");
        assert!(Frame::from_image(r, ramp(4, 2), Instant::now(), FrameVia::Gdi, None).is_none(), "clipped");
        let empty = Rect::new(0, 0, 0, 3);
        assert!(Frame::from_image(empty, CapturedImage { w: 0, h: 3, rgba: Vec::new() }, Instant::now(), FrameVia::Gdi, None).is_none());
    }

    #[test]
    fn contains_edges_and_empty() {
        let f = frame(10, 20, 30, 40);
        assert!(f.contains(Rect::new(10, 20, 30, 40)));
        assert!(f.contains(Rect::new(39, 59, 1, 1)));
        assert!(!f.contains(Rect::new(39, 59, 2, 1)));
        assert!(!f.contains(Rect::new(9, 20, 5, 5)));
        assert!(!f.contains(Rect::new(15, 25, 0, 0)), "an empty region is inside nothing");
        assert!(f.contains_point(10, 20) && f.contains_point(39, 59));
        assert!(!f.contains_point(40, 20) && !f.contains_point(10, 60) && !f.contains_point(9, 25));
    }

    #[test]
    fn clip_partial_whole_and_none() {
        let f = frame(10, 20, 30, 40);
        assert_eq!(f.clip(Rect::new(0, 0, 20, 30)), Some(Rect::new(10, 20, 10, 10)));
        assert_eq!(f.clip(Rect::new(0, 0, 1000, 1000)), Some(f.rect), "a region around it is all of it");
        assert_eq!(f.clip(Rect::new(15, 25, 5, 5)), Some(Rect::new(15, 25, 5, 5)));
        assert_eq!(f.clip(Rect::new(40, 20, 5, 5)), None, "beside it");
        assert_eq!(f.clip(Rect::new(15, 25, 0, 5)), None, "empty");
    }

    #[test]
    fn sample_inside_and_outside() {
        let f = frame(-5, 100, 8, 6);
        assert_eq!(f.sample(-5, 100), Some((0, 0, 7)));
        assert_eq!(f.sample(2, 105), Some((7, 5, 7)), "the last pixel");
        assert_eq!(f.sample(3, 105), None);
        assert_eq!(f.sample(-6, 100), None);
        assert_eq!(f.sample(0, 99), None);
    }

    #[test]
    fn sub_is_frame_relative() {
        let f = frame(100, 50, 20, 10);
        assert_eq!(f.area(Rect::new(105, 52, 3, 4)), Some(Area { x: 5, y: 2, w: 3, h: 4 }));
        assert_eq!(f.area(f.rect), Some(Area::full(20, 10)));
        assert_eq!(f.area(Rect::new(118, 52, 3, 4)), None, "not wholly inside");
    }

    #[test]
    fn crop_image_copies_rows() {
        let f = frame(10, 10, 6, 5);
        let c = f.crop_image(Rect::new(12, 11, 3, 2)).unwrap();
        assert_eq!((c.w, c.h), (3, 2));
        let px = |x: usize, y: usize| &c.rgba[(y * 3 + x) * 4..][..4];
        assert_eq!(px(0, 0), &[2, 1, 7, 255]);
        assert_eq!(px(2, 1), &[4, 2, 7, 255]);
        assert!(f.crop_image(Rect::new(14, 11, 3, 2)).is_none(), "reaching past the right edge");
        let all = f.crop_image(f.rect).unwrap();
        assert_eq!(all.rgba, f.img.rgba, "the whole frame is its own pixels");
    }

    #[test]
    fn crop_keeps_via_and_taken() {
        let mut f = frame(0, 0, 10, 10);
        f.via = FrameVia::Duplication;
        let c = f.crop(Rect::new(2, 3, 4, 5)).unwrap();
        assert_eq!(c.rect, Rect::new(2, 3, 4, 5));
        assert_eq!((c.img.w, c.img.h), (4, 5));
        assert_eq!(c.via, FrameVia::Duplication);
        assert_eq!(c.taken, f.taken);
        assert_eq!(c.sample(2, 3), f.sample(2, 3));
        assert_eq!(c.bytes(), 4 * 5 * 4);
        assert!(f.crop(Rect::new(8, 8, 4, 4)).is_none());
        f.input_epoch = 9;
        assert_eq!(f.crop(Rect::new(2, 3, 4, 5)).unwrap().input_epoch, 9, "a copy keeps the epoch");
        let p = f.pixels_only(Rect::new(2, 3, 4, 5)).unwrap();
        assert_eq!((p.rect, p.via, p.taken, p.input_epoch), (Rect::new(2, 3, 4, 5), FrameVia::Duplication, f.taken, 9));
        assert!(p.native.is_none());
        assert_eq!(p.img.rgba, c.img.rgba, "the same pixels as crop");
        assert!(f.pixels_only(Rect::new(8, 8, 4, 4)).is_none());
    }

    #[test]
    fn ocr_rect_is_rect_without_native() {
        let f = frame(10, 20, 30, 40);
        assert_eq!(f.ocr_rect(), f.rect);
        assert_eq!(f.crop(Rect::new(12, 22, 5, 5)).unwrap().ocr_rect(), Rect::new(12, 22, 5, 5));
    }

    #[test]
    fn the_words_modules_read() {
        let words: Vec<&str> = [FrameVia::Gdi, FrameVia::Duplication, FrameVia::ScreenCaptureKit, FrameVia::CoreGraphics]
            .iter()
            .map(|v| v.word())
            .collect();
        assert_eq!(words, ["gdi", "duplication", "screencapturekit", "coregraphics"]);
    }

    #[test]
    fn bbox_single_point_is_1x1() {
        assert_eq!(bbox(&[(5, 7)]), Some(Rect::new(5, 7, 1, 1)));
        assert_eq!(bbox(&[(5, 7), (5, 7)]), Some(Rect::new(5, 7, 1, 1)));
        assert_eq!(bbox(&[(5, 7), (9, 8)]), Some(Rect::new(5, 7, 5, 2)));
        assert_eq!(bbox(&[]), None);
    }

    #[test]
    fn bbox_negative_coords() {
        assert_eq!(bbox(&[(-1920, -10), (-1, 5)]), Some(Rect::new(-1920, -10, 1920, 16)));
    }

    #[test]
    fn bbox_overflow_is_none() {
        assert_eq!(bbox(&[(i32::MIN, 0), (i32::MAX, 0)]), None, "wider than the coordinate range");
        assert_eq!(bbox(&[(i32::MAX, i32::MAX)]), Some(Rect::new(i32::MAX, i32::MAX, 1, 1)));
    }

    #[test]
    fn plan_points_box_under_2mpx() {
        let pts = [(100, 200), (140, 205), (120, 230)];
        assert_eq!(plan_points(&pts), vec![Rect::new(100, 200, 41, 31)]);
        // A 1000x2000 box is exactly the limit.
        assert_eq!(plan_points(&[(0, 0), (999, 1999)]), vec![Rect::new(0, 0, 1000, 2000)]);
    }

    #[test]
    fn plan_points_two_far_points_read_alone() {
        assert_eq!(plan_points(&[(0, 0), (1920, 1080)]), vec![Rect::new(0, 0, 1, 1), Rect::new(1920, 1080, 1, 1)]);
        // Two distinct points, however often each is asked for.
        assert_eq!(
            plan_points(&[(0, 0), (1920, 1080), (0, 0)]),
            vec![Rect::new(0, 0, 1, 1), Rect::new(1920, 1080, 1, 1)]
        );
        // Three far apart: one capture per tile they lie in, never the 1921x1081 box around them.
        // (1920, 1080) and (0, 1080) share the tile at (0, 976), so that is one thin strip.
        assert_eq!(
            plan_points(&[(0, 0), (1920, 1080), (0, 1080)]),
            vec![Rect::new(0, 0, 1, 1), Rect::new(0, 1080, 1921, 1)]
        );
    }

    #[test]
    fn plan_points_huge_box_reads_each() {
        let pts = [(-2_000_000_000, 0), (2_000_000_000, 0), (0, 5)];
        let plan = plan_points(&pts);
        assert_eq!(plan.len(), 3, "a box that does not fit is never captured");
        assert!(plan.iter().all(|r| r.w == 1 && r.h == 1));
        let pts = [(0, 0), (10_000, 10_000), (5, 5)];
        assert_eq!(plan_points(&pts), vec![Rect::new(0, 0, 6, 6), Rect::new(10_000, 10_000, 1, 1)], "near points share a capture");
        assert!(plan_points(&[]).is_empty());
    }

    /// Thousands of points in two clusters far apart are two captures, not one per point and not
    /// one of the desktop between them.
    #[test]
    fn plan_points_clusters_are_one_capture_each() {
        let mut pts = Vec::new();
        for i in 0..2048 {
            pts.push((100 + i % 64, 200 + i / 64));
            pts.push((8000 + i % 64, 4000 + i / 64));
        }
        assert_eq!(plan_points(&pts), vec![Rect::new(100, 200, 64, 32), Rect::new(8000, 4000, 64, 32)]);
    }

    /// Points spread over an 8K monitor and a 1080p one beside it (a 9600x4320 box, past 40
    /// million pixels): one capture per tile they reach, none of them larger than
    /// `POINTS_BOX_PIXELS`, and every point in exactly one of them.
    #[test]
    fn plan_points_spread_over_a_large_desktop_is_bounded() {
        let mut pts = Vec::new();
        for gy in 0..64 {
            for gx in 0..64 {
                pts.push((gx * 9599 / 63, gy * 4319 / 63));
            }
        }
        let plan = plan_points(&pts);
        assert!(plan.len() <= 5 * 5, "{} captures for 4096 points over 9600x4320", plan.len());
        assert!(plan.iter().all(|r| r.area() <= POINTS_BOX_PIXELS), "{plan:?}");
        for &(x, y) in &pts {
            let n = plan.iter().filter(|r| r.contains(&Rect::new(x, y, 1, 1))).count();
            assert_eq!(n, 1, "({x}, {y}) is in {n} captures");
        }
        // Negative coordinates, a monitor left of and above the primary, tile the same way.
        let plan = plan_points(&[(-3000, -1500), (-1, -1), (3000, 1500)]);
        assert_eq!(plan, vec![Rect::new(-3000, -1500, 1, 1), Rect::new(-1, -1, 1, 1), Rect::new(3000, 1500, 1, 1)]);
    }

    #[test]
    fn read_points_samples_each_from_its_capture_and_fails_as_one() {
        let mut asked = Vec::new();
        let got = read_points(&[(12, 11), (10, 10), (14, 13)], |r| {
            asked.push(r);
            Ok(frame(r.x, r.y, r.w as u32, r.h as u32).img)
        })
        .unwrap();
        assert_eq!(asked, vec![Rect::new(10, 10, 5, 4)], "one capture of the box");
        assert_eq!(got, vec![(2, 1, 7), (0, 0, 7), (4, 3, 7)], "in the order asked");
        let far = read_points(&[(0, 0), (5000, 5000)], |r| Ok(frame(r.x, r.y, 1, 1).img)).unwrap();
        assert_eq!(far, vec![(0, 0, 7), (0, 0, 7)]);
        let e = read_points(&[(0, 0), (5000, 5000)], |r| if r.x == 0 { Ok(ramp(1, 1)) } else { Err("no".to_string()) });
        assert_eq!(e, Err("no".to_string()), "one failed capture fails the call");
        let short = read_points(&[(0, 0)], |_| Ok(CapturedImage { w: 1, h: 1, rgba: Vec::new() }));
        assert_eq!(short, Err(CAPTURE_FAILED.to_string()), "a capture of the wrong size is never indexed");
        assert_eq!(read_points(&[], |_| panic!("nothing to capture")), Ok(Vec::new()));
        // Several captures: each point from the one that holds it, in the order asked.
        let mut asked = Vec::new();
        let got = read_points(&[(5000, 5000), (0, 0), (5001, 5002), (0, 0)], |r| {
            asked.push(r);
            Ok(frame(r.x, r.y, r.w as u32, r.h as u32).img)
        })
        .unwrap();
        assert_eq!(asked, vec![Rect::new(0, 0, 1, 1), Rect::new(5000, 5000, 2, 3)]);
        assert_eq!(got, vec![(0, 0, 7), (0, 0, 7), (1, 2, 7), (0, 0, 7)]);
    }

    /// A point on no display reads black with nothing asked for it; the others are read in the
    /// order asked and put back in their places; a reader that answers too few fails the call.
    #[test]
    fn black_where_off_reads_only_the_points_on_a_display() {
        let off = |x: i32, _: i32| x < 0;
        let mut handed = Vec::new();
        let got = black_where_off(&[(-9, 0), (10, 10), (-5, 3), (4, 4)], off, |on| {
            handed = on.to_vec();
            Ok(on.iter().map(|&(x, y)| (x as u8, y as u8, 9)).collect())
        });
        assert_eq!(got, Ok(vec![(0, 0, 0), (10, 10, 9), (0, 0, 0), (4, 4, 9)]));
        assert_eq!(handed, vec![(10, 10), (4, 4)]);
        assert_eq!(black_where_off(&[(-1, 0)], off, |_| panic!("asked for a point on no display")), Ok(vec![(0, 0, 0)]));
        assert_eq!(black_where_off(&[], off, |_| panic!("asked for nothing")), Ok(Vec::new()));
        assert_eq!(black_where_off(&[(1, 1), (2, 2)], off, |_| Ok(vec![(1, 1, 1)])), Err(CAPTURE_FAILED.to_string()));
        assert_eq!(black_where_off(&[(1, 1)], off, |_| Err("no".to_string())), Err("no".to_string()));
    }
}
