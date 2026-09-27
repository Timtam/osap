//! Saying things through VoiceOver, so they come out in the user's own voice and — the
//! part nothing else can do — on their braille display.
//!
//! Every line here reaches VoiceOver as an Apple Event, which needs the **Automation**
//! permission — a different one from Accessibility, in its own pane, and refused by default.
//! The start-up check and the request live with the other permissions in
//! `backend::macos::perm`, not here. The one question this path asks the system itself —
//! whether an Automation question it found open has been answered since, asked without asking
//! the user — is [`automation_status`], on the speech thread.
//!
//! Its own file rather than a block inside `speech`, because this is macOS code written
//! without a Mac: `crates/macos-check` borrows it by path and asks the compiler whether it
//! is true, which a module nested inside a file that needs `tts` could not be.

use std::cell::{Cell, RefCell};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use objc2_app_kit::NSRunningApplication;
use objc2_core_services::{typeWildCard, AEDeterminePermissionToAutomateTarget};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventSendOptions, NSString};

use super::vo_park::{self, Park, Refusal, Rung};

/// Is VoiceOver up?
///
/// Asked before every line, and not as an optimisation. `tell application "VoiceOver"` goes
/// through Launch Services, and Launch Services **starts an application that is not
/// running**. Turn the setting on with VoiceOver not running and the first thing the overlay
/// said would start the screen reader for somebody who had not asked for one.
///
/// By bundle id, the same way `backend::macos::perm` asks it — one lookup against the
/// workspace index rather than a scan of every running application.
///
/// Called from the thread that hands the line over, not from the worker: whether these
/// queries are safe off the main thread is something this project cannot check, and the
/// caller is already on the main thread.
pub fn is_running() -> bool {
    running_pid().is_some()
}

/// VoiceOver's process id, or `None` when it is not running — the same lookup as
/// [`is_running`], which is this answer's `is_some`. The id is what tells a VoiceOver restarted
/// since a refusal (Command+F5 twice, or after a wake) from the one that refused.
pub fn running_pid() -> Option<i32> {
    let id = NSString::from_str("com.apple.VoiceOver");
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&id)
        .firstObject()
        .map(|app| app.processIdentifier())
}

/// A line for VoiceOver, whether it displaces what is already waiting, and the opening of the
/// gate it was handed over in (`VoiceOver::opened`).
struct Utterance {
    text: String,
    interrupt: bool,
    opening: u64,
}

/// What the worker tells the event loop about the path, besides the lines it hands back.
enum Note {
    /// A line handed over in `opening` was refused.
    Failed { refusal: Refusal, opening: u64 },
    /// A line was taken after one or more were refused.
    Spoke,
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
    notes_rx: Receiver<Note>,
    /// The gate [`say`](Self::say) reads: false from the moment VoiceOver turns a line down
    /// (the worker lowers it) until [`Park`] says to try again, and behind a look's one line
    /// until VoiceOver has answered it. Everything in between goes to the fallback at once,
    /// without paying for another refusal or waiting behind one.
    healthy: Arc<AtomicBool>,
    /// How many times the gate has gone up: each line carries the count it was handed over
    /// under, and the worker does not offer VoiceOver a line from an opening in which it has
    /// already refused one — those were handed over before the refusal lowered the gate.
    opened: Arc<AtomicU64>,
    /// The opening the setting was last ticked in. A refusal of a line handed over before the
    /// tick is not held against the path the tick re-armed. Event loop only.
    rearmed: Cell<u64>,
    /// Whether the `osascript` rung may still put the Automation question on screen: once per
    /// arming of the path (`vo_park::after_event`). Up at the start and at each tick.
    may_prompt: Arc<AtomicBool>,
    /// Handed over but not yet answered, so `is_speaking` has something true to say.
    pending: Arc<AtomicUsize>,
    /// When the path is tried again after a refusal — see `vo_park.rs`. Event loop only.
    park: RefCell<Park>,
}

