//! `automation-platform capture-check`: the screen captured the three ways the host captures it,
//! on the Mac it runs on, with the path of `capture.rs` that answered each — and, with
//! `--older-functions`, with ScreenCaptureKit's rectangle capture kept out of the process, so
//! that the two older functions looked up at run time answer, as on every Mac from 12.3 to 15.1.
//!
//! Why the switch: the rectangle capture arrived in macOS 15.2, and GitHub retired its last
//! runner older than that (macos-14) in November 2026. Every runner left answers from
//! ScreenCaptureKit and never calls the older two, which still exist there at run time; a run
//! with ScreenCaptureKit kept out is what CI has of the path a Mac below 15.2 takes. Only this
//! subcommand keeps it out — `capture::ask_older_functions_only` is called here and nowhere
//! else — and the application's own run never reaches this file.
//!
//! What it captures: 200x60 points at the top left of the main display, the capture probe's
//! region (`tools/capture-probe`), which holds the menu bar's corner — at one pixel a point, as a
//! region read and a pixel read take it (`capture_region`); at the display's own resolution, as
//! a text read takes it (`capture_backing`); and as a snapshot (`frame`). Each is a `check` line,
//! `ok` or `FAILED`: a capture that gave nothing back fails, and so does one of a single colour
//! over the whole picture, as an empty image is; with `--older-functions`, so does one that
//! ScreenCaptureKit answered, or that no older function did. The paths this macOS has and what
//! the system reports of Screen Recording are data.
//!
//! On a thread of its own, as the host captures on its `screen-capture` thread and its image
//! worker and never on the main thread. Exit status 0 when every check passed, 3 when one failed,
//! 1 when it could not run, 2 for a wrong argument. Every line starts with `CAPTURE CHECK:`; the
//! application's log is never opened, so what `capture.rs` would write there goes nowhere, and
//! what of it the check needs is printed here instead.

use std::collections::HashSet;
use std::io::Write as _;
use std::time::Instant;

use objc2_core_graphics::{CGImage, CGPreflightScreenCaptureAccess};
use objc2_foundation::NSProcessInfo;

use super::capture;

const USAGE: &str = "\
automation-platform capture-check [--older-functions]

Captures 200x60 points at the top left of the main display the three ways the application
captures the screen — at one pixel a point, as a region read and a pixel read do; at the
display's own resolution, as a text read does; and as a snapshot — and says for each what came
back and which way of capturing answered it. With --older-functions, ScreenCaptureKit's
rectangle capture is not asked: every capture goes to CGWindowListCreateImage or
CGDisplayCreateImageForRect, looked up at run time, as on macOS 12.3 to 15.1. For CI: it opens
no window, loads no module, never speaks and writes nothing to the application's log. Every
line it prints starts with CAPTURE CHECK:, and a check that fails says FAILED. Exit status 0
when every check passed, 3 when one failed, 1 when it could not run, 2 for a wrong argument.";

/// What every capture asks for, in points: the capture probe's region, the main display's top
/// left with the menu bar's corner in it, which no desktop shows in one colour.
const REGION: (i32, i32, i32, i32) = (0, 0, 200, 60);

pub(crate) fn run(args: &[String]) -> i32 {
    let mut older = false;
    for a in args {
        match a.as_str() {
            "--older-functions" => older = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return 0;
            }
            other => {
                eprintln!("capture-check: unknown argument '{other}'\n\n{USAGE}");
                return 2;
            }
        }
    }
    let system = objc2::rc::autoreleasepool(|_| NSProcessInfo::processInfo().operatingSystemVersionString().to_string());
    say(&format!("{system} on {}", std::env::consts::ARCH));
    if older {
        capture::ask_older_functions_only();
        say(
            "forced: ScreenCaptureKit's rectangle capture is not asked in this run (--older-functions); every capture \
             goes to the older functions, looked up at run time, as on macOS 12.3 to 15.1",
        );
    } else {
        say("as the application captures: ScreenCaptureKit's rectangle capture first where this macOS has it, then the older functions");
    }
    say(&capture::paths_line());
    say(&format!(
        "screen recording: the system reports this process as {} (data: without it a capture shows the desktop \
         with other applications' windows removed)",
        if CGPreflightScreenCaptureAccess() { "permitted" } else { "NOT permitted" }
    ));
    let failed = std::thread::Builder::new()
        .name("capture-check".to_string())
        .spawn(move || captures(older))
        .ok()
        .and_then(|t| t.join().ok());
    let Some(failed) = failed else {
        say("cannot run: the capture thread did not finish");
        return 1;
    };
    say(&format!("done; {failed} of 3 checks failed"));
    if failed > 0 {
        3
    } else {
        0
    }
}

