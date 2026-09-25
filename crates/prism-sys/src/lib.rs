//! Bindings to [prism](https://github.com/ethindp/prism), the screen-reader abstraction this
//! application speaks through on Windows.
//!
//! Empty on every other platform: macOS reaches VoiceOver through `speech/voiceover.rs`,
//! which is better at it than prism's own VoiceOver backend, and nothing else is supported.
//!
//! The library is compiled from `vendor/` by `build.rs` and linked statically, so there is
//! no DLL to ship. See `docs/prism-speech-design.md` for what that took.
//!
//! This layer is deliberately thin — a pointer check and a `&str` conversion — with one
//! exception that is not thin at all: empty text never reaches prism. See [`Backend::speak`].
#![cfg(windows)]

use std::ffi::{CStr, CString};
use std::marker::PhantomData;

#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    clippy::all
)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/prism.rs"));
}

/// The backends this build contains, by the name each one reports.
///
/// Checked against the running library by the smoke tests, because the compiler cannot: a
/// backend that silently fails to be compiled in is a screen reader that stops being
/// supported without anything saying so.
pub const BACKENDS: &[&str] = &[
    "NVDA",
    "JAWS",
    "ZoomText",
    "SAPI",
    "OneCore",
    "ZDSR",
    "PCTalker",
    "BoyPCReader",
    "SenseReader",
];

/// The backends that mean a screen reader is actually running, in the order to try them.
///
/// Deliberately not `prism_registry_create_best`: that walks priority order and lands on
/// OneCore whenever no screen reader is up, whose `initialize` was measured at 2.9 seconds.
/// And not `acquire_best` either, which caches the instance and would hand the same dead
/// backend back forever.
pub const SCREEN_READERS: &[&str] = &["NVDA", "JAWS", "ZoomText", "ZDSR", "PCTalker", "BoyPCReader", "SenseReader"];

/// The engines that speak when no screen reader is running.
///
/// A module that calls `host.speech` asked to be heard; the application itself does not use
/// these. Initialising one is slow enough to matter, so it happens off the event loop and
/// only when something is actually going to be said.
pub const SYNTHESISERS: &[&str] = &["OneCore", "SAPI"];

/// Feature bits, declared by hand.
///
/// `PrismBackendFeature` is an enum whose members are 64-bit and whose underlying type is
/// not: `sizeof` is 4, and its last member — `1 << 63` — silently evaluates to 0.
/// `prism_backend_get_features` returns a `uint64_t`, so importing that enum would import
/// wrong numbers. `build.rs` blocks it.
pub mod feature {
    pub const IS_SUPPORTED_AT_RUNTIME: u64 = 1 << 0;
    pub const SPEAK: u64 = 1 << 2;
    pub const BRAILLE: u64 = 1 << 4;
    pub const OUTPUT: u64 = 1 << 5;
    pub const IS_SPEAKING: u64 = 1 << 6;
    pub const STOP: u64 = 1 << 7;
    pub const SET_VOLUME: u64 = 1 << 10;
}

/// A failed prism call, as the code it returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub i32);

impl Error {
    pub const NOT_IMPLEMENTED: Error = Error(sys::PrismError_PRISM_ERROR_NOT_IMPLEMENTED);
    pub const INVALID_UTF8: Error = Error(sys::PrismError_PRISM_ERROR_INVALID_UTF8);
    pub const BACKEND_NOT_AVAILABLE: Error = Error(sys::PrismError_PRISM_ERROR_BACKEND_NOT_AVAILABLE);

    /// Whether this says something about the caller rather than about the screen reader.
    ///
    /// The distinction matters because a failure is otherwise taken as evidence that the
    /// screen reader has gone and the speech path should be demoted — permanently, since
    /// prism's NVDA binding is fixed for the lifetime of the instance and never retried. A
    /// rejected string is not that evidence. Measured: `speak("")` comes back
    /// `INVALID_UTF8`, and an untitled plug-in window reaches `host.speech.output` with an
    /// empty string today, through `modules/daw-hosts/src/main.luau`.
    pub fn is_our_fault(self) -> bool {
        self == Error::INVALID_UTF8 || self == Error(sys::PrismError_PRISM_ERROR_INVALID_PARAM)
    }

