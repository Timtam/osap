//! Template matching: the needle side of image search, and the scan that looks for it.
//!
//! Pure code — no Lua, no backend state, no platform switch — so every verdict it gives can be
//! pinned by a unit test on CI, and a Mac runs exactly the code a Windows machine does. The
//! bindings that hand templates in from Luau live in `image_search.rs`; the capture that makes
//! the haystack belongs to the backend.
//!
//! The matcher itself moved here from `lib.rs` unchanged: `matches_at`, `probe_order` and the
//! bilinear `resize_rgba` are the same functions, and `tests::reference_equivalence` keeps a
//! verbatim copy of the old `find_template_scaled` to prove that what changed around them —
//! the variant cache, the search sub-rectangle, the bounds checks — did not change a single
//! verdict.

use std::sync::{Arc, Mutex};

use crate::backend::CapturedImage;

/// Largest side a template built from Luau may have.
///
/// A limit on handles only, never on files: the repository already ships a 1209x987 PNG, and
/// a PNG template has always loaded whatever its size. The bound exists because a handle's
/// bytes are handed over by a module at run time, where a wrong `w` is a typo away from a
/// gigabyte.
pub(crate) const MAX_SIDE: u32 = 4096;
/// Largest pixel count a template built from Luau may have (1024x1024, or any shape of the
/// same area). See `MAX_SIDE` for why files are exempt.
pub(crate) const MAX_PIXELS: u64 = 1 << 20;

/// Template pixels compared before all the others — see `probe_order`.
const PROBES: usize = 12;

/// At most this many scaled variants are kept per template.
///
/// The overlay runtime's widest ladder is ten scales (`MATCH_SCALES`), so sixteen holds every
/// variant a real caller asks for while still bounding one that computes a new scale on every
/// call.
const SCALED_CAP: usize = 16;
/// And at most this many bytes of them. The entry count alone does not bound memory: sixteen
/// variants of a large template at scale 2 would be sixteen times four times its size.
const SCALED_BYTES_CAP: usize = 16 << 20;

/// A rectangle inside a captured frame, in that frame's own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// The whole of a `w` x `h` frame.
    pub(crate) fn full(w: u32, h: u32) -> Self {
        Rect { x: 0, y: 0, w, h }
    }

    /// This rectangle cut down to a `w` x `h` frame. Empty (w or h 0) when nothing of it is
    /// inside, which every search treats as "cannot match here".
    pub(crate) fn clipped_to(self, w: u32, h: u32) -> Self {
        let x = self.x.min(w);
        let y = self.y.min(h);
        let r = self.x.saturating_add(self.w).min(w);
        let b = self.y.saturating_add(self.h).min(h);
        Rect { x, y, w: r - x, h: b - y }
    }
}

/// Where a template matched, in the searched frame's pixels, and at which size — a scaled
/// variant's size, when that is what matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A handle-constructor refusal, worded for the module author who made the mistake. The
/// binding prefixes it with the function's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpecError(pub String);

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How a template's pixels are held.
///
/// One variant today. The sparse point templates of the design (a list of checks rather than
/// a bitmap, for signatures that name a few hundred pixels of a much larger box) arrive as a
/// second variant once it is settled what one cell of such a signature is; a match on this
/// enum is where each of them will say how it is compared.
pub(crate) enum Body {
    /// Row-major, top-down RGBA — the layout a PNG decodes to and the layout the matcher has
    /// always read. Alpha 0 marks a wildcard; any other alpha is compared in full, so alpha is
    /// a binary mask and never a weight.
    Dense {
        rgba: Vec<u8>,
        /// Pixel indices to compare FIRST, rarest colour first — see `probe_order`.
        probes: Vec<u32>,
    },
}

/// A decoded template, shared behind an `Arc` by the path cache, by handles, and by every
/// search task the worker is running with it.
pub(crate) struct Decoded {
    pub w: u32,
    pub h: u32,
    pub body: Body,
    /// Scaled variants already built, keyed by the size they were built at.
    ///
    /// Keyed by SIZE rather than by the scale factor, because the resize depends on nothing
    /// else: 1.24 and 1.26 round a 40-pixel template to the same 50 pixels, and the second
    /// one should cost nothing. Built outside the lock and inserted under it; the lock is only
    /// ever held for a lookup or an insert, never for a resize or a scan.
    scaled: Mutex<Vec<((u32, u32), Arc<Decoded>)>>,
}

impl Decoded {
    /// A decoded PNG, exactly as `load_template` has always kept it: no size limit, and a
    /// fully transparent image stays legal (it matches at the first position it is tried at,
    /// as it always has).
    pub(crate) fn from_png_rgba(w: u32, h: u32, rgba: Vec<u8>) -> Self {
        let probes = probe_order(w, h, &rgba);
        Decoded { w, h, body: Body::Dense { rgba, probes }, scaled: Mutex::new(Vec::new()) }
    }

    /// RGBA bytes handed over by a module, with the handle limits applied.
    pub(crate) fn from_rgba(w: u32, h: u32, rgba: Vec<u8>) -> Result<Self, SpecError> {
        check_size(w, h)?;
        let need = (w as usize) * (h as usize) * 4;
        if rgba.len() != need {
            return Err(SpecError(format!(
                "rgba is {} bytes; {w}x{h} RGBA needs {need}",
                rgba.len()
            )));
        }
        Ok(Self::from_png_rgba(w, h, rgba))
    }

