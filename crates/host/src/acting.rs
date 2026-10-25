//! What a module asks of another program that the host does on a thread of its own, and the way
//! the outcome comes back to the module.
//!
//! **Why a thread.** Some ways of acting wait for the other program: giving a window of another
//! program the no-activate style (`host.input.click`'s `via = "noactivate"`) sends that program's
//! thread a message and waits for its answer, with no bound Windows lets us set. Nothing that can
//! wait for another program runs on the event loop, which carries every key a person presses, so
//! such work is a job ([`crate::backend::ActJob`]) for a thread of the host's — one per [`Lane`],
//! started at its first job, each lane's jobs in the order they were handed over. And a gesture
//! whose timing is its definition — a double-click, `clicks = 2`, whose press lasts 20 ms and whose
//! second press follows 60 ms after the first release (backend/double_click.rs) — waits on a lane
//! of its own: on the event loop the waits would hold every key, and timers there would stretch
//! with any turn that runs long, a press into a long press, the gap past the double-click time.
//! (Pressing an element, `host.element.press`, is a question to an accessibility tree like the
//! others, asked on the accessibility thread: element.rs.)
//!
//! **The way back.** A module that hands over a callback gets the outcome on a later tick: the
//! thread sends it here, the tick takes it ([`Acting::take_due`]) and hands it to the module's
//! mailbox as an event of its own (`Event::Acted`), in the lane it was asked from — so it runs as a
//! handler, after whatever the module was busy with, and never into a VM that was disabled,
//! reloaded or removed meanwhile (`mailbox::deliver` drops it then). An outcome known at the call —
//! a click made at once, or refused — goes the same way ([`Acting::answer_now`]), so a callback is
//! called exactly once, on a later tick, whichever way the click went.
//!
//! **What `host.input.click` takes** ([`click_opts`]): the button, the window the click is for,
//! the way (`via`), how many clicks (`clicks`) and, for a posted click only, whether the pointer
//! moves. Read and checked here, before anything acts, so a mistake raises at the call.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::{ActJob, MouseButton};
use crate::errors::input::{
    ENDING, GESTURE_THREAD_GONE, GESTURE_THREAD_NOT_STARTED, STYLE_THREAD_FAILED, STYLE_THREAD_GONE, STYLE_THREAD_NOT_STARTED,
};
use crate::errors::HostError;
use crate::{errors_lua, logging};
use crate::ocr::types::{current_priority, Priority};

/// A thread of the host's for one kind of job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// Windows' styles: a click with `via = "noactivate"`, the style it gives, and giving it back.
    /// One thread, so a style is given back only after every click handed over before.
    Style = 0,
    /// Gestures timed as they are made: a double-click (`clicks = 2`), its waits on this thread,
    /// one gesture after the other.
    Gesture = 1,
}

/// How many lanes there are.
const LANES: usize = 2;

impl Lane {
    /// Its thread's name.
    fn thread(self) -> &'static str {
        match self {
            Lane::Style => "window-style",
            Lane::Gesture => "input-gesture",
        }
    }

    /// What a job is answered with when the lane's thread is gone: in its own Reason's words.
    fn gone(self) -> HostError {
        let reason = match self {
            Lane::Style => &STYLE_THREAD_GONE,
            Lane::Gesture => &GESTURE_THREAD_GONE,
        };
        HostError::with(reason, format!("the host's {} thread is gone", self.thread()))
    }

    /// What a job is answered with when the lane's thread could not be started, `e` the system's
    /// word for it.
    fn not_started(self, e: &std::io::Error) -> HostError {
        let reason = match self {
            Lane::Style => &STYLE_THREAD_NOT_STARTED,
            Lane::Gesture => &GESTURE_THREAD_NOT_STARTED,
        };
        HostError::with(reason, format!("the host could not start its {} thread: {e}", self.thread()))
    }
}

/// Who an outcome is for: a module's callback, in the VM it came from, and the name its handler's
/// errors are reported under (`"input.click"`; `"element.press"` and the other questions to an
/// accessibility tree, whose answers element.rs hands back the same way).
pub(crate) struct Answer {
    pub idx: usize,
    pub lua: Lua,
    pub cb: Function,
    pub what: &'static str,
}

