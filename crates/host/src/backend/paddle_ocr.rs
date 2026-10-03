//! Secondary OCR engine: a PaddleOCR recognition model run in-process via ONNX
//! Runtime (the `ort` crate), recognition-only. The system recognisers are fast but
//! blind to isolated single glyphs (a lone "1"); this neural recognizer reads them.
//!
//! **On Windows** it is the fallback of `Windows.Media.Ocr`: used only when the system recogniser
//! returns nothing on a small region — so the fast path stays the OS engine and this pays its cost
//! rarely. The recognition model is embedded in the binary (via `include_bytes!`) and ONNX Runtime
//! is linked in statically, so the app stays fully self-contained and portable.
//!
//! **On a Mac** it is Vision's fallback by the same rule: once it is ready, every small read hands
//! it the region's content crop the moment the read has it, at the reading thread's own quality
//! of service ([`Qos::Reader`]), and its text answers only when Vision's accurate ladder read
//! nothing (`ocr/merge.rs` holds the rule, `macos/ocr.rs` climbs it); on an Intel Mac it also
//! checks the fast level's first pass, whose text answers where the two read the same. A read
//! waits for it within the ladder's budget, and its reading is counted beside Vision's
//! (`ocr/shadow.rs`). ONNX Runtime is Microsoft's universal dylib in the bundle's
//! `Contents/Frameworks`, opened at run time, and the model is `Contents/Resources/ppocr-rec.onnx`
//! (`package-macos.sh --onnxruntime`); see [`init`] for why it is loaded and not linked, and in
//! what order. Where either is missing, on a macOS before 13.3, where it is not opened at all, or
//! where the dylib does not load — a slice it lacks — the recogniser is not available, the log
//! says so once, and Vision reads alone, as before. `ocr-bench` measures it beside Vision
//! (`--paddle`, `--paddle-probe`, and the pipeline's strategies).
//!
//! **One recogniser thread, asked in parallel with the system engine.** `recognize_image`
//! (`windows.rs`) hands every small region to this recogniser the moment it has the pixels —
//! before the system recogniser starts on it — and waits for the answer only when the system
//! recogniser read nothing. That rule is unchanged, and deliberate: the two run side by side on
//! every small region, so a miss costs the slower of the two rather than their sum. What changed
//! is who runs the recognition: it used to be a new thread per region, left running when the
//! system recogniser had answered or the region was blank. At Melodyne's ten regions a second
//! that was half a core of answers nobody read, and at seventy blank regions a second two and a
//! half cores, with as many threads as regions arrived faster than the one session drained them.
//! Now one long-lived thread ([`ask`]) takes the regions in order, and a region whose answer is
//! no longer wanted is cancelled when its [`Asked`] is dropped — skipped if it has not started,
//! stopped before its preprocessing or before the session if it has. The session was already
//! one, behind a lock, with one intra-op thread, so recognitions were one at a time before too;
//! only the preprocessing of regions nobody waits for no longer runs. On a Mac each region says
//! how urgent it is ([`Qos`]), and the thread asks for that quality of service before it runs it.
//!
//! **The arithmetic around the model is `ocr/paddle_pre.rs`**: the crop to the ink, the model's
//! input, the decoding of its output. It was taken out of this file unchanged in what it computes
//! here (Windows passes `Tighten::WINDOWS`; a golden test there pins the input bit for bit), so
//! that it compiles and is tested where this file cannot be. `ocr-bench --paddle` measures this
//! recogniser alone (`bench.rs`, a child of this file).
//!
//! **On a Mac every process that made the engine releases it before it ends** ([`release`]): the
//! session, then ONNX Runtime's environment, while everything they use is still there. ONNX
//! Runtime 1.22 keeps the environment in a static `unique_ptr` (`OrtEnv::p_instance_`) whose
//! destructor, run by `exit()`, deletes one that is still alive; and `ort` never releases it — it
//! keeps it in a static that is never dropped, as this file keeps the session. That deletion
//! locks the mutex of ONNX Runtime's logging (`DefaultLoggerMutex`), a function-local static made
//! when the environment was, so destroyed by `exit()` before the `unique_ptr`; macOS's libc++
//! throws "mutex lock failed: Invalid argument" out of a destructor, and the process aborts
//! (signal 6) after its work is done. Every process of CI's first run with the library did, the
//! `--paddle-probe` that never started the recogniser's thread among them. Released before
//! `exit()`, the `unique_ptr` is empty when its destructor runs.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use super::CapturedImage;
use crate::ocr::paddle_pre::{self, Tighten};

/// `ocr-bench --paddle` and `--paddle-probe`: the recogniser measured alone, on the bench's
/// pictures.
mod bench;
pub(crate) use bench::{bench_rows, probe};

struct Engine {
    /// `None` once [`release`] has taken it, on a Mac at the end of a process; never on Windows.
    session: Mutex<Option<Session>>,
    /// Recognition alphabet: class index `i` (1-based, after the CTC blank at 0)
    /// maps to `dict[i - 1]`.
    dict: Vec<String>,
}

/// The engine, or why there is none; made once, by the first recognition or the warm-up.
static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();

fn engine() -> Option<&'static Engine> {
    made().as_ref().ok()
}

/// Makes the engine the first time, inside a catch: a panic while it is made — in `ort`, which
/// panics rather than fails on a library it cannot load (see [`init`]) — is kept as the reason
/// there is none, once, instead of panicking again on every region. On a Mac it says once in
/// the log whether the recogniser is ready; on Windows only when it is not, once, since the
/// catch keeps the panic from the panic hook's own line and nothing makes it again.
fn made() -> &'static Result<Engine, String> {
    if let Some(made) = ENGINE.get() {
        return made;
    }
    // Counted in `IN_ORT` from before ONNX Runtime is first touched until the engine is in
    // `ENGINE`, where `release` looks for its session — not only until it is made: a release in
    // between would find no session there and take the environment from under it. Once `release`
    // has begun, nothing is made, and nothing said about it.
    let in_ort = IN_ORT.start();
    ENGINE.get_or_init(|| {
        if in_ort.is_none() {
            return Err(RELEASED.to_string());
        }
        let started = Instant::now();
        let made = match crate::logging::contain(init) {
            Ok(Ok(engine)) => Ok(engine),
            Ok(Err(e)) => Err(format!("{e:#}")),
            Err(panic) => Err(format!("making it panicked: {panic}")),
        };
        #[cfg(target_os = "macos")]
        crate::logging::line(
            "ocr",
            &match &made {
                Ok(_) => format!(
                    "the neural recogniser is ready (ONNX Runtime {}, session in {} ms)",
                    RUNTIME.get().map_or("?", String::as_str),
                    started.elapsed().as_millis()
                ),
                Err(why) => format!(
                    "the neural recogniser is not available on this Mac ({why}); text is read by Vision alone"
                ),
            },
        );
        #[cfg(not(target_os = "macos"))]
        {
            let _ = started;
            if let Err(why) = &made {
                crate::logging::line(
                    "ocr",
                    &format!(
                        "the neural recogniser is not available ({why}); small regions are read by the system \
                         recogniser alone"
                    ),
                );
            }
        }
        made
    })
    // `in_ort` dropped here, after the engine is in `ENGINE`.
}

/// Whether the engine has been made and works: asked without making it, so that a read can ask
/// on any thread without paying for the model. False until the warm-up or a first recognition
/// has made it, and for good when it could not be made. (On a Mac only: Windows asks every small
/// region regardless.) True a moment before the warm-up's own run over its dummy has ended: a read
/// in that moment queues behind it, and that run took 3.7 to 7 ms in CI's processes of their own
/// (`ocr-bench`'s first passes, 2026-10-03) — less than a fast pass, so not waited for here.
#[cfg_attr(windows, allow(dead_code))]
pub(super) fn ready() -> bool {
    matches!(ENGINE.get(), Some(Ok(_)))
}

/// Takes a lock whether or not an earlier holder panicked while holding it.
///
/// For the session lock that is the right answer, and `lock().ok()?` was the wrong one: one
/// panic under it — in `ort`'s wrapper, or in the decode that reads the outputs while the
/// session is still borrowed — poisoned the mutex for good, and every later recognition
/// answered `None` without a word, which is to say the lone-digit fallback was switched off for
/// the rest of the session. A panic cannot leave the session half-changed: the only thing done
/// to it under the lock is `run` (and, at the end of a Mac's process, [`release`] taking it
/// out), and ONNX Runtime's `Run` does not change the session — its
/// documentation allows several threads to call it on one session at once. (The mutex is
/// there because `ort`'s `run` takes `&mut self`, not because the session needs it.)
fn lock_even_if_poisoned<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Recognitions that are, or are about to be, inside ONNX Runtime.
///
/// `recognize_image` (`windows.rs`) asks the recogniser thread for one of these for every small
/// region, beside the system recogniser, and when that answers it does not wait for it: that is
/// the point of running them side by side, so that a miss costs the slower of the two rather than
/// their sum. (On a Mac every small read asks it beside Vision by the same rule, and `ocr-bench`
/// the same way.) The recogniser can therefore still be inside ONNX Runtime when
/// the application exits, and a thread in there while `ort`'s static cleanup runs is the fault
/// the warm-up join in `run` exists for: an access violation at exit. This counts them — from
/// the moment one is asked for until it is done, skipped or cancelled — so that the exit can
/// wait for them the way it waits for the warm-up — see `settle`. The recogniser thread itself,
/// waiting for work, is in no call of ONNX Runtime's and is not counted.
///
/// And once that wait has begun it is CLOSED: `start` refuses, so nothing enters ONNX Runtime
/// after the exit stopped waiting. The recognise thread of `host.ocr.read` can still be working
/// through a job of many regions then — the exit waits for it only a second — and without this
/// each small region it reached would start a recognition nobody waits for.
pub(super) struct InFlight {
    running: AtomicUsize,
    closed: AtomicBool,
}

