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
//! **And VoiceOver's own permission**, which no system call reports: "Allow VoiceOver to be
//! controlled with AppleScript", in VoiceOver Utility. Unticked, VoiceOver takes every event and
//! drops it — nothing is refused, so nothing here used to notice. The rules are in `vo_script.rs`;
//! the reads ([`read_box`]) and the question ([`ask_voiceover`]) are here, and while the box is
//! not ticked every line goes to the system voice.
//!
//! Its own file rather than a block inside `speech`, because this is macOS code written
//! without a Mac: `crates/macos-check` borrows it by path and asks the compiler whether it
//! is true, which a module nested inside a file that needs `tts` could not be.

use std::cell::{Cell, RefCell};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use objc2_app_kit::NSRunningApplication;
use objc2_core_foundation::{CFPreferencesAppSynchronize, CFPreferencesGetAppBooleanValue, CFString};
use objc2_core_services::{typeWildCard, AEDeterminePermissionToAutomateTarget};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventSendOptions, NSProcessInfo, NSString};

use super::vo_park::{self, Park, Refusal, Rung};
use super::vo_script::{self, Gate, Key, Marker, Probe, Reads, Scripting, Verdict};

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

/// What the worker is handed, in order.
enum Job {
    Say(Utterance),
    /// Ask VoiceOver whether it accepts AppleScript ([`ask_voiceover`]), on what was read just
    /// before. Lines handed over after it wait behind it, and go where its answer says.
    Check(Reads),
}

/// What the worker tells the event loop about the path, besides the lines it hands back.
enum Note {
    /// A line handed over in `opening` was refused.
    Failed { refusal: Refusal, opening: u64 },
    /// A line was taken after one or more were refused.
    Spoke,
    /// VoiceOver's answer to the AppleScript question, asked on `reads`.
    Checked { reads: Reads, probe: Probe },
}

/// VoiceOver's last answer to the AppleScript question and the reads it was asked on, for
/// whoever shows the box's state ([`applescript_now`]) — the Permissions page, which has no way
/// to the transport. Written by the worker.
static LAST_ASKED: Mutex<Option<(Reads, Probe)>> = Mutex::new(None);

/// VoiceOver Utility quit since the event loop last looked ([`application_quit`]).
static UTILITY_QUIT: AtomicBool = AtomicBool::new(false);

/// VoiceOver, spoken to through `osascript` on a thread of its own.
///
/// Not inline: each line costs a process launch and whatever `osascript` then has to do
/// before VoiceOver answers, and this event loop also carries the keyboard. A screen reader
/// that stalls the keys it is describing is not usable, so the wait happens elsewhere — and
/// the thing being waited on is a child process, whose hangs and crashes are contained in
/// somebody else's address space.
pub struct VoiceOver {
    to_vo: Sender<Job>,
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
    /// The second gate: false while VoiceOver does not accept AppleScript, as far as is known
    /// (`vo_script::Gate`). Kept by the event loop, and by the worker for the lines that waited
    /// behind its question — both by the same rule, so they cannot disagree for long.
    accepts: Arc<AtomicBool>,
    /// Whether VoiceOver accepts AppleScript, and when that is looked at again. Event loop only.
    script: RefCell<Gate>,
    /// The VoiceOver process last asked about the box, so a new one is asked again.
    asked_pid: Cell<Option<i32>>,
    /// Said through the system voice on the next pass ([`take_tell`](Self::take_tell)).
    tell: Cell<Option<&'static str>>,
    /// Said through VoiceOver on the next pass ([`take_confirm`](Self::take_confirm)).
    confirm: Cell<Option<&'static str>>,
}