/// A no-activate click's button, for its module's record of the buttons it holds, which the
/// click's outcome decides once it is known (lib.rs, `Shared::after_release`): module `idx`'s
/// `button`, and how many buttons the module had pressed when the click was handed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Release {
    pub idx: usize,
    pub button: MouseButton,
    pub presses: u64,
}

/// An outcome that came back: for its module's mailbox, the [`Answer`] when there is one and the
/// priority of the dispatch that asked; the [`Release`] it decides; and the outcome — `Err` with
/// what the module is told.
pub(crate) struct Due {
    pub answer: Option<Answer>,
    pub release: Option<Release>,
    pub prio: Priority,
    pub result: Result<(), HostError>,
}

/// What an outcome is waited for by: an answer, a release, or both.
struct Waiting {
    answer: Option<Answer>,
    release: Option<Release>,
    prio: Priority,
}

/// A lane's way to its thread: each job with its number, and whether it runs even once the lane is
/// ending ([`Acting::end_and_wait`]) — the end's own job.
type ToThread = Sender<(u64, ActJob, bool)>;

/// The jobs on their way and the outcomes coming back. Main thread only, like the rest of `Shared`;
/// the threads hold only their own end of a channel.
pub(crate) struct Acting {
    next: Cell<u64>,
    /// The answers and releases owed, by the job's number. A job handed over without either is not
    /// in here: what became of it is in the log, where the job writes it — and its thread, should
    /// it panic.
    waiting: RefCell<HashMap<u64, Waiting>>,
    done_tx: Sender<(u64, Result<(), HostError>)>,
    done_rx: Receiver<(u64, Result<(), HostError>)>,
    /// Each lane's way to its thread, once that thread is started.
    lanes: [RefCell<Option<ToThread>>; LANES],
    /// Each lane's word that the application is ending ([`Acting::end_and_wait`]), which its thread
    /// reads before each job: a job not begun by then is not begun at all.
    ending: [Arc<AtomicBool>; LANES],
}

impl Acting {
    pub(crate) fn new() -> Acting {
        let (done_tx, done_rx) = channel();
        Acting {
            next: Cell::new(0),
            waiting: RefCell::new(HashMap::new()),
            done_tx,
            done_rx,
            lanes: Default::default(),
            ending: Default::default(),
        }
    }

    /// Hands `job` to `lane`'s thread; `answer`, when given, gets its outcome on a later tick. A
    /// thread that cannot be started answers the job as not done, with the reason.
    pub(crate) fn run(&self, lane: Lane, job: ActJob, answer: Option<Answer>) {
        self.run_with(lane, job, answer, None);
    }

    /// [`Acting::run`] for a click, whose outcome also decides `release` on the tick it comes back.
    pub(crate) fn run_releasing(&self, lane: Lane, job: ActJob, answer: Option<Answer>, release: Release) {
        self.run_with(lane, job, answer, Some(release));
    }

    fn run_with(&self, lane: Lane, job: ActJob, answer: Option<Answer>, release: Option<Release>) {
        self.send(lane, job, answer, release, false);
    }

    /// Hands `job` to `lane`'s thread with its number; `always`: it runs even once the lane is
    /// ending. A thread that cannot be started, or is gone, answers the job as not done.
    fn send(&self, lane: Lane, job: ActJob, answer: Option<Answer>, release: Option<Release>, always: bool) {
        let token = self.token(answer, release);
        let sent = self.sender(lane).and_then(|tx| tx.send((token, job, always)).map_err(|_| lane.gone()));
        if let Err(why) = sent {
            logging::line("input", &format!("a job for the {} thread was not done: {why}", lane.thread()));
            let _ = self.done_tx.send((token, Err(why)));
        }
    }

    /// Hands `job` to `lane`'s thread, after everything handed to it before, and waits for it to
    /// be done — at most `within`: whether it was done in that time. For the end of the
    /// application alone, which has no tick left to hear an outcome on and must end all the same
    /// when the other program a job waits for does not answer. One log line when it was not done.
    pub(crate) fn run_and_wait(&self, lane: Lane, job: ActJob, within: std::time::Duration) -> bool {
        let (tx, rx) = channel::<()>();
        self.send(
            lane,
            Box::new(move || {
                let r = job();
                let _ = tx.send(());
                r
            }),
            None,
            None,
            true,
        );
        match rx.recv_timeout(within) {
            Ok(()) => true,
            Err(RecvTimeoutError::Timeout) => {
                logging::line(
                    "input",
                    &format!("the {} thread's last job was not done within {} ms; the application ends without it", lane.thread(), within.as_millis()),
                );
                false
            }
            Err(RecvTimeoutError::Disconnected) => {
                logging::line("input", &format!("the {} thread's last job ended without being done", lane.thread()));
                false
            }
        }
    }