    /// Whether the backend simply does not offer this call. Measured: NVDA answers
    /// `is_speaking` this way unless the running copy exposes a newer RPC interface.
    pub fn is_unsupported(self) -> bool {
        self == Error::NOT_IMPLEMENTED
    }
}

/// `prism error 9: Internal backend error` — the code, and prism's own words for it.
///
/// The words are what a log reader needs: "prism error 9" sent somebody to the header to count
/// enum members. They come from `prism_error_string`, a lookup in a static table that needs no
/// context and accepts any value (one out of range is "Unknown error").
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: a pure lookup; the returned string is static and never freed.
        let text = unsafe { sys::prism_error_string(self.0) };
        if text.is_null() {
            return write!(f, "prism error {}", self.0);
        }
        // SAFETY: non-null, NUL-terminated, and static (see above).
        let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        write!(f, "prism error {}: {text}", self.0)
    }
}

impl std::error::Error for Error {}

fn check(code: sys::PrismError) -> Result<(), Error> {
    if code == sys::PrismError_PRISM_OK {
        Ok(())
    } else {
        Err(Error(code))
    }
}

/// An initialised prism library.
///
/// Not `Send`, and that is the point: prism documents a backend handle as unsafe for
/// concurrent use even across logically independent calls, `prism_init` decides the COM
/// apartment of whichever thread creates it, and `prism_shutdown` calls `CoUninitialize` on
/// that same thread. One owning thread satisfies all three by construction, and the compiler
/// is what keeps it that way.
pub struct Context {
    ptr: *mut sys::PrismContext,
    _not_send: PhantomData<*const ()>,
}

impl Context {
    /// Initialises prism, or reports that it could not be.
    pub fn open() -> Result<Self, Error> {
        // SAFETY: fills a plain struct; the version field is what `prism_init` gates on.
        let mut cfg = unsafe { sys::prism_config_init() };
        if u32::from(cfg.version) != sys::PRISM_CONFIG_VERSION {
            // Cannot happen while the header and the library come from the same build, and
            // checked anyway: this is the field a submodule bump changes, and prism answers
            // a stale one with a null context and no explanation.
            return Err(Error(sys::PrismError_PRISM_ERROR_INCOMPATIBLE_ABI));
        }
        // No availability callback: prism creates its polling thread only when one is set,
        // and a thread that watches for screen readers appearing is not wanted here.
        cfg.availability_callback = None;

        // SAFETY: `cfg` outlives the call, which copies what it needs.
        let ptr = unsafe { sys::prism_init(&mut cfg) };
        if ptr.is_null() {
            return Err(Error(sys::PrismError_PRISM_ERROR_NOT_INITIALIZED));
        }
        Ok(Self { ptr, _not_send: PhantomData })
    }

    /// Every backend compiled into this build, by name.
    pub fn backend_names(&self) -> Vec<String> {
        // SAFETY: the context is non-null for as long as this value exists.
        let count = unsafe { sys::prism_registry_count(self.ptr) };
        (0..count)
            .filter_map(|i| {
                // SAFETY: `i` is below the count the registry just reported.
                let id = unsafe { sys::prism_registry_id_at(self.ptr, i) };
                self.name_of(id)
            })
            .collect()
    }

    fn name_of(&self, id: u64) -> Option<String> {
        // SAFETY: an id the registry itself handed out.
        let name = unsafe { sys::prism_registry_name(self.ptr, id) };
        if name.is_null() {
            return None;
        }
        // SAFETY: prism owns this string and keeps it for the life of the registry; it is
        // copied here rather than borrowed so that no caller can outlive it.
        Some(unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned())
    }

