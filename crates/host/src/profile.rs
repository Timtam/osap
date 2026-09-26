//! `host.screen.profile`'s reduction: a part of a picture reduced to a few statistics per
//! column and per row. The binding (`lib.rs`) reads the arguments, takes the picture — a live
//! capture, or a snapshot's — and builds the Luau tables; the loop over the pixels is here.
//!
//! It is the loop that was inline in the binding, moved unchanged, with one addition: it reads
//! an `Area` of the image instead of all of it, so a snapshot's region is profiled where it
//! lies without copying it out first. For a live capture the area is the whole image, and the
//! answer is what it always was (`tests::profile_core_matches_legacy` holds it to a copy of the
//! old loop).
//!
//! Pure: no Lua, no OS. Borrowed by `crates/macos-check`.

use crate::backend::frame::Area;
use crate::backend::CapturedImage;

/// One axis of a profile: per column (or per row), the darkest luminance, the lightest, how many
/// pixels are darker than the threshold, the mean luminance, and the mean of each channel.
///
/// The means are REAL numbers, not truncated to a byte. Integer division looked harmless and is
/// not: a mean over 522 rows moves by less than one whole unit when a note-sized shape changes
/// inside it, so `as u8` would floor exactly the signal a caller is looking for down to zero.
/// `min` and `max` stay bytes because they ARE bytes — a particular pixel's value.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub min: Vec<u8>,
    pub max: Vec<u8>,
    pub dark: Vec<u32>,
    pub mean: Vec<f64>,
    pub r: Vec<f64>,
    pub g: Vec<f64>,
    pub b: Vec<f64>,
}

/// Accumulators for one axis, one slot per column (or row).
struct Acc {
    min: Vec<u8>,
    max: Vec<u8>,
    dark: Vec<u32>,
    sum: Vec<u64>,
    ch: Vec<[u64; 3]>,
}

impl Acc {
    fn new(n: usize) -> Acc {
        Acc { min: vec![255; n], max: vec![0; n], dark: vec![0; n], sum: vec![0; n], ch: vec![[0; 3]; n] }
    }

    fn add(&mut self, i: usize, l: u8, dark: u8, (r, g, b): (u8, u8, u8)) {
        if l < dark {
            self.dark[i] += 1;
        }
        if l < self.min[i] {
            self.min[i] = l;
        }
        if l > self.max[i] {
            self.max[i] = l;
        }
        self.sum[i] += l as u64;
        self.ch[i][0] += r as u64;
        self.ch[i][1] += g as u64;
        self.ch[i][2] += b as u64;
    }

    /// The axis, each mean over `n` pixels (at least one).
    fn finish(self, n: u64) -> Axis {
        let n = n.max(1) as f64;
        let mean = |v: &[u64]| v.iter().map(|s| *s as f64 / n).collect();
        let channel = |i: usize| self.ch.iter().map(|c| c[i] as f64 / n).collect();
        Axis {
            mean: mean(&self.sum),
            r: channel(0),
            g: channel(1),
            b: channel(2),
            min: self.min,
            max: self.max,
            dark: self.dark,
        }
    }
}

