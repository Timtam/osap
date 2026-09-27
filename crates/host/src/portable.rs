//! Where the application keeps its own things: the log, `settings.toml`, and `modules/`.
//!
//! One rule, decided once: **beside the application**, not in a per-user system folder.
//! The whole folder can be copied to another machine or deleted to reset, which is what
//! "portable" buys — and for a user who cannot see, "delete this folder" is a far better
//! recovery instruction than "open Explorer and navigate to a hidden AppData path".
//!
//! macOS is the reason this is a module rather than three copies of `current_exe()`.
//! There, the executable is buried inside the application: `AutomationPlatform.app/
//! Contents/MacOS/automation-platform`. Writing beside *the executable* would write inside
//! the bundle — which breaks code signing, is read-only for an app in /Applications, and is
//! invisible to the person trying to send us their log. Beside *the application* means
//! beside the `.app`, which is the folder the user actually sees.
//!
//! And macOS is also why the `.app` is not always where it seems to be. A download opened while
//! it still carries its quarantine flag — Open on the Finder's context menu, or "Open Anyway" in
//! Privacy & Security — is run from a read-only copy of the `.app` ALONE, which macOS mounts at a
//! random path under `/private/var/folders/…/AppTranslocation/` (App Translocation, since macOS
//! 10.12). The folder around that copy holds nothing else: no `modules`, no settings, and no room
//! for a log. So on macOS the folder is the one around the ORIGINAL `.app`, which the Security
//! framework is asked for — see [`translocation`].
//!
//! That undoes what App Translocation is for, on purpose: it exists so that a signed application
//! cannot be made to load what somebody placed beside it in a download, and `modules/` beside
//! the original is exactly that. Here it vouches for nothing: the build is signed ad hoc and not
//! notarized, so Gatekeeper's only say was the user's own Open or Open Anyway; the README's
//! `xattr -dr` step loads the same modules; and native code in a module is gated separately.
//! A notarized, Developer ID-signed build would have to decide this again.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder the application lives in: next to the `.exe`, or next to the `.app` — the
/// original `.app` when macOS runs a translocated copy of it (see [`translocation`]).
///
/// Computed once, by whichever call comes first — the launcher's list of installed modules,
/// before the log is open — so everything that reads it gets the same answer. It cannot change
/// while the process runs, and a disagreement between the log path and the settings path would
/// be the kind of bug that only shows up as "my settings did not stick".
///
/// Nothing that works this out may log or panic. The log's own path is this folder, so a line
/// written from in here would ask for this answer while it is being worked out; and the panic
/// hook opens the log, so a panic in here would do the same. The outcome is kept instead
/// ([`translocation`]) and the log's header writes it once the log is open.
pub fn base_dir() -> &'static Path {
    &placement().dir
}

/// What App Translocation did to this launch, decided together with [`base_dir`].
pub fn translocation() -> &'static Translocation {
    &placement().translocation
}

/// Where installed modules are read from and installed to: `modules/` in [`base_dir`].
pub fn modules_dir() -> PathBuf {
    base_dir().join("modules")
}

struct Placement {
    dir: PathBuf,
    translocation: Translocation,
}

fn placement() -> &'static Placement {
    static PLACEMENT: OnceLock<Placement> = OnceLock::new();
    PLACEMENT.get_or_init(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
        place(&dir, ask_the_system)
    })
}

/// Whether this launch runs from a copy of the `.app` that macOS translocated, and what the
/// application did about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translocation {
    /// Not asked: not running from an `.app` (a loose binary out of `target/`), or not on
    /// macOS, which is the only system that does this.
    NotAsked,
    /// Asked, and the `.app` is where it seems to be.
    No,
    /// Running from the translocated copy `running`, whose original is `original`.
    /// [`base_dir`] is the folder around the original.
    Resolved { running: PathBuf, original: PathBuf },
    /// Running from the translocated copy `running`, and where its original is could not be
    /// found (`why`). [`base_dir`] is the folder around the copy, as it was before the original
    /// was ever asked for: it holds nothing, and nothing loads.
    Unresolved { running: PathBuf, why: String },
    /// The system could not be asked (`why`), and the path does not look like a translocated
    /// copy's. [`base_dir`] is the folder around the `.app`.
    Unknown { why: String },
}

