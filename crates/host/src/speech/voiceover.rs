//! Saying things through VoiceOver, so they come out in the user's own voice and — the
//! part nothing else can do — on their braille display.
//!
//! Its own file rather than a block inside `speech`, because this is macOS code written
//! without a Mac: `crates/macos-check` borrows it by path and asks the compiler whether it
//! is true, which a module nested inside a file that needs `tts` could not be.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

/// Is VoiceOver up?
///
/// Asked before every line, and not as an optimisation. `tell application "VoiceOver"` goes
/// through Launch Services, and Launch Services **starts an application that is not
/// running**. Speaking through VoiceOver is on by default on macOS, so without this check
/// the first thing the overlay ever says would turn the screen reader on for somebody who
/// had not asked for one.
///
/// By bundle id, the same way `backend::macos::perm` asks it — one lookup against the
/// workspace index rather than a scan of every running application.
///
/// Called from the thread that hands the line over, not from the worker: whether these
/// queries are safe off the main thread is something this project cannot check, and the
/// caller is already on the main thread.
pub fn is_running() -> bool {
    let id = NSString::from_str("com.apple.VoiceOver");
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).is_empty()
}

/// A line for VoiceOver, and whether it displaces what is already waiting.
struct Utterance {
    text: String,
    interrupt: bool,
}

/// VoiceOver, spoken to through `osascript` on a thread of its own.
///
/// Not inline: each line costs a process launch and whatever `osascript` then has to do
/// before VoiceOver answers, and this event loop also carries the keyboard. A screen reader
/// that stalls the keys it is describing is not usable, so the wait happens elsewhere — and
/// the thing being waited on is a child process, whose hangs and crashes are contained in
/// somebody else's address space.
pub struct VoiceOver {
    to_vo: Sender<Utterance>,
    refused_rx: Receiver<String>,
    /// False once VoiceOver has turned us down — not running, or AppleScript control not
    /// allowed. Everything after goes to the fallback without paying for another launch.
    healthy: Arc<AtomicBool>,
    /// Handed over but not yet answered, so `is_speaking` has something true to say.
    pending: Arc<AtomicUsize>,
}