impl VoiceOver {
    pub fn new() -> Self {
        let (to_vo, rx) = channel::<Utterance>();
        let (refused_tx, refused_rx) = channel::<String>();
        let (notes_tx, notes_rx) = channel::<Note>();
        let healthy = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicUsize::new(0));
        let may_prompt = Arc::new(AtomicBool::new(true));
        let (h, p, m) = (healthy.clone(), pending.clone(), may_prompt.clone());
        std::thread::Builder::new()
            .name("voiceover".into())
            .spawn(move || run(rx, refused_tx, notes_tx, h, p, m))
            .ok();
        Self {
            to_vo,
            refused_rx,
            notes_rx,
            healthy,
            opened: Arc::new(AtomicU64::new(0)),
            rearmed: Cell::new(0),
            may_prompt,
            pending,
            park: RefCell::new(Park::default()),
        }
    }

    /// Raises the gate; a new opening when it was down.
    fn raise(&self) {
        if !self.healthy.swap(true, Ordering::Relaxed) {
            self.opened.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Hands `text` to VoiceOver. `false` means it cannot be, and the caller has to say
    /// it another way — now, not later.
    pub fn say(&self, text: &str, interrupt: bool) -> bool {
        if !self.healthy.load(Ordering::Relaxed) {
            return false;
        }
        // A look lets one line through: the gate goes down behind it until VoiceOver has
        // answered, so what is said meanwhile goes to the system voice now instead of waiting
        // behind a try that may take the child process's whole limit.
        if self.park.borrow_mut().handed() {
            self.healthy.store(false, Ordering::Relaxed);
        }
        self.pending.fetch_add(1, Ordering::Relaxed);
        let opening = self.opened.load(Ordering::Relaxed);
        if self.to_vo.send(Utterance { text: text.to_string(), interrupt, opening }).is_err() {
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

    /// Try VoiceOver again after a failure — what ticking the setting means. The one way back
    /// from a permission refusal.
    pub fn rearm(&self) {
        self.park.borrow_mut().reset();
        self.may_prompt.store(true, Ordering::Relaxed);
        let was_down = !self.healthy.load(Ordering::Relaxed);
        self.raise();
        self.rearmed.set(self.opened.load(Ordering::Relaxed));
        if was_down {
            crate::logging::line("speech", "trying VoiceOver again, because its setting was ticked");
        }
    }

    /// VoiceOver runs as `pid` now (`None`: not running). Asked before every line: a path
    /// parked after a refusal opens at once for a VoiceOver that has started or restarted since.
    pub fn note_pid(&self, pid: Option<i32>) {
        let line = self.park.borrow_mut().pid_seen(pid);
        if let Some(line) = line {
            self.raise();
            crate::logging::line("speech", &line);
        }
    }

    /// What the worker said since the last call, and the scheduled look. Called from the event
    /// loop on every pass; a few loads when there is nothing to do, and one question about
    /// VoiceOver's process per refusal.
    pub fn tick(&self) {
        let now = Instant::now();
        while let Ok(note) = self.notes_rx.try_recv() {
            let line = match note {
                // Handed over before the setting was ticked: the tick re-armed the path after it,
                // and holding it against the path would undo the tick.
                Note::Failed { opening, .. } if opening < self.rearmed.get() => None,
                Note::Failed { refusal, .. } => {
                    let pid = running_pid();
                    self.park.borrow_mut().failed(&refusal, now, pid)
                }
                Note::Spoke => self.park.borrow_mut().spoke(now),
            };
            if let Some(line) = line {
                crate::logging::line("speech", &line);
            }
            // A line taken opens the path; one refused closes it. Either way the gate follows
            // the park, which has now heard everything the worker said.
            if self.park.borrow().open() {
                self.raise();
            } else {
                self.healthy.store(false, Ordering::Relaxed);
            }
        }
        if self.park.borrow_mut().due(now) {
            self.raise();
            crate::logging::trace("speech", || "trying VoiceOver again: its next look came due".to_string());
        }
    }

    /// The Mac woke, the screen was unlocked or this session came back (`why`): a path parked
    /// after a refusal opens at once rather than at its next look.
    pub fn retry_now(&self, why: &str) {
        if self.park.borrow_mut().wake() {
            self.raise();
            crate::logging::line("speech", &format!("trying VoiceOver again at once: the {why}"));
        }
    }
}

fn run(
    rx: Receiver<Utterance>,
    refused: Sender<String>,
    notes: Sender<Note>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    may_prompt: Arc<AtomicBool>,
) {
    // Whether the last line was refused, so the first one taken after it is reported.
    let mut failing = false;
    // The opening of the gate in which VoiceOver last refused a line.
    let mut refused_in: Option<u64> = None;
    // An Apple Event came back -1744: until the system says the Automation question has been
    // answered, each line asks it first, without asking the user (`output`).
    let mut consent_pending = false;
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
        // Handed over in an opening VoiceOver has already refused a line in — before that
        // refusal lowered the gate. Not offered: each would pay for the same refusal again, one
        // after the other, and come out late. The system voice says it now.
        if refused_in.is_some_and(|r| u.opening <= r) {
            pending.fetch_sub(1 + dropped, Ordering::Relaxed);
            let _ = refused.send(u.text);
            continue;
        }
        let started = Instant::now();
        let outcome = output(&u.text, &mut consent_pending, &may_prompt);
        let took = started.elapsed().as_millis() as u64;
        let left = pending.fetch_sub(1 + dropped, Ordering::Relaxed) - (1 + dropped);
        cost.record(took, u.text.chars().count(), left == 0);
        match outcome {
            Err(refusal) => {
                // Lowered here, at once, so the lines already on their way are not handed over
                // too; the event loop decides when it goes up again (`VoiceOver::tick`), and
                // writes the line — once per run of refusals, not once per line.
                healthy.store(false, Ordering::Relaxed);
                failing = true;
                refused_in = Some(u.opening);
                let _ = notes.send(Note::Failed { refusal, opening: u.opening });
                let _ = refused.send(u.text);
            }
            Ok(()) if failing => {
                failing = false;
                let _ = notes.send(Note::Spoke);
            }
            Ok(()) => {}
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

/// Has the Apple Event failed where the child process then worked? Then stop paying for it and
/// use the child process.
///
/// One flag rather than a per-call decision: the failures this guards against — a wrong
/// four-character code, an Apple Event layer that will not send from this thread — are
/// properties of the build and the machine, not of the line being spoken. If it goes wrong
/// once it will go wrong every time, and the fallback is right there.
///
/// Only when the child process then TOOK the line, though, and only after a refusal that says
/// nothing about permission (`vo_park::event_broken`). An event refused because VoiceOver is
/// restarting, busy, or not allowed to be controlled is refused through `osascript` as well,
/// and says nothing about the event; switching over on it used to put every later line of a
/// days-long session on the slower path for a VoiceOver that had merely restarted once. And a
/// child that took a line after -1744 took it because the Automation question was answered
/// while it waited: the event works from then on too.
static EVENT_WORKS: AtomicBool = AtomicBool::new(true);

/// VoiceOver's `output` command, sent as an Apple Event from this process.
///
/// The codes come from VoiceOver's own scripting dictionary, which the macOS tester
/// extracted with `sdef` — see docs/voiceover-scripting-codes.md. They cannot be derived
/// from anywhere but a Mac, which is why they are written down rather than looked up.
///
/// **Why this instead of `osascript`,** measured rather than assumed: on the tester's 2015
/// MacBook Air a spoken line through the child process took 195 ms, of which 69 ms was the
/// bare launch of a script that talks to nobody. The remaining 126 ms is mostly resolving
/// VoiceOver's terminology, which a fresh process pays again every single time. And 195 ms
/// is nowhere near how long 25 characters take to say out loud, which is what settles the
/// question underneath: `output` returns as soon as VoiceOver has the text, so this is fixed
/// overhead and not the sound of speech.
///
/// **No reply is asked for**, and that is deliberate twice over. There is nothing in the
/// answer worth having, and waiting for one would mean an Apple Event reply arriving on a
/// worker thread that has no run loop to deliver it — the one part of this whose behaviour
/// nobody here could establish. Failures that matter still surface: a target that is not
/// authorised is refused at send time, which is how every application discovers it needs
/// permission.
fn send_event(text: &str) -> Result<(), Refusal> {
    // 'VOAS' / 'outp', and '----' is keyDirectObject — the parameter every command's direct
    // argument travels in.
    const VOAS: u32 = 0x564F_4153;
    const OUTP: u32 = 0x6F75_7470;
    const KEY_DIRECT_OBJECT: u32 = 0x2D2D_2D2D;
    // kAutoGenerateReturnID and kAnyTransactionID: we are not correlating replies or
    // grouping this into a transaction, and the constants for "do neither" are these.
    const AUTO_RETURN_ID: i16 = -1;
    const ANY_TRANSACTION: i32 = 0;

    let target = NSAppleEventDescriptor::descriptorWithBundleIdentifier(&NSString::from_str(
        "com.apple.VoiceOver",
    ));
    let event =
        NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
            VOAS,
            OUTP,
            Some(&target),
            AUTO_RETURN_ID,
            ANY_TRANSACTION,
        );
    let arg = NSAppleEventDescriptor::descriptorWithString(&NSString::from_str(text));
    event.setParamDescriptor_forKeyword(&arg, KEY_DIRECT_OBJECT);

    // NeverInteract because nothing about saying a line should ever put a window on screen;
    // DontRecord because this is not a user action worth a script recorder's attention.
    let options = NSAppleEventSendOptions::NoReply
        | NSAppleEventSendOptions::NeverInteract
        | NSAppleEventSendOptions::DontRecord;
    match event.sendEventWithOptions_timeout_error(options, CHILD_LIMIT.as_secs_f64()) {
        Ok(_) => Ok(()),
        Err(e) => {
            let code = e.code() as i64;
            Err(Refusal { code: Some(code), text: format!("{e} [{code}]"), stalled: false })
        }
    }
}

/// Whether this application may send VoiceOver Apple Events, asked of the system WITHOUT
/// asking the user: `AEDeterminePermissionToAutomateTarget` with `askUserIfNeeded` false, which
/// puts nothing on screen and waits for nobody. noErr, -1743, -1744, or -600 when VoiceOver is
/// not running. The question `backend::macos::perm` asks for the start-up block, asked here on
/// the speech thread (the call is documented thread-safe since macOS 10.14).
fn automation_status() -> i64 {
    let target = NSAppleEventDescriptor::descriptorWithBundleIdentifier(&NSString::from_str(
        "com.apple.VoiceOver",
    ));
    // SAFETY: `aeDesc` borrows the descriptor's own storage, which outlives the call, and the
    // call does not keep it; the wildcards ask about any event of any class, as perm.rs does.
    unsafe {
        let desc = target.aeDesc();
        if desc.is_null() {
            return vo_park::WOULD_REQUIRE_CONSENT;
        }
        AEDeterminePermissionToAutomateTarget(desc, typeWildCard, typeWildCard, false) as i64
    }
}

/// Says one line, by the cheapest route that still works.
///
/// The Apple Event first; the child process if the event failed in a way the child can help
/// with (`vo_park::after_event`), and for every line once a child has taken one the event could
/// not. Two rungs rather than one because the fast path is written blind — the codes came off
/// a machine nobody here can run, and a wrong one must degrade to what already worked instead
/// of taking the speech with it.
///
/// `consent_pending` is the worker's: an event came back -1744, the Automation question not
/// answered. Until the system says it has been, a line asks the system first
/// ([`automation_status`], which puts nothing on screen) and, while the question is still
/// open, goes to the system voice at once — rather than sending an event that is refused the
/// same way, or starting a child that would wait on the question for [`CHILD_LIMIT`].
/// `may_prompt` is the one allowance per arming for that child, which is also how the question
/// gets asked at all when the setting was already on at start-up.
///
/// Whether any of this interrupts what VoiceOver is already saying is VoiceOver's decision,
/// not ours; `interrupt` above governs only our own queue, which is all we can honour.
fn output(text: &str, consent_pending: &mut bool, may_prompt: &AtomicBool) -> Result<(), Refusal> {
    if *consent_pending {
        match automation_status() {
            0 => *consent_pending = false,
            code => {
                return Err(Refusal {
                    code: Some(code),
                    text: format!(
                        "the system says this application may not send VoiceOver Apple Events \
                         yet{} [{code}]",
                        if code == vo_park::WOULD_REQUIRE_CONSENT {
                            " — the Automation question has not been answered"
                        } else {
                            ""
                        }
                    ),
                    stalled: false,
                })
            }
        }
    }
    if !EVENT_WORKS.load(Ordering::Relaxed) {
        return run_osascript(text);
    }
    let event = match send_event(text) {
        Ok(()) => return Ok(()),
        Err(refusal) => refusal,
    };
    if event.code == Some(vo_park::WOULD_REQUIRE_CONSENT) {
        *consent_pending = true;
    }
    if vo_park::after_event(event.code, || may_prompt.swap(false, Ordering::Relaxed)) == Rung::Stop {
        return Err(event);
    }
    match run_osascript(text) {
        Ok(()) => {
            if vo_park::event_broken(event.code) {
                EVENT_WORKS.store(false, Ordering::Relaxed);
                crate::logging::line(
                    "speech",
                    &format!(
                        "the Apple Event to VoiceOver was refused ({}) and the same line went \
                         through osascript, so speech launches osascript per line for the rest of \
                         this session",
                        event.text
                    ),
                );
            } else {
                // The Automation question was answered while the child waited on it.
                *consent_pending = false;
                crate::logging::line(
                    "speech",
                    &format!(
                        "the Apple Event to VoiceOver was refused ({}) and the same line then went \
                         through osascript: the Automation question was answered meanwhile, so the \
                         next line goes as an Apple Event again",
                        event.text
                    ),
                );
            }
            Ok(())
        }
        // Both refused: VoiceOver's answer, not the event's. The event's code is the one to
        // classify by; the child's words go with it into the log.
        Err(child) => Err(Refusal {
            code: event.code,
            text: format!("{}; through osascript: {}", event.text, child.text),
            stalled: child.stalled,
        }),
    }
}

/// `tell application "VoiceOver" to output "…"`, through a child process — the rung below.
/// A refusal carries no code of its own: the child's error output ends with it in parentheses,
/// and that is what `vo_park::Refusal::cause` reads. `stalled` when the child was stopped at
/// [`CHILD_LIMIT`].
fn run_osascript(text: &str) -> Result<(), Refusal> {
    run_osascript_inner(text).map_err(|(text, stalled)| Refusal { code: None, text, stalled })
}

fn run_osascript_inner(text: &str) -> Result<(), (String, bool)> {
    let script =
        format!("tell application \"VoiceOver\" to output \"{}\"", applescript_string(text));
    let mut child = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| (format!("osascript could not be started ({e})"), false))?;

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err((format!("lost track of osascript ({e})"), false)),
        }
        let waited = started.elapsed();
        if waited >= CHILD_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return Err((
                format!(
                    "osascript did not come back within {} s and was stopped — VoiceOver is \
                     wedged, or something is waiting for an answer on screen",
                    CHILD_LIMIT.as_secs()
                ),
                true,
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
    Err((if why.is_empty() { format!("exit {status}") } else { why.to_string() }, false))
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