/// The recognitions of this process. A struct rather than a bare counter so the tests can
/// have counters of their own and run beside each other.
pub(super) static IN_FLIGHT: InFlight = InFlight::new();

/// The calls of this file into ONNX Runtime under way now, on any thread: the engine being made
/// ([`made`]) and a model run, from its input tensor to the drop of its outputs ([`run_model`]).
/// Closed by [`release`], which waits for it to be idle before it releases anything, so that
/// nothing is inside ONNX Runtime when its session and environment go, and nothing enters it
/// after: a run asked later answers nothing, and an engine not made yet is never made. Never
/// closed on Windows. The recogniser's thread waiting for work, and the preprocessing, are not
/// in it.
static IN_ORT: InFlight = InFlight::new();

/// Why there is no engine once [`release`] has begun; never logged.
const RELEASED: &str = "the process is ending, and ONNX Runtime has been released";

/// One recognition counted in `InFlight`. Ends when it is dropped, unwinding included, so a
/// recognition that panics does not keep the exit waiting for it.
pub(super) struct Running<'a> {
    of: &'a InFlight,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.of.running.fetch_sub(1, Ordering::AcqRel);
    }
}

impl InFlight {
    pub(super) const fn new() -> Self {
        InFlight { running: AtomicUsize::new(0), closed: AtomicBool::new(false) }
    }

    /// Counts one recognition, from now until the returned guard is dropped; `None`, counting
    /// nothing, once [`close`](Self::close) was called — the caller then does not start it.
    ///
    /// Taken by the caller BEFORE it hands the region over and moved into the job: counted only
    /// once the recogniser had started it, a recognition asked for a moment before the exit
    /// would not be counted yet, and the exit would not wait for it.
    ///
    /// Counted first and checked second, both sequentially consistent, against `close`, which
    /// sets the flag first and reads the count second: whichever order the two threads meet
    /// in, either this sees the flag and backs out, or the exit sees this counted and waits.
    pub(super) fn start(&self) -> Option<Running<'_>> {
        self.running.fetch_add(1, Ordering::SeqCst);
        if self.closed.load(Ordering::SeqCst) {
            self.running.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Running { of: self })
    }

    /// No recognition starts from now on (see the type).
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub(super) fn count(&self) -> usize {
        self.running.load(Ordering::SeqCst)
    }

    /// Waits until nothing is counted, for at most `bound`. True when nothing is.
    ///
    /// Polled, in steps of a few milliseconds: this runs once, at exit, and a recognition takes
    /// tens of milliseconds, so a condition variable would buy nothing a person could notice.
    pub(super) fn wait_idle(&self, bound: Duration) -> bool {
        const STEP: Duration = Duration::from_millis(5);
        let deadline = Instant::now() + bound;
        loop {
            if self.count() == 0 {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            std::thread::sleep((deadline - now).min(STEP));
        }
    }
}

/// How long the exit waits for recognitions still running: many times the ~15 ms a warm
/// recognition of a small region takes, and short enough that an exit that has to give up is
/// still an exit. The same half second the desktop duplication thread is given at exit.
const SETTLE_BOUND: Duration = Duration::from_millis(500);

/// Waits, for at most half a second, until no recognition is inside ONNX Runtime any more.
/// Called by `run` at exit, beside the warm-up join; see `InFlight`. Closes the count first, so
/// no recognition starts after this has looked. Returns at once when nothing is running.
/// Otherwise it logs how long it waited, or, when the bound is hit, that it gave up — and then
/// leaves what is still running to the process exit, which is the state of things before this
/// wait existed, now with a line in the log when it happens.
pub fn settle() {
    let started = Instant::now();
    IN_FLIGHT.close();
    let at_exit = IN_FLIGHT.count();
    if at_exit == 0 {
        return;
    }
    // Logged either way once there was something to wait for — once per exit, and the only
    // record of whether this wait ever matters in practice.
    if IN_FLIGHT.wait_idle(SETTLE_BOUND) {
        crate::logging::line(
            "ocr",
            &format!(
                "waited {} ms at exit for {at_exit} neural OCR recognition(s) to finish",
                started.elapsed().as_millis()
            ),
        );
    } else {
        crate::logging::line(
            "ocr",
            &format!(
                "{} neural OCR recognition(s) still running after {} ms at exit; leaving them to \
                 the process exit, which can fault inside ONNX Runtime",
                IN_FLIGHT.count(),
                started.elapsed().as_millis()
            ),
        );
    }
}

/// Recognition model, embedded so the app is self-contained; its dictionary is
/// `paddle_pre::dict`'s. Windows only: a Mac reads it from the bundle (see [`init`]). The path is
/// relative to this file, so that it resolves where the file is borrowed as well.
#[cfg(windows)]
const MODEL: &[u8] = include_bytes!("../../models/ppocr-rec.onnx");

#[cfg(windows)]
fn init() -> anyhow::Result<Engine> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .commit_from_memory(MODEL)?;
    Ok(Engine { session: Mutex::new(Some(session)), dict: paddle_pre::dict() })
}

/// The version of ONNX Runtime the Mac loaded, as its library says it (`GetVersionString`).
#[cfg(target_os = "macos")]
static RUNTIME: OnceLock<String> = OnceLock::new();

/// The version this build's `ort` (2.0.0-rc.10) is made for. `ort` takes a newer minor version
/// with a warning and panics on an older one; Microsoft's 1.22.0 is what the package carries
/// (`tools/onnxruntime-mac.txt`), so anything else is refused before `ort` sees it.
#[cfg(target_os = "macos")]
const RUNTIME_WANTED: &str = "1.22.";

/// Where the bundle keeps ONNX Runtime and the model: `Contents/Frameworks/libonnxruntime.dylib`
/// and `Contents/Resources/ppocr-rec.onnx`, from the executable in `Contents/MacOS`, absolute.
/// Translocation copies the whole bundle, so the executable's own path finds them in the copy.
#[cfg(target_os = "macos")]
fn bundle_files() -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let exe = std::env::current_exe().map_err(|e| format!("this executable cannot find itself: {e}"))?;
    let exe = std::path::absolute(&exe).unwrap_or(exe);
    let contents = exe
        .parent()
        .and_then(std::path::Path::parent)
        .ok_or_else(|| format!("{} is not inside an application bundle", exe.display()))?;
    Ok((contents.join("Frameworks").join("libonnxruntime.dylib"), contents.join("Resources").join("ppocr-rec.onnx")))
}

/// Makes the engine on a Mac, in this order, and every step's failure is the reason the
/// recogniser is not available:
///
/// 1. Where the library and the model are ([`bundle_files`]).
/// 2. `ort::init_from` with the library's path, at once and in every case, before anything else
///    of `ort` is called. It does nothing but fix the path; `ort` reads `ORT_DYLIB_PATH` from the
///    environment for whichever call of its comes first otherwise, and this application reads no
///    environment variable for it. A source test below holds this order.
/// 3. This macOS is one the library is built for ([`RUNTIME_MACOS`]): below it the library is
///    not opened at all, rather than trusting `dlopen` to refuse it cleanly.
/// 4. The model is a file in the bundle.
/// 5. The library opened by this file itself ([`open_runtime`]), and its version asked: `ort`
///    panics on a library it cannot load or that is older than it expects, where this can say
///    why. The handle stays open for the life of the process.
/// 6. Only then the environment (`commit`) — said in [`ENV_MADE`] the moment it exists, and the
///    [`backstop`] registered right after it — and the session, from the model's file. CPU only,
///    one intra-op thread, graph optimisation level 3, as on Windows: the Core ML provider brings
///    nothing a model this small could use, and no Mac without a Neural Engine has one anyway.
#[cfg(target_os = "macos")]
fn init() -> anyhow::Result<Engine> {
    let files = bundle_files();
    let environment = ort::init_from(files.as_ref().map(|(dylib, _)| dylib.display().to_string()).unwrap_or_default())
        .with_name("automation-platform")
        .with_telemetry(false);
    let (dylib, model) = files.map_err(anyhow::Error::msg)?;
    macos_new_enough().map_err(anyhow::Error::msg)?;
    if !model.is_file() {
        anyhow::bail!("the model is not in the bundle: {}", model.display());
    }
    let version = open_runtime(&dylib).map_err(anyhow::Error::msg)?;
    let _ = RUNTIME.set(version);
    environment.commit()?;
    // Before the session, which can fail: an environment made is released at the end whether
    // or not a session ever was.
    ENV_MADE.store(true, Ordering::SeqCst);
    register_backstop();
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .commit_from_file(&model)?;
    Ok(Engine { session: Mutex::new(Some(session)), dict: paddle_pre::dict() })
}

/// Whether ONNX Runtime's environment exists and has not been released: set by [`init`] once
/// `commit` has made it, cleared by [`release`] as it releases it. What the [`backstop`] asks.
#[cfg(target_os = "macos")]
static ENV_MADE: AtomicBool = AtomicBool::new(false);