impl VoiceOver {
    pub fn new() -> Self {
        let (to_vo, rx) = channel::<Job>();
        let (refused_tx, refused_rx) = channel::<String>();
        let (notes_tx, notes_rx) = channel::<Note>();
        let healthy = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicUsize::new(0));
        let may_prompt = Arc::new(AtomicBool::new(true));
        let accepts = Arc::new(AtomicBool::new(true));
        let (h, p, m, a) = (healthy.clone(), pending.clone(), may_prompt.clone(), accepts.clone());
        std::thread::Builder::new()
            .name("voiceover".into())
            .spawn(move || run(rx, refused_tx, notes_tx, h, p, m, a))
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
            accepts,
            script: RefCell::new(Gate::default()),
            asked_pid: Cell::new(None),
            tell: Cell::new(None),
            confirm: Cell::new(None),
        }
    }

    /// Whether VoiceOver accepts AppleScript, as far as is known — false only once something said
    /// it does not.
    pub fn accepts_applescript(&self) -> bool {
        self.accepts.load(Ordering::Relaxed)
    }

    /// The sentence the user is to hear through the system voice now, once.
    pub fn take_tell(&self) -> Option<&'static str> {
        self.tell.take()
    }

    /// The sentence the user is to hear through VoiceOver now, once.
    pub fn take_confirm(&self) -> Option<&'static str> {
        self.confirm.take()
    }

    /// Writes what a step of the box's gate said, keeps the transport's gate in step with it,
    /// and keeps what is to be said for the pump.
    ///
    /// The transport's gate is lowered whenever the box's gate is closed, and raised only by an
    /// `answer` — the step that has heard the latest question back. The worker writes the same
    /// flag, and only ever lowers it (`ask_voiceover`): a step taken here before the event loop
    /// has read the worker's newer answer must not raise what that answer lowered, and an answer
    /// the worker heard before a trigger here must not raise what the trigger lowered.
    fn apply(&self, out: vo_script::Out, answer: bool) {
        for line in &out.log {
            crate::logging::line("speech", line);
        }
        if out.tell.is_some() {
            self.tell.set(out.tell);
        }
        if out.confirm.is_some() {
            self.confirm.set(out.confirm);
        }
        let open = self.script.borrow().open();
        if !open {
            self.accepts.store(false, Ordering::Relaxed);
        } else if answer {
            self.accepts.store(true, Ordering::Relaxed);
        }
    }

    /// Asks again whether VoiceOver accepts AppleScript, because of `why`: the file and the
    /// preference read now, on this thread — a `stat` and one preference read, timed into the
    /// log — and VoiceOver itself asked on the worker when it runs.
    fn check(&self, why: &str, running: bool) {
        let started = Instant::now();
        let reads = read_box();
        let took = started.elapsed().as_micros();
        let out = self.script.borrow_mut().triggered(&reads, &format!("{why}; read in {took} µs"));
        let ask = out.ask;
        self.apply(out, false);
        if ask {
            self.ask(reads, running);
        }
    }

    /// Hands the question to the worker; answered here and now when it cannot be asked, so the
    /// box's gate is never left waiting for an answer that cannot come.
    fn ask(&self, reads: Reads, running: bool) {
        let why = if !running {
            "VoiceOver is not running, so it was not asked"
        } else {
            match self.to_vo.send(Job::Check(reads.clone())) {
                Ok(()) => return,
                Err(_) => "the speech thread is gone, so VoiceOver was not asked",
            }
        };
        let out = self.script.borrow_mut().answered(reads, Probe::Inconclusive(why.to_string()), Instant::now(), running);
        let again = out.ask;
        self.apply(out, true);
        if again {
            self.check("asked again: something happened while VoiceOver was being asked", running);
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
        // VoiceOver would take the line and drop it: its AppleScript box is not ticked.
        if !self.healthy.load(Ordering::Relaxed) || !self.accepts.load(Ordering::Relaxed) {
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
        if self.to_vo.send(Job::Say(Utterance { text: text.to_string(), interrupt, opening })).is_err() {
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
    /// from a permission refusal. The AppleScript box is asked about again at the next
    /// [`note_pid`](Self::note_pid), which the caller makes at once, and the user hears its
    /// answer again: ticking the setting is a deliberate act.
    pub fn rearm(&self) {
        self.park.borrow_mut().reset();
        self.script.borrow_mut().rearmed();
        self.asked_pid.set(None);
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
    ///
    /// And a VoiceOver process not asked about its AppleScript box yet is asked — at start,
    /// after the setting was ticked, and when VoiceOver starts or restarts — before the line
    /// this is asked for is handed over, so that line waits for the answer instead of being
    /// dropped.
    pub fn note_pid(&self, pid: Option<i32>) {
        let line = self.park.borrow_mut().pid_seen(pid);
        if let Some(line) = line {
            self.raise();
            crate::logging::line("speech", &line);
        }
        let Some(now) = pid else {
            return;
        };
        match self.asked_pid.replace(Some(now)) {
            Some(before) if before == now => {}
            Some(before) => self.check(&format!("VoiceOver runs as a new process (pid {before}, now {now})"), true),
            None => self.check(&format!("VoiceOver runs as pid {now}, not asked yet"), true),
        }
    }

    /// What the worker said since the last call, and the scheduled look. Called from the event
    /// loop on every pass; a few loads when there is nothing to do, and one question about
    /// VoiceOver's process per refusal. `wanted`: the setting is on, or a module chose
    /// `voiceover` — the AppleScript box is looked after only then.
    pub fn tick(&self, wanted: bool) {
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
                // The AppleScript box has its own gate, and says its own lines.
                Note::Checked { reads, probe } => {
                    let running = running_pid().is_some();
                    let out = self.script.borrow_mut().answered(reads, probe, now, running);
                    let again = out.ask;
                    self.apply(out, true);
                    if again {
                        self.check("asked again: something happened while VoiceOver was being asked", running);
                    }
                    continue;
                }
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
        // The AppleScript box, only while VoiceOver is wanted — the setting on, or a module that
        // chose `voiceover`; otherwise looking for it would be work done against the user's
        // answer. VoiceOver Utility quitting is the moment the box is likeliest to have changed;
        // while it is known to be unticked, the reads are looked at again on the refusals' pace.
        // A process lookup per look, a `stat` and a preference read; a question to VoiceOver only
        // when those say it may be worth one.
        let utility_quit = UTILITY_QUIT.swap(false, Ordering::Relaxed);
        if !wanted {
            return;
        }
        if utility_quit {
            // With VoiceOver not running there is nobody to ask, nothing to drop, and nothing to
            // tell; the next VoiceOver is asked before its first line (`note_pid`).
            if running_pid().is_some() {
                self.check("VoiceOver Utility quit", true);
            }
        } else if self.script.borrow().due(now) {
            let running = running_pid().is_some();
            let reads = read_box();
            let out = self.script.borrow_mut().looked(&reads, now, running);
            let ask = out.ask;
            if ask {
                crate::logging::trace("speech", || format!("the AppleScript box, looked at again: {}", reads.words()));
            }
            self.apply(out, false);
            if ask {
                self.ask(reads, running);
            }
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
    rx: Receiver<Job>,
    refused: Sender<String>,
    notes: Sender<Note>,
    healthy: Arc<AtomicBool>,
    pending: Arc<AtomicUsize>,
    may_prompt: Arc<AtomicBool>,
    accepts: Arc<AtomicBool>,
) {
    // Whether the last line was refused, so the first one taken after it is reported.
    let mut failing = false;
    // The opening of the gate in which VoiceOver last refused a line.
    let mut refused_in: Option<u64> = None;
    // An Apple Event came back -1744: until the system says the Automation question has been
    // answered, each line asks it first, without asking the user (`output`).
    let mut consent_pending = false;
    let mut cost = Cost::default();
    let mut asked = 0u32;
    while let Ok(job) = rx.recv() {
        let mut u = match job {
            Job::Say(u) => u,
            Job::Check(reads) => {
                ask_voiceover(reads, &accepts, &notes, &mut asked);
                continue;
            }
        };
        let mut dropped = 0usize;
        // A question about the AppleScript box found among the lines dropped below: asked
        // before the line that is kept is offered, which then goes where its answer says.
        let mut check = None;
        // An interrupting line makes everything still waiting stale. Saying those anyway
        // would mean announcing where the cursor USED to be, several controls late,
        // which is exactly the failure a screen reader must not have.
        if u.interrupt {
            loop {
                match rx.try_recv() {
                    Ok(Job::Say(next)) => {
                        dropped += 1;
                        u = next;
                    }
                    Ok(Job::Check(reads)) => check = Some(reads),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }
        }
        if let Some(reads) = check {
            ask_voiceover(reads, &accepts, &notes, &mut asked);
        }
        // Handed over in an opening VoiceOver has already refused a line in — before that
        // refusal lowered the gate. Not offered: each would pay for the same refusal again, one
        // after the other, and come out late. The system voice says it now.
        //
        // And handed over before VoiceOver was found not to accept AppleScript — waiting behind
        // the question that found it, often: VoiceOver would drop it without a word. The system
        // voice says it now.
        if refused_in.is_some_and(|r| u.opening <= r) || !accepts.load(Ordering::Relaxed) {
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
/// nobody here could establish. Failures of the sending still surface: a target that is not
/// authorised is refused at send time, which is how every application discovers it needs
/// permission. **One failure does not**, and a Mac session of 2026-10-01 met it: VoiceOver with
/// its AppleScript box unticked takes the event and drops it, and the line is lost without a
/// word. That is asked about separately, with a reply ([`ask_voiceover`], `vo_script.rs`), and
/// while it is so no line is sent here.
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
    run_child(&[script.as_str()], false).map(|_| ())
}

/// `osascript` with one `-e` per line of `lines`, stopped at [`CHILD_LIMIT`]. What it printed
/// when it succeeded — only with `stdout`, which is otherwise not even opened — or its error
/// output and whether it had to be stopped.
fn run_child(lines: &[&str], stdout: bool) -> Result<String, (String, bool)> {
    let mut command = Command::new("/usr/bin/osascript");
    for line in lines {
        command.arg("-e").arg(line);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(if stdout { Stdio::piped() } else { Stdio::null() })
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

    use std::io::Read;
    if status.success() {
        let mut out = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut out);
        }
        return Ok(out);
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let why = stderr.trim();
    Err((if why.is_empty() { format!("exit {status}") } else { why.to_string() }, false))
}

/// The AppleScript box's file and preference, read without asking VoiceOver anything — see
/// `vo_script.rs` for what each is worth on which macOS. A `stat`, and one preference read
/// through cfprefsd; nothing is written, and nothing here puts a question on screen.
///
/// The preference domain is re-read first (`CFPreferencesAppSynchronize`): a value this process
/// read earlier would otherwise be served from its own cache, and would not show a box ticked
/// since. With nothing of ours pending in that domain — nothing here ever sets a value in it —
/// that call has nothing to write.
pub(super) fn read_box() -> Reads {
    let marker = Marker::from_stat(std::fs::metadata(vo_script::MARKER).map(|_| ()));
    let domain = CFString::from_static_str(vo_script::DOMAIN);
    let name = CFString::from_static_str(vo_script::KEY);
    CFPreferencesAppSynchronize(&domain);
    let mut valid: u8 = 0;
    // SAFETY: `valid` is a live local, written once by the call.
    let on = unsafe { CFPreferencesGetAppBooleanValue(&name, &domain, &mut valid) };
    let key = match (valid != 0, on) {
        (false, _) => Key::Unset,
        (true, true) => Key::On,
        (true, false) => Key::Off,
    };
    let major = NSProcessInfo::processInfo().operatingSystemVersion().majorVersion as i64;
    Reads { marker, key, key_live: major < vo_script::KEY_MOVED_IN }
}

/// Asks VoiceOver one read-only question through AppleScript ([`vo_script::PROBE`]) and waits
/// for its answer — the one way to hear from VoiceOver itself whether its AppleScript box is
/// ticked. On the worker, so the event loop never waits for it; lines handed over meanwhile wait
/// behind it and go where its answer says ([`run`]).
///
/// **Only with the Automation permission granted** ([`automation_status`], which asks nobody):
/// otherwise `osascript` would put the Automation question on screen and wait for it, for a
/// question the user did not ask. Then VoiceOver is not asked, and the file and the preference
/// decide ([`vo_script::unasked`]). While that question is on screen because the setting was
/// just ticked (`backend::voiceover_automation_asking`), the user is not told about the box over
/// it, and the next look asks again. The question the `osascript` rung puts on screen
/// (`vo_park::after_event`) is not counted: only a line handed to VoiceOver puts it, so only
/// while the reads do not say "not allowed" — when there is nothing to tell.
///
/// Through `osascript` and its [`CHILD_LIMIT`], not an Apple Event of our own, for the reason
/// [`send_event`] gives: a reply arriving on a thread with no run loop is the part of that
/// nobody here could establish. The cost of the first question that launched `osascript` goes
/// into the log, and so does any later one slower than [`SLOW_LINE_MS`] — lines handed over
/// meanwhile wait for it; the answer, read, goes to the event loop, and with the reads it was
/// asked on to [`LAST_ASKED`].
fn ask_voiceover(reads: Reads, accepts: &AtomicBool, notes: &Sender<Note>, asked: &mut u32) {
    let probe = match automation_status() {
        0 => {
            let started = Instant::now();
            let probe = vo_script::classify(&run_child(&vo_script::PROBE, true));
            let ms = started.elapsed().as_millis();
            *asked += 1;
            if *asked == 1 {
                crate::logging::line(
                    "speech",
                    &format!("the first AppleScript question to VoiceOver took {ms} ms, osascript's launch included"),
                );
            } else if ms >= u128::from(SLOW_LINE_MS) {
                crate::logging::line(
                    "speech",
                    &format!(
                        "an AppleScript question to VoiceOver took {ms} ms; lines handed over meanwhile \
                         waited for it"
                    ),
                );
            } else {
                crate::logging::trace("speech", || format!("an AppleScript question to VoiceOver took {ms} ms"));
            }
            probe
        }
        status => vo_script::unasked(status, crate::backend::voiceover_automation_asking()),
    };
    // The lines waiting behind this question go to the system voice when its answer says "not
    // allowed": the event loop's rule, applied here first, because they are offered before the
    // event loop hears the answer. Only lowered here — raised by the event loop alone, once it
    // has heard this answer in order with everything else (`VoiceOver::apply`).
    if vo_script::verdict(&reads, Some(&probe)).state == Scripting::NotAllowed {
        accepts.store(false, Ordering::Relaxed);
    }
    if let Ok(mut last) = LAST_ASKED.lock() {
        *last = Some((reads.clone(), probe.clone()));
    }
    let _ = notes.send(Note::Checked { reads, probe });
}

/// The AppleScript box as it stands: read now, with VoiceOver's last answer when it was given on
/// the same reads. For the Permissions page, on the main thread: a `stat` and a preference read,
/// and no question to VoiceOver.
pub(super) fn applescript_now() -> Verdict {
    let reads = read_box();
    let last = LAST_ASKED.lock().ok().and_then(|l| l.clone());
    vo_script::shown(&reads, last.as_ref())
}

/// An application quit (the backend's notice, on the main thread), by its bundle identifier and
/// its bundle's file name, either of which may be missing. VoiceOver Utility quitting is when its
/// AppleScript box is likeliest to have just been changed; the event loop's next pass asks again.
///
/// Both names go into the log when it is recognised — the identifier has one source, and the log
/// settles it — and when an application whose names mention VoiceOver is not, so that a wrong
/// guess at both shows itself too. VoiceOver's own quitting is not written: `note_pid` sees it.
pub(super) fn application_quit(bundle_id: Option<&str>, bundle_file: Option<&str>) {
    let names = || format!("bundle id {}, bundle {}", bundle_id.unwrap_or("none"), bundle_file.unwrap_or("none"));
    if vo_script::is_utility(bundle_id, bundle_file) {
        UTILITY_QUIT.store(true, Ordering::Relaxed);
        crate::logging::line("speech", &format!("VoiceOver Utility quit ({})", names()));
    } else if bundle_id != Some("com.apple.VoiceOver")
        && [bundle_id, bundle_file].iter().flatten().any(|n| n.contains("VoiceOver"))
    {
        crate::logging::line(
            "speech",
            &format!("an application named for VoiceOver quit, not taken for VoiceOver Utility ({})", names()),
        );
    }
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