impl Translocation {
    /// The `translocated` line's value, for the log's header and the macOS environment block;
    /// `None` when the question did not arise.
    ///
    /// Every translocated case repeats what to do, because removing the quarantine flag is
    /// still the clean way even when the original was found: the copy is read-only, macOS
    /// makes a new one at a new random path at every launch until the flag is gone, and
    /// finding the original rests on two functions that are not in macOS's public headers.
    pub fn report(&self) -> Option<String> {
        const FIX: &str = "Quit, run `xattr -dr com.apple.quarantine` on the .app (see \
                           README.txt beside it), and open it again.";
        match self {
            Translocation::NotAsked => None,
            Translocation::No => Some("no".into()),
            Translocation::Resolved { running, original } => Some(format!(
                "YES — macOS is running a read-only copy of the .app from {}, because it was \
                 opened with its download quarantine flag still set. The original is {}, and the \
                 folder around it is the application's folder (the log's `app folder` line says \
                 whether it can be written). Removing the flag is still the clean way: macOS \
                 makes a new copy at every launch until it is gone. {FIX}",
                running.display(),
                original.display()
            )),
            Translocation::Unresolved { running, why } => Some(format!(
                "YES — macOS is running a read-only copy of the .app from {}, and where the \
                 original is could not be found ({why}). So the modules, settings and log beside \
                 the original are not used, and no module will load. {FIX}",
                running.display()
            )),
            Translocation::Unknown { why } => Some(format!(
                "not known ({why}); the path does not look like a translocated copy's, so the \
                 folder around the .app is used as it is"
            )),
        }
    }
}

/// The system's answer about one `.app`: what [`place`] decides from. Only macOS gives the
/// answers after the first, and only the other systems give the first.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
enum Asked {
    /// This system does not translocate (every system but macOS).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    NotApplicable,
    /// Not translocated.
    No,
    /// Translocated, and this is the original `.app`.
    Original(PathBuf),
    /// Translocated, and the original could not be had: why.
    NoOriginal(String),
    /// The question could not be put: why.
    CannotAsk(String),
}

/// Decides the application's folder from the folder its executable is in and the system's
/// answer about the `.app` around it.
///
/// This is the whole rule apart from the question itself, which only a Mac can answer — kept
/// apart from the system so that every branch is unit-tested on the machine where it is
/// written. Anything short of an original that is another `.app` leaves the folder what it was
/// before translocation was handled at all: the folder around the running `.app`.
fn place(exe_dir: &Path, ask: impl FnOnce(&Path) -> Asked) -> Placement {
    let Some(bundle) = bundle_of(exe_dir) else {
        return Placement { dir: exe_dir.to_path_buf(), translocation: Translocation::NotAsked };
    };
    let around = bundle.parent().map(Path::to_path_buf).unwrap_or_default();
    let translocation = match ask(&bundle) {
        Asked::NotApplicable => Translocation::NotAsked,
        // The two functions are not public, so an answer that contradicts the path is not
        // trusted to be the whole story: saying "no" beside an executable under AppTranslocation
        // would hide the one thing a tester's log has to show.
        Asked::No if looks_translocated(&bundle) => Translocation::Unresolved {
            running: bundle,
            why: "the Security framework says it is not translocated, but it runs from under \
                  AppTranslocation"
                .into(),
        },
        Asked::No => Translocation::No,
        Asked::Original(original) => {
            match folder_around_app(&original) {
                Some(folder) if original != bundle => {
                    return Placement {
                        dir: folder,
                        translocation: Translocation::Resolved { running: bundle, original },
                    };
                }
                _ => Translocation::Unresolved {
                    why: format!(
                        "the Security framework named {} as the original, which is not another .app",
                        original.display()
                    ),
                    running: bundle,
                },
            }
        }
        Asked::NoOriginal(why) => Translocation::Unresolved { running: bundle, why },
        // The framework could not be asked, but the path is the evidence it would have used.
        Asked::CannotAsk(why) if looks_translocated(&bundle) => {
            Translocation::Unresolved { running: bundle, why }
        }
        Asked::CannotAsk(why) => Translocation::Unknown { why },
    };
    Placement { dir: around, translocation }
}