    /// RGB bytes handed over by a module: every pixel is compared, so each gets alpha 255.
    pub(crate) fn from_rgb(w: u32, h: u32, rgb: &[u8]) -> Result<Self, SpecError> {
        check_size(w, h)?;
        let need = (w as usize) * (h as usize) * 3;
        if rgb.len() != need {
            return Err(SpecError(format!(
                "rgb is {} bytes; {w}x{h} RGB needs {need}",
                rgb.len()
            )));
        }
        let mut rgba = Vec::with_capacity(need / 3 * 4);
        for px in rgb.chunks_exact(3) {
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
        Ok(Self::from_png_rgba(w, h, rgba))
    }

    /// A captured frame turned into a template.
    ///
    /// Alpha is forced to 255 here even though both backends now deliver opaque captures,
    /// because a template of nothing but wildcards matches everywhere and says nothing — the
    /// one failure a capture must never be allowed to produce silently. It costs one pass over
    /// bytes that were just copied anyway.
    pub(crate) fn from_capture(cap: CapturedImage) -> Result<Self, SpecError> {
        let CapturedImage { w, h, mut rgba } = cap;
        check_size(w, h)?;
        let need = (w as usize) * (h as usize) * 4;
        if rgba.len() < need {
            return Err(SpecError(format!(
                "the capture came back {} bytes short of {w}x{h}",
                need - rgba.len()
            )));
        }
        rgba.truncate(need);
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Ok(Self::from_png_rgba(w, h, rgba))
    }

    /// How many of its pixels a search compares — the non-wildcard ones.
    pub(crate) fn count(&self) -> u32 {
        match &self.body {
            Body::Dense { rgba, .. } => rgba.chunks_exact(4).filter(|p| p[3] != 0).count() as u32,
        }
    }

    /// The heap this template holds, for the per-VM budget. Scaled variants are not in it:
    /// they are bounded per template by `SCALED_CAP` and `SCALED_BYTES_CAP` instead, and they
    /// are built by searches, not by the module that owns the handle.
    pub(crate) fn heap_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match &self.body {
                Body::Dense { rgba, probes } => rgba.len() + probes.len() * 4,
            }
    }

    /// This template at scale `s`, or `None` when that scale cannot be searched in a
    /// `max_w` x `max_h` area.
    ///
    /// The bounds come FIRST, before anything is built. Without them `scales = {40}` on a large
    /// template would allocate gigabytes to find out that the result cannot fit in the region
    /// anyway — the old code checked the size before resizing too, and that order is kept.
    ///
    /// The size arithmetic is the old `find_template_scaled`'s exactly, including its
    /// tolerance for "1.0": a factor within 1e-4 of one, or any factor that rounds back to the
    /// template's own size, returns the template itself, probes and all.
    pub(crate) fn at_scale(self: &Arc<Self>, s: f32, max_w: u32, max_h: u32) -> Option<Arc<Decoded>> {
        if !s.is_finite() || s <= 0.0 {
            return None;
        }
        let (sw, sh) = if (s - 1.0).abs() < 1e-4 {
            (self.w, self.h)
        } else {
            (((self.w as f32) * s).round() as u32, ((self.h as f32) * s).round() as u32)
        };
        if sw == 0 || sh == 0 || sw > max_w || sh > max_h {
            return None;
        }
        if (sw, sh) == (self.w, self.h) {
            return Some(self.clone());
        }
        if let Some(v) = self.cached(sw, sh) {
            return Some(v);
        }
        // Built outside the lock: a resize of a large template is the slow part, and two
        // searches that both need the variant are better off building it twice than one of
        // them waiting on the other with the rayon pool's thread.
        let built = Arc::new(match &self.body {
            Body::Dense { rgba, .. } => {
                let scaled = resize_rgba(rgba, self.w, self.h, sw, sh);
                // Probes again, which the old code could not give a resized needle: it
                // reused the original's indices nowhere and scanned the scaled one without any.
                // Probes cannot change a verdict (they are a subset of the same conjunction),
                // only how fast the non-matching positions are rejected.
                Decoded::from_png_rgba(sw, sh, scaled)
            }
        });
        let size = built.heap_bytes();
        if size <= SCALED_BYTES_CAP {
            let mut cache = self.scaled.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((_, v)) = cache.iter().find(|(k, _)| *k == (sw, sh)) {
                // Another search built the same variant meanwhile; keep the one already
                // shared, so every caller holds the same bytes.
                return Some(v.clone());
            }
            // Oldest out first until both caps hold. A caller whose scale drifts (one
            // computed from a window width that changes on resize) then keeps its current
            // variant cached instead of being locked out by sizes it no longer uses.
            let mut total: usize = cache.iter().map(|(_, v)| v.heap_bytes()).sum();
            while !cache.is_empty()
                && (cache.len() >= SCALED_CAP || total + size > SCALED_BYTES_CAP)
            {
                let (_, old) = cache.remove(0);
                total -= old.heap_bytes();
            }
            cache.push(((sw, sh), built.clone()));
        }
        Some(built)
    }

    fn cached(&self, sw: u32, sh: u32) -> Option<Arc<Decoded>> {
        // Poison-tolerant: the lock guards a plain list that is valid between any two
        // statements, so a panic elsewhere while it was held leaves nothing to repair — and
        // refusing to read it would turn one contained panic into every later search failing.
        let cache = self.scaled.lock().unwrap_or_else(|e| e.into_inner());
        cache.iter().find(|(k, _)| *k == (sw, sh)).map(|(_, v)| v.clone())
    }