impl VoiceOver {
    pub fn new() -> Self {
        let (to_vo, rx) = channel::<Utterance>();
        let (refused_tx, refused_rx) = channel::<String>();
        let healthy = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicUsize::new(0));
        let (h, p) = (healthy.clone(), pending.clone());
        std::thread::Builder::new()
            .name("voiceover".into())
            .spawn(move || run(rx, refused_tx, h, p))
            .ok();
        Self { to_vo, refused_rx, healthy, pending }
    }

    /// Hands `text` to VoiceOver. `false` means it cannot be, and the caller has to say
    /// it another way — now, not later.
    pub fn say(&self, text: &str, interrupt: bool) -> bool {
        if !self.healthy.load(Ordering::Relaxed) {
            return false;
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        if self.to_vo.send(Utterance { text: text.to_string(), interrupt }).is_err() {
            self.pending.fetch_sub(1, Ordering::Relaxed);
            self.healthy.store(false, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub fn pending(&self) -> bool {
        self.pending.load(Ordering::Relaxed) > 0
    }

    pub fn refused(&self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(t) = self.refused_rx.try_recv() {
            out.push(t);
        }
        out
    }

    /// Try VoiceOver again after a failure — what ticking the setting means.
    pub fn rearm(&self) {
        if !self.healthy.swap(true, Ordering::Relaxed) {
            crate::logging::line("speech", "trying VoiceOver again, because its setting was ticked");
        }
    }
}

fn run(
    rx: Receiver<Utterance>,
    refused: Sender<String>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
) {
    let mut reported = false;
    let mut cost = Cost::default();
    while let Ok(mut u) = rx.recv() {
        let mut dropped = 0usize;
        // An interrupting line makes everything still waiting stale. Saying those anyway
        // would mean announcing where the cursor USED to be, several controls late,
        // which is exactly the failure a screen reader must not have.
        if u.interrupt {
            loop {
                match rx.try_recv() {
                    Ok(next) => {
                        dropped += 1;
                        u = next;
                    }
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        let started = Instant::now();
        let outcome = output(&u.text);
        let took = started.elapsed().as_millis() as u64;
        let left = pending.fetch_sub(1 + dropped, Ordering::Relaxed) - (1 + dropped);
        cost.record(took, u.text.chars().count(), left == 0);
        if let Err(why) = outcome {
            healthy.store(false, Ordering::Relaxed);
            if !reported {
                reported = true;
                crate::logging::line(
                    "speech",
                    &format!(
                        "VoiceOver would not take what we said, so the overlay's own \
                         voice takes over from here: {why}. With VoiceOver running, this \
                         is usually \"Allow VoiceOver to be controlled with AppleScript\" \
                         in VoiceOver Utility's General pane, or this application not \
                         yet being allowed under Privacy & Security, Automation."
                    ),
                );
            }
            let _ = refused.send(u.text);
        }
    }
}

/// What a spoken line costs, because nobody here can measure it.
///
/// This path launches a process per line, and that process has work to do before VoiceOver
/// hears anything. How that time divides — process launch, resolving VoiceOver's scripting
/// terminology, compiling the one-line script, the event round trip — is **not known**, and
/// neither is the question underneath it: whether `output` returns as soon as VoiceOver has
/// the text, or blocks until the phrase has been spoken. If it blocks, the time is speech
/// duration and no change of transport is worth writing; the alternatives (an
/// `NSAppleScript` kept compiled, the Apple Event built directly) would each buy nothing.
///
/// So the numbers logged here are shaped to tell those two apart rather than to look
/// impressive. **The character count is the point:** if the milliseconds track the length of
/// the line, `output` blocks on speech and the question is closed. If they are flat across a
/// three-word line and a forty-word one, it is fixed overhead and worth attacking. The
/// one-off bare-spawn baseline separates the launch from everything after it.
///
/// The first line is reported whatever it cost, because it is the cold one and the worst
/// case anybody will hear; after that a summary every [`REPORT_EVERY`] lines, and any single
/// line over [`SLOW_LINE_MS`] on its own. Not behind tracing — this is the number the next
/// remote session has to come back with, and it must not depend on somebody having ticked
/// something first.
#[derive(Default)]
struct Cost {
    n: u64,
    total_ms: u64,
    min_ms: u64,
    max_ms: u64,
    baseline_done: bool,
}

/// A line slower than this is worth naming on its own: it is past the point where an
/// announcement stops feeling like a response to the keystroke that caused it.
const SLOW_LINE_MS: u64 = 150;
const REPORT_EVERY: u64 = 50;

impl Cost {
    /// `idle` says the queue is empty — the only moment the baseline may be taken, because
    /// it is a whole process launch and nothing is allowed to delay a real announcement.
    fn record(&mut self, ms: u64, chars: usize, idle: bool) {
        self.n += 1;
        self.total_ms += ms;
        self.max_ms = self.max_ms.max(ms);
        self.min_ms = if self.n == 1 { ms } else { self.min_ms.min(ms) };
        if self.n == 1 {
            crate::logging::line(
                "speech",
                &format!(
                    "the first line through VoiceOver took {ms} ms for {chars} characters \
                     — the cold one, and the worst case anybody will hear"
                ),
            );
        } else if ms >= SLOW_LINE_MS {
            crate::logging::line(
                "speech",
                &format!("a line through VoiceOver took {ms} ms for {chars} characters"),
            );
        }
        if self.n % REPORT_EVERY == 0 {
            crate::logging::line(
                "speech",
                &format!(
                    "{} lines through VoiceOver: {} ms on average, {} at best, {} at worst",
                    self.n,
                    self.total_ms / self.n,
                    self.min_ms,
                    self.max_ms
                ),
            );
        }
        if idle && !self.baseline_done {
            self.baseline_done = true;
            baseline();
        }
    }
}

/// One `osascript` that talks to nobody, timed once per session.
///
/// It runs a script with no `tell application` in it: no target to resolve, no scripting
/// terminology to read, no Apple Event. What it costs is therefore the launch and the
/// interpreter coming up, and nothing else. Subtracting it from a real line is the only way
/// anyone gets to say which half of the cost is which — and the difference decides whether
/// there is anything worth rewriting.
///
/// Taken only when the queue has run dry, so it never sits in front of something to say.
fn baseline() {
    let started = Instant::now();
    let ran = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg("return 1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let ms = started.elapsed().as_millis();
    match ran {
        Ok(_) => crate::logging::line(
            "speech",
            &format!(
                "an osascript that talks to nobody took {ms} ms — that is the launch alone, \
                 so a real line minus this is what going to VoiceOver costs"
            ),
        ),
        Err(e) => crate::logging::line("speech", &format!("could not time a bare osascript: {e}")),
    }
}

/// How long a single line is given before the child is killed.
///
/// Not a latency control — it is an un-wedge. `output` may legitimately take a while, and
/// until somebody has measured it nobody knows how long is legitimate. What this rules out
/// is the failure with no bottom: a child that never exits parks the worker forever, so
/// `pending` never drains, `is_speaking` stays true for the rest of the session, no refusal
/// is ever sent, the fallback never speaks, and the overlay goes silent with nothing in the
/// log. A consent dialog waiting for an answer a blind user cannot see does exactly that.
const CHILD_LIMIT: Duration = Duration::from_secs(5);

/// `tell application "VoiceOver" to output "…"` — VoiceOver's own voice, its rate, and
/// its braille display, which is the entire point of going through it.
///
/// Whether that interrupts what VoiceOver is already saying is VoiceOver's decision, not
/// ours; `interrupt` above governs only our own queue, which is all we can honour.
fn output(text: &str) -> Result<(), String> {
    let script =
        format!("tell application \"VoiceOver\" to output \"{}\"", applescript_string(text));
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("osascript could not be started ({e})"))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(format!("lost track of osascript ({e})")),
        }
        let waited = started.elapsed();
        if waited >= CHILD_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "osascript did not come back within {} s and was stopped — VoiceOver is \
                 wedged, or something is waiting for an answer on screen",
                CHILD_LIMIT.as_secs()
            ));
        }
        // Fine-grained at first and coarse afterwards, because this poll is inside the
        // number being measured: a flat 5 ms tick would put a 5 ms floor under every
        // reading and the measurement would be of the sleep.
        std::thread::sleep(if waited < Duration::from_millis(50) {
            Duration::from_millis(1)
        } else {
            Duration::from_millis(10)
        });
    };

    if status.success() {
        return Ok(());
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut stderr);
    }
    let why = stderr.trim();
    Err(if why.is_empty() { format!("exit {status}") } else { why.to_string() })
}

/// The body of an AppleScript string literal.
///
/// The text is a plugin's, so it can hold anything a plugin can draw. It reaches
/// `osascript` as one argument, with no shell in between, so this is the only escaping
/// layer there is: backslash and quote, and control characters folded to spaces so that
/// a stray newline cannot close the literal and open a statement.
fn applescript_string(text: &str) -> String {
    let mut s = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\\\""),
            c if (c as u32) < 0x20 => s.push(' '),
            c => s.push(c),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::applescript_string;

    #[test]
    fn a_quote_in_a_plugin_label_cannot_end_the_script() {
        assert_eq!(applescript_string("a4, +12 cents"), "a4, +12 cents");
        assert_eq!(applescript_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(applescript_string("C:\\path"), "C:\\\\path");
        assert_eq!(applescript_string("two\nlines"), "two lines");
    }
}