/// Given the directory an executable sits in, the `.app` around it.
///
/// Matched by shape (`…/Something.app/Contents/MacOS`) rather than by asking the OS,
/// because that is all a bundle is, and because it lets the rule be unit-tested on the
/// machine where the port is being written — which has no bundles at all.
///
/// Deliberately not gated to macOS: a rule that only exists on the platform nobody here can
/// run is a rule nobody can test.
fn bundle_of(dir: &Path) -> Option<PathBuf> {
    if dir.file_name()? != "MacOS" {
        return None;
    }
    let contents = dir.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    if !bundle.extension().is_some_and(|e| e == "app") {
        return None;
    }
    bundle.parent()?;
    Some(bundle.to_path_buf())
}

/// The folder an `.app` is in, or `None` when `app` is not an `.app` in a folder.
fn folder_around_app(app: &Path) -> Option<PathBuf> {
    if !app.extension().is_some_and(|e| e == "app") {
        return None;
    }
    let folder = app.parent()?;
    (!folder.as_os_str().is_empty()).then(|| folder.to_path_buf())
}

/// Whether a path has the shape of a translocated copy's: a component named `AppTranslocation`
/// (`/private/var/folders/…/T/AppTranslocation/<UUID>/d/Name.app`). Only consulted when the
/// Security framework could not be asked or says "not translocated"; its answer is the only one
/// that can find the original.
fn looks_translocated(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "AppTranslocation")
}

#[cfg(target_os = "macos")]
fn ask_the_system(bundle: &Path) -> Asked {
    translocate::ask(bundle)
}

#[cfg(not(target_os = "macos"))]
fn ask_the_system(_bundle: &Path) -> Asked {
    Asked::NotApplicable
}

/// The two Security framework functions that know about App Translocation.
///
/// `SecTranslocateIsTranslocatedURL` and `SecTranslocateCreateOriginalPathForURL` have been in
/// Security.framework since macOS 10.12, but are declared only in `SecTranslocate.h` in Apple's
/// open-source Security project, not in the SDK's headers, and no crate in the lock file binds
/// them (neither `objc2-security` nor `security-framework`). So they are looked up by name at
/// run time, as `IOHIDCheckAccess` is in `backend/macos/perm.rs` — and as Dolphin, DuckStation
/// and others do for exactly this — and never linked: a symbol this binary imported and a later
/// macOS no longer exported would stop the application from launching at all, before a line of
/// the log is written. A macOS without them answers "could not be asked", and the application
/// behaves as it did before this existed.
///
/// Both are documented there as callable from any thread, and take a path to a file or a
/// folder. `CreateOriginalPathForURL` maps a path inside the translocation mount back to the
/// same path under the original, checks that it exists, and fails with a POSIX error (`EPERM`,
/// `ENOENT`, `EINVAL`) otherwise. Asked about the `.app` itself, it answers the original `.app`.
#[cfg(target_os = "macos")]
mod translocate {
    use std::ffi::{c_void, CStr};
    use std::path::Path;
    use std::ptr::NonNull;

    use objc2_core_foundation::{CFCopyDescription, CFRetained, CFType, CFURL};

    use super::Asked;

    /// `Boolean SecTranslocateIsTranslocatedURL(CFURLRef path, bool *isTranslocated,
    /// CFErrorRef *error)`. `Boolean` is an unsigned char, and so is C's `bool` here: both are
    /// read as a byte rather than trusted to be 0 or 1.
    type IsTranslocatedFn = unsafe extern "C" fn(*const CFURL, *mut u8, *mut *mut CFType) -> u8;
    /// `CFURLRef SecTranslocateCreateOriginalPathForURL(CFURLRef translocatedPath,
    /// CFErrorRef *error)`: a URL under the Create rule, or NULL with the error set.
    type CreateOriginalFn = unsafe extern "C" fn(*const CFURL, *mut *mut CFType) -> *mut CFURL;

    const SECURITY: &CStr = c"/System/Library/Frameworks/Security.framework/Security";