    #[cfg(test)]
    fn scaled_len(&self) -> (usize, usize) {
        let cache = self.scaled.lock().unwrap_or_else(|e| e.into_inner());
        (cache.len(), cache.iter().map(|(_, v)| v.heap_bytes()).sum())
    }
}

/// What a dense `w` x `h` template built from Luau is charged against its VM's budget: its
/// pixels, the most probes it can have, and the struct. An upper bound, taken BEFORE the
/// template exists, so the bytes are refused before they are allocated.
pub(crate) fn dense_bytes(w: u32, h: u32) -> usize {
    std::mem::size_of::<Decoded>() + (w as usize) * (h as usize) * 4 + PROBES * 4
}

/// The handle limits, worded with both numbers so the author can see which one is wrong.
pub(crate) fn check_size(w: u32, h: u32) -> Result<(), SpecError> {
    if w == 0 || h == 0 {
        return Err(SpecError(format!("{w}x{h} is empty; w and h must both be at least 1")));
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(SpecError(format!(
            "{w}x{h} is too large; no side may exceed {MAX_SIDE}"
        )));
    }
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(SpecError(format!(
            "{w}x{h} is {} pixels; the limit is {MAX_PIXELS}",
            (w as u64) * (h as u64)
        )));
    }
    Ok(())
}

/// The order in which a template's pixels should be compared, most discriminating first.
///
/// `matches_at` rejects a candidate position at its first mismatching pixel, so which
/// pixel it looks at first decides how much work the ~700k rejections cost. Row-major
/// order starts at (0,0), which on a wordmark or a label is plain background — measured
/// against a real plugin frame, the top-left pixel of a Cerberus landmark survives at
/// 68.8 % of all candidate positions, while a pixel on a glyph stroke survives at 0.49 %.
/// Testing the second one first is 139x fewer positions that need any further comparison.
///
/// The template alone tells us which pixels those are: its own rarest colours are its
/// content, its commonest is its background. So order by ascending frequency of the
/// coarsely-quantised colour and keep the front of that list. Wildcard (alpha 0) pixels
/// are skipped — they match everything by definition.
///
/// This changes NOTHING about the verdict. It is the same conjunction over the same
/// pixels, evaluated in a better order.
///
/// Linear in the pixel count. It used to sort EVERY pixel index with a hash lookup in the sort
/// key and then keep twelve — tolerable once per PNG, which is cached, but a scaled variant too
/// large for the variant cache (`SCALED_BYTES_CAP`) is rebuilt on every search, and a
/// 1024x1024 template at scale 2 then sorted four million indices per call on the worker. Now
/// the histogram is a flat array over the 4096 possible keys, and the twelve smallest
/// (count, index) pairs are kept in a heap of twelve as the pixels go by. Ties go to the lower
/// index, as the stable sort gave them, so the probes are exactly the ones they were
/// (`tests::probes_are_what_the_old_sort_chose`).
fn probe_order(w: u32, h: u32, rgba: &[u8]) -> Vec<u32> {
    let n = (w as usize) * (h as usize);
    let key = |o: usize| {
        (((rgba[o] >> 4) as usize) << 8) | (((rgba[o + 1] >> 4) as usize) << 4) | (rgba[o + 2] >> 4) as usize
    };
    let mut hist = vec![0u32; 1 << 12];
    for i in 0..n {
        let o = i * 4;
        if rgba[o + 3] != 0 {
            hist[key(o)] += 1;
        }
    }
    // A max-heap of the best PROBES so far: its top is the worst of them, the one to replace.
    let mut best: std::collections::BinaryHeap<(u32, u32)> = std::collections::BinaryHeap::with_capacity(PROBES + 1);
    for i in 0..n {
        let o = i * 4;
        if rgba[o + 3] == 0 {
            continue;
        }
        let cand = (hist[key(o)], i as u32);
        if best.len() < PROBES {
            best.push(cand);
        } else if best.peek().is_some_and(|top| cand < *top) {
            best.pop();
            best.push(cand);
        }
    }
    best.into_sorted_vec().into_iter().map(|(_, i)| i).collect()
}

/// Whether `hay` holds as many bytes as its own dimensions say.
///
/// Every index the scan computes comes from `w` and `h`, so a capture that came back short
/// would read past its end — and a panic on the image worker used to take the worker down for
/// the rest of the session. Answering "no match" is what a failed capture answers anyway.
fn frame_is_whole(hay: &CapturedImage) -> bool {
    hay.rgba.len() >= (hay.w as usize) * (hay.h as usize) * 4
}

/// The first position inside `area` (topmost, then leftmost) where `t` matches at its own
/// size. With `area` the whole frame this is the old `find_template` exactly: the same two
/// loops in the same order over the same positions.
fn scan(hay: &CapturedImage, area: Rect, t: &Decoded, tol: u8) -> Option<(u32, u32)> {
    let Body::Dense { rgba, probes } = &t.body;
    let (tw, th) = (t.w, t.h);
    if tw == 0 || th == 0 || tw > area.w || th > area.h {
        return None;
    }
    if !frame_is_whole(hay) || rgba.len() < (tw as usize) * (th as usize) * 4 {
        return None;
    }
    for oy in area.y..=(area.y + area.h - th) {
        for ox in area.x..=(area.x + area.w - tw) {
            if matches_at(hay, ox, oy, tw, th, rgba, tol, probes) {
                return Some((ox, oy));
            }
        }
    }
    None
}