    /// Whether the named backend could speak right now, WITHOUT paying to open it.
    ///
    /// The distinction is the whole cost model of any "what can speak" list. Opening a
    /// speech engine was measured at 2.0 s (SAPI) and 3.5 s (OneCore), which is far too much
    /// to spend answering a question — but prism's own documentation says `initialize` is
    /// required before every call *except* `prism_backend_name`, `prism_backend_free` and
    /// `prism_backend_get_features`. So the instance is created, asked whether it is
    /// supported at runtime, and freed again, with the expensive step never taken.
    pub fn is_available(&self, name: &str) -> bool {
        let Ok(cname) = CString::new(name) else {
            return false;
        };
        // SAFETY: both pointers are valid for the duration of the call.
        let id = unsafe { sys::prism_registry_id(self.ptr, cname.as_ptr()) };
        if id == 0 {
            return false;
        }
        // OneCore is never asked, and this is not a shortcut: its `get_features` calls
        // WinRT's `ApiInformation::IsTypePresent`, and the statics that call builds do not
        // survive the `CoUninitialize` that closing a context performs. Probing it once is
        // harmless; probing it in a later context kills the process with an access violation
        // — found by measuring exactly that, and reproduced down to this single backend.
        //
        // An API that is safe the first time and fatal the second is worse than one that
        // refuses, so it refuses. The answer it would have given is `true` in any case:
        // OneCore ships with Windows, and if it turns out not to work, opening it fails and
        // the ordinary one-strike machinery handles that.
        if name.eq_ignore_ascii_case("OneCore") {
            return true;
        }

        // SAFETY: an id the registry recognised.
        let ptr = unsafe { sys::prism_registry_create(self.ptr, id) };
        if ptr.is_null() {
            return false;
        }
        let backend = Backend { ptr, _not_send: PhantomData };
        backend.supports(feature::IS_SUPPORTED_AT_RUNTIME)
    }

    /// Creates the named backend and initialises it, or returns why it could not.
    ///
    /// A fresh instance rather than a shared one, so that a backend which has died can be
    /// dropped and made again — prism's own documented recovery, which the caching
    /// `acquire` family makes impossible.
    pub fn open_backend(&self, name: &str) -> Result<Backend, Error> {
        let cname = CString::new(name).map_err(|_| Error(sys::PrismError_PRISM_ERROR_INVALID_PARAM))?;
        // SAFETY: both pointers are valid for the duration of the call.
        let id = unsafe { sys::prism_registry_id(self.ptr, cname.as_ptr()) };
        if id == 0 {
            // PRISM_BACKEND_INVALID. The name is not in this build.
            return Err(Error(sys::PrismError_PRISM_ERROR_BACKEND_NOT_AVAILABLE));
        }
        // SAFETY: an id the registry recognised.
        let ptr = unsafe { sys::prism_registry_create(self.ptr, id) };
        if ptr.is_null() {
            return Err(Error(sys::PrismError_PRISM_ERROR_BACKEND_NOT_AVAILABLE));
        }
        let backend = Backend { ptr, _not_send: PhantomData };
        // SAFETY: freshly created and non-null.
        check(unsafe { sys::prism_backend_initialize(backend.ptr) })?;
        Ok(backend)
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: created here, dropped once, and on the same thread — which `!Send`
        // guarantees, and which matters because this calls `CoUninitialize`.
        unsafe { sys::prism_shutdown(self.ptr) };
    }
}

/// One screen reader or speech engine, ready to be spoken through.
pub struct Backend {
    ptr: *mut sys::PrismBackend,
    _not_send: PhantomData<*const ()>,
}