    /// At the end of the application: `lane` is ending — every job handed to it that its thread has
    /// not begun yet is not begun at all, answered `cancelled` (`the application is ending`) — and
    /// the one it is running, if any, is waited for, at most `within`, behind an empty job that runs
    /// all the same: whether it ended in that time. So the end waits for one gesture in flight,
    /// never for those queued behind it, and the process does not end between a press and its
    /// release unless that one gesture holds the thread past `within`.
    pub(crate) fn end_and_wait(&self, lane: Lane, within: std::time::Duration) -> bool {
        self.ending[lane as usize].store(true, Ordering::Release);
        self.run_and_wait(lane, Box::new(|| Ok(())), within)
    }

    /// An outcome known at the call, for `answer`: delivered on the next tick, as one from a
    /// thread is, so a callback is never called inside the call that handed it over.
    pub(crate) fn answer_now(&self, answer: Answer, result: Result<(), HostError>) {
        let token = self.token(Some(answer), None);
        let _ = self.done_tx.send((token, result));
    }

    /// The next job's number, with its answer and its release noted — under the priority of the
    /// dispatch that asks.
    fn token(&self, answer: Option<Answer>, release: Option<Release>) -> u64 {
        let token = self.next.get() + 1;
        self.next.set(token);
        if answer.is_some() || release.is_some() {
            self.waiting.borrow_mut().insert(token, Waiting { answer, release, prio: current_priority() });
        }
        token
    }

    /// Whether `lane`'s thread was started: a job was handed to it once. For the end of the
    /// application, which waits for a lane only where something may still be on it.
    pub(crate) fn started(&self, lane: Lane) -> bool {
        self.lanes[lane as usize].borrow().is_some()
    }

    /// `lane`'s thread, started at its first job. It runs each job inside `logging::contain`, so a
    /// job that panics is answered with the panic's report — and the report is in the log, for a
    /// job nobody waits for — and the thread goes on. What a lane's thread answers for itself —
    /// gone, not started — is in its own Reasons' words ([`Lane::gone`], [`Lane::not_started`]); a
    /// job that failed, in the words of `STYLE_THREAD_FAILED`, which name no lane.
    fn sender(&self, lane: Lane) -> Result<ToThread, HostError> {
        let mut slot = self.lanes[lane as usize].borrow_mut();
        if let Some(tx) = slot.as_ref() {
            return Ok(tx.clone());
        }
        let (tx, rx) = channel::<(u64, ActJob, bool)>();
        let done = self.done_tx.clone();
        let ending = self.ending[lane as usize].clone();
        let name = lane.thread();
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                for (token, job, always) in rx {
                    if !always && ending.load(Ordering::Acquire) {
                        drop(job);
                        logging::line("input", &format!("a job for the host's {name} thread not begun: the application is ending"));
                        if done.send((token, Err(HostError::of(&ENDING)))).is_err() {
                            break;
                        }
                        continue;
                    }
                    let result = logging::contain(job).unwrap_or_else(|p| {
                        logging::line("input", &format!("a job on the host's {name} thread failed: {p}"));
                        Err(HostError::with(&STYLE_THREAD_FAILED, format!("the host's thread for it failed: {p}")))
                    });
                    if done.send((token, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| lane.not_started(&e))?;
        *slot = Some(tx.clone());
        Ok(tx)
    }

    /// The outcomes that came back since the last tick that an answer or a release waits for, in
    /// the order they came. The others are dropped here: what became of them is in the log.
    pub(crate) fn take_due(&self) -> Vec<Due> {
        let mut out = Vec::new();
        while let Ok((token, result)) = self.done_rx.try_recv() {
            if let Some(Waiting { answer, release, prio }) = self.waiting.borrow_mut().remove(&token) {
                out.push(Due { answer, release, prio, result });
            }
        }
        out
    }
}

/// An outcome as its callback is called: `(true)` or `(false, err)`.
pub(crate) fn outcome_args(lua: &Lua, result: &Result<(), HostError>) -> mlua::Result<MultiValue> {
    errors_lua::done(lua, result)
}

/// The outcome as a call returns it: `true`, or `false, err`.
pub(crate) fn outcome_values(lua: &Lua, result: Result<(), HostError>) -> mlua::Result<MultiValue> {
    outcome_args(lua, &result)
}

/// A callback a call takes last: `None` for none, or the function; anything else raises, naming
/// the call `f`.
pub(crate) fn callback(f: &str, cb: Value) -> mlua::Result<Option<Function>> {
    match cb {
        Value::Nil => Ok(None),
        Value::Function(cb) => Ok(Some(cb)),
        other => Err(mlua::Error::external(format!(
            "{f}: the callback (last argument) must be a function, got {}",
            crate::describe_value(&other)
        ))),
    }
}

/// How `host.input.click` clicks (`opts.via`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Via {
    /// Through Windows' input, as every click always was: the default.
    Input,
    /// Posted to the window as messages: nothing comes to the front.
    Post,
    /// Through Windows' input, the window's top-level window holding the no-activate style.
    NoActivate,
}

impl Via {
    /// The word a module writes for it.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Via::Input => "input",
            Via::Post => "post",
            Via::NoActivate => "noactivate",
        }
    }
}