/// Multi-scale template search over `area` of `hay` (the whole frame when `None`): tries each
/// factor in `scales` in the caller's order and returns the first match, at the size that
/// matched. Empty `scales` is a single pass at 1.0. Resized needles rely on `tol` (bilinear
/// interpolation perturbs pixels), so pair scaling with a non-zero colour tolerance.
///
/// Scales are tried in the order given and nothing else — an exact hit at 1.0 costs nothing
/// extra only when the caller lists 1.0 first, as the overlay runtime's `MATCH_SCALES` does.
/// (The old comment said 1.0 "is tried first"; the code never did that, and reordering now
/// would change which of two matching scales wins.)
pub(crate) fn find(
    hay: &CapturedImage,
    area: Option<Rect>,
    t: &Arc<Decoded>,
    tol: u8,
    scales: &[f32],
) -> Option<Hit> {
    let area = area.unwrap_or(Rect::full(hay.w, hay.h)).clipped_to(hay.w, hay.h);
    let one = [1.0f32];
    let list: &[f32] = if scales.is_empty() { &one } else { scales };
    for &s in list {
        let Some(v) = t.at_scale(s, area.w, area.h) else { continue };
        if let Some((x, y)) = scan(hay, area, &v, tol) {
            return Some(Hit { x, y, w: v.w, h: v.h });
        }
    }
    None
}

/// Every match of `t` at its own size, for deciding whether a template is safe to click
/// blindly (`imageSearchAll`).
///
/// Non-overlapping along a row: after a hit the scan resumes past its right edge on that row,
/// so one match is reported once rather than once per pixel of slop. The next row starts from
/// the left again — unchanged from the binding this came out of.
pub(crate) fn find_all(hay: &CapturedImage, t: &Decoded, tol: u8) -> Vec<(u32, u32)> {
    let Body::Dense { rgba, probes } = &t.body;
    let (tw, th) = (t.w, t.h);
    let mut out = Vec::new();
    // A zero-width template would never advance past a hit, and a short frame would be read
    // past its end; neither can come from a PNG, and both must not hang or panic.
    if tw == 0 || th == 0 || !frame_is_whole(hay) || rgba.len() < (tw as usize) * (th as usize) * 4 {
        return out;
    }
    let mut y = 0;
    while y + th <= hay.h {
        let mut x = 0;
        while x + tw <= hay.w {
            if matches_at(hay, x, y, tw, th, rgba, tol, probes) {
                out.push((x, y));
                x += tw;
            } else {
                x += 1;
            }
        }
        y += 1;
    }
    out
}

/// True if the template sits at (ox, oy) with every non-wildcard pixel inside `tol`.
///
/// `probes` are template pixel indices to test FIRST (see `probe_order`). They are a
/// subset of the same conjunction, so testing them early cannot change the verdict — it
/// only decides how fast the overwhelming majority of positions, which do NOT match, are
/// rejected. Row-major order begins at (0,0), which on a wordmark is background and
/// therefore agrees almost everywhere; measured on a real frame, that first comparison
/// eliminated 31 % of positions where a glyph pixel eliminates 99.5 %.
#[allow(clippy::too_many_arguments)]
fn matches_at(
    hay: &CapturedImage,
    ox: u32,
    oy: u32,
    tw: u32,
    th: u32,
    tmpl: &[u8],
    tol: u8,
    probes: &[u32],
) -> bool {
    let tol = tol as i16;
    let px = |i: u32| -> bool {
        let ti = (i as usize) * 4;
        let (tx, ty) = (i % tw, i / tw);
        let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
        for c in 0..3 {
            if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                return false;
            }
        }
        true
    };
    for &i in probes {
        if !px(i) {
            return false;
        }
    }
    for ty in 0..th {
        for tx in 0..tw {
            let ti = ((ty * tw + tx) * 4) as usize;
            if tmpl[ti + 3] == 0 {
                continue; // transparent template pixel = wildcard
            }
            let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
            for c in 0..3 {
                if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                    return false;
                }
            }
        }
    }
    true
}