/// The profile of `area` of `img`: its columns when `cols`, its rows when `rows`, counting as
/// dark every pixel whose luminance is below `dark`. `None` when `area` is empty, does not lie
/// inside the image, or the image holds fewer bytes than its own size says — nothing is read
/// then, so no index can run past the buffer.
///
/// Both axes in ONE traversal: the capture is already the whole cost, and walking it twice to
/// keep the code symmetrical would be the only part of this that scales with area. Luminance is
/// ITU-R BT.601, so a coloured mark is weighted the way an eye would weight it.
pub fn profile(img: &CapturedImage, area: Area, cols: bool, rows: bool, dark: u8) -> Option<(Option<Axis>, Option<Axis>)> {
    let (w, h) = (area.w as usize, area.h as usize);
    let inside = area.x as u64 + area.w as u64 <= img.w as u64 && area.y as u64 + area.h as u64 <= img.h as u64;
    if w == 0 || h == 0 || !inside || (img.rgba.len() as u64) < img.w as u64 * img.h as u64 * 4 {
        return None;
    }
    let (mut c, mut r) = (Acc::new(if cols { w } else { 0 }), Acc::new(if rows { h } else { 0 }));
    let stride = img.w as usize * 4;
    for y in 0..h {
        let row = (area.y as usize + y) * stride + area.x as usize * 4;
        for x in 0..w {
            let o = row + x * 4;
            let px = (img.rgba[o], img.rgba[o + 1], img.rgba[o + 2]);
            let l = ((px.0 as u32 * 299 + px.1 as u32 * 587 + px.2 as u32 * 114) / 1000) as u8;
            if cols {
                c.add(x, l, dark, px);
            }
            if rows {
                r.add(y, l, dark, px);
            }
        }
    }
    Some((cols.then(|| c.finish(h as u64)), rows.then(|| r.finish(w as u64))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The loop as it stood inline in `lib.rs`'s `host.screen.profile` binding, copied verbatim
    /// but for the Lua tables — the oracle the new one is held to.
    #[allow(clippy::type_complexity)]
    fn legacy(cap: &CapturedImage, want_cols: bool, want_rows: bool, dark_t: u8) -> (Option<Axis>, Option<Axis>) {
        let (w, h) = (cap.w as usize, cap.h as usize);
        let (mut cdark, mut rdark) = (vec![0u32; w], vec![0u32; h]);
        let (mut cmin, mut cmax) = (vec![255u8; w], vec![0u8; w]);
        let (mut rmin, mut rmax) = (vec![255u8; h], vec![0u8; h]);
        let (mut csum, mut rsum) = (vec![0u64; w], vec![0u64; h]);
        let mut cch = vec![[0u64; 3]; w];
        let mut rch = vec![[0u64; 3]; h];
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                let (r, g, b) = (cap.rgba[o], cap.rgba[o + 1], cap.rgba[o + 2]);
                let l = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
                if want_cols {
                    if l < dark_t {
                        cdark[x] += 1;
                    }
                    if l < cmin[x] {
                        cmin[x] = l;
                    }
                    if l > cmax[x] {
                        cmax[x] = l;
                    }
                    csum[x] += l as u64;
                    cch[x][0] += r as u64;
                    cch[x][1] += g as u64;
                    cch[x][2] += b as u64;
                }
                if want_rows {
                    if l < dark_t {
                        rdark[y] += 1;
                    }
                    if l < rmin[y] {
                        rmin[y] = l;
                    }
                    if l > rmax[y] {
                        rmax[y] = l;
                    }
                    rsum[y] += l as u64;
                    rch[y][0] += r as u64;
                    rch[y][1] += g as u64;
                    rch[y][2] += b as u64;
                }
            }
        }
        let axis = |min: &[u8], max: &[u8], dark: &[u32], sum: &[u64], ch: &[[u64; 3]], n: u64| -> Axis {
            let n = n.max(1) as f64;
            Axis {
                min: min.to_vec(),
                max: max.to_vec(),
                dark: dark.to_vec(),
                mean: sum.iter().map(|s| *s as f64 / n).collect(),
                r: ch.iter().map(|c| c[0] as f64 / n).collect(),
                g: ch.iter().map(|c| c[1] as f64 / n).collect(),
                b: ch.iter().map(|c| c[2] as f64 / n).collect(),
            }
        };
        (
            want_cols.then(|| axis(&cmin, &cmax, &cdark, &csum, &cch, h as u64)),
            want_rows.then(|| axis(&rmin, &rmax, &rdark, &rsum, &rch, w as u64)),
        )
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
        fn below(&mut self, n: u32) -> u32 {
            self.next() % n
        }
    }

    fn noise(rng: &mut Lcg, w: u32, h: u32) -> CapturedImage {
        CapturedImage { w, h, rgba: (0..w * h * 4).map(|_| rng.next() as u8).collect() }
    }

    fn crop(img: &CapturedImage, a: Area) -> CapturedImage {
        let mut rgba = Vec::new();
        for y in a.y..a.y + a.h {
            let o = ((y * img.w + a.x) * 4) as usize;
            rgba.extend_from_slice(&img.rgba[o..o + (a.w * 4) as usize]);
        }
        CapturedImage { w: a.w, h: a.h, rgba }
    }

    /// The whole image through the new loop is the old loop's answer, bit for bit — the means
    /// included, which are the same divisions in the same order.
    #[test]
    fn profile_core_matches_legacy() {
        let mut rng = Lcg(0x9f0f11e);
        for case in 0..200 {
            let (w, h) = (1 + rng.below(40), 1 + rng.below(30));
            let img = noise(&mut rng, w, h);
            let dark = [0u8, 1, 128, 190, 255][case % 5];
            let (cols, rows) = [(true, true), (true, false), (false, true)][case % 3];
            let got = profile(&img, Area::full(w, h), cols, rows, dark).expect("a whole image");
            assert_eq!(got, legacy(&img, cols, rows, dark), "case {case}: {w}x{h}, dark {dark}");
        }
    }

    /// A part of a picture profiles exactly as that part, copied out, would.
    #[test]
    fn a_sub_rect_equals_the_cropped_image() {
        let mut rng = Lcg(0x5ab_7ec7);
        for case in 0..200 {
            let (w, h) = (1 + rng.below(40), 1 + rng.below(30));
            let img = noise(&mut rng, w, h);
            let (x, y) = (rng.below(w), rng.below(h));
            let a = Area { x, y, w: 1 + rng.below(w - x), h: 1 + rng.below(h - y) };
            let got = profile(&img, a, true, true, 100).expect("inside");
            assert_eq!(got, legacy(&crop(&img, a), true, true, 100), "case {case}: {a:?} of {w}x{h}");
        }
    }

    #[test]
    fn nothing_is_read_outside_or_past_a_short_buffer() {
        let img = CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
        assert!(profile(&img, Area { x: 2, y: 0, w: 3, h: 1 }, true, true, 128).is_none(), "past the right edge");
        assert!(profile(&img, Area { x: 0, y: 0, w: 0, h: 4 }, true, true, 128).is_none(), "empty");
        let short = CapturedImage { w: 4, h: 4, rgba: vec![0; 60] };
        assert!(profile(&short, Area::full(4, 4), true, true, 128).is_none());
        let (cols, rows) = profile(&img, Area::full(4, 4), true, false, 128).unwrap();
        assert_eq!((cols.map(|c| c.dark), rows), (Some(vec![4, 4, 4, 4]), None), "only the axis asked for");
    }
}