/// What [`release`] did.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
pub(crate) enum Released {
    /// Nothing of ONNX Runtime's was held: the engine was never made, or released before.
    Nothing,
    /// Released now: the session when there was one, then the environment when there was one.
    Now { session: bool, environment: bool },
    /// This many calls were still inside ONNX Runtime at the bound: nothing was released, and
    /// the process must not end through `exit()`, whose static destructors would delete the
    /// environment under them ([`super::end_now`]).
    Busy(usize),
}

/// How long [`release`] waits for the calls still inside ONNX Runtime: the half second the exit
/// gives recognitions in flight ([`SETTLE_BOUND`]). By then the application has joined its
/// warm-up and waited that half second already, and the bench has joined its own threads, so
/// there is nothing left to wait for and it returns at once. The bound matters only for a call
/// that is stuck, or for the warm-up still making the session when a panic skipped the exit's
/// join — 68 to 135 ms on CI's arm64 Macs, but 389 to 751 ms on its Intel one, which can outlast
/// it. Either way a process that has to give up ends promptly, through `_exit`, without the
/// teardown that aborts: a longer bound would only make such a quit slower.
#[cfg_attr(windows, allow(dead_code))]
const RELEASE_BOUND: Duration = SETTLE_BOUND;

/// Releases ONNX Runtime before this process ends, on a Mac: [`release_with`] over this file's
/// session and ONNX Runtime's environment, after closing [`IN_FLIGHT`] so that no region is even
/// queued any more. Logged when it released something or had to give up. Called by every way
/// this process ends that runs its own code (`super::release_ocr`); a second call finds nothing.
#[cfg(target_os = "macos")]
pub(crate) fn release() -> Released {
    let started = Instant::now();
    IN_FLIGHT.close();
    let released = release_with(&IN_ORT, RELEASE_BOUND, take_session, release_environment);
    match &released {
        Released::Nothing => {}
        Released::Now { session, environment } => crate::logging::line(
            "ocr",
            &format!(
                "released {}{}{} before the process ends ({} ms)",
                if *session { "the neural recogniser's session" } else { "" },
                if *session && *environment { " and " } else { "" },
                if *environment { "ONNX Runtime's environment" } else { "" },
                started.elapsed().as_millis()
            ),
        ),
        Released::Busy(n) => crate::logging::line(
            "ocr",
            &format!(
                "{n} call(s) still inside ONNX Runtime after {} ms as the process ends; nothing was released, \
                 and the process ends without exit()'s teardown, which would abort on it",
                started.elapsed().as_millis()
            ),
        ),
    }
    released
}

/// The release, in its order, over what it acts on — apart from it, so that the tests run it:
///
/// 1. `in_ort` closed: nothing enters ONNX Runtime from here on (a run answers nothing, an
///    engine is not made).
/// 2. Waited for, at most `bound`, until nothing is inside it; when something still is, nothing
///    is released ([`Released::Busy`]) — the session and the environment are in use.
/// 3. The session (`session`, true when there was one), first: it logs through the
///    environment's logging, and was made in it.
/// 4. The environment (`environment`, true when there was one).
///
/// A second call closes what is closed, finds nothing inside, and the two steps find nothing.
#[cfg_attr(windows, allow(dead_code))]
fn release_with(
    in_ort: &InFlight,
    bound: Duration,
    session: impl FnOnce() -> bool,
    environment: impl FnOnce() -> bool,
) -> Released {
    in_ort.close();
    if !in_ort.wait_idle(bound) {
        // Counted once more: the last call can leave between the wait's last look and its giving
        // up, and with the count closed, none now is none for good.
        let inside = in_ort.count();
        if inside != 0 {
            return Released::Busy(inside);
        }
    }
    let session = session();
    let environment = environment();
    if session || environment {
        Released::Now { session, environment }
    } else {
        Released::Nothing
    }
}

/// The session taken out of the engine and dropped, which is ONNX Runtime's `ReleaseSession`;
/// true when there was one. Called with nothing inside ONNX Runtime and nothing to enter it.
#[cfg(target_os = "macos")]
fn take_session() -> bool {
    let Some(Ok(eng)) = ENGINE.get() else { return false };
    let session = lock_even_if_poisoned(&eng.session).take();
    let had = session.is_some();
    drop(session);
    had
}

/// ONNX Runtime's environment released (`ReleaseEnv`), when [`init`] made it: its count goes from
/// one to none, and ONNX Runtime deletes it now — its logging's mutex still there — and leaves
/// `OrtEnv::p_instance_` empty for `exit()`. `ort` keeps its handle to it in a static that is
/// never dropped, so it is not released twice; nothing of `ort` asks for it after, since a
/// session is the only thing this file makes in it and none is made once [`IN_ORT`] is closed.
#[cfg(target_os = "macos")]
fn release_environment() -> bool {
    use ort::AsPointer as _;
    if !ENV_MADE.swap(false, Ordering::SeqCst) {
        return false;
    }
    // Made already, so this hands back `ort`'s own and makes none. Should it ever fail, the
    // environment is still alive, and the backstop is armed for it again.
    let Ok(environment) = ort::environment::get_environment() else {
        ENV_MADE.store(true, Ordering::SeqCst);
        return false;
    };
    // SAFETY: the environment `CreateEnv` made, released once, with no session left in it and no
    // call of this process inside ONNX Runtime.
    unsafe { (ort::api().ReleaseEnv)(environment.ptr().cast_mut()) };
    true
}

/// Registers the [`backstop`] with `atexit`, once, right after the environment is made. The
/// order is the point: `exit()` runs what was registered in reverse, so the backstop runs before
/// everything ONNX Runtime registered up to then — the dylib's own statics, registered when it
/// was opened, and the logging's mutex, registered when the environment was made — and after
/// what it registers later, first used while the session is made or run. A failure is logged.
#[cfg(target_os = "macos")]
fn register_backstop() {
    // SAFETY: a function that takes nothing, returns nothing and never unwinds.
    let rc = unsafe { libc::atexit(backstop) };
    if rc != 0 {
        crate::logging::line(
            "ocr",
            &format!("could not register the exit backstop for ONNX Runtime ({rc}); a quit that bypasses the release may abort"),
        );
    }
}

/// Run by `exit()` when the environment was never released — an `exit()` this application's own
/// code did not begin, which no way of quitting it is known to reach. The tray's Quit, File >
/// Quit and Command-Q (wxWidgets' own Quit) end the main loop, and `run` releases at its end.
/// AppKit's `terminate:` would call `exit(0)` itself, but it asks wxWidgets first, which asks the
/// module window to close; that window refuses (it hides instead), so `terminate:` is cancelled
/// for as long as it exists — and so is the quit Apple Event of the Dock's Quit and of a logout,
/// which then ends nothing at all (TODO.md). It cannot release anything: what ONNX Runtime
/// registered after it — first used while the session was made or run — is destroyed by then,
/// and the release could reach it. So it ends the process with `_exit(0)` — 0 whatever status
/// `exit()` was given, which it cannot know; AppKit's own for `terminate:` — before ONNX
/// Runtime's static destructors run, with one line on standard error — a raw `write`, since a
/// lock here could be held by a thread `exit()` is ending around. Nothing when the release ran.
#[cfg(target_os = "macos")]
extern "C" fn backstop() {
    if ENV_MADE.load(Ordering::SeqCst) {
        // SAFETY: a static buffer of the length given; `_exit` takes no lock and does not return.
        unsafe {
            libc::write(libc::STDERR_FILENO, BACKSTOP_LINE.as_ptr().cast(), BACKSTOP_LINE.len());
            libc::_exit(0);
        }
    }
}

/// What the [`backstop`] writes; CI fails a step whose output holds it.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) const BACKSTOP_LINE: &[u8] =
    b"automation-platform: exit() began with ONNX Runtime's environment unreleased; ended by _exit(0) before its teardown\n";

/// The oldest macOS Microsoft builds ONNX Runtime 1.22's dylib for, as major and minor version:
/// its `minos`, which CI prints for both slices and warns about when it is another
/// (`macos-build.yml`). Below it libc++ lacks symbols the library imports, and `dlopen` with
/// `RTLD_NOW` should refuse it; no runner is that old to show it does (TODO.md), so the
/// application does not ask there.
#[cfg(target_os = "macos")]
const RUNTIME_MACOS: (isize, isize) = (13, 3);

/// Whether this macOS is [`RUNTIME_MACOS`] or later; why not, when it is older.
#[cfg(target_os = "macos")]
fn macos_new_enough() -> Result<(), String> {
    let v = objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersion();
    if (v.majorVersion, v.minorVersion) < RUNTIME_MACOS {
        return Err(format!(
            "macOS {}.{}.{} is older than the {}.{} ONNX Runtime {RUNTIME_WANTED}x is built for",
            v.majorVersion, v.minorVersion, v.patchVersion, RUNTIME_MACOS.0, RUNTIME_MACOS.1
        ));
    }
    Ok(())
}

