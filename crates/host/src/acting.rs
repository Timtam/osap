//! What a module asks of another program that the host does on a thread of its own, and the way
//! the outcome comes back to the module.
//!
//! **Why a thread.** Some ways of acting wait for the other program: giving a window of another
//! program the no-activate style (`host.input.click`'s `via = "noactivate"`) sends that program's
//! thread a message and waits for its answer, with no bound Windows lets us set. Nothing that can
//! wait for another program runs on the event loop, which carries every key a person presses, so
//! such work is a job ([`crate::backend::ActJob`]) for a thread of the host's — one per [`Lane`],
//! started at its first job, each lane's jobs in the order they were handed over. (Pressing an
//! element, `host.element.press`, is a question to an accessibility tree like the others, asked on
//! the accessibility thread: element.rs.)
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
//! the way (`via`) and, for a posted click only, whether the pointer moves. Read and checked here,
//! before anything acts, so a mistake raises at the call.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::{ActJob, MouseButton};
use crate::logging;
use crate::ocr::types::{current_priority, Priority};

/// A thread of the host's for one kind of job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// Windows' styles: a click with `via = "noactivate"`, the style it gives, and giving it back.
    /// One thread, so a style is given back only after every click handed over before.
    Style = 0,
}

/// How many lanes there are.
const LANES: usize = 1;

impl Lane {
    /// Its thread's name.
    fn thread(self) -> &'static str {
        match self {
            Lane::Style => "window-style",
        }
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

/// An outcome that came back, for its module's mailbox: the [`Answer`], the priority of the
/// dispatch that asked, and the outcome — `Err` with what the module is told.
pub(crate) struct Due {
    pub answer: Answer,
    pub prio: Priority,
    pub result: Result<(), String>,
}

/// A lane's way to its thread: each job with its number.
type ToThread = Sender<(u64, ActJob)>;

/// The jobs on their way and the outcomes coming back. Main thread only, like the rest of `Shared`;
/// the threads hold only their own end of a channel.
pub(crate) struct Acting {
    next: Cell<u64>,
    /// The answers owed, by the job's number. A job handed over without one is not in here: what
    /// became of it is in the log, where the job writes it — and its thread, should it panic.
    waiting: RefCell<HashMap<u64, (Answer, Priority)>>,
    done_tx: Sender<(u64, Result<(), String>)>,
    done_rx: Receiver<(u64, Result<(), String>)>,
    /// Each lane's way to its thread, once that thread is started.
    lanes: [RefCell<Option<ToThread>>; LANES],
}

impl Acting {
    pub(crate) fn new() -> Acting {
        let (done_tx, done_rx) = channel();
        Acting { next: Cell::new(0), waiting: RefCell::new(HashMap::new()), done_tx, done_rx, lanes: Default::default() }
    }