/// `host.input.click`'s options, read and checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClickOpts {
    pub button: MouseButton,
    /// The window the click is for: refused when the point is not in it or something else is drawn
    /// over it.
    pub window: Option<isize>,
    pub via: Via,
    /// Whether the pointer moves to the point first; false only with `via = "post"`.
    pub move_pointer: bool,
    /// How many clicks: 1, or 2 for a double-click, which is made through the system's input only.
    pub clicks: u8,
}

/// Reads `host.input.click`'s `opts`: `button` as it always was read (case-insensitive, anything
/// else left), `window` a window's id, `via` one of `"input"`, `"post"` and `"noactivate"`,
/// `move` true or false, `clicks` 1 or 2. A `via` other than `"input"` without a `window`,
/// `move = false` with a way that clicks where the pointer is, and `clicks = 2` with a way other
/// than `"input"` raise — each naming the option.
///
/// `clicks` is a count, as every system and toolkit names the quantity — AppKit's `clickCount`,
/// Quartz's click state, JUCE's number of clicks, AutoHotkey's `Click X, Y, ClickCount` — so it
/// has room for more than two without another option; this host makes one or two.
pub(crate) fn click_opts(opts: Option<&Table>) -> mlua::Result<ClickOpts> {
    const F: &str = "host.input.click";
    let bad = |msg: String| mlua::Error::external(format!("{F}: {msg}"));
    let button = crate::button_from(opts);
    let Some(t) = opts else {
        return Ok(ClickOpts { button, window: None, via: Via::Input, move_pointer: true, clicks: 1 });
    };
    let window = match t.get::<Value>("window")? {
        Value::Nil => None,
        Value::Integer(i) => Some(i as isize),
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 => Some(n as isize),
        other => {
            return Err(bad(format!(
                "opts.window is a window's id — a whole number from host.window or an overlay's O:hwnd() — not {}",
                crate::describe_value(&other)
            )))
        }
    };
    let via = match t.get::<Value>("via")? {
        Value::Nil => Via::Input,
        Value::String(s) => match &*s.to_str()? {
            "input" => Via::Input,
            "post" => Via::Post,
            "noactivate" => Via::NoActivate,
            other => {
                return Err(bad(format!("opts.via is \"input\", \"post\" or \"noactivate\", not {other:?}")));
            }
        },
        other => {
            return Err(bad(format!(
                "opts.via is \"input\", \"post\" or \"noactivate\", not {}",
                crate::describe_value(&other)
            )))
        }
    };
    let move_pointer = crate::opt_bool(Some(t), "move", true, F)?;
    if via != Via::Input && window.is_none() {
        return Err(bad(format!(
            "via = \"{}\" needs opts.window, the window the click is for",
            via.word()
        )));
    }
    if !move_pointer && via != Via::Post {
        return Err(bad(format!(
            "move = false is for via = \"post\" only: via = \"{}\" clicks where the pointer is",
            via.word()
        )));
    }
    let clicks = match t.get::<Value>("clicks")? {
        Value::Nil => 1,
        Value::Integer(i @ (1 | 2)) => i as u8,
        Value::Number(n) if n == 1.0 || n == 2.0 => n as u8,
        other => return Err(bad(format!("opts.clicks is 1 or 2, not {}", crate::describe_value(&other)))),
    };
    if clicks == 2 && via != Via::Input {
        return Err(bad(format!("clicks = 2 is for via = \"input\" only, not via = \"{}\"", via.word())));
    }
    Ok(ClickOpts { button, window, via, move_pointer, clicks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(lua: &Lua, src: &str) -> mlua::Result<ClickOpts> {
        let t: Option<Table> = lua.load(src).eval().unwrap();
        click_opts(t.as_ref())
    }

    fn raised(r: mlua::Result<ClickOpts>) -> String {
        r.expect_err("raises").to_string()
    }

    /// Without options, or with only a button, a click is today's: through Windows' input, for no
    /// window in particular, the pointer moved to it.
    #[test]
    fn a_click_without_a_way_is_the_one_it_always_was() {
        let lua = Lua::new();
        let plain = ClickOpts { button: MouseButton::Left, window: None, via: Via::Input, move_pointer: true, clicks: 1 };
        assert_eq!(opts(&lua, "return nil").unwrap(), plain);
        assert_eq!(opts(&lua, "return {}").unwrap(), plain);
        assert_eq!(opts(&lua, "return { button = 'RIGHT' }").unwrap().button, MouseButton::Right);
        assert_eq!(opts(&lua, "return { button = 'side' }").unwrap().button, MouseButton::Left, "anything else is left");
        let w = opts(&lua, "return { window = 4242 }").unwrap();
        assert_eq!((w.window, w.via), (Some(4242), Via::Input), "a window, checked, clicked as ever");
    }

    /// The two ways that keep the window in front where it is need the window the click is for;
    /// `move = false` is the posted click's alone; every other value raises, naming the option.
    #[test]
    fn a_way_needs_its_window_and_a_bad_option_raises() {
        let lua = Lua::new();
        let post = opts(&lua, "return { window = 7, via = 'post', button = 'middle' }").unwrap();
        assert_eq!(post, ClickOpts { button: MouseButton::Middle, window: Some(7), via: Via::Post, move_pointer: true, clicks: 1 });
        assert!(!opts(&lua, "return { window = 7, via = 'post', move = false }").unwrap().move_pointer);
        assert_eq!(opts(&lua, "return { window = 7, via = 'noactivate' }").unwrap().via, Via::NoActivate);
        assert_eq!(opts(&lua, "return { window = 7, via = 'input' }").unwrap().via, Via::Input);
        let e = raised(opts(&lua, "return { via = 'post' }"));
        assert!(e.contains("host.input.click: via = \"post\" needs opts.window, the window the click is for"), "{e}");
        let e = raised(opts(&lua, "return { via = 'noactivate' }"));
        assert!(e.contains("via = \"noactivate\" needs opts.window"), "{e}");
        let e = raised(opts(&lua, "return { window = 7, via = 'Post' }"));
        assert!(e.contains("opts.via is \"input\", \"post\" or \"noactivate\", not \"Post\""), "{e}");
        let e = raised(opts(&lua, "return { window = 7, via = true }"));
        assert!(e.contains("opts.via is \"input\", \"post\" or \"noactivate\", not a boolean"), "{e}");
        let e = raised(opts(&lua, "return { window = 7, move = false }"));
        assert!(e.contains("move = false is for via = \"post\" only: via = \"input\" clicks where the pointer is"), "{e}");
        let e = raised(opts(&lua, "return { window = 7, via = 'noactivate', move = false }"));
        assert!(e.contains("via = \"noactivate\" clicks where the pointer is"), "{e}");
        let e = raised(opts(&lua, "return { window = 7, via = 'post', move = 'no' }"));
        assert!(e.contains("host.input.click: move is true or false"), "{e}");
        let e = raised(opts(&lua, "return { window = 'FX' }"));
        assert!(e.contains("opts.window is a window's id"), "{e}");
        let e = raised(opts(&lua, "return { window = 1.5 }"));
        assert!(e.contains("not 1.5"), "{e}");
    }

    /// `clicks`: one by default, 1 and 2 as numbers taken — with a button, a window, and the input's
    /// way said or not; anything else raises, naming the option, and so does a double-click by a way
    /// other than the system's input.
    #[test]
    fn clicks_is_one_or_two_and_a_double_click_goes_through_the_input() {
        let lua = Lua::new();
        assert_eq!(opts(&lua, "return {}").unwrap().clicks, 1);
        assert_eq!(opts(&lua, "return { clicks = 1 }").unwrap().clicks, 1);
        let double = opts(&lua, "return { clicks = 2, button = 'right', window = 9 }").unwrap();
        assert_eq!(double, ClickOpts { button: MouseButton::Right, window: Some(9), via: Via::Input, move_pointer: true, clicks: 2 });
        assert_eq!(opts(&lua, "return { clicks = 2, via = 'input' }").unwrap().clicks, 2);
        assert_eq!(opts(&lua, "return { clicks = 4 / 2 }").unwrap().clicks, 2, "a number that is 2");
        for (src, said) in [
            ("return { clicks = 3 }", "opts.clicks is 1 or 2, not 3"),
            ("return { clicks = 0 }", "opts.clicks is 1 or 2, not 0"),
            ("return { clicks = 1.5 }", "opts.clicks is 1 or 2, not 1.5"),
            ("return { clicks = '2' }", "opts.clicks is 1 or 2, not the string \"2\""),
            ("return { clicks = true }", "opts.clicks is 1 or 2, not a boolean"),
            ("return { clicks = 2, window = 7, via = 'post' }", "clicks = 2 is for via = \"input\" only, not via = \"post\""),
            ("return { clicks = 2, window = 7, via = 'noactivate' }", "clicks = 2 is for via = \"input\" only, not via = \"noactivate\""),
        ] {
            let e = raised(opts(&lua, src));
            assert!(e.contains(&format!("host.input.click: {said}")), "{src}: {e}");
        }
        // A one-click with a posted way stays what it was.
        assert_eq!(opts(&lua, "return { clicks = 1, window = 7, via = 'post' }").unwrap().via, Via::Post);
    }

    /// The gesture lane runs its jobs on a thread of its own, named for it, and answers the job's
    /// outcome to who waits, as the style lane does; whether a lane was started is known, for the
    /// end of the application; and its own answers are in its Reasons' words. Pure threads.
    #[test]
    fn the_gesture_lane_runs_on_its_own_thread_and_answers_in_its_words() {
        use crate::errors::input::DOUBLE_CUT;
        let lua = Lua::new();
        let acting = Acting::new();
        assert!(!acting.started(Lane::Gesture) && !acting.started(Lane::Style));
        let named = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let n = named.clone();
        let cb = lua.create_function(|_, ()| Ok(())).unwrap();
        let release = Release { idx: 2, button: MouseButton::Left, presses: 0 };
        acting.run_releasing(
            Lane::Gesture,
            Box::new(move || {
                *n.lock().unwrap() = std::thread::current().name().unwrap_or_default().to_string();
                Err(HostError::of(&DOUBLE_CUT))
            }),
            Some(Answer { idx: 2, lua: lua.clone(), cb, what: "input.click" }),
            release,
        );
        assert!(acting.started(Lane::Gesture) && !acting.started(Lane::Style), "one lane started, not the other");
        let mut due = Vec::new();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while due.is_empty() && std::time::Instant::now() < until {
            due.extend(acting.take_due());
            std::thread::yield_now();
        }
        assert_eq!(due.len(), 1);
        assert_eq!((due[0].release, &due[0].result), (Some(release), &Err(HostError::of(&DOUBLE_CUT))));
        assert_eq!(due[0].answer.as_ref().map(|a| a.what), Some("input.click"));
        assert_eq!(*named.lock().unwrap(), "input-gesture");
        assert_eq!(Lane::Gesture.gone().message(), "the host's input-gesture thread is gone");
        assert!(Lane::Gesture.gone().reason() == &GESTURE_THREAD_GONE);
        let e = std::io::Error::other("no threads left");
        let not = Lane::Gesture.not_started(&e);
        assert!(not.reason() == &GESTURE_THREAD_NOT_STARTED && not.message() == "the host could not start its input-gesture thread: no threads left", "{not:?}");
        assert!(Lane::Style.not_started(&e).reason() == &STYLE_THREAD_NOT_STARTED);
        assert!(acting.run_and_wait(Lane::Gesture, Box::new(|| Ok(())), std::time::Duration::from_secs(10)), "waited for at the end");
    }

    /// The end of the application on the gesture lane: the gesture the thread is making goes on to its
    /// end, every one queued behind it is not begun — answered `cancelled`, `the application is
    /// ending` — and the end waits for the one in flight only, not for the queue. Pure threads.
    #[test]
    fn at_the_end_a_gesture_in_flight_ends_and_the_ones_queued_are_not_begun() {
        use std::time::{Duration, Instant};
        let lua = Lua::new();
        let acting = Acting::new();
        let ran = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (started_tx, started_rx) = channel::<()>();
        let (go_tx, go_rx) = channel::<()>();
        let r1 = ran.clone();
        // The one in flight: begun, and held until the end has been asked for.
        acting.run(
            Lane::Gesture,
            Box::new(move || {
                let _ = started_tx.send(());
                let _ = go_rx.recv_timeout(Duration::from_secs(10));
                r1.lock().unwrap().push(1);
                Ok(())
            }),
            None,
        );
        let cb = || lua.create_function(|_, ()| Ok(())).unwrap();
        for i in 2..=4 {
            let r = ran.clone();
            acting.run(
                Lane::Gesture,
                Box::new(move || {
                    r.lock().unwrap().push(i);
                    std::thread::sleep(Duration::from_millis(400));
                    Ok(())
                }),
                Some(Answer { idx: i, lua: lua.clone(), cb: cb(), what: "input.click" }),
            );
        }
        started_rx.recv_timeout(Duration::from_secs(10)).expect("the first gesture began");
        let t = Instant::now();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            let _ = go_tx.send(());
        });
        assert!(acting.end_and_wait(Lane::Gesture, Duration::from_secs(5)), "the one in flight ended in time");
        assert!(t.elapsed() < Duration::from_millis(1000), "the three queued, 400 ms each, were not waited for: {:?}", t.elapsed());
        release.join().unwrap();
        assert_eq!(*ran.lock().unwrap(), vec![1], "only the one in flight ran");
        let mut due = Vec::new();
        let until = Instant::now() + Duration::from_secs(10);
        while due.len() < 3 && Instant::now() < until {
            due.extend(acting.take_due());
            std::thread::yield_now();
        }
        assert_eq!(due.len(), 3);
        assert!(due.iter().all(|d| d.result == Err(HostError::of(&ENDING))), "each answered as not begun");
        assert_eq!(HostError::of(&ENDING).message(), "the application is ending");
        // The style lane is not ending: its jobs run as ever.
        assert!(acting.run_and_wait(Lane::Style, Box::new(|| Ok(())), Duration::from_secs(10)));
    }

    /// A callback is a function or nothing.
    #[test]
    fn a_callback_is_a_function_or_nothing() {
        let lua = Lua::new();
        assert!(callback("host.input.click", Value::Nil).unwrap().is_none());
        let f = lua.create_function(|_, ()| Ok(())).unwrap();
        assert!(callback("host.input.click", Value::Function(f)).unwrap().is_some());
        let e = callback("host.input.click", Value::Boolean(true)).unwrap_err().to_string();
        assert!(e.contains("host.input.click: the callback (last argument) must be a function, got a boolean"), "{e}");
    }

    /// A job runs on its lane's thread, in order; its outcome comes back for the answer that waits
    /// for it, a panic as an outcome of its own; an outcome known at the call comes back the same
    /// way; one nobody waits for is dropped. Pure threads: nothing here touches a window.
    #[test]
    fn a_job_runs_on_its_thread_and_its_outcome_comes_back_to_who_waits() {
        use crate::errors::input::{COVERED, NOT_KEPT};
        let lua = Lua::new();
        let acting = Acting::new();
        let cb = || lua.create_function(|_, ()| Ok(())).unwrap();
        let answer = |what| Some(Answer { idx: 3, lua: lua.clone(), cb: cb(), what });
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (o1, o2) = (order.clone(), order.clone());
        acting.run(Lane::Style, Box::new(move || { o1.lock().unwrap().push(1); Ok(()) }), answer("first"));
        acting.run(Lane::Style, Box::new(move || { o2.lock().unwrap().push(2); Err(HostError::of(&NOT_KEPT)) }), None);
        acting.run(Lane::Style, Box::new(|| panic!("{}a job that fails", crate::EXPECTED_PANIC)), answer("panics"));
        acting.answer_now(Answer { idx: 4, lua: lua.clone(), cb: cb(), what: "at once" }, Err(HostError::of(&COVERED)));
        // A click's release comes back with its outcome, answered or not.
        let release = Release { idx: 5, button: MouseButton::Right, presses: 2 };
        acting.run_releasing(Lane::Style, Box::new(|| Err(HostError::of(&NOT_KEPT))), None, release);
        crate::quiet_expected_panics();
        let mut due = Vec::new();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while due.len() < 4 && std::time::Instant::now() < until {
            due.extend(acting.take_due());
            std::thread::yield_now();
        }
        let (released, due): (Vec<Due>, Vec<Due>) = due.into_iter().partition(|d| d.answer.is_none());
        assert_eq!(released.len(), 1);
        assert_eq!((released[0].release, &released[0].result), (Some(release), &Err(HostError::of(&NOT_KEPT))));
        assert!(due.iter().all(|d| d.release.is_none()));
        let mut got: Vec<(&str, usize, Result<(), HostError>)> = due
            .into_iter()
            .map(|d| {
                let a = d.answer.unwrap();
                (a.what, a.idx, d.result)
            })
            .collect();
        got.sort_by_key(|g| g.0);
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!((got[0].0, got[0].1, &got[0].2), ("at once", 4, &Err(HostError::of(&COVERED))));
        assert_eq!((got[1].0, &got[1].2), ("first", &Ok(())));
        assert_eq!(got[2].0, "panics");
        let failed = got[2].2.as_ref().unwrap_err();
        assert!(failed.reason() == &STYLE_THREAD_FAILED && failed.message().starts_with("the host's thread for it failed: "), "{failed:?}");
        assert_eq!(*order.lock().unwrap(), vec![1, 2], "in the order they were handed over");
        assert!(acting.waiting.borrow().is_empty(), "every answer given once");
    }

    /// The end of the application's wait: a job done in time says so; one that is not — the
    /// program it waits for does not answer — is given up at the bound, and the application goes
    /// on; one that panics is not done either, and says so at once. Pure threads.
    #[test]
    fn the_last_job_is_waited_for_a_bounded_time() {
        use std::time::{Duration, Instant};
        let acting = Acting::new();
        assert!(acting.run_and_wait(Lane::Style, Box::new(|| Ok(())), Duration::from_secs(10)));
        let (release, held) = channel::<()>();
        let t = Instant::now();
        let hangs: ActJob = Box::new(move || {
            let _ = held.recv();
            Ok(())
        });
        assert!(!acting.run_and_wait(Lane::Style, hangs, Duration::from_millis(50)));
        assert!(t.elapsed() >= Duration::from_millis(50), "{:?}", t.elapsed());
        drop(release);
        crate::quiet_expected_panics();
        let t = Instant::now();
        let fails: ActJob = Box::new(|| panic!("{}a last job that fails", crate::EXPECTED_PANIC));
        assert!(!acting.run_and_wait(Lane::Style, fails, Duration::from_secs(10)));
        assert!(t.elapsed() < Duration::from_secs(10), "a panic is not waited out");
    }

    /// A callback's arguments, and a call's values: `true`, or `false` and the error value.
    #[test]
    fn an_outcome_is_true_or_false_and_why() {
        let lua = Lua::new();
        let ok = outcome_args(&lua, &Ok(())).unwrap();
        assert_eq!(ok.len(), 1);
        assert!(matches!(ok.front(), Some(Value::Boolean(true))));
        let no = outcome_values(&lua, Err(HostError::of(&crate::errors::common::WINDOW_GONE))).unwrap();
        assert!(matches!(no.front(), Some(Value::Boolean(false))));
        let Some(Value::Table(err)) = no.get(1) else { panic!("{no:?}") };
        assert_eq!(err.raw_get::<String>("code").unwrap(), "gone");
        assert_eq!(err.raw_get::<String>("message").unwrap(), "the window is gone");
    }

    /// What the window-style thread answers for itself is in its Reasons' words: the thread gone,
    /// not started, a job that failed — as `lane.thread()` names it.
    #[test]
    fn the_style_threads_own_answers_are_its_reasons() {
        assert_eq!(Lane::Style.thread(), "window-style");
        let gone = Lane::Style.gone();
        assert!(gone.reason() == &STYLE_THREAD_GONE);
        assert_eq!(gone.message(), "the host's window-style thread is gone");
        let not = Lane::Style.not_started(&std::io::Error::other("e"));
        assert_eq!((not.code(), not.message()), (crate::errors::Code::Failed, "the host could not start its window-style thread: e"));
    }
}