/// Opens ONNX Runtime at `path` and asks its version: the version, or why it cannot be used.
///
/// `RTLD_NOW`, so that a library missing a symbol of this macOS — Microsoft builds 1.22 for macOS
/// 13.3, and below that libc++ lacks some of what it imports — fails here, with `dlerror`'s
/// reason, rather than on its first call. The same holds for a library without this Mac's slice.
/// `RTLD_LOCAL`, so that its symbols do not join the process's. Never closed: `ort` opens the same
/// file again by its path, and the two handles are one library.
#[cfg(target_os = "macos")]
fn open_runtime(path: &std::path::Path) -> Result<String, String> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt as _;
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| format!("{} holds a NUL byte", path.display()))?;
    let said = || {
        // SAFETY: `dlerror` returns this thread's last message or null, valid until the next call.
        let e = unsafe { libc::dlerror() };
        if e.is_null() {
            "no reason given".to_string()
        } else {
            // SAFETY: a NUL-terminated string `dlerror` owns.
            unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
        }
    };
    if !path.is_file() {
        return Err(format!("ONNX Runtime is not in the bundle: {}", path.display()));
    }
    // SAFETY: a NUL-terminated path; the handle is kept for the life of the process.
    let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if handle.is_null() {
        return Err(format!("ONNX Runtime did not load from {}: {}", path.display(), said()));
    }
    // SAFETY: a live handle and a NUL-terminated name.
    let symbol = unsafe { libc::dlsym(handle, c"OrtGetApiBase".as_ptr()) };
    if symbol.is_null() {
        return Err(format!("{} has no OrtGetApiBase: {}", path.display(), said()));
    }
    type GetApiBase = unsafe extern "C" fn() -> *const ort::sys::OrtApiBase;
    // SAFETY: the address of `OrtGetApiBase`, whose C signature is the type above.
    let get = unsafe { std::mem::transmute::<*mut libc::c_void, GetApiBase>(symbol) };
    // SAFETY: ONNX Runtime's documented entry point, which takes nothing and returns a pointer to
    // a static table, or null.
    let base = unsafe { get() };
    if base.is_null() {
        return Err(format!("{}'s OrtGetApiBase answered nothing", path.display()));
    }
    // SAFETY: a valid table, whose `GetVersionString` returns a static NUL-terminated string.
    let version = unsafe {
        let v = ((*base).GetVersionString)();
        if v.is_null() {
            return Err(format!("{} gives no version", path.display()));
        }
        CStr::from_ptr(v).to_string_lossy().into_owned()
    };
    if !version.starts_with(RUNTIME_WANTED) {
        return Err(format!("{} is ONNX Runtime {version}, and this build is made for {RUNTIME_WANTED}x", path.display()));
    }
    Ok(version)
}

/// What the engine is, for the bench's header: the model, where it comes from, and the runtime.
pub(crate) fn engine_words() -> String {
    #[cfg(windows)]
    let words =
        format!("PaddleOCR's recognition model ({} bytes, inside the executable) under ONNX Runtime, linked in", MODEL.len());
    #[cfg(target_os = "macos")]
    let words = match bundle_files() {
        Ok((dylib, model)) => format!(
            "PaddleOCR's recognition model ({} bytes, {}) under ONNX Runtime {} ({})",
            std::fs::metadata(&model).map_or_else(|_| "?".to_string(), |m| m.len().to_string()),
            model.display(),
            RUNTIME.get().map_or("?", String::as_str),
            dylib.display()
        ),
        Err(why) => format!("PaddleOCR's recognition model, not found: {why}"),
    };
    words
}

/// Model runs begun in this process, on any thread: the recogniser thread's and the bench's.
static RUNS_STARTED: AtomicU64 = AtomicU64::new(0);
/// Model runs under way now.
static RUNS_NOW: AtomicUsize = AtomicUsize::new(0);

/// How many runs of the model have begun in this process. For the bench, which counts the runs a
/// read made as it counts Vision's passes; and for the slow-read line, which says whether one
/// began during a Vision pass.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn runs_started() -> u64 {
    RUNS_STARTED.load(Ordering::SeqCst)
}

/// Whether the model is running now, on any thread.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn running() -> bool {
    RUNS_NOW.load(Ordering::SeqCst) > 0
}

/// One run counted in [`RUNS_NOW`] until it is dropped, unwinding included.
struct InModel;

impl InModel {
    fn enter() -> InModel {
        RUNS_STARTED.fetch_add(1, Ordering::SeqCst);
        RUNS_NOW.fetch_add(1, Ordering::SeqCst);
        InModel
    }
}

impl Drop for InModel {
    fn drop(&mut self) {
        RUNS_NOW.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The model over one prepared input ([`paddle_pre::preprocess`]'s width and data): the decoded
/// text, untrimmed, and its score. `None` when the session could not run it, when `cancelled`
/// says so — looked at before waiting for the session and once it is in hand — and once
/// [`release`] has begun or taken the session.
fn run_model(eng: &Engine, width: u32, input: Vec<f32>, cancelled: &dyn Fn() -> bool) -> Option<(String, f32)> {
    // First, so that it is dropped last: after the outputs, the session's lock and the tensor.
    let _in_ort = IN_ORT.start()?;
    let tensor = Tensor::from_array(([1usize, 3, paddle_pre::INPUT_H as usize, width as usize], input)).ok()?;
    if cancelled() {
        return None;
    }
    let mut held = lock_even_if_poisoned(&eng.session);
    let session = held.as_mut()?;
    if cancelled() {
        return None;
    }
    let _in_model = InModel::enter();
    let outputs = session.run(ort::inputs![tensor]).ok()?;
    let (shape, logits) = outputs[0].try_extract_tensor::<f32>().ok()?;
    // `[1, steps, classes]`; anything else reads as nothing.
    if shape.len() != 3 {
        return Some((String::new(), f32::NAN));
    }
    Some(paddle_pre::decode(logits, shape[1] as usize, shape[2] as usize, &eng.dict))
}

/// A captured region as the RGB the preprocessing takes: its alpha dropped.
fn rgb_of(cap: &CapturedImage) -> Option<image::RgbImage> {
    let rgba = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone())?;
    Some(image::RgbImage::from_fn(cap.w, cap.h, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    }))
}

/// What the recogniser read in one region: its text, trimmed and never empty, the score
/// [`paddle_pre::decode`] gave it, and the milliseconds its recognition took on the recogniser
/// thread — the preprocessing and the model, without the wait in the queue.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PaddleRead {
    pub text: String,
    pub score: f32,
    pub ms: f64,
}

/// How urgent a region is, which on a Mac is the quality of service the recogniser thread asks
/// for before it runs it. Nothing on Windows, where every region is as it always was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Qos {
    /// A read waits for the answer: the recognise thread's own class, user-initiated. What
    /// `ocr-bench` asks with where it measures the recogniser at a read's urgency.
    Interactive,
    /// Nobody waits for it, or only after a Vision pass has read nothing: utility, meant to leave
    /// the cores to the Vision pass beside it (measured by `ocr-bench`, not assumed). Only
    /// `ocr-bench`'s strategies and threads ask with it now.
    #[cfg_attr(windows, allow(dead_code))]
    Utility,
    /// As urgent as the thread that asks, read from it when it asks ([`class_for`]): the event
    /// loop's user-interactive, the recognise thread's user-initiated. What every read of the
    /// application asks with on a Mac, since it may wait for the answer — a wait on a thread of a
    /// lower class than its own would let any work between the two classes delay the reader.
    #[cfg_attr(windows, allow(dead_code))]
    Reader,
}

/// The quality-of-service class, as `<sys/qos.h>` numbers it, the recogniser thread asks for
/// before a region asked with `qos`: user-initiated (0x19) for [`Qos::Interactive`], utility (0x11)
/// for [`Qos::Utility`], and for [`Qos::Reader`] the asking thread's own class (`reader`, asked
/// only then) — user-initiated when that is unspecified or a value the header does not name. The
/// decision apart from the call, so that it is tested where the call cannot be made.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn class_for(qos: Qos, reader: impl FnOnce() -> u32) -> u32 {
    const USER_INITIATED: u32 = 0x19;
    match qos {
        Qos::Interactive => USER_INITIATED,
        Qos::Utility => 0x11,
        Qos::Reader => match reader() {
            // User-interactive, user-initiated, default, utility, background.
            c @ (0x21 | 0x19 | 0x15 | 0x11 | 0x09) => c,
            _ => USER_INITIATED,
        },
    }
}

/// A recognition asked of the recogniser thread ([`ask_with`]). Dropping it cancels the
/// recognition: skipped if the thread has not reached it, stopped at the next point it looks if
/// it has.
pub(super) struct Asked {
    cancel: Arc<AtomicBool>,
    answer: Receiver<Option<PaddleRead>>,
}

impl Asked {
    /// Waits for the answer: the text, or `None` when the recogniser found nothing, is not
    /// available, could not read the region, or its thread has gone. As long as the recognition
    /// takes — tens of milliseconds warm — and no longer: the thread answers every job it takes,
    /// or drops it, which ends the wait as well.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn wait(self) -> Option<String> {
        self.wait_read().map(|r| r.text)
    }

    /// [`Asked::wait`], with the score and the time beside the text.
    pub(super) fn wait_read(self) -> Option<PaddleRead> {
        self.answer.recv().ok().flatten()
    }

    /// The answer if it is there, without waiting ([`Polled`]). Once it has answered it is taken,
    /// and a later call says [`Polled::Failed`].
    #[cfg_attr(windows, allow(dead_code))]
    pub(super) fn try_wait(&self) -> Polled {
        match self.answer.try_recv() {
            Ok(read) => Polled::Answered(read),
            Err(TryRecvError::Empty) => Polled::NotYet,
            Err(TryRecvError::Disconnected) => Polled::Failed,
        }
    }

    /// The answer, waited for at most `bound`: [`Asked::try_wait`] when `bound` is zero, and
    /// [`Polled::NotYet`] when the recogniser has not answered by then — the recognition is not
    /// cancelled, and a later call can still find its answer. What a Mac's read waits with, within
    /// the ladder's budget (`ocr/merge.rs`); Windows waits with [`Asked::wait`], as it always did.
    #[cfg_attr(windows, allow(dead_code))]
    pub(super) fn wait_for(&self, bound: Duration) -> Polled {
        if bound.is_zero() {
            return self.try_wait();
        }
        match self.answer.recv_timeout(bound) {
            Ok(read) => Polled::Answered(read),
            Err(RecvTimeoutError::Timeout) => Polled::NotYet,
            Err(RecvTimeoutError::Disconnected) => Polled::Failed,
        }
    }
}