/// One line of output, written at once.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "CAPTURE CHECK: {line}");
    let _ = out.flush();
}

/// What came back of one capture: its size in pixels, its pixels' colours counted, and anything
/// else its line says of it.
struct Got {
    w: usize,
    h: usize,
    colours: usize,
    more: String,
}

/// The three captures, each judged as it came back; the number that failed.
fn captures(older: bool) -> usize {
    let (x, y, w, h) = REGION;
    // Nothing has captured in this process yet; from here on the paths are asked per capture.
    let _ = capture::take_paths_used();
    let mut failed = 0;

    let started = Instant::now();
    let got = capture::capture_region(x, y, w, h)
        .map(|c| Got { w: c.w as usize, h: c.h as usize, colours: colours(&c.rgba), more: String::new() });
    let what = format!("region {w}x{h} at {x},{y}, one pixel a point (a region read, a pixel read)");
    failed += judge(&what, older, got, started) as usize;

    let started = Instant::now();
    let got = capture::capture_backing(x, y, w, h).map(|(image, scale)| {
        let (iw, ih) = (CGImage::width(Some(&image)), CGImage::height(Some(&image)));
        let rgba = capture::image_to_rgba(&image, iw as i32, ih as i32, 0, 0, iw as i32, ih as i32);
        Got { w: iw, h: ih, colours: rgba.as_deref().map_or(0, colours), more: format!(", {scale:.2}x") }
    });
    let what = format!("region {w}x{h} at {x},{y}, at the display's own resolution (a text read)");
    failed += judge(&what, older, got, started) as usize;

    let started = Instant::now();
    let got = capture::frame(x, y, w, h, false).ok().map(|f| Got {
        w: f.img.w as usize,
        h: f.img.h as usize,
        colours: colours(&f.img.rgba),
        more: format!(", via {}", f.via.word()),
    });
    let what = format!("snapshot {w}x{h} at {x},{y}");
    failed += judge(&what, older, got, started) as usize;

    failed
}

/// One capture's `check` line: what came back, how long it took from `started`, and which path
/// answered. True when it failed.
fn judge(what: &str, older: bool, got: Option<Got>, started: Instant) -> bool {
    let ms = started.elapsed().as_millis();
    let paths = capture::take_paths_used();
    let by = if paths.is_empty() {
        "no path".to_string()
    } else {
        paths.iter().map(|p| p.1).collect::<Vec<_>>().join(" and ")
    };
    let fault = match &got {
        None => Some("nothing came back"),
        Some(g) if g.colours < 2 => Some("one colour over the whole picture, as an empty image is"),
        Some(_) if older && paths.iter().any(|p| !p.0) => {
            Some("ScreenCaptureKit answered, though --older-functions keeps it out")
        }
        Some(_) if older && paths.is_empty() => Some("no older function answered"),
        Some(_) => None,
    };
    let seen = got.map_or(String::new(), |g| format!(", {}x{} px{}, {} colours", g.w, g.h, g.more, g.colours));
    let forced = if older { ", the older functions forced" } else { "" };
    let verdict = fault.map_or("ok".to_string(), |f| format!("FAILED, {f}"));
    say(&format!("check {what}: {verdict}{seen}, {ms} ms, answered by {by}{forced}"));
    fault.is_some()
}

/// How many colours the pixels hold, alpha aside.
fn colours(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).map(|px| [px[0], px[1], px[2]]).collect::<HashSet<_>>().len()
}