impl Backend {
    /// What this backend calls itself — `"NVDA"`, `"SAPI"` and so on.
    pub fn name(&self) -> String {
        // SAFETY: non-null for the life of this value.
        let name = unsafe { sys::prism_backend_name(self.ptr) };
        if name.is_null() {
            return String::new();
        }
        // SAFETY: owned by the backend, copied rather than borrowed.
        unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
    }

    /// The feature bits, as [`feature`] constants.
    pub fn features(&self) -> u64 {
        // SAFETY: non-null for the life of this value.
        unsafe { sys::prism_backend_get_features(self.ptr) }
    }

    pub fn supports(&self, bit: u64) -> bool {
        self.features() & bit != 0
    }

    /// Says `text`, dropping anything unsaid when `interrupt`.
    ///
    /// **Empty text never reaches prism.** `speak("")` comes back `INVALID_UTF8`, because
    /// the backend treats "converted to zero UTF-16 units" as invalid input — and an empty
    /// string is reachable from a module today, so a caller that demotes on any error would
    /// lose the screen reader the first time somebody opened an untitled plug-in window.
    /// Saying nothing is the correct response to being asked to say nothing.
    pub fn speak(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        let Some(text) = sayable(text) else {
            return Ok(());
        };
        // SAFETY: the string outlives the call and is NUL-terminated.
        check(unsafe { sys::prism_backend_speak(self.ptr, text.as_ptr(), interrupt) })
    }

    /// Says `text` **and** sends it to a braille display, where the backend has one.
    pub fn output(&self, text: &str, interrupt: bool) -> Result<(), Error> {
        let Some(text) = sayable(text) else {
            return Ok(());
        };
        // SAFETY: as above.
        check(unsafe { sys::prism_backend_output(self.ptr, text.as_ptr(), interrupt) })
    }

    /// Sets the volume, where the backend has one — `0.0` to `1.0`.
    ///
    /// A screen reader does not: it speaks at whatever rate and volume its user chose, which
    /// is the entire reason for preferring it. Check [`feature::SET_VOLUME`] first. This
    /// exists so that a test can exercise the speaking calls without shouting at whoever is
    /// running it.
    pub fn set_volume(&self, volume: f32) -> Result<(), Error> {
        // SAFETY: non-null for the life of this value.
        check(unsafe { sys::prism_backend_set_volume(self.ptr, volume) })
    }

    /// Drops whatever has not been said yet.
    pub fn stop(&self) -> Result<(), Error> {
        // SAFETY: non-null for the life of this value.
        check(unsafe { sys::prism_backend_stop(self.ptr) })
    }

    /// Whether the backend is still speaking.
    ///
    /// Most are not able to answer — NVDA reports `NOT_IMPLEMENTED` unless the running copy
    /// exposes a newer RPC interface — so a caller must treat [`Error::is_unsupported`] as
    /// "no idea" rather than as a failure. The speech layer counts its own outstanding
    /// utterances instead of asking.
    pub fn is_speaking(&self) -> Result<bool, Error> {
        let mut speaking = false;
        // SAFETY: `speaking` outlives the call.
        check(unsafe { sys::prism_backend_is_speaking(self.ptr, &mut speaking) })?;
        Ok(speaking)
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // SAFETY: created by `open_backend`, freed once.
        unsafe { sys::prism_backend_free(self.ptr) };
    }
}

/// `text` as something prism will accept, or `None` when there is nothing to say.
///
/// Interior NUL bytes are dropped rather than rejected: they cannot be spoken, they cannot
/// survive a C string, and refusing the whole line over one would lose a sentence the user
/// was meant to hear.
fn sayable(text: &str) -> Option<CString> {
    // Stripped BEFORE the emptiness check, not after: a NUL is not whitespace, so `"\0"`
    // survives `trim` and would then be filtered down to an empty string and handed to
    // prism anyway. The smoke test found that; reading the function did not.
    let cleaned: String = text.chars().filter(|c| *c != '\0').collect();
    if cleaned.trim().is_empty() {
        return None;
    }
    CString::new(cleaned).ok()
}