/// What [`Asked::try_wait`] found.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(windows, allow(dead_code))]
pub(crate) enum Polled {
    /// The recogniser has not answered yet.
    NotYet,
    /// It answered: what it read, `None` for nothing.
    Answered(Option<PaddleRead>),
    /// No answer will come: the recognition panicked, the region's pixels did not make a picture,
    /// there is no engine, or the recogniser's thread has gone — each said in the log on its own.
    /// The thread answers those by dropping the reply, so that the shadow can count them apart from
    /// a region in which it read nothing; [`Asked::wait`] answers `None` for both, as it always did.
    Failed,
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

/// A region for the recogniser thread, counted in [`IN_FLIGHT`] until it is dropped.
struct Job {
    cap: CapturedImage,
    /// How it is cropped before the model sees it: `Tighten::WINDOWS` for Windows' captures,
    /// `Tighten::for_scale` for a Mac's.
    crop: Tighten,
    /// On a Mac, the class the recogniser thread asks for before it runs the region: how urgent
    /// it was asked to be ([`Qos`]), as [`class_for`] resolved it on the thread that asked.
    #[cfg(target_os = "macos")]
    class: u32,
    cancel: Arc<AtomicBool>,
    reply: SyncSender<Option<PaddleRead>>,
    _running: Running<'static>,
}

/// Regions waiting for the recogniser: a caller waits for its own region or cancels it before
/// it hands over the next, so a handful at most — the cap only guards against a recogniser
/// that has stopped answering.
const QUEUE_MAX: usize = 32;

static JOBS: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static JOB_READY: Condvar = Condvar::new();
/// Whether the recogniser thread was started; decided once, on the first region.
static RECOGNISER: OnceLock<bool> = OnceLock::new();
/// Whether the recogniser thread is still there to answer. Set before it starts, cleared — under
/// the [`JOBS`] lock, with the queue emptied — if it ever ends; [`ask_with`] reads it under the
/// same lock, so no region is queued for a thread that has gone. A queued region holds its
/// caller's only way to be answered: left in the queue of a thread that has gone, its caller's
/// `wait` would never return, and a synchronous `host.ocr.recognize` waits on the event loop,
/// which carries every captured key.
static ALIVE: AtomicBool = AtomicBool::new(false);

/// Regions the recogniser thread began, ran through the model to the end, and gave up on because
/// they were cancelled (in the queue, or before the model): `ocr-bench` counts what a read cost
/// with them. Crate-internal; no host API.
static JOBS_BEGUN: AtomicU64 = AtomicU64::new(0);
static JOBS_RAN: AtomicU64 = AtomicU64::new(0);
static JOBS_CANCELLED: AtomicU64 = AtomicU64::new(0);

/// The three counts above: begun, ran to the end, cancelled.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn job_counts() -> (u64, u64, u64) {
    (JOBS_BEGUN.load(Ordering::SeqCst), JOBS_RAN.load(Ordering::SeqCst), JOBS_CANCELLED.load(Ordering::SeqCst))
}

/// Hands a small region to the recogniser thread, which starts on it as soon as it is free —
/// before the caller runs the system recogniser on the same region. Windows' captures, at 1x,
/// with Windows' crop and at once ([`ask_with`] says the rest).
#[cfg(windows)]
pub(super) fn ask(cap: &CapturedImage) -> Option<Asked> {
    ask_with(cap.w, cap.h, &cap.rgba, Tighten::WINDOWS, Qos::Interactive)
}