    fn lookup() -> Result<(IsTranslocatedFn, CreateOriginalFn), String> {
        // Never closed: the function pointers are only valid while the image is loaded, and the
        // Security framework stays loaded for the life of the process anyway (AppKit uses it).
        // SAFETY: a NUL-terminated path; dlopen only reads it.
        let handle = unsafe { libc::dlopen(SECURITY.as_ptr(), libc::RTLD_LAZY) };
        if handle.is_null() {
            return Err(format!("the Security framework did not open: {}", dl_error()));
        }
        let find = |name: &CStr| -> Result<*mut c_void, String> {
            // SAFETY: a live handle and a NUL-terminated name; dlsym only reads.
            let p = unsafe { libc::dlsym(handle, name.as_ptr()) };
            if p.is_null() {
                Err(format!("the Security framework has no {}", name.to_string_lossy()))
            } else {
                Ok(p)
            }
        };
        let is = find(c"SecTranslocateIsTranslocatedURL")?;
        let original = find(c"SecTranslocateCreateOriginalPathForURL")?;
        // SAFETY: the addresses of the two functions whose C signatures are the types above.
        Ok(unsafe {
            (
                std::mem::transmute::<*mut c_void, IsTranslocatedFn>(is),
                std::mem::transmute::<*mut c_void, CreateOriginalFn>(original),
            )
        })
    }

    fn dl_error() -> String {
        // SAFETY: dlerror returns NULL or a NUL-terminated string valid until the next dl call.
        let e = unsafe { libc::dlerror() };
        if e.is_null() {
            "no reason given".into()
        } else {
            // SAFETY: as above.
            unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
        }
    }

    /// An error handed back through a `CFErrorRef *`, taken over (it is the caller's to
    /// release) and put into words.
    fn describe(error: *mut CFType) -> String {
        match NonNull::new(error) {
            // SAFETY: an out-error is returned retained, and nothing else holds this one.
            Some(e) => {
                let e = unsafe { CFRetained::from_raw(e) };
                CFCopyDescription(Some(&*e))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "an error without a description".into())
            }
            None => "no error was returned".into(),
        }
    }

    /// Asks about `bundle`, the `.app` this executable is inside.
    pub(super) fn ask(bundle: &Path) -> Asked {
        let (is_translocated, create_original) = match lookup() {
            Ok(f) => f,
            Err(why) => return Asked::CannotAsk(why),
        };
        let Some(url) = CFURL::from_directory_path(bundle) else {
            return Asked::CannotAsk(format!("{} could not be made into a URL", bundle.display()));
        };
        let mut translocated: u8 = 0;
        let mut error: *mut CFType = std::ptr::null_mut();
        // SAFETY: a live URL and two out-pointers to locals, per the signature above.
        let ok = unsafe { is_translocated(&*url, &mut translocated, &mut error) };
        let why = describe(error);
        if ok == 0 {
            return Asked::CannotAsk(format!("SecTranslocateIsTranslocatedURL failed: {why}"));
        }
        if translocated == 0 {
            return Asked::No;
        }
        let mut error: *mut CFType = std::ptr::null_mut();
        // SAFETY: as above.
        let original = unsafe { create_original(&*url, &mut error) };
        let why = describe(error);
        let Some(original) = NonNull::new(original) else {
            return Asked::NoOriginal(format!("SecTranslocateCreateOriginalPathForURL failed: {why}"));
        };
        // SAFETY: a Create function's result, returned retained.
        let original = unsafe { CFRetained::from_raw(original) };
        match original.to_file_path() {
            // Collected from its components to drop the trailing slash a directory URL has,
            // which the log would otherwise print.
            Some(path) => Asked::Original(path.components().collect()),
            None => Asked::NoOriginal(format!(
                "SecTranslocateCreateOriginalPathForURL answered {}, which is not a file path",
                original.string()
            )),
        }
    }
}

/// What a folder holds, as far as looking for modules in it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Folder {
    /// There is no such folder.
    Missing,
    /// It is there and could not be listed (or is not a folder): the reason.
    Unreadable(String),
    /// This many folders in it whose names do not start with a dot — the ones a module could
    /// be in. Zero is an empty folder.
    Holds(usize),
}