/// Resizes an RGBA template (`tw`×`th`) to `sw`×`sh` (bilinear), returning the new
/// raw RGBA bytes. Falls back to the original bytes if the buffer can't be wrapped.
fn resize_rgba(tmpl: &[u8], tw: u32, th: u32, sw: u32, sh: u32) -> Vec<u8> {
    match image::RgbaImage::from_raw(tw, th, tmpl.to_vec()) {
        Some(img) => {
            image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle).into_raw()
        }
        None => tmpl.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The matcher as it stood in `lib.rs` at 09c5640, copied verbatim, so the new code is
    /// compared against what it replaced rather than against a second reading of it.
    mod reference {
        use crate::backend::CapturedImage;

        pub struct Decoded {
            pub w: u32,
            pub h: u32,
            pub rgba: Vec<u8>,
            pub probes: Vec<u32>,
        }

        pub fn probe_order(w: u32, h: u32, rgba: &[u8]) -> Vec<u32> {
            const PROBES: usize = 12;
            let n = (w as usize) * (h as usize);
            let mut hist = std::collections::HashMap::<u32, u32>::new();
            for i in 0..n {
                let o = i * 4;
                if rgba[o + 3] == 0 {
                    continue;
                }
                let key = ((rgba[o] as u32 >> 4) << 8) | ((rgba[o + 1] as u32 >> 4) << 4) | (rgba[o + 2] as u32 >> 4);
                *hist.entry(key).or_insert(0) += 1;
            }
            let mut idx: Vec<u32> = (0..n as u32)
                .filter(|i| rgba[(*i as usize) * 4 + 3] != 0)
                .collect();
            idx.sort_by_key(|i| {
                let o = (*i as usize) * 4;
                let key = ((rgba[o] as u32 >> 4) << 8) | ((rgba[o + 1] as u32 >> 4) << 4) | (rgba[o + 2] as u32 >> 4);
                hist.get(&key).copied().unwrap_or(0)
            });
            idx.truncate(PROBES);
            idx
        }

        pub fn find_template(
            hay: &CapturedImage,
            tw: u32,
            th: u32,
            tmpl: &[u8],
            tol: u8,
            probes: &[u32],
        ) -> Option<(u32, u32)> {
            if tw == 0 || th == 0 || tw > hay.w || th > hay.h {
                return None;
            }
            for oy in 0..=(hay.h - th) {
                for ox in 0..=(hay.w - tw) {
                    if matches_at(hay, ox, oy, tw, th, tmpl, tol, probes) {
                        return Some((ox, oy));
                    }
                }
            }
            None
        }

        #[allow(clippy::too_many_arguments)]
        pub fn matches_at(
            hay: &CapturedImage,
            ox: u32,
            oy: u32,
            tw: u32,
            th: u32,
            tmpl: &[u8],
            tol: u8,
            probes: &[u32],
        ) -> bool {
            let tol = tol as i16;
            let px = |i: u32| -> bool {
                let ti = (i as usize) * 4;
                let (tx, ty) = (i % tw, i / tw);
                let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
                for c in 0..3 {
                    if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                        return false;
                    }
                }
                true
            };
            for &i in probes {
                if !px(i) {
                    return false;
                }
            }
            for ty in 0..th {
                for tx in 0..tw {
                    let ti = ((ty * tw + tx) * 4) as usize;
                    if tmpl[ti + 3] == 0 {
                        continue; // transparent template pixel = wildcard
                    }
                    let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
                    for c in 0..3 {
                        if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                            return false;
                        }
                    }
                }
            }
            true
        }

        pub fn resize_rgba(tmpl: &[u8], tw: u32, th: u32, sw: u32, sh: u32) -> Vec<u8> {
            match image::RgbaImage::from_raw(tw, th, tmpl.to_vec()) {
                Some(img) => {
                    image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle).into_raw()
                }
                None => tmpl.to_vec(),
            }
        }

        pub fn find_template_scaled(
            hay: &CapturedImage,
            tw: u32,
            th: u32,
            tmpl: &[u8],
            tol: u8,
            scales: &[f32],
            probes: &[u32],
        ) -> Option<(u32, u32, u32, u32)> {
            let one = [1.0f32];
            let list: &[f32] = if scales.is_empty() { &one } else { scales };
            for &s in list {
                if s <= 0.0 {
                    continue;
                }
                let (sw, sh) = if (s - 1.0).abs() < 1e-4 {
                    (tw, th)
                } else {
                    (((tw as f32) * s).round() as u32, ((th as f32) * s).round() as u32)
                };
                if sw == 0 || sh == 0 || sw > hay.w || sh > hay.h {
                    continue;
                }
                let hit = if sw == tw && sh == th {
                    find_template(hay, tw, th, tmpl, tol, probes)
                } else {
                    let scaled = resize_rgba(tmpl, tw, th, sw, sh);
                    find_template(hay, sw, sh, &scaled, tol, &[])
                };
                if let Some((ox, oy)) = hit {
                    return Some((ox, oy, sw, sh));
                }
            }
            None
        }

        /// The body of the old `imageSearchAll` binding, minus the Lua tables.
        pub fn find_all(cap: &CapturedImage, tmpl: &Decoded, tol: u8) -> Vec<(u32, u32)> {
            let mut out = Vec::new();
            let (tw, th) = (tmpl.w, tmpl.h);
            let mut y = 0;
            while y + th <= cap.h {
                let mut x = 0;
                while x + tw <= cap.w {
                    if matches_at(cap, x, y, tw, th, &tmpl.rgba, tol, &tmpl.probes) {
                        out.push((x, y));
                        x += tw;
                    } else {
                        x += 1;
                    }
                }
                y += 1;
            }
            out
        }
    }

    /// A small seeded generator, so a failing case can be replayed and no dependency is added.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            if n == 0 { 0 } else { self.next() % n }
        }
        fn byte(&mut self) -> u8 {
            self.next() as u8
        }
    }

    fn frame(w: u32, h: u32, rgba: Vec<u8>) -> CapturedImage {
        CapturedImage { w, h, rgba }
    }

    /// Writes `t` (w x h RGBA) into `hay` at (x, y), leaving the haystack's pixel where the
    /// template is a wildcard — which is what a masked template exists to ignore.
    fn plant(hay: &mut CapturedImage, x: u32, y: u32, w: u32, h: u32, t: &[u8]) {
        for ty in 0..h {
            for tx in 0..w {
                let ti = ((ty * w + tx) * 4) as usize;
                if t[ti + 3] == 0 {
                    continue;
                }
                let hi = (((y + ty) * hay.w + (x + tx)) * 4) as usize;
                hay.rgba[hi..hi + 3].copy_from_slice(&t[ti..ti + 3]);
                hay.rgba[hi + 3] = 255;
            }
        }
    }

    /// One random case: a haystack (noise or a small palette, so near-misses are common), a
    /// template (sometimes masked), planted zero, one or two times, then nudged by exactly
    /// `tol` or by `tol + 1` in places.
    fn case(rng: &mut Lcg) -> (CapturedImage, u32, u32, Vec<u8>, u8) {
        let palette: Vec<[u8; 3]> =
            (0..3).map(|_| [rng.byte(), rng.byte(), rng.byte()]).collect();
        let few_colours = rng.below(2) == 0;
        let (hw, hh) = (8 + rng.below(40), 8 + rng.below(30));
        let mut hay = frame(hw, hh, vec![0; (hw * hh * 4) as usize]);
        for px in hay.rgba.chunks_exact_mut(4) {
            let c = if few_colours {
                palette[rng.below(3) as usize]
            } else {
                [rng.byte(), rng.byte(), rng.byte()]
            };
            px.copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let (tw, th) = (1 + rng.below(7), 1 + rng.below(6));
        let masked = rng.below(3) == 0;
        let mut t = vec![0u8; (tw * th * 4) as usize];
        for px in t.chunks_exact_mut(4) {
            let c = if few_colours {
                palette[rng.below(3) as usize]
            } else {
                [rng.byte(), rng.byte(), rng.byte()]
            };
            let a = if masked && rng.below(3) == 0 { 0 } else { 1 + rng.below(255) as u8 };
            px.copy_from_slice(&[c[0], c[1], c[2], a]);
        }
        let tol = [0u8, 1, 8, 20, 255][rng.below(5) as usize];
        if tw <= hw && th <= hh {
            for _ in 0..rng.below(3) {
                // The bottom-right corner is planted on purpose now and then: it is the last
                // position the loops reach, and an off-by-one there is invisible elsewhere.
                let (x, y) = if rng.below(4) == 0 {
                    (hw - tw, hh - th)
                } else {
                    (rng.below(hw - tw + 1), rng.below(hh - th + 1))
                };
                plant(&mut hay, x, y, tw, th, &t);
                // Nudge one planted pixel by exactly the tolerance (must still match) or by one
                // more (must not) — the boundary the comparison is about.
                let (nx, ny) = (x + rng.below(tw), y + rng.below(th));
                let hi = (((ny * hw) + nx) * 4) as usize + rng.below(3) as usize;
                let by = if rng.below(2) == 0 { tol as i16 } else { tol as i16 + 1 };
                hay.rgba[hi] = (hay.rgba[hi] as i16 + by).clamp(0, 255) as u8;
            }
        }
        (hay, tw, th, t, tol)
    }

    /// The new matcher gives the old one's verdict, position and size, case for case.
    ///
    /// Issue 15 of the design critique: on plain random noise the two would agree simply
    /// because neither ever matches, which proves nothing. So the template is PLANTED, at
    /// random places and at the far corner, masked a third of the time, and nudged by exactly
    /// the tolerance and by one more; the palette cases make partial matches — and so the
    /// probes' early exits — the common path rather than the rare one. For scaled cases the
    /// RESIZED needle is planted, so the scaled scans have something to find.
    #[test]
    fn reference_equivalence() {
        let mut rng = Lcg(0x5eed_cafe);
        let scale_lists: [&[f32]; 7] = [
            &[],
            &[1.0],
            &[0.9, 1.1],
            &[1.5, 1.0, 0.5],
            &[2.0, 0.75, 1.25],
            &[f32::NAN, -1.0, 0.0, 40.0, 1.0],
            &[1.01, 0.99, f32::INFINITY],
        ];
        let (mut matched, mut scaled_hits, mut masked_hits) = (0, 0, 0);
        for i in 0..600 {
            let (mut hay, tw, th, t, tol) = case(&mut rng);
            let scales = scale_lists[i % scale_lists.len()];
            // Plant a resized needle as well for the scaled lists, so a scaled hit is possible.
            if let Some(&s) = scales.iter().find(|s| s.is_finite() && **s > 0.0 && **s != 1.0) {
                let (sw, sh) = (((tw as f32) * s).round() as u32, ((th as f32) * s).round() as u32);
                if sw > 0 && sh > 0 && sw <= hay.w && sh <= hay.h && rng.below(2) == 0 {
                    let scaled = reference::resize_rgba(&t, tw, th, sw, sh);
                    let (x, y) = (rng.below(hay.w - sw + 1), rng.below(hay.h - sh + 1));
                    plant(&mut hay, x, y, sw, sh, &scaled);
                }
            }
            let old_probes = reference::probe_order(tw, th, &t);
            let old = reference::find_template_scaled(&hay, tw, th, &t, tol, scales, &old_probes);
            let dec = Arc::new(Decoded::from_png_rgba(tw, th, t.clone()));
            // Twice: the second call is served from the variant cache, and must agree too.
            for pass in 0..2 {
                let new = find(&hay, None, &dec, tol, scales).map(|h| (h.x, h.y, h.w, h.h));
                assert_eq!(new, old, "case {i} pass {pass}: tol {tol}, scales {scales:?}, template {tw}x{th}");
            }
            let old_all = reference::find_all(
                &hay,
                &reference::Decoded { w: tw, h: th, rgba: t.clone(), probes: old_probes },
                tol,
            );
            assert_eq!(find_all(&hay, &dec, tol), old_all, "case {i}: find_all");
            if let Some((_, _, w, h)) = old {
                matched += 1;
                if (w, h) != (tw, th) {
                    scaled_hits += 1;
                }
                if t.chunks_exact(4).any(|p| p[3] == 0) {
                    masked_hits += 1;
                }
            }
        }
        // Guard against the vacuous version of this test: it must see hits and misses, scaled
        // hits and masked hits in numbers, or agreement proves nothing about those paths.
        eprintln!("equivalence: {matched} hits of 600, {scaled_hits} scaled, {masked_hits} masked");
        assert!((200..=500).contains(&matched), "{matched} of 600 cases matched");
        assert!(scaled_hits >= 20, "only {scaled_hits} scaled hits");
        assert!(masked_hits >= 20, "only {masked_hits} masked hits");
    }

    /// The linear `probe_order` picks the same pixels, in the same order, as the sort it
    /// replaced — on templates with few colours, where ties are everywhere, with wildcards, and
    /// with fewer compared pixels than there are probes.
    #[test]
    fn probes_are_what_the_old_sort_chose() {
        let mut rng = Lcg(0x5eed_0f_9e0b);
        for case in 0..400 {
            let w = 1 + rng.below(40);
            let h = 1 + rng.below(40);
            let palette = 1 + rng.below(if case % 2 == 0 { 3 } else { 40 });
            let colours: Vec<[u8; 3]> = (0..palette).map(|_| [rng.byte(), rng.byte(), rng.byte()]).collect();
            let mut t = Vec::with_capacity((w * h * 4) as usize);
            for _ in 0..w * h {
                let c = colours[rng.below(palette) as usize];
                let a = match rng.below(4) {
                    0 if case % 3 == 0 => 0,
                    1 => 1 + rng.byte() % 254,
                    _ => 255,
                };
                t.extend_from_slice(&[c[0], c[1], c[2], a]);
            }
            assert_eq!(probe_order(w, h, &t), reference::probe_order(w, h, &t), "case {case}: {w}x{h}");
        }
    }

    #[test]
    fn first_hit_is_topmost_then_leftmost() {
        let mut hay = frame(10, 10, vec![0; 400]);
        for px in hay.rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let t = vec![200, 10, 10, 255];
        plant(&mut hay, 7, 2, 1, 1, &t);
        plant(&mut hay, 2, 5, 1, 1, &t);
        plant(&mut hay, 1, 5, 1, 1, &t);
        let dec = Arc::new(Decoded::from_png_rgba(1, 1, t));
        assert_eq!(find(&hay, None, &dec, 0, &[]), Some(Hit { x: 7, y: 2, w: 1, h: 1 }));
        // Below that row, the leftmost of the two wins.
        let area = Rect { x: 0, y: 3, w: 10, h: 7 };
        assert_eq!(find(&hay, Some(area), &dec, 0, &[]), Some(Hit { x: 1, y: 5, w: 1, h: 1 }));
    }

    #[test]
    fn alpha_zero_is_a_wildcard_and_alpha_one_is_compared() {
        let hay = frame(2, 1, vec![10, 10, 10, 255, 99, 99, 99, 255]);
        // Second pixel is a wildcard: matches whatever is there.
        let dec = Arc::new(Decoded::from_png_rgba(2, 1, vec![10, 10, 10, 255, 0, 0, 0, 0]));
        assert!(find(&hay, None, &dec, 0, &[]).is_some());
        // Alpha 1 is compared in full: alpha is a mask, never a weight.
        let dec = Arc::new(Decoded::from_png_rgba(2, 1, vec![10, 10, 10, 255, 0, 0, 0, 1]));
        assert!(find(&hay, None, &dec, 0, &[]).is_none());
    }

    #[test]
    fn a_fully_transparent_file_hits_the_origin() {
        let hay = frame(4, 4, vec![7; 64]);
        let dec = Arc::new(Decoded::from_png_rgba(2, 2, vec![0; 16]));
        assert_eq!(find(&hay, None, &dec, 0, &[]), Some(Hit { x: 0, y: 0, w: 2, h: 2 }));
        assert_eq!(dec.count(), 0);
    }

    #[test]
    fn a_short_frame_is_no_match_not_a_panic() {
        let short = frame(10, 10, vec![0; 40]);
        let dec = Arc::new(Decoded::from_png_rgba(1, 1, vec![0, 0, 0, 255]));
        assert_eq!(find(&short, None, &dec, 0, &[]), None);
        assert!(find_all(&short, &dec, 0).is_empty());
    }

    /// The within rectangle limits where a hit may START and END, and a template larger than
    /// it cannot match there at all.
    #[test]
    fn a_search_area_is_honoured_and_clipped() {
        let mut hay = frame(12, 8, vec![0; 12 * 8 * 4]);
        let t = vec![255, 255, 255, 255, 255, 255, 255, 255];
        plant(&mut hay, 1, 1, 2, 1, &t);
        plant(&mut hay, 9, 6, 2, 1, &t);
        let dec = Arc::new(Decoded::from_png_rgba(2, 1, t));
        let right = Rect { x: 6, y: 0, w: 6, h: 8 };
        assert_eq!(find(&hay, Some(right), &dec, 0, &[]), Some(Hit { x: 9, y: 6, w: 2, h: 1 }));
        // Cut off one pixel short of the right-hand hit: nothing.
        let short = Rect { x: 6, y: 0, w: 4, h: 8 };
        assert_eq!(find(&hay, Some(short), &dec, 0, &[]), None);
        // Larger than the frame: clipped, not an out-of-bounds read.
        let huge = Rect { x: 5, y: 5, w: 1000, h: 1000 };
        assert_eq!(find(&hay, Some(huge), &dec, 0, &[]), Some(Hit { x: 9, y: 6, w: 2, h: 1 }));
        // Entirely outside: empty, so no match.
        let outside = Rect { x: 50, y: 50, w: 10, h: 10 };
        assert_eq!(find(&hay, Some(outside), &dec, 0, &[]), None);
        // Narrower than the template.
        let narrow = Rect { x: 0, y: 0, w: 1, h: 8 };
        assert_eq!(find(&hay, Some(narrow), &dec, 0, &[]), None);
    }

    #[test]
    fn the_scaled_cache_reuses_and_stays_bounded() {
        let dec = Arc::new(Decoded::from_png_rgba(10, 10, vec![128; 400]));
        let a = dec.at_scale(1.5, 100, 100).unwrap();
        let b = dec.at_scale(1.5, 100, 100).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "the same scale is built once");
        // A different factor that rounds to the same size is the same variant.
        let c = dec.at_scale(1.51, 100, 100).unwrap();
        assert!(Arc::ptr_eq(&a, &c));
        // 1.0 and anything that rounds back to the template's own size is the template itself.
        assert!(Arc::ptr_eq(&dec.at_scale(1.0, 100, 100).unwrap(), &dec));
        assert!(Arc::ptr_eq(&dec.at_scale(1.02, 100, 100).unwrap(), &dec));
        for i in 0..40 {
            dec.at_scale(2.0 + i as f32 * 0.1, 1000, 1000).unwrap();
        }
        let (n, _) = dec.scaled_len();
        assert!(n <= SCALED_CAP, "{n} variants kept");

        // The byte cap: variants of a large template are dropped oldest-first to stay under it.
        let big = Arc::new(Decoded::from_png_rgba(600, 600, vec![1; 600 * 600 * 4]));
        for s in [1.5f32, 1.6, 1.7, 1.8, 1.9, 2.0] {
            big.at_scale(s, 4096, 4096).unwrap();
        }
        let (_, bytes) = big.scaled_len();
        assert!(bytes <= SCALED_BYTES_CAP, "{bytes} bytes of variants kept");
    }

    /// Issue 3 of the critique: the bounds are checked before anything is built.
    #[test]
    fn a_scale_that_cannot_fit_is_refused_before_building() {
        let dec = Arc::new(Decoded::from_png_rgba(64, 64, vec![9; 64 * 64 * 4]));
        assert!(dec.at_scale(40.0, 640, 480).is_none());
        assert!(dec.at_scale(f32::NAN, 640, 480).is_none());
        assert!(dec.at_scale(f32::INFINITY, 640, 480).is_none());
        assert!(dec.at_scale(-2.0, 640, 480).is_none());
        assert!(dec.at_scale(0.0, 640, 480).is_none());
        assert!(dec.at_scale(0.001, 640, 480).is_none(), "rounds to zero pixels");
        assert_eq!(dec.scaled_len().0, 0, "nothing was built for the refused scales");
    }

    #[test]
    fn handle_constructors_validate_their_sizes() {
        let d = Decoded::from_rgb(2, 1, &[1, 2, 3, 4, 5, 6]).unwrap();
        let Body::Dense { rgba, .. } = &d.body;
        assert_eq!(rgba, &vec![1, 2, 3, 255, 4, 5, 6, 255], "rgb becomes opaque rgba");
        assert_eq!(d.count(), 2);

        let e = Decoded::from_rgba(10, 36, vec![0; 1436]).err().unwrap();
        assert!(e.0.contains("1436") && e.0.contains("1440"), "{e}");
        assert!(Decoded::from_rgb(2, 2, &[0; 11]).is_err());
        assert!(Decoded::from_rgba(0, 5, vec![]).is_err());
        assert!(Decoded::from_rgba(5, 0, vec![]).is_err());
        assert!(Decoded::from_rgba(4097, 1, vec![0; 4097 * 4]).is_err());
        let e = Decoded::from_rgba(1025, 1024, vec![0; 1025 * 1024 * 4]).err().unwrap();
        assert!(e.0.contains("limit"), "{e}");
        assert!(Decoded::from_rgba(1024, 1024, vec![0; 1024 * 1024 * 4]).is_ok());
    }

    #[test]
    fn a_capture_template_is_opaque_even_when_the_capture_is_not() {
        let cap = frame(2, 1, vec![1, 2, 3, 0, 4, 5, 6, 0]);
        let d = Decoded::from_capture(cap).unwrap();
        assert_eq!(d.count(), 2, "no pixel of a captured template may be a wildcard");
        assert!(Decoded::from_capture(frame(2, 2, vec![0; 8])).is_err(), "short capture");
    }
}
