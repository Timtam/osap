//! The golden test of `host.screen.cells` against an outside game-menu reader's own vectors.
//!
//! His test package is a lossless frame of a commercial game plus what his reader computed
//! from it: two signatures as hex, and a table of every block of each (edges, how many pixels
//! passed, how many were looked at, the byte). The frame must never be committed, so the
//! package lives in a folder of its own under `crates/host/tests-data-local/` (ignored by git)
//! on the machines that have it, and this test SKIPS, saying why, everywhere else. The four
//! region cases of the package, which are only numbers, are also in `region.rs`'s own tests.
//!
//! What it proves is the reduction, byte for byte: bounds, every block's edges and counts,
//! every cell, the hex. Not that a screen gives the same pixels here as in his reader — that is
//! a question about capture, answered only by reading the same menu with both (TODO.md).
//!
//! Everything it needs to know about the package — the folder, the frame's file name and
//! checksum, the vectors' names, regions and grids — is found by what the folder holds and read
//! from the package's `testvectors.json`, so this file names nothing from it.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::backend::CapturedImage;
use crate::cells::{self, CellSpec, Predicate, Sub};
use crate::region::{self, Fraction};

/// His colour test, as his C# source writes it.
const WARM: &str = "(red >= 80 && red*10 >= green*13 && red*10 >= blue*12) || \
                    (red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue)";

/// Where local test data lives: `crates/host/tests-data-local/`, ignored by git.
fn local_data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests-data-local")
}

/// The package: the first folder under [`local_data`], in name order, that holds a
/// `testvectors.json`; `None` when there is none, or no such folder at all.
fn package() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(local_data())
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("testvectors.json").is_file())
        .collect();
    dirs.sort();
    dirs.into_iter().next()
}

/// His region object, `{ xStart, xEnd, yStart, yEnd }`.
fn fraction(r: &Value) -> Fraction {
    let n = |k: &str| r[k].as_f64().unwrap_or_else(|| panic!("region.{k}"));
    Fraction::new(n("xStart"), n("yStart"), n("xEnd"), n("yEnd")).unwrap()
}

fn int(v: &Value, k: &str) -> i64 {
    v[k].as_i64().unwrap_or_else(|| panic!("{k}"))
}

/// `s` of `img` as an image of its own: what a capture of just that region delivers.
fn crop(img: &CapturedImage, s: Sub) -> CapturedImage {
    let mut rgba = Vec::with_capacity((s.w * s.h * 4) as usize);
    for y in s.y..s.y + s.h {
        let from = ((y * img.w + s.x) * 4) as usize;
        rgba.extend_from_slice(&img.rgba[from..from + (s.w * 4) as usize]);
    }
    CapturedImage { w: s.w, h: s.h, rgba }
}