/// Looks into `dir`, counting what [`Folder::Holds`] counts.
pub fn look_into(dir: &Path) -> Folder {
    match std::fs::read_dir(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Folder::Missing,
        Err(e) => Folder::Unreadable(e.to_string()),
        Ok(entries) => Folder::Holds(
            entries
                .flatten()
                .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
                .count(),
        ),
    }
}

impl Folder {
    /// The state of the modules folder, for the log's header: `3 folders`, `empty`,
    /// `does not exist` or `could not be read (why)`.
    ///
    /// Only the state, and no verdict on it: the header is written before anybody knows
    /// whether the installed modules are the ones this session runs (a start with module
    /// folders on the command line does not read them, so every development run has no such
    /// folder). What an absent folder means is said by the `no modules to load` line, which
    /// is only written when nothing else was given.
    pub fn modules_state(&self) -> String {
        match self {
            Folder::Missing => "does not exist".into(),
            Folder::Unreadable(why) => format!("could not be read ({why})"),
            Folder::Holds(0) => "empty".into(),
            Folder::Holds(1) => "1 folder".into(),
            Folder::Holds(n) => format!("{n} folders"),
        }
    }
}

/// Can we actually write there?
///
/// Asked rather than assumed, and asked by *writing*: permissions on macOS are not a
/// property of the folder alone (a quarantined download, an app moved into /Applications,
/// and a sandboxed launch all fail differently), and the read-only metadata bit answers a
/// different question than "will my log line land".
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Where to put something that MUST be writable, when the application's own folder is not.
///
/// Only the log really qualifies: settings and modules failing loudly beside the app is
/// honest and fixable ("move the folder somewhere you can write"), but a log that cannot be
/// written leaves a remote tester with nothing to send, which is the one failure that makes
/// every other failure unreportable.
pub fn fallback_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join("Library").join("Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("HOME").map(PathBuf::from).map(|h| h.join(".local/share"));

    base.unwrap_or_else(std::env::temp_dir).join("AutomationPlatform")
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSIDE: &str = "/Users/t/Desktop/AutomationPlatform.app/Contents/MacOS";
    const RUNNING: &str =
        "/private/var/folders/xy/abc/T/AppTranslocation/0A1B-2C3D/d/AutomationPlatform.app";
    const RUNNING_EXE_DIR: &str =
        "/private/var/folders/xy/abc/T/AppTranslocation/0A1B-2C3D/d/AutomationPlatform.app/Contents/MacOS";
    const ORIGINAL: &str = "/Users/t/Downloads/AutomationPlatform/AutomationPlatform.app";

    fn asked(answer: Asked) -> impl FnOnce(&Path) -> Asked {
        move |_| answer
    }

    #[test]
    fn a_bundle_resolves_to_the_folder_holding_the_app() {
        assert_eq!(
            bundle_of(Path::new(INSIDE)),
            Some(PathBuf::from("/Users/t/Desktop/AutomationPlatform.app"))
        );
        let p = place(Path::new(INSIDE), asked(Asked::No));
        assert_eq!(p.dir, PathBuf::from("/Users/t/Desktop"));
        assert_eq!(p.translocation, Translocation::No);
    }

    #[test]
    fn a_plain_directory_is_left_alone() {
        // The Windows and loose-binary cases: nothing to strip, and nothing must be
        // invented — no bundle is what makes `base_dir` fall back to the exe folder.
        assert_eq!(bundle_of(Path::new("C:/tools/platform")), None);
        assert_eq!(bundle_of(Path::new("/usr/local/bin")), None);
        // And the system is not even asked: there is no `.app` to ask about.
        let p = place(Path::new("/usr/local/bin"), |_| panic!("asked about a loose binary"));
        assert_eq!(p.dir, PathBuf::from("/usr/local/bin"));
        assert_eq!(p.translocation, Translocation::NotAsked);
        assert_eq!(p.translocation.report(), None);
    }

    #[test]
    fn a_near_miss_is_not_a_bundle() {
        // Every part has to match, or a folder that merely happens to be called MacOS
        // would send the log two levels up into somebody else's directory.
        assert_eq!(bundle_of(Path::new("/x/Thing.app/Resources/MacOS")), None);
        assert_eq!(bundle_of(Path::new("/x/Thing/Contents/MacOS")), None);
        assert_eq!(bundle_of(Path::new("/x/Thing.app/Contents/Frameworks")), None);
    }

    #[test]
    fn the_system_is_asked_about_the_app_not_the_executable_folder() {
        let mut seen = None;
        place(Path::new(RUNNING_EXE_DIR), |b| {
            seen = Some(b.to_path_buf());
            Asked::No
        });
        assert_eq!(seen, Some(PathBuf::from(RUNNING)));
    }

    #[test]
    fn a_translocated_copy_uses_the_folder_around_the_original() {
        // The report: a download unzipped whole, its .app opened from the Finder with the
        // quarantine flag still set. The modules are beside the original, not the copy.
        let p = place(Path::new(RUNNING_EXE_DIR), asked(Asked::Original(ORIGINAL.into())));
        assert_eq!(p.dir, PathBuf::from("/Users/t/Downloads/AutomationPlatform"));
        assert_eq!(
            p.translocation,
            Translocation::Resolved { running: RUNNING.into(), original: ORIGINAL.into() }
        );
        let line = p.translocation.report().unwrap();
        assert!(line.starts_with("YES"), "{line}");
        assert!(line.contains(RUNNING) && line.contains(ORIGINAL), "{line}");
        // No claim that the log is beside the original: a folder that cannot be written sends
        // it to Application Support, and the `app folder` line is the one that knows.
        assert!(line.contains("`app folder` line") && !line.contains("log beside"), "{line}");
        // Still told to remove the flag: the copy is made again at every launch.
        assert!(line.contains("xattr -dr com.apple.quarantine"), "{line}");
    }

    #[test]
    fn a_no_is_not_believed_under_app_translocation() {
        // The functions are not public; if a later macOS answered "no" for a translocated copy,
        // the log must not say `translocated: no` beside an executable under AppTranslocation.
        let p = place(Path::new(RUNNING_EXE_DIR), asked(Asked::No));
        assert_eq!(p.dir, Path::new(RUNNING).parent().unwrap());
        assert!(matches!(p.translocation, Translocation::Unresolved { .. }));
        let line = p.translocation.report().unwrap();
        assert!(line.starts_with("YES") && line.contains("says it is not translocated"), "{line}");
        assert!(line.contains("xattr -dr"), "{line}");
    }

    #[test]
    fn a_directory_url_s_trailing_slash_does_not_matter() {
        // `translocate::ask` drops it; this is the rule not depending on that.
        let p = place(Path::new(RUNNING_EXE_DIR), asked(Asked::Original(format!("{ORIGINAL}/").into())));
        assert_eq!(p.dir, PathBuf::from("/Users/t/Downloads/AutomationPlatform"));
        assert!(matches!(p.translocation, Translocation::Resolved { .. }));
    }

    #[test]
    fn an_original_that_is_not_another_app_changes_nothing() {
        // Whatever the framework says, only a different .app in a folder moves the application's
        // folder; anything else leaves it where it was before this was handled at all.
        for odd in [
            "/Users/t/Downloads/AutomationPlatform",
            "/Users/t/Downloads/AutomationPlatform/AutomationPlatform.app/Contents/MacOS/automation-platform",
            "AutomationPlatform.app",
            RUNNING,
        ] {
            let p = place(Path::new(RUNNING_EXE_DIR), asked(Asked::Original(odd.into())));
            assert_eq!(p.dir, Path::new(RUNNING).parent().unwrap(), "{odd}");
            assert!(matches!(p.translocation, Translocation::Unresolved { .. }), "{odd}");
        }
    }

    #[test]
    fn an_original_that_cannot_be_had_behaves_as_before() {
        let p = place(
            Path::new(RUNNING_EXE_DIR),
            asked(Asked::NoOriginal("SecTranslocateCreateOriginalPathForURL failed: ENOENT".into())),
        );
        assert_eq!(p.dir, Path::new(RUNNING).parent().unwrap());
        let line = p.translocation.report().unwrap();
        assert!(line.starts_with("YES") && line.contains("ENOENT"), "{line}");
        assert!(line.contains("no module will load") && line.contains("xattr -dr"), "{line}");
    }

    #[test]
    fn without_the_framework_the_path_is_the_evidence() {
        // A macOS without the two functions: the translocated shape is still called what it is,
        // and anything else is said to be unknown rather than claimed not translocated.
        let p = place(Path::new(RUNNING_EXE_DIR), asked(Asked::CannotAsk("no symbol".into())));
        assert_eq!(p.dir, Path::new(RUNNING).parent().unwrap());
        assert!(matches!(p.translocation, Translocation::Unresolved { .. }));

        let p = place(Path::new(INSIDE), asked(Asked::CannotAsk("no symbol".into())));
        assert_eq!(p.dir, PathBuf::from("/Users/t/Desktop"));
        assert_eq!(p.translocation, Translocation::Unknown { why: "no symbol".into() });
        assert!(p.translocation.report().unwrap().starts_with("not known (no symbol)"));
    }

    #[test]
    fn a_system_that_does_not_translocate_says_nothing() {
        let p = place(Path::new(INSIDE), asked(Asked::NotApplicable));
        assert_eq!(p.dir, PathBuf::from("/Users/t/Desktop"));
        assert_eq!(p.translocation.report(), None);
    }

    #[test]
    fn the_shape_of_a_translocated_path() {
        assert!(looks_translocated(Path::new(RUNNING)));
        assert!(!looks_translocated(Path::new(ORIGINAL)));
        // A component, not a substring.
        assert!(!looks_translocated(Path::new("/Users/t/AppTranslocationNotes/X.app")));
    }

    /// A fresh folder of this test's own, removed when dropped. Not `tempfile`, which
    /// `crates/macos-check` (it borrows this file, tests included) does not have.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let p = std::env::temp_dir().join(format!("portable-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Scratch(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_modules_folder_is_described_by_what_is_in_it() {
        let dir = Scratch::new("modules");
        let modules = dir.0.join("modules");
        assert_eq!(look_into(&modules), Folder::Missing);
        // The state and nothing more: a development run has no such folder and needs none.
        assert_eq!(Folder::Missing.modules_state(), "does not exist");
        assert_eq!(Folder::Unreadable("denied".into()).modules_state(), "could not be read (denied)");

        std::fs::create_dir(&modules).unwrap();
        assert_eq!(look_into(&modules), Folder::Holds(0));
        assert_eq!(Folder::Holds(0).modules_state(), "empty");

        // Counted as the loader counts: folders, and not the ones whose names start with a dot.
        std::fs::create_dir(modules.join("probe")).unwrap();
        std::fs::create_dir(modules.join(".staging-1")).unwrap();
        std::fs::write(modules.join("stray.zip"), b"").unwrap();
        assert_eq!(look_into(&modules), Folder::Holds(1));
        assert_eq!(Folder::Holds(1).modules_state(), "1 folder");
        assert_eq!(Folder::Holds(3).modules_state(), "3 folders");

        // A file where the folder should be is not a folder that can be read.
        let file = dir.0.join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(matches!(look_into(&file), Folder::Unreadable(_)));
    }
}

/// The part only a Mac can answer, asked of the real Security framework: the macOS CI job runs
/// these, and `macos-check --tests` compiles them.
#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    use super::*;

    #[test]
    fn the_security_framework_answers_for_a_folder_that_is_not_translocated() {
        // This crate's own folder: it exists, and nothing translocates a checkout.
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(translocate::ask(here), Asked::No);
    }

    #[test]
    fn a_bundle_that_is_not_translocated_keeps_its_folder() {
        // The whole chain with the real question: a bundle-shaped folder, asked about for real.
        let root = std::env::temp_dir().join(format!("portable-bundle-{}", std::process::id()));
        let exe_dir = root.join("Shape.app").join("Contents").join("MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        let p = place(&exe_dir, ask_the_system);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(p.dir, root);
        assert_eq!(p.translocation, Translocation::No);
        assert_eq!(p.translocation.report().as_deref(), Some("no"));
    }
}
