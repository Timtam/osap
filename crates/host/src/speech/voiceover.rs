//! Saying things through VoiceOver, so they come out in the user's own voice and — the
//! part nothing else can do — on their braille display.
//!
//! Its own file rather than a block inside `speech`, because this is macOS code written
//! without a Mac: `crates/macos-check` borrows it by path and asks the compiler whether it
//! is true, which a module nested inside a file that needs `tts` could not be.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;

/// A line for VoiceOver, and whether it displaces what is already waiting.
struct Utterance {
    text: String,
    interrupt: bool,
}

/// VoiceOver, spoken to through `osascript` on a thread of its own.
///
/// Not inline: each line costs a process launch plus an AppleScript compile — tens of
/// milliseconds — and this event loop also carries the keyboard. A screen reader that
/// stalls the keys it is describing is not usable, so the wait happens elsewhere.
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
            crate::logging::line(
                "speech",
                "trying VoiceOver again, because its setting was ticked",
            );
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
        let outcome = output(&u.text);
        pending.fetch_sub(1 + dropped, Ordering::Relaxed);
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

/// `tell application "VoiceOver" to output "…"` — VoiceOver's own voice, its rate, and
/// its braille display, which is the entire point of going through it.
///
/// Whether that interrupts what VoiceOver is already saying is VoiceOver's decision, not
/// ours; `interrupt` above governs only our own queue, which is all we can honour.
fn output(text: &str) -> Result<(), String> {
    let script =
        format!("tell application \"VoiceOver\" to output \"{}\"", applescript_string(text));
    let out = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("osascript could not be started ({e})"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let why = stderr.trim();
    Err(if why.is_empty() { format!("exit {}", out.status) } else { why.to_string() })
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