/// Hands `w` by `h` pixels of RGBA, top-down, to the recogniser thread, cropped by `crop` and as
/// urgent as `qos` says ([`Qos::Reader`] is read from the calling thread here). `None`, and
/// nothing started, once the exit's wait has begun (see [`InFlight::start`]), when the thread
/// could not be started or has gone ([`ALIVE`]), or when [`QUEUE_MAX`] regions already wait (a
/// recogniser that stopped answering; said once). Copies the pixels.
pub(super) fn ask_with(w: u32, h: u32, rgba: &[u8], crop: Tighten, qos: Qos) -> Option<Asked> {
    // Windows has one priority for every region, as it always had.
    #[cfg(not(target_os = "macos"))]
    let _ = qos;
    let running = IN_FLIGHT.start()?;
    if !*RECOGNISER.get_or_init(start_recogniser) {
        return None;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let (reply, answer) = sync_channel(1);
    let cap = CapturedImage { w, h, rgba: rgba.to_vec() };
    let job = Job {
        cap,
        crop,
        #[cfg(target_os = "macos")]
        class: class_for(qos, super::macos::ocr::thread_qos),
        cancel: cancel.clone(),
        reply,
        _running: running,
    };
    let admitted = admit(&mut lock_even_if_poisoned(&JOBS), &ALIVE, job);
    match admitted {
        Ok(()) => {}
        // The thread has gone (said when it went): the system recogniser reads alone.
        Err(Refused::Gone) => return None,
        Err(Refused::Full) => {
            static SAID: AtomicBool = AtomicBool::new(false);
            if !SAID.swap(true, Ordering::Relaxed) {
                crate::logging::line(
                    "ocr",
                    &format!(
                        "{QUEUE_MAX} small regions wait for the neural recogniser, which has stopped \
                         keeping up or answering; regions are read by the system recogniser alone until \
                         it catches up (said once)"
                    ),
                );
            }
            return None;
        }
    }
    JOB_READY.notify_one();
    Some(Asked { cancel, answer })
}

/// Why [`admit`] did not queue a region.
#[derive(Debug, PartialEq, Eq)]
enum Refused {
    /// The recogniser thread has gone.
    Gone,
    /// [`QUEUE_MAX`] regions already wait.
    Full,
}

/// Queues `job` — asked with the queue's lock held — unless the recogniser thread has gone
/// (`alive` false) or the queue is full; the cancelled ones leave first. A refused job is
/// dropped, and its caller is not handed a wait.
fn admit(jobs: &mut VecDeque<Job>, alive: &AtomicBool, job: Job) -> Result<(), Refused> {
    if !alive.load(Ordering::SeqCst) {
        return Err(Refused::Gone);
    }
    prune_cancelled(jobs);
    if jobs.len() >= QUEUE_MAX {
        return Err(Refused::Full);
    }
    jobs.push_back(job);
    Ok(())
}

/// The recogniser thread has gone: `alive` cleared and the queue emptied, both under the queue's
/// lock, so [`admit`] queues nothing after. Returns the jobs that waited, for the caller to drop
/// outside the lock — each one dropped answers its caller's `wait` with `None`.
fn orphan_all(jobs: &Mutex<VecDeque<Job>>, alive: &AtomicBool) -> Vec<Job> {
    let mut jobs = lock_even_if_poisoned(jobs);
    alive.store(false, Ordering::SeqCst);
    jobs.drain(..).collect()
}

/// What was cancelled while it waited goes now, uncounted with it.
fn prune_cancelled(jobs: &mut VecDeque<Job>) {
    let before = jobs.len();
    jobs.retain(|j| !j.cancel.load(Ordering::Acquire));
    JOBS_CANCELLED.fetch_add((before - jobs.len()) as u64, Ordering::Relaxed);
}

fn start_recogniser() -> bool {
    ALIVE.store(true, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("paddle-ocr".to_string()).spawn(serve);
    if let Err(e) = &spawned {
        ALIVE.store(false, Ordering::SeqCst);
        crate::logging::line(
            "ocr",
            &format!("could not start the neural recogniser's thread ({e}); small regions are read by the system recogniser alone"),
        );
    }
    spawned.is_ok()
}

/// Marks the recogniser gone when its thread ends, however it ends, and answers everything still
/// queued with nothing: each job dropped drops its reply channel, and its caller's `wait` returns
/// `None`. See [`ALIVE`].
struct Gone;

impl Drop for Gone {
    fn drop(&mut self) {
        let orphans = orphan_all(&JOBS, &ALIVE);
        let n = orphans.len();
        drop(orphans);
        crate::logging::line(
            "ocr",
            &format!(
                "the neural recogniser's thread ended; {n} waiting region(s) were answered with \
                 nothing, and small regions are read by the system recogniser alone from now on"
            ),
        );
    }
}

/// The recogniser thread: one region at a time, in the order asked, for as long as the process
/// runs. A panic in one recognition answers that region `None` and the thread carries on; a
/// panic anywhere else in its loop (the wait, the log) drops only the job in hand, whose caller
/// is answered `None`, and the loop starts again. Should the thread end all the same, [`Gone`]
/// answers what is left.
fn serve() {
    let _gone = Gone;
    loop {
        if std::panic::catch_unwind(serve_jobs).is_ok() {
            return; // `serve_jobs` never returns; kept for the type
        }
        static OUTER: AtomicUsize = AtomicUsize::new(0);
        let n = OUTER.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_power_of_two() {
            crate::logging::line(
                "ocr",
                &format!("the neural recogniser's loop panicked outside a recognition ({n} so far); it carries on"),
            );
        }
    }
}

/// The loop of [`serve`], one job at a time.
fn serve_jobs() {
    // The quality of service this thread asked for last, so that it asks again only on a change.
    #[cfg(target_os = "macos")]
    let mut asked_for: Option<u32> = None;
    loop {
        let job = {
            let mut jobs = lock_even_if_poisoned(&JOBS);
            loop {
                if let Some(job) = jobs.pop_front() {
                    break job;
                }
                jobs = JOB_READY.wait(jobs).unwrap_or_else(|p| p.into_inner());
            }
        };
        if job.cancel.load(Ordering::Acquire) {
            JOBS_CANCELLED.fetch_add(1, Ordering::Relaxed);
            continue; // dropped here, uncounted
        }
        JOBS_BEGUN.fetch_add(1, Ordering::Relaxed);
        #[cfg(target_os = "macos")]
        if asked_for != Some(job.class) {
            take_qos(job.class);
            asked_for = Some(job.class);
        }
        let done = match crate::logging::contain(|| recognize_unless(&job.cap, job.crop, &job.cancel)) {
            Ok(done) => done,
            Err(report) => {
                // The 1st, 2nd, 4th … — one region that trips something would trip it again.
                static PANICS: AtomicUsize = AtomicUsize::new(0);
                let n = PANICS.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_power_of_two() {
                    crate::logging::line(
                        "ocr",
                        &format!(
                            "the neural recogniser panicked on a region ({n} so far this session); \
                             it was answered with nothing and the recogniser carries on: {report}"
                        ),
                    );
                }
                Done::NoEngine
            }
        };
        let answer = match done {
            Done::Ran(read) => {
                JOBS_RAN.fetch_add(1, Ordering::Relaxed);
                read
            }
            Done::Cancelled => {
                JOBS_CANCELLED.fetch_add(1, Ordering::Relaxed);
                None
            }
            // No answer at all: the job is dropped with its reply, and the caller's `wait` returns
            // `None` as it would for nothing, while `try_wait` can tell it apart (`Polled::Failed`).
            Done::NoEngine => continue,
        };
        let _ = job.reply.send(answer);
    }
}

/// The quality of service the recogniser thread asks for before a region asked at `class`
/// ([`class_for`]), by the recognise thread's own helper (`macos::ocr::set_thread_qos`). A refusal
/// is said once.
#[cfg(target_os = "macos")]
fn take_qos(class: u32) {
    use libc::qos_class_t as Q;
    let class = match class {
        0x21 => Q::QOS_CLASS_USER_INTERACTIVE,
        0x15 => Q::QOS_CLASS_DEFAULT,
        0x11 => Q::QOS_CLASS_UTILITY,
        0x09 => Q::QOS_CLASS_BACKGROUND,
        _ => Q::QOS_CLASS_USER_INITIATED,
    };
    let rc = super::macos::ocr::set_thread_qos(class);
    static SAID: AtomicBool = AtomicBool::new(false);
    if rc != 0 && !SAID.swap(true, Ordering::Relaxed) {
        crate::logging::line("ocr", &format!("the neural recogniser's thread kept its quality of service ({rc}; said once)"));
    }
}

/// How one region's recognition ended.
#[derive(Debug, PartialEq)]
enum Done {
    /// The model ran, or could not run the input: what it read, `None` for nothing.
    Ran(Option<PaddleRead>),
    /// `cancel` was set before the model ran.
    Cancelled,
    /// There is no engine, the region's pixels did not make a picture, or the recognition
    /// panicked.
    NoEngine,
}

/// Recognizes text in a captured region via the neural recognizer, cropped by `crop` — or says
/// why not: the engine is unavailable, or `cancel` was set — looked at before the preprocessing,
/// before waiting for the session, and once the session is in hand.
fn recognize_unless(cap: &CapturedImage, crop: Tighten, cancel: &AtomicBool) -> Done {
    let started = Instant::now();
    let cancelled = || cancel.load(Ordering::Acquire);
    if cancelled() {
        return Done::Cancelled;
    }
    let Some(eng) = engine() else { return Done::NoEngine };
    let Some(rgb) = rgb_of(cap) else { return Done::NoEngine };
    // Windows' captures are cropped by `Tighten::WINDOWS`: 3 pixels of margin, at least twice the
    // size, as since the recogniser came in.
    let (width, input) = paddle_pre::preprocess(&paddle_pre::tighten(&rgb, crop));
    let Some((text, score)) = run_model(eng, width, input, &cancelled) else {
        return if cancelled() { Done::Cancelled } else { Done::Ran(None) };
    };
    let text = text.trim().to_string();
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    Done::Ran((!text.is_empty()).then_some(PaddleRead { text, score, ms }))
}

/// One recognition, its two halves timed: what `ocr-bench --paddle` measures. Crate-internal,
/// for the bench; no host API.
pub(crate) struct PaddleSample {
    /// Trimmed; empty when the model read nothing.
    pub text: String,
    /// [`paddle_pre::decode`]'s score; NaN when nothing was read.
    pub score: f32,
    /// `tighten` and `preprocess`.
    pub pre_ms: f64,
    /// The tensor, the session's run and the decoding.
    pub run_ms: f64,
}

/// Makes the engine if it is not made yet, and says how long that took: milliseconds, or why
/// there is none. Made already (by an earlier call or a recognition), it answers in no time.
pub(crate) fn init_timed() -> Result<f64, String> {
    let t = Instant::now();
    match made() {
        Ok(_) => Ok(t.elapsed().as_secs_f64() * 1000.0),
        Err(e) => Err(e.clone()),
    }
}

/// `rgb` recognised on the calling thread with crop setting `t`, timed. `None` when there is no
/// engine or the session could not run. Takes the session's lock like the recogniser thread,
/// so a recognition of that thread's in progress is waited for (and timed).
pub(crate) fn recognize_timed(rgb: &image::RgbImage, t: Tighten) -> Option<PaddleSample> {
    let eng = engine()?;
    let started = Instant::now();
    let (width, input) = paddle_pre::preprocess(&paddle_pre::tighten(rgb, t));
    let pre_ms = started.elapsed().as_secs_f64() * 1000.0;
    let ran = Instant::now();
    let (text, score) = run_model(eng, width, input, &|| false)?;
    let run_ms = ran.elapsed().as_secs_f64() * 1000.0;
    let text = text.trim().to_string();
    let score = if text.is_empty() { f32::NAN } else { score };
    Some(PaddleSample { text, score, pre_ms, run_ms })
}

/// The warm-up's own work, on the calling thread: the engine made, and one inference over a
/// dark 40x16 dummy, so that the first real region does not pay for the graph's first run.
pub(crate) fn warm_now() {
    let Some(eng) = engine() else { return };
    let dummy = image::RgbImage::from_pixel(40, 16, image::Rgb([20, 20, 20]));
    let (width, input) = paddle_pre::preprocess(&paddle_pre::tighten(&dummy, Tighten::WINDOWS));
    let _ = run_model(eng, width, input, &|| false);
}

/// Loads the model and warms the inference graph on a background thread, so the
/// first real OCR doesn't pay the (one-time, hundreds-of-ms) init cost on the
/// hot path. No-op if the model files aren't present. Returns the thread handle:
/// the caller MUST join it before the process exits, or a fast exit can tear the
/// process down while this thread is still inside ONNX Runtime init, racing ort's
/// static cleanup (an access violation — the "headless segfault").
#[cfg(windows)]
pub fn warmup() -> std::thread::JoinHandle<()> {
    std::thread::spawn(warm_now)
}

/// The Mac's warm-up, on a thread named `paddle-warm-up` at the utility quality of service, below
/// the reads it serves later: it waits until `vision` is opened — Vision's
/// own warm-up has ended — and then makes the engine and runs it once, as on Windows. The
/// recognise thread's warm-up pass is let go at that same moment (`macos/ocr.rs`, `warm_up`), and
/// the lower class is what keeps the two from competing as equals on an Intel Mac's few cores;
/// whether this should wait for that pass too is measured first (`ocr-bench`'s
/// `paddle-first:with-vision`, TODO.md). When the wait is stopped (the exit) it ends without
/// touching ONNX Runtime. The same join at exit as on Windows; `None` when the thread could not be
/// started, and the recogniser then stays unloaded for the session — no read makes it on a Mac
/// ([`ready`]).
#[cfg(target_os = "macos")]
pub fn warmup_after(vision: &'static crate::ocr::cost::Latch) -> Option<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("paddle-warm-up".to_string())
        .spawn(move || {
            // Asked before the wait, so that the session's making and its first run never run above
            // it; a refusal leaves the thread as it was, which the recogniser's thread says once.
            let _ = super::macos::ocr::set_thread_qos(libc::qos_class_t::QOS_CLASS_UTILITY);
            if vision.wait_or_stopped() {
                warm_now();
            }
        })
        .ok()
}

#[cfg(test)]
mod exit_safety_tests {
    use super::*;
    use crate::{quiet_expected_panics, EXPECTED_PANIC};

    /// Counted before the thread exists, uncounted when it is done, and the wait returns as
    /// soon as it is — not at its bound.
    #[test]
    fn the_exit_waits_for_a_recognition_and_no_longer() {
        let c = InFlight::new();
        assert!(c.wait_idle(Duration::ZERO), "nothing running: no wait at all");

        let running = c.start().expect("open");
        assert_eq!(c.count(), 1, "counted before the thread is even spawned");
        let waited = std::thread::scope(|s| {
            s.spawn(move || {
                let _running = running;
                std::thread::sleep(Duration::from_millis(30));
            });
            let t = Instant::now();
            assert!(c.wait_idle(Duration::from_secs(10)));
            t.elapsed()
        });
        assert_eq!(c.count(), 0);
        assert!(waited < Duration::from_secs(5), "returned at the end of the run: {waited:?}");
    }

    /// Once the exit's wait has begun nothing new is counted or started, and what was already
    /// running is still waited for.
    #[test]
    fn closed_refuses_new_recognitions_and_still_counts_the_running_ones() {
        let c = InFlight::new();
        let running = c.start().expect("open");
        c.close();
        assert!(c.start().is_none(), "no recognition starts after the exit looked");
        assert_eq!(c.count(), 1, "the refused one left no count behind");
        assert!(!c.wait_idle(Duration::ZERO));
        drop(running);
        assert!(c.wait_idle(Duration::ZERO));
        assert!(c.start().is_none(), "closed stays closed");
    }

    /// A recognition that never finishes costs the exit its bound and no more.
    #[test]
    fn the_wait_gives_up_at_its_bound() {
        let c = InFlight::new();
        let _stuck = c.start().expect("open");
        let t = Instant::now();
        assert!(!c.wait_idle(Duration::from_millis(40)));
        assert!(t.elapsed() >= Duration::from_millis(40));
        assert_eq!(c.count(), 1);
    }

    /// A recognition that panics is uncounted all the same, or every exit after it would sit
    /// out the whole bound.
    #[test]
    fn a_recognition_that_panics_is_uncounted() {
        quiet_expected_panics();
        let c = InFlight::new();
        let joined = std::thread::scope(|s| {
            s.spawn(|| {
                let _running = c.start().expect("open");
                panic!("{EXPECTED_PANIC}inside the recogniser");
            })
            .join()
        });
        assert!(joined.is_err());
        assert_eq!(c.count(), 0);
    }

    /// A region whose answer is no longer wanted — WinRT read it, or it was blank — is cancelled
    /// by dropping its handle, and leaves the queue uncounted; a cancelled one never reaches the
    /// engine at all.
    #[test]
    fn a_cancelled_region_leaves_the_queue_uncounted_and_is_never_recognised() {
        static C: InFlight = InFlight::new();
        let mut jobs = VecDeque::new();
        let mut asked = Vec::new();
        for _ in 0..3 {
            let cancel = Arc::new(AtomicBool::new(false));
            let (reply, answer) = sync_channel(1);
            let cap = CapturedImage { w: 1, h: 1, rgba: vec![0; 4] };
            jobs.push_back(Job {
                cap,
                crop: Tighten::WINDOWS,
                #[cfg(target_os = "macos")]
                class: 0x19,
                cancel: cancel.clone(),
                reply,
                _running: C.start().expect("open"),
            });
            asked.push(Asked { cancel, answer });
        }
        assert_eq!(C.count(), 3);
        drop(asked.remove(0)); // the first: WinRT answered
        prune_cancelled(&mut jobs);
        assert_eq!((jobs.len(), C.count()), (2, 2));
        // A job answered: its caller hears it, and it is uncounted once dropped.
        let job = jobs.pop_front().unwrap();
        job.reply.send(Some(PaddleRead { text: "7".into(), score: 0.9, ms: 5.0 })).unwrap();
        drop(job);
        assert_eq!(asked.remove(0).wait(), Some("7".to_string()));
        assert_eq!(C.count(), 1);
        // A recogniser gone without answering: the wait ends with nothing rather than hanging.
        drop(jobs.pop_front());
        assert_eq!(asked.remove(0).wait(), None);
        assert_eq!(C.count(), 0);
        // Cancelled before it starts: no engine is even opened for it.
        let cap = CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
        assert_eq!(recognize_unless(&cap, Tighten::WINDOWS, &AtomicBool::new(true)), Done::Cancelled);
    }

    /// The recogniser thread gone: everything it left in the queue is answered with nothing, and
    /// nothing is queued for it after — a synchronous recognition waiting on the event loop
    /// would otherwise wait for ever, and every captured key with it.
    #[test]
    fn a_recogniser_that_has_gone_answers_its_queue_and_takes_no_more() {
        static C: InFlight = InFlight::new();
        let jobs: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
        let alive = AtomicBool::new(true);
        let job = || {
            let cancel = Arc::new(AtomicBool::new(false));
            let (reply, answer) = sync_channel(1);
            let cap = CapturedImage { w: 1, h: 1, rgba: vec![0; 4] };
            let job = Job {
                cap,
                crop: Tighten::WINDOWS,
                #[cfg(target_os = "macos")]
                class: 0x19,
                cancel: cancel.clone(),
                reply,
                _running: C.start().expect("open"),
            };
            (job, Asked { cancel, answer })
        };
        let mut waiting = Vec::new();
        for _ in 0..3 {
            let (j, a) = job();
            assert_eq!(admit(&mut jobs.lock().unwrap(), &alive, j), Ok(()));
            waiting.push(a);
        }
        // Waiters on other threads, as the event loop and the OCR thread would be.
        let waits: Vec<_> = waiting.into_iter().map(|a| std::thread::spawn(move || a.wait())).collect();
        let orphans = orphan_all(&jobs, &alive);
        assert_eq!(orphans.len(), 3);
        drop(orphans);
        for w in waits {
            assert_eq!(w.join().unwrap(), None, "answered with nothing, not left waiting");
        }
        assert_eq!(C.count(), 0);
        let (j, a) = job();
        assert_eq!(admit(&mut jobs.lock().unwrap(), &alive, j), Err(Refused::Gone));
        assert_eq!(a.wait(), None, "the refused job was dropped, so even a wait returns");
        assert!(jobs.lock().unwrap().is_empty());
        // A full queue refuses too.
        let alive = AtomicBool::new(true);
        let mut q = VecDeque::new();
        let mut held = Vec::new();
        for _ in 0..QUEUE_MAX {
            let (j, a) = job();
            assert_eq!(admit(&mut q, &alive, j), Ok(()));
            held.push(a);
        }
        let (j, _a) = job();
        assert_eq!(admit(&mut q, &alive, j), Err(Refused::Full));
        // Cancelling one makes room.
        drop(held.remove(0));
        let (j, _b) = job();
        assert_eq!(admit(&mut q, &alive, j), Ok(()));
    }

    /// The thread's loop is kept going by an outer catch, and its end is what answers the queue:
    /// checked where it is written, since the real thread holds the real model.
    #[test]
    fn the_recogniser_thread_answers_its_queue_however_it_ends() {
        const SRC: &str = include_str!("paddle_ocr.rs");
        let serve = &SRC[SRC.find("fn serve() {").unwrap()..SRC.find("fn serve_jobs() {").unwrap()];
        assert!(serve.contains("let _gone = Gone;"), "the guard that answers the queue");
        assert!(serve.contains("std::panic::catch_unwind(serve_jobs)"), "the loop starts again after a panic");
        let ask = &SRC[SRC.find("pub(super) fn ask_with(").unwrap()..SRC.find("enum Refused").unwrap()];
        assert!(ask.contains("admit(&mut lock_even_if_poisoned(&JOBS), &ALIVE, job)"));
        let gone = &SRC[SRC.find("impl Drop for Gone").unwrap()..SRC.find("fn serve() {").unwrap()];
        assert!(gone.contains("orphan_all(&JOBS, &ALIVE)"));
    }

    /// An answer is there for `try_wait` once sent, taken by it; nothing read is an answer, and a
    /// reply dropped unsent — a recognition that failed, a recogniser gone — is told apart from
    /// it and from "not yet"; `wait` keeps Windows' answer, the text alone, and `None` for both.
    #[test]
    fn an_answer_can_be_looked_for_without_waiting() {
        let read = PaddleRead { text: "1".into(), score: 0.95, ms: 6.0 };
        let (reply, answer) = sync_channel(1);
        let asked = Asked { cancel: Arc::new(AtomicBool::new(false)), answer };
        assert_eq!(asked.try_wait(), Polled::NotYet, "not answered yet");
        reply.send(Some(read.clone())).unwrap();
        assert_eq!(asked.try_wait(), Polled::Answered(Some(read.clone())));
        drop(reply);
        assert_eq!(asked.try_wait(), Polled::Failed, "taken, and the recogniser has let go");
        let (reply, answer) = sync_channel(1);
        reply.send(None).unwrap();
        assert_eq!(Asked { cancel: Arc::new(AtomicBool::new(false)), answer }.try_wait(), Polled::Answered(None));
        let (reply, answer) = sync_channel::<Option<PaddleRead>>(1);
        drop(reply);
        let failed = Asked { cancel: Arc::new(AtomicBool::new(false)), answer };
        assert_eq!(failed.try_wait(), Polled::Failed, "dropped unsent");
        assert_eq!(failed.wait(), None, "and Windows' wait answers nothing, as before");
        let (reply, answer) = sync_channel(1);
        reply.send(Some(read)).unwrap();
        assert_eq!(Asked { cancel: Arc::new(AtomicBool::new(false)), answer }.wait(), Some("1".to_string()));
    }

    /// A bounded wait: nothing within the bound is "not yet", and neither cancels the recognition
    /// nor loses its answer, which a later call finds; an answer sent in time is taken; a reply
    /// dropped unsent ends the wait at once, as a failure, and so does one already taken.
    #[test]
    fn a_bounded_wait_leaves_a_late_answer_for_a_later_one() {
        let read = PaddleRead { text: "0".into(), score: 0.55, ms: 5.0 };
        let (reply, answer) = sync_channel(1);
        let asked = Asked { cancel: Arc::new(AtomicBool::new(false)), answer };
        assert_eq!(asked.wait_for(Duration::ZERO), Polled::NotYet, "no bound: a look");
        let t = Instant::now();
        assert_eq!(asked.wait_for(Duration::from_millis(30)), Polled::NotYet);
        assert!(t.elapsed() >= Duration::from_millis(30), "waited its bound");
        assert!(!asked.cancel.load(Ordering::Acquire), "not cancelled by being late");
        let late = read.clone();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            reply.send(Some(late)).unwrap();
        });
        assert_eq!(asked.wait_for(Duration::from_secs(10)), Polled::Answered(Some(read)));
        sender.join().unwrap();
        let t = Instant::now();
        assert_eq!(asked.wait_for(Duration::from_secs(10)), Polled::Failed, "taken, and the reply is gone");
        assert!(t.elapsed() < Duration::from_secs(5), "at once, not at the bound");
        let (reply, answer) = sync_channel::<Option<PaddleRead>>(1);
        drop(reply);
        let t = Instant::now();
        assert_eq!(Asked { cancel: Arc::new(AtomicBool::new(false)), answer }.wait_for(Duration::from_secs(10)), Polled::Failed);
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    /// The class the recogniser thread asks for: the two fixed ones, and a read's own — the event
    /// loop's user-interactive, the recognise thread's user-initiated — with an unspecified class
    /// or a value the header does not name taken as user-initiated; the reader asked only for a
    /// read's region.
    #[test]
    fn a_reads_region_runs_at_the_readers_class() {
        let never = || -> u32 { panic!("the reader's class was asked for a fixed one") };
        assert_eq!(class_for(Qos::Interactive, never), 0x19);
        assert_eq!(class_for(Qos::Utility, never), 0x11);
        assert_eq!(class_for(Qos::Reader, || 0x21), 0x21, "the event loop");
        assert_eq!(class_for(Qos::Reader, || 0x19), 0x19, "the recognise thread");
        for c in [0x15, 0x11, 0x09] {
            assert_eq!(class_for(Qos::Reader, || c), c);
        }
        assert_eq!(class_for(Qos::Reader, || 0), 0x19, "unspecified");
        assert_eq!(class_for(Qos::Reader, || 0x1a), 0x19, "a value the header does not name");
    }

    /// On a Mac `ort::init_from` comes first in `init`, before anything else of `ort`: any other
    /// call of it would make it read `ORT_DYLIB_PATH` and load whatever that names. And the
    /// library is opened by this file, and its version asked, before `ort` loads it. Checked
    /// where it is written, since the Mac's `init` compiles only there.
    #[test]
    fn a_mac_fixes_the_librarys_path_before_any_other_call_of_ort() {
        const SRC: &str = include_str!("paddle_ocr.rs");
        let start = SRC.find("#[cfg(target_os = \"macos\")]
fn init()").expect("the Mac's init");
        let body = &SRC[start..start + SRC[start..].find("
}
").unwrap()];
        let first = |what: &str| body.find(what).unwrap_or_else(|| panic!("{what} is in the Mac's init"));
        let pinned = first("ort::init_from(");
        for later in ["open_runtime(", "environment.commit()", "Session::builder()", "commit_from_file("] {
            assert!(pinned < first(later), "init_from comes before {later}");
        }
        assert!(first("open_runtime(") < first("environment.commit()"), "our own dlopen before ort's");
        assert!(first("macos_new_enough(") < first("open_runtime("), "no dlopen below the macOS it is built for");
        // Nothing of `ort` before it but its path, worked out without `ort`.
        assert!(!body[..pinned].contains("ort::") && !body[..pinned].contains("Session"), "{}", &body[..pinned]);
    }

    /// The release's order: nothing enters ONNX Runtime once it has begun, it waits for what is
    /// inside, the session goes before the environment, and a second call finds nothing.
    #[test]
    fn the_release_waits_for_ort_then_takes_the_session_then_the_environment() {
        let in_ort = InFlight::new();
        let order = std::cell::RefCell::new(Vec::new());
        let inside = in_ort.start().expect("open");
        let released = std::thread::scope(|s| {
            s.spawn(move || {
                let _inside = inside;
                std::thread::sleep(Duration::from_millis(30));
            });
            release_with(
                &in_ort,
                Duration::from_secs(10),
                || {
                    order.borrow_mut().push("session");
                    true
                },
                || {
                    order.borrow_mut().push("environment");
                    true
                },
            )
        });
        assert_eq!(released, Released::Now { session: true, environment: true });
        assert_eq!(*order.borrow(), ["session", "environment"], "the session first: it was made in the environment");
        assert!(in_ort.start().is_none(), "nothing enters ONNX Runtime after the release");
        // A second call (the exit's, then main's guard): nothing to wait for, nothing found.
        assert_eq!(release_with(&in_ort, Duration::ZERO, || false, || false), Released::Nothing);
        // An environment made without a session (the session's making failed) is released too.
        assert_eq!(
            release_with(&InFlight::new(), Duration::ZERO, || false, || true),
            Released::Now { session: false, environment: true }
        );
    }

    /// Something still inside ONNX Runtime at the bound: nothing is released under it, and the
    /// caller is told how many, so that it ends the process without `exit()`'s teardown.
    #[test]
    fn a_release_that_cannot_wait_releases_nothing() {
        let in_ort = InFlight::new();
        let _stuck = in_ort.start().expect("open");
        let touched = std::cell::Cell::new(false);
        let t = Instant::now();
        let released = release_with(
            &in_ort,
            Duration::from_millis(40),
            || {
                touched.set(true);
                true
            },
            || {
                touched.set(true);
                true
            },
        );
        assert_eq!(released, Released::Busy(1));
        assert!(!touched.get(), "neither the session nor the environment is released under a call");
        assert!(t.elapsed() >= Duration::from_millis(40));
        assert!(in_ort.start().is_none(), "closed all the same: nothing new enters");
    }

    /// Every call of this file into ONNX Runtime is counted from before its first touch to after
    /// its last — the engine's making, and a run from its tensor to the drop of its outputs — and
    /// the Mac's environment is said made and the backstop registered right after `commit`, before
    /// the session that can fail. Checked where it is written: the real calls need the model, and
    /// the Mac's only compile there.
    #[test]
    fn every_call_into_ort_is_counted_and_the_mac_backstop_follows_the_environment() {
        const SRC: &str = include_str!("paddle_ocr.rs");
        let body = |head: &str| {
            let start = SRC.find(head).unwrap_or_else(|| panic!("{head}"));
            &SRC[start..start + SRC[start..].find("\n}\n").unwrap()]
        };
        let made = body("fn made() -> &'static Result<Engine, String> {");
        let guard = made.find("let in_ort = IN_ORT.start();").expect("made() counts itself");
        // Taken outside `get_or_init`, so that it is held until the engine is in `ENGINE`, where
        // the release looks for its session; inside, it would end before that.
        assert!(guard < made.find("ENGINE.get_or_init(").unwrap(), "counted until the engine is published");
        assert!(made.find("if in_ort.is_none()").unwrap() < made.find("crate::logging::contain(init)").unwrap());
        assert!(!made.contains("drop(in_ort)") && !made.contains("let _ = in_ort"), "held to the end of made()");
        let run = body("fn run_model(");
        let first = run.find("let _in_ort = IN_ORT.start()?;").expect("run_model counts itself");
        for later in ["Tensor::from_array(", "lock_even_if_poisoned(&eng.session)", "session.run("] {
            assert!(first < run.find(later).unwrap(), "counted before {later}");
        }
        assert!(run.contains("held.as_mut()?"), "a session taken by the release answers nothing");
        let init = body("#[cfg(target_os = \"macos\")]\nfn init()");
        let at = |what: &str| init.find(what).unwrap_or_else(|| panic!("{what} in the Mac's init"));
        assert!(at("environment.commit()?;") < at("ENV_MADE.store(true"));
        assert!(at("ENV_MADE.store(true") < at("register_backstop();"));
        assert!(at("register_backstop();") < at("Session::builder()"), "before the session, which can fail");
        let release = body("pub(crate) fn release() -> Released {");
        assert!(release.find("IN_FLIGHT.close()").unwrap() < release.find("release_with(").unwrap());
        assert!(release.contains("release_with(&IN_ORT, RELEASE_BOUND, take_session, release_environment)"));
        let backstop = body("extern \"C\" fn backstop() {");
        assert!(backstop.contains("libc::_exit(0)"));
        // CI fails a step whose output holds the backstop's line, by these words.
        const WORKFLOW: &str = include_str!("../../../../.github/workflows/macos-build.yml");
        let words = "ONNX Runtime's environment unreleased";
        assert!(std::str::from_utf8(BACKSTOP_LINE).unwrap().contains(words));
        assert!(WORKFLOW.matches(words).count() >= 4, "the run, the quarantined probe and newer-macos look for it");
        assert!(!backstop.contains("logging::") && !backstop.contains(".lock(") && !backstop.contains("println"), "{backstop}");
    }

    /// One panic under the session lock no longer switches the recogniser off for good.
    #[test]
    fn a_panic_under_the_lock_does_not_disable_it() {
        quiet_expected_panics();
        let m = Mutex::new(41);
        let joined = std::thread::scope(|s| {
            s.spawn(|| {
                let _session = m.lock().unwrap();
                panic!("{EXPECTED_PANIC}under the session lock");
            })
            .join()
        });
        assert!(joined.is_err());
        assert!(m.is_poisoned());
        assert!(m.lock().is_err(), "`lock().ok()?` would answer None from here on");
        *lock_even_if_poisoned(&m) += 1;
        assert_eq!(*lock_even_if_poisoned(&m), 42);
    }
}