#[test]
fn cells_reproduce_the_readers_own_vectors() {
    let Some(dir) = package() else {
        eprintln!(
            "skipped: no folder under {} holds a testvectors.json. The reference frame and vectors \
             are local test data that is never committed (see the top of \
             crates/host/src/cells_golden_tests.rs).",
            local_data().display()
        );
        return;
    };
    let text = std::fs::read_to_string(dir.join("testvectors.json")).expect("testvectors.json");
    let tv: Value = serde_json::from_str(&text).expect("testvectors.json");

    // The frame, checked to be the one the vectors were computed from.
    let frame = &tv["frame"];
    let bytes = std::fs::read(dir.join(frame["file"].as_str().expect("frame.file"))).expect("the frame");
    assert_eq!(
        hex::encode(Sha256::digest(&bytes)),
        frame["sha256"].as_str().expect("frame.sha256"),
        "not the frame the vectors belong to"
    );
    let png = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).expect("a PNG").to_rgba8();
    let (w, h) = png.dimensions();
    assert_eq!((w as i64, h as i64), (int(frame, "width"), int(frame, "height")));
    let img = CapturedImage { w, h, rgba: png.into_raw() };
    let pred = Predicate::parse(WARM).unwrap();

    let vectors = tv["vectors"].as_array().expect("vectors");
    assert_eq!(vectors.len(), 2);
    let mut signatures = Vec::new();
    for v in vectors {
        let name = v["name"].as_str().expect("name");
        assert_eq!(v["mode"], "warmSelection", "{name}: the colour test this file reproduces");
        let cols = int(v, "signatureWidth") as u32;
        let rows = int(v, "signatureHeight") as u32;
        let spec = CellSpec::new(cols, rows, pred.clone()).unwrap();

        // His bounds, and the same region resolved as a window region over a client area
        // that is the frame.
        let f = fraction(&v["region"]);
        let b = region::frame_bounds(w as i64, h as i64, &f).unwrap();
        let want = &v["bounds1280x1024"];
        assert_eq!(
            (b.x0, b.x1, b.y0, b.y1),
            (int(want, "x0"), int(want, "x1"), int(want, "y0"), int(want, "y1")),
            "{name}: bounds"
        );
        let r = region::resolve(region::Client { x: 0, y: 0, w: w as i64, h: h as i64 }, &f).unwrap();
        assert_eq!((r.x as i64, r.y as i64, r.w as i64, r.h as i64), (b.x0, b.y0, b.x1 - b.x0, b.y1 - b.y0));
        let sub = Sub { x: r.x as u32, y: r.y as u32, w: r.w as u32, h: r.h as u32 };

        // Every block of his table: edges in frame coordinates, counts, and the byte.
        let counts = cells::count(&img, sub, &spec).unwrap();
        let (xb, yb) = (cells::blocks(sub.w, cols), cells::blocks(sub.h, rows));
        let tsv = std::fs::read_to_string(dir.join(format!("{name}.blocks.tsv"))).expect("the block table");
        let mut lines = tsv.lines();
        let header: Vec<&str> = lines.next().expect("a header").trim().split('\t').collect();
        assert_eq!(header, ["sy", "sx", "xStart", "xEnd", "yStart", "yEnd", "matching", "total", "value"]);
        let mut n = 0usize;
        for line in lines.filter(|l| !l.trim().is_empty()) {
            let his: Vec<u64> = line.trim().split('\t').map(|s| s.trim().parse().expect("a number")).collect();
            let (sy, sx) = (his[0] as usize, his[1] as usize);
            let i = sy * cols as usize + sx;
            let ((xs, xe), (ys, ye)) = (xb[sx], yb[sy]);
            let (m, t) = (counts.matching[i], counts.total[i]);
            let ours = [
                sy as u64,
                sx as u64,
                (sub.x + xs) as u64,
                (sub.x + xe) as u64,
                (sub.y + ys) as u64,
                (sub.y + ye) as u64,
                m as u64,
                t as u64,
                cells::cell_value(m, t) as u64,
            ];
            assert_eq!(ours.as_slice(), his.as_slice(), "{name}: cell (row {sy}, column {sx})");
            assert_eq!(cells::round_half_even(255 * m as u64, t as u64), his[8], "{name}: the integer form too");
            n += 1;
        }
        assert_eq!(n, (cols * rows) as usize, "{name}: one line per cell");

        // The signature, as hex, against both of his copies of it.
        let sig = cells::reduce(&img, sub, &spec).unwrap();
        let hexed = cells::to_hex(&sig);
        assert_eq!(hexed, v["hex"].as_str().expect("hex"), "{name}: hex");
        let file_hex = std::fs::read_to_string(dir.join(format!("{name}.hex.txt"))).expect("the hex file");
        assert_eq!(hexed, file_hex.trim(), "{name}: the hex file");
        assert_eq!(cells::from_hex(file_hex.trim().as_bytes(), spec.cells()).unwrap(), sig);

        // And the way the bindings get there: a capture of just the region, read whole.
        assert_eq!(cells::of_capture(&crop(&img, sub), r.w, r.h, &spec).unwrap(), sig, "{name}: from a capture");
        signatures.push(sig);
    }

    // His bounds tests: four regions at two frame sizes each.
    let tests = tv["boundsTests"].as_array().expect("boundsTests");
    assert_eq!(tests.len(), 4);
    for t in tests {
        for (fw, fh) in [(1280i64, 1024i64), (1024, 768)] {
            let want = &t[format!("{fw}x{fh}").as_str()];
            let b = region::frame_bounds(fw, fh, &fraction(&t["region"])).unwrap();
            assert_eq!(
                (b.x0, b.x1, b.y0, b.y1),
                (int(want, "x0"), int(want, "x1"), int(want, "y0"), int(want, "y1")),
                "{} at {fw}x{fh}",
                t["name"]
            );
        }
    }

    // Ranked against each other, each signature is its own best match, and the other one is
    // the runner-up.
    let states: Vec<Box<[u8]>> = signatures.iter().map(|s| s.clone().into_boxed_slice()).collect();
    let across = cells::similarity(&signatures[0], &signatures[1]);
    assert!(across < 1.0);
    for (k, s) in signatures.iter().enumerate() {
        let r = cells::rank(s, &states, &[0, 1]).unwrap();
        assert_eq!((r.state, r.similarity, r.distance, r.runner_up), (k, 1.0, 0, across));
    }
}