    /// Hands `job` to `lane`'s thread; `answer`, when given, gets its outcome on a later tick. A
    /// thread that cannot be started answers the job as not done, with the reason.
    pub(crate) fn run(&self, lane: Lane, job: ActJob, answer: Option<Answer>) {
        let token = self.token(answer);
        let sent = self.sender(lane).and_then(|tx| {
            tx.send((token, job)).map_err(|_| format!("the host's {} thread is gone", lane.thread()))
        });
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
        self.run(
            lane,
            Box::new(move || {
                let r = job();
                let _ = tx.send(());
                r
            }),
            None,
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

    /// An outcome known at the call, for `answer`: delivered on the next tick, as one from a
    /// thread is, so a callback is never called inside the call that handed it over.
    pub(crate) fn answer_now(&self, answer: Answer, result: Result<(), String>) {
        let token = self.token(Some(answer));
        let _ = self.done_tx.send((token, result));
    }

    /// The next job's number, with its answer noted — under the priority of the dispatch that asks.
    fn token(&self, answer: Option<Answer>) -> u64 {
        let token = self.next.get() + 1;
        self.next.set(token);
        if let Some(a) = answer {
            self.waiting.borrow_mut().insert(token, (a, current_priority()));
        }
        token
    }

    /// `lane`'s thread, started at its first job. It runs each job inside `logging::contain`, so a
    /// job that panics is answered with the panic's report — and the report is in the log, for a
    /// job nobody waits for — and the thread goes on.
    fn sender(&self, lane: Lane) -> Result<ToThread, String> {
        let mut slot = self.lanes[lane as usize].borrow_mut();
        if let Some(tx) = slot.as_ref() {
            return Ok(tx.clone());
        }
        let (tx, rx) = channel::<(u64, ActJob)>();
        let done = self.done_tx.clone();
        let name = lane.thread();
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                for (token, job) in rx {
                    let result = logging::contain(job).unwrap_or_else(|p| {
                        logging::line("input", &format!("a job on the host's {name} thread failed: {p}"));
                        Err(format!("the host's thread for it failed: {p}"))
                    });
                    if done.send((token, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("the host could not start its {} thread: {e}", lane.thread()))?;
        *slot = Some(tx.clone());
        Ok(tx)
    }

    /// The outcomes that came back since the last tick that somebody waits for, in the order they
    /// came. The others are dropped here: what became of them is in the log.
    pub(crate) fn take_due(&self) -> Vec<Due> {
        let mut out = Vec::new();
        while let Ok((token, result)) = self.done_rx.try_recv() {
            if let Some((answer, prio)) = self.waiting.borrow_mut().remove(&token) {
                out.push(Due { answer, prio, result });
            }
        }
        out
    }
}

/// An outcome as its callback is called: `(true)` or `(false, why)`.
pub(crate) fn outcome_args(lua: &Lua, result: &Result<(), String>) -> mlua::Result<MultiValue> {
    Ok(match result {
        Ok(()) => MultiValue::from_vec(vec![Value::Boolean(true)]),
        Err(why) => MultiValue::from_vec(vec![Value::Boolean(false), Value::String(lua.create_string(why)?)]),
    })
}

/// The outcome as a call returns it: `true`, or `false, why`.
pub(crate) fn outcome_values(lua: &Lua, result: Result<(), String>) -> mlua::Result<MultiValue> {
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
}

/// Reads `host.input.click`'s `opts`: `button` as it always was read (case-insensitive, anything
/// else left), `window` a window's id, `via` one of `"input"`, `"post"` and `"noactivate"`,
/// `move` true or false. A `via` other than `"input"` without a `window`, and `move = false` with
/// a way that clicks where the pointer is, raise — each naming the option.
pub(crate) fn click_opts(opts: Option<&Table>) -> mlua::Result<ClickOpts> {
    const F: &str = "host.input.click";
    let bad = |msg: String| mlua::Error::external(format!("{F}: {msg}"));
    let button = crate::button_from(opts);
    let Some(t) = opts else {
        return Ok(ClickOpts { button, window: None, via: Via::Input, move_pointer: true });
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
    Ok(ClickOpts { button, window, via, move_pointer })
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
        let plain = ClickOpts { button: MouseButton::Left, window: None, via: Via::Input, move_pointer: true };
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
        assert_eq!(post, ClickOpts { button: MouseButton::Middle, window: Some(7), via: Via::Post, move_pointer: true });
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
        let lua = Lua::new();
        let acting = Acting::new();
        let cb = || lua.create_function(|_, ()| Ok(())).unwrap();
        let answer = |what| Some(Answer { idx: 3, lua: lua.clone(), cb: cb(), what });
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (o1, o2) = (order.clone(), order.clone());
        acting.run(Lane::Style, Box::new(move || { o1.lock().unwrap().push(1); Ok(()) }), answer("first"));
        acting.run(Lane::Style, Box::new(move || { o2.lock().unwrap().push(2); Err("no".into()) }), None);
        acting.run(Lane::Style, Box::new(|| panic!("{}a job that fails", crate::EXPECTED_PANIC)), answer("panics"));
        acting.answer_now(Answer { idx: 4, lua: lua.clone(), cb: cb(), what: "at once" }, Err("refused".into()));
        crate::quiet_expected_panics();
        let mut due = Vec::new();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while due.len() < 3 && std::time::Instant::now() < until {
            due.extend(acting.take_due());
            std::thread::yield_now();
        }
        let mut got: Vec<(&str, usize, Result<(), String>)> =
            due.into_iter().map(|d| (d.answer.what, d.answer.idx, d.result)).collect();
        got.sort_by_key(|g| g.0);
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!((got[0].0, got[0].1, &got[0].2), ("at once", 4, &Err("refused".to_string())));
        assert_eq!((got[1].0, &got[1].2), ("first", &Ok(())));
        assert_eq!(got[2].0, "panics");
        assert!(got[2].2.as_ref().unwrap_err().starts_with("the host's thread for it failed: "), "{:?}", got[2].2);
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

    /// A callback's arguments, and a call's values: `true`, or `false` and the reason.
    #[test]
    fn an_outcome_is_true_or_false_and_why() {
        let lua = Lua::new();
        let ok = outcome_args(&lua, &Ok(())).unwrap();
        assert_eq!(ok.len(), 1);
        assert!(matches!(ok.front(), Some(Value::Boolean(true))));
        let no = outcome_values(&lua, Err("the window is gone".into())).unwrap();
        assert!(matches!(no.front(), Some(Value::Boolean(false))));
        assert!(matches!(no.get(1), Some(Value::String(s)) if s.to_str().unwrap() == "the window is gone"));
    }
}
