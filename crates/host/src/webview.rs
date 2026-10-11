//! `host.gui.webview`'s rules, with no window in them: which answer `show` gives and when, what
//! happens to a message posted before the page is there, which file a request is served and with
//! which headers, which navigations and messages are let through, which window a page is in and
//! where it goes, which window the front goes back to, what a key equivalent does in a Mac's page,
//! and the page's own `platform` object. The window itself is `webview_win.rs` on Windows and
//! `backend/macos/page.rs` on a Mac (stage 4, TODO.md); the Luau side is `webview_lua.rs`, the
//! same on both.
//!
//! **Experimental.** Stage 1 of the plan the maintainer decided on 2026-09-28: a page a module
//! shows beside a plug-in window, for the Windows probe (`tools/html-probe`) to measure with NVDA.
//! The API may change after the probe, and after the Mac's has met VoiceOver.
//!
//! Pure on purpose: everything here decides something, and a decision about a browser window is
//! only testable if nothing opens one. `crates/macos-check` borrows the file, so the answers a Mac
//! gives are compiled for the Mac as well, and so are the rules the Mac's page window is built on.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Every answer `show` gives instead of `true`, and the words it gives it in.
///
/// The words are the API: a module compares them, so they are fixed strings and not messages.
/// What went wrong in detail is in the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A headless run has no window system to show anything in.
    Headless,
    /// A platform with no page window: neither Windows nor macOS ([`Where::supports`]). The
    /// application runs on no other, so no module meets it today.
    Unsupported,
    /// The page is being built, or has not finished loading. `onReady` says when it has.
    NotReady,
    /// Windows without the WebView2 runtime. Asked at every `show` that would start a build, so
    /// installing the runtime needs no restart. Never on a Mac, whose WebKit is the system's.
    RuntimeMissing,
    /// The last build failed; the log says why. Said once, and the next `show` tries again.
    BuildFailed,
    /// The page is on screen, and the system refused to bring it to the front — or, where it would
    /// have been over `avoid` ([`over_refused`]), it is hidden again. On a Mac only an answer the
    /// system gives at once (backend/macos/page.rs): one that comes later is in the log.
    Declined,
    /// A show without `focus` found no room beside `avoid`: the page would have gone over it
    /// without the keyboard ([`over_refused`]). A show with `focus` goes over it instead.
    NoRoom,
}

impl Refusal {
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::Headless => "headless",
            Refusal::Unsupported => "not supported on this platform yet",
            Refusal::NotReady => "not ready yet",
            Refusal::RuntimeMissing => "WebView2 runtime not installed",
            Refusal::BuildFailed => "could not be built",
            Refusal::Declined => "declined by the system",
            Refusal::NoRoom => "no room beside the rectangle to avoid",
        }
    }
}

/// What a successful `show` adds as its second value when there was no room beside `avoid` and
/// the page was put over part of it ([`Placement::Over`]). Its only note: every other success
/// answers `true` alone.
pub const OVER: &str = "over the rectangle to avoid";

/// Whether a show whose page went over `avoid` ([`Placement::Over`]) ends with the page hidden,
/// and with which refusal: kept only when the page ends in front — `focus`, and not `declined` by
/// Windows. The user then has the page, and the plug-in is reached through a round trip that
/// brings its window in front of the page. A page raised over the plug-in without the keyboard
/// would cover points the application's own clicks and screen reads land on, while the plug-in
/// window keeps the front — and so cannot be raised above the page by being brought forward
/// again. `None`: kept.
pub fn over_refused(focus: bool, declined: bool) -> Option<Refusal> {
    if !focus {
        Some(Refusal::NoRoom)
    } else if declined {
        Some(Refusal::Declined)
    } else {
        None
    }
}

/// Where a page is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Described and not built: before the first `show`, and after `close`. The next `show`
    /// starts a build.
    Idle,
    /// A build is queued or running, on the event loop at the end of a tick.
    Building,
    /// Built; the page's document has not finished loading.
    Loading,
    /// Loaded: `show` shows it, and a message is delivered as it is posted.
    Ready,
    /// The build failed, or the page's first document did not load. The next `show` answers
    /// [`Refusal::BuildFailed`] and goes back to idle.
    Failed,
}

/// What a `show` does, decided by [`Lifecycle::on_show`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowStep {
    Refuse(Refusal),
    /// Queue a build and answer [`Refusal::NotReady`].
    Build,
    /// Show the page that is there.
    Show,
}

/// The facts about this run a `show` is decided by.
#[derive(Clone, Copy, Debug)]
pub struct Where {
    pub headless: bool,
    pub supported: bool,
}

impl Where {
    /// This run: headless or not, and whether this platform can show a page at all.
    pub fn here(headless: bool) -> Self {
        Where { headless, supported: Where::supports(std::env::consts::OS) }
    }

    /// Whether the platform Rust names `os` (`std::env::consts::OS`) has a page window: Windows
    /// (`webview_win.rs`) and macOS (`backend/macos/page.rs`), and nothing else.
    pub fn supports(os: &str) -> bool {
        matches!(os, "windows" | "macos")
    }

    /// Whether a page can ever be ready here. A message posted where it cannot is dropped, not
    /// kept for a page that will never come.
    pub fn can_show(self) -> bool {
        !self.headless && self.supported
    }
}

/// How many messages wait for a page that is not ready. A module that posts its state on every
/// change and never shows the page would otherwise grow this for the whole session.
pub const MAX_QUEUED_POSTS: usize = 256;

/// The largest message a page may send the module, in bytes of JSON. Anything larger is dropped
/// with a line in the log: the page's script is the module's own, but the data in it comes from
/// the screen and from files, and one runaway string should not stall the event loop.
pub const MAX_MESSAGE_BYTES: usize = 1 << 20;

/// One page's life, and the messages waiting for it.
#[derive(Debug)]
pub struct Lifecycle {
    phase: Phase,
    queued: VecDeque<String>,
    dropped: usize,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Lifecycle { phase: Phase::Idle, queued: VecDeque::new(), dropped: 0 }
    }
}

impl Lifecycle {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Decides a `show`. `runtime` is asked only when a build would start: whether the
    /// platform's web engine is installed.
    ///
    /// The order is the documentation's: a headless run and a platform without pages say so
    /// before anything else, and a failed build is said once and then tried again.
    pub fn on_show(&mut self, at: Where, runtime: impl FnOnce() -> bool) -> ShowStep {
        if at.headless {
            return ShowStep::Refuse(Refusal::Headless);
        }
        if !at.supported {
            return ShowStep::Refuse(Refusal::Unsupported);
        }
        match self.phase {
            Phase::Ready => ShowStep::Show,
            Phase::Building | Phase::Loading => ShowStep::Refuse(Refusal::NotReady),
            Phase::Failed => {
                self.phase = Phase::Idle;
                ShowStep::Refuse(Refusal::BuildFailed)
            }
            Phase::Idle => {
                if !runtime() {
                    return ShowStep::Refuse(Refusal::RuntimeMissing);
                }
                self.phase = Phase::Building;
                ShowStep::Build
            }
        }
    }

    /// The build came back. Ignored unless one was running: a page closed while it was being
    /// built is idle, and a build that finishes for it is thrown away by the caller.
    pub fn built(&mut self, ok: bool) {
        if self.phase == Phase::Building {
            self.phase = if ok { Phase::Loading } else { Phase::Failed };
        }
    }

    /// A document of the page's own finished loading, successfully — the first time, or again
    /// after the page reloaded itself. Returns the messages that waited for it, in the order they
    /// were posted. `None` when the page is not built (the event came from a page since closed),
    /// which delivers nothing.
    pub fn loaded(&mut self) -> Option<Vec<String>> {
        match self.phase {
            Phase::Building | Phase::Loading | Phase::Ready => {
                self.phase = Phase::Ready;
                Some(self.queued.drain(..).collect())
            }
            Phase::Idle | Phase::Failed => None,
        }
    }

    /// A document of the page failed to load (not a navigation the host refused, which leaves
    /// the page as it was). `true` when it was the page's first document: the page never became
    /// ready, so it counts as a failed build — the caller takes its window down, and the next
    /// `show` answers [`Refusal::BuildFailed`] and then builds again. A ready page that fails a
    /// later load stays ready.
    pub fn load_failed(&mut self) -> bool {
        if self.phase == Phase::Loading {
            self.phase = Phase::Failed;
            self.queued.clear();
            return true;
        }
        false
    }

    /// A message the module posted, as JSON. `Some` when it is to be delivered now; otherwise it
    /// waits for the page, the oldest giving way past [`MAX_QUEUED_POSTS`].
    pub fn post(&mut self, json: String) -> Option<String> {
        if self.phase == Phase::Ready {
            return Some(json);
        }
        if self.queued.len() >= MAX_QUEUED_POSTS {
            self.queued.pop_front();
            self.dropped += 1;
        }
        self.queued.push_back(json);
        None
    }

    /// The page is gone — closed by the module, or its browser process ended. What waited for
    /// it goes with it; the next `show` builds a new one.
    pub fn closed(&mut self) {
        self.phase = Phase::Idle;
        self.queued.clear();
    }

    /// How many waiting messages gave way since the last call, for one line in the log.
    pub fn take_dropped(&mut self) -> usize {
        std::mem::take(&mut self.dropped)
    }

    #[cfg(test)]
    pub fn queued(&self) -> usize {
        self.queued.len()
    }
}

/// What a page's window sent the host, queued by the window system's callbacks and handed to
/// the module on the next tick — never into Luau from inside a callback of the web view's.
///
/// `build` names the build it came from: a page closed and built again is a different page,
/// and an event of the old one is dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageEvent {
    /// `platform.post(value)`, as the JSON text the page's script made of it.
    Message { page: u64, build: u64, text: String },
    /// A document of the page's own origin finished loading, successfully.
    Loaded { page: u64, build: u64 },
    /// A document failed to load — an error, not a navigation the host refused. `why` is the
    /// engine's word for it, for the log.
    LoadFailed { page: u64, build: u64, why: String },
    /// The window went away without the module asking: `"user"` (its close box), or `"failed"`
    /// (its browser or renderer process ended, or the page's script closed its web view).
    Closed { page: u64, build: u64, why: &'static str },
}

/// The URL scheme a module's pages are served under, and with it the page's origin — one per
/// module, so two modules' pages never share storage. Derived from the module's id: its
/// letters and digits in lower case, everything else a hyphen, cut at 40, then the first 64 bits
/// of the SHA-256 of the whole id, so two ids that read the same that way (`com.a-b`, `com.a.b`)
/// still differ — and no id can be chosen to land on another's. (The first version hashed with
/// 32 bits of FNV, and the review of 2026-09-28 found a second id with a given id's origin in
/// seconds.) At most 61 characters: a host name's label on Windows, where the page is
/// `http://<scheme>.localhost`.
pub fn scheme_for(module_id: &str) -> String {
    let mut readable = String::new();
    for c in module_id.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            readable.push(c);
        } else if !readable.ends_with('-') {
            readable.push('-');
        }
    }
    let readable: String = readable.trim_matches('-').chars().take(40).collect();
    let readable = readable.trim_end_matches('-');
    // Stable across builds and platforms, which a std hasher is not.
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(module_id.as_bytes());
    let hash: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
    if readable.is_empty() {
        format!("mod-{hash}")
    } else {
        format!("mod-{readable}-{hash}")
    }
}

/// The page's origin, as the web view reports it for a document and a message: on Windows the
/// custom scheme is served as a subdomain of `localhost` (wry's way around WebView2 accepting no
/// scheme of its own).
pub fn origin_for(scheme: &str) -> String {
    format!("http://{scheme}.localhost")
}

/// The page's origin in WebKit (`backend/macos/page.rs`), which serves an application's own scheme
/// as it is: `<scheme>://localhost`, the host every [`entry_url`] names. Windows' is
/// [`origin_for`]'s.
#[cfg(any(test, target_os = "macos"))]
pub fn webkit_origin_for(scheme: &str) -> String {
    format!("{scheme}://localhost")
}

/// What a URL path may carry unencoded: the unreserved characters and the sub-delimiters. `/`
/// is encoded too, because this encodes one segment at a time.
const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b'@');

/// The URL a page is opened at, for its module-relative file `rel` (`/` between its parts).
pub fn entry_url(scheme: &str, rel: &str) -> String {
    let path: Vec<String> = rel
        .split('/')
        .map(|p| percent_encoding::utf8_percent_encode(p, SEGMENT).to_string())
        .collect();
    format!("{scheme}://localhost/{}", path.join("/"))
}

/// The path a request asks for, still percent-encoded, without its query and fragment: what
/// follows `<scheme>://localhost/`. `None` for any other host or scheme.
pub fn request_path<'a>(uri: &'a str, scheme: &str) -> Option<&'a str> {
    let prefix = format!("{scheme}://localhost/");
    // `get`, not indexing: a URI with a multi-byte character where the prefix would end is not
    // ours, and must not be a panic on the thread that serves the page.
    if !uri.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(&prefix)) {
        return None;
    }
    let rest = &uri[prefix.len()..];
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    Some(&rest[..end])
}

/// The file a request path names inside the module, or `None` — and `None` is all anyone
/// learns, which the server answers with 404.
///
/// `root` must be canonical (the caller canonicalises it once). The path is decoded, every part
/// has to be a plain name — no empty part, no `.` or `..`, no `\`, no `:` (a drive, or an NTFS
/// stream), no control character — and the joined path is then resolved in full by the file
/// system, links included, and has to lie inside `root` and be a file. The last step is the
/// real one: it is what keeps a link or a junction inside the module folder from leading out of
/// it, which checking the text alone cannot.
pub fn resolve(root: &Path, raw: &str) -> Option<PathBuf> {
    let decoded = percent_encoding::percent_decode_str(raw).decode_utf8().ok()?;
    resolve_file(root, &decoded)
}

/// [`resolve`] for a path that is not URL-encoded: a module's own `file` option.
pub fn resolve_file(root: &Path, rel_path: &str) -> Option<PathBuf> {
    if rel_path.is_empty() {
        return None;
    }
    let mut rel = PathBuf::new();
    for part in rel_path.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        if part.chars().any(|c| c.is_control() || c == '\\' || c == ':') {
            return None;
        }
        rel.push(part);
    }
    let full = root.join(&rel).canonicalize().ok()?;
    (full.starts_with(root) && full.is_file()).then_some(full)
}

/// The path of `file`, a file inside `root` (both canonical), as the page's URL names it: its
/// parts from `root` down, joined by `/`.
pub fn url_path(root: &Path, file: &Path) -> Option<String> {
    let parts: Vec<String> = file
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The Content-Security-Policy every response carries, as a header — the host's, not the
/// page's, so no page can loosen it.
///
/// Scripts only from the module's own files: no inline script, no `eval`, no event-handler
/// attribute, so text that reaches the page from the screen or from a file cannot run as code
/// even if a page puts it into `innerHTML`. `connect-src 'self'` lets a page `fetch` its own
/// data files and nothing else. No frames, no form submissions, no plugins, no workers, no
/// `<base>`. Inline style is allowed — it cannot run anything. What no policy covers — WebRTC,
/// and the browser resolving and connecting ahead of a link (`dns-prefetch`, `preconnect`) — is
/// not stopped yet (TODO.md, stage 3); every request outside the origin is answered 403 on
/// Windows (webview_win.rs), whatever the policy says.
pub const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; font-src 'self'; media-src 'self'; connect-src 'self'; \
    manifest-src 'none'; worker-src 'none'; frame-src 'none'; child-src 'none'; \
    object-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'";

/// The type a file is served with, from its extension, with the charset for text: a page's
/// files are UTF-8.
pub fn content_type(path: &Path) -> String {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let essence = mime.essence_str();
    let textual = mime.type_() == mime_guess::mime::TEXT
        || ["javascript", "json", "xml"].iter().any(|t| essence.ends_with(t));
    if textual {
        format!("{essence}; charset=utf-8")
    } else {
        essence.to_string()
    }
}

/// A response, before the platform's web view turns it into its own kind.
#[derive(Debug)]
pub struct Served {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

/// The headers every response carries, whatever its status — the host's own answers (the 403
/// for another origin, the 500 when a response cannot be made) included.
pub fn base_headers() -> Vec<(&'static str, String)> {
    vec![
        ("Content-Security-Policy", CSP.to_string()),
        ("X-Content-Type-Options", "nosniff".to_string()),
        // Nothing of a module's page is kept on disk, so an edited file is what the page shows
        // the next time it is built.
        ("Cache-Control", "no-store".to_string()),
        ("Referrer-Policy", "no-referrer".to_string()),
        // A page served over `http:` would otherwise have Chromium look up the host names of its
        // links ahead of time, which sends them out; "off" cannot be turned on again by the page.
        ("X-DNS-Prefetch-Control", "off".to_string()),
    ]
}

/// The largest file a page is served, in bytes. A file is read whole, on the event loop that
/// also carries every hotkey, so a larger one is answered 413 rather than read.
pub const MAX_FILE_BYTES: u64 = 16 << 20;

/// Answers one request of a page: the file it names inside `root`, or 404; `GET` and `HEAD`
/// only; 413 for a file larger than [`MAX_FILE_BYTES`]. The file is read when it is asked for, on
/// the thread the web view asks on — the event loop on Windows — so a page's files should be
/// small.
pub fn serve(root: &Path, scheme: &str, method: &str, uri: &str) -> Served {
    let mut headers = base_headers();
    let head = method.eq_ignore_ascii_case("HEAD");
    if !head && !method.eq_ignore_ascii_case("GET") {
        headers.push(("Allow", "GET, HEAD".to_string()));
        return Served { status: 405, headers, body: Vec::new() };
    }
    let found = request_path(uri, scheme).and_then(|p| resolve(root, p));
    if found.as_ref().and_then(|p| std::fs::metadata(p).ok()).is_some_and(|m| m.len() > MAX_FILE_BYTES) {
        headers.push(("Content-Type", "text/plain; charset=utf-8".to_string()));
        return Served { status: 413, headers, body: if head { Vec::new() } else { b"too large".to_vec() } };
    }
    let bytes = found.as_ref().and_then(|p| std::fs::read(p).ok().map(|b| (p, b)));
    match bytes {
        Some((path, body)) => {
            headers.push(("Content-Type", content_type(path)));
            Served { status: 200, headers, body: if head { Vec::new() } else { body } }
        }
        None => {
            headers.push(("Content-Type", "text/plain; charset=utf-8".to_string()));
            Served { status: 404, headers, body: if head { Vec::new() } else { b"not found".to_vec() } }
        }
    }
}

/// Whether `uri` is a document of the page's own origin: the only navigation the page is let
/// make, and the only source a message is accepted from.
pub fn own_origin(uri: &str, origin: &str) -> bool {
    let n = origin.len();
    uri.get(..n).is_some_and(|p| p.eq_ignore_ascii_case(origin)) && uri.as_bytes().get(n) == Some(&b'/')
}

/// The browser switches that open a page to other programs or take its protection away, with
/// what each does: said on a line of their own when an environment variable adds them, so no
/// length of harmless switches before them hides one.
const DANGEROUS_SWITCHES: [(&str, &str); 10] = [
    ("--remote-debugging-port", "opens a port through which any local program can drive the pages"),
    ("--remote-debugging-pipe", "hands another program the pages' debugging channel"),
    ("--remote-allow-origins", "lets web pages use the debugging port"),
    ("--no-sandbox", "turns the renderer's sandbox off"),
    ("--disable-web-security", "turns the same-origin rule off"),
    ("--renderer-cmd-prefix", "starts every renderer through another program"),
    ("--utility-cmd-prefix", "starts every utility process through another program"),
    ("--gpu-launcher", "starts the GPU process through another program"),
    ("--browser-subprocess-path", "starts the pages' processes from another program"),
    ("--load-extension", "loads a browser extension into the pages"),
];

/// `text` with every control character written out (`\n`, `\u{1b}`), so a value cannot put a
/// line of its own into the log.
pub fn escape_controls(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { c.escape_default().to_string() } else { c.to_string() })
        .collect()
}

/// The environment variables WebView2 reads on its own, as lines for the log: each set one, with
/// its whole value (control characters written out) and what it does, and one line more for
/// each [dangerous switch](DANGEROUS_SWITCHES) an arguments variable carries. Sorted, so two logs
/// compare line by line.
///
/// Microsoft's loader reads these whatever the application passes: `…_ADDITIONAL_BROWSER_ARGUMENTS`
/// is appended to our arguments, `…_USER_DATA_FOLDER` and `…_BROWSER_EXECUTABLE_FOLDER` replace
/// ours, and the rest choose a runtime or attach a debugger. Any process of the same user can set
/// them, so the host says so — it logs, it does not refuse (the maintainer's decision, TODO.md).
/// The registry policies of the same meaning need an administrator to set, and are not read.
pub fn webview2_overrides(vars: impl IntoIterator<Item = (String, String)>) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for (k, v) in vars {
        if !(k.len() > 9 && k.get(..9).is_some_and(|p| p.eq_ignore_ascii_case("WEBVIEW2_"))) {
            continue;
        }
        let upper = k.to_ascii_uppercase();
        let what = match upper.as_str() {
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS" => "added to the browser's arguments",
            "WEBVIEW2_USER_DATA_FOLDER" => "replaces the data folder beside the application",
            "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER" => "replaces the installed runtime",
            "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE" | "WEBVIEW2_CHANNEL_SEARCH_KIND" => "chooses which runtime is used",
            "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER" | "WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER" => "attaches a script debugger",
            _ => "read by WebView2",
        };
        let key = escape_controls(&k);
        lines.push(format!("{key}={} ({what})", escape_controls(&v)));
        if upper == "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS" {
            for arg in v.split_whitespace() {
                let arg = arg.trim_matches(['"', '\'']).to_ascii_lowercase();
                for (switch, does) in DANGEROUS_SWITCHES {
                    let tail = arg.strip_prefix(switch);
                    if tail.is_some_and(|t| t.is_empty() || t.starts_with('=')) {
                        lines.push(format!("{key} carries {switch}, which {does}"));
                    }
                }
            }
        }
    }
    lines.sort();
    lines.dedup();
    lines
}

/// A rectangle in screen coordinates, as `host.window` gives them: physical pixels on Windows, and
/// points from the top left of the primary display on a Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn right(&self) -> i32 {
        self.x + self.w
    }
    fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// Whether the two share any pixel.
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    fn contains(&self, o: &Rect) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
    fn overlap(&self, o: &Rect) -> i64 {
        let w = (self.right().min(o.right()) - self.x.max(o.x)).max(0);
        let h = (self.bottom().min(o.bottom()) - self.y.max(o.y)).max(0);
        i64::from(w) * i64::from(h)
    }
}

/// The four places beside `avoid` on `area`, for a page of `w` by `h`: right, left, below and
/// above, each lined up with `avoid` and pushed inside the area along the other axis.
fn beside(avoid: Rect, area: Rect, w: i32, h: i32) -> [Rect; 4] {
    let y = avoid.y.clamp(area.y, (area.bottom() - h).max(area.y));
    let x = avoid.x.clamp(area.x, (area.right() - w).max(area.x));
    [
        Rect { x: avoid.right(), y, w, h },
        Rect { x: avoid.x - w, y, w, h },
        Rect { x, y: avoid.bottom(), w, h },
        Rect { x, y: avoid.y - h, w, h },
    ]
}

/// Where [`place`] put the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Clear of the rectangle to avoid.
    Beside(Rect),
    /// Over part of it: nowhere was there room beside it (a maximised or full-screen plug-in
    /// window). `covered` is how many of its pixels the page covers.
    Over { rect: Rect, covered: i64 },
}

impl Placement {
    pub fn rect(&self) -> Rect {
        match *self {
            Placement::Beside(r) | Placement::Over { rect: r, .. } => r,
        }
    }
}

/// Where a page goes so that it does not cover `avoid`, given the monitors' work areas, and at
/// which size — [`beside_only`] — and where it goes when nothing is clear of `avoid`: at `min`,
/// in the corner of the monitor `avoid` is mostly on (the first monitor when it is on none)
/// where it covers the least of it, ties going to the bottom right, then the top right, the
/// bottom left and the top left. A plug-in draws its interface from the top left of its window,
/// and a maximised window's extra room goes to the right and the bottom; a page no larger than it
/// has to be, in a corner, covers the least of what the plug-in shows. It is never refused: a
/// page that cannot be shown is a page the user cannot reach, which on 2026-09-28 left the
/// maintainer with a key that only answered "no room". `None` only with no work area at all.
///
/// A page over the plug-in is one the application's own screen reads and clicks would land on,
/// so it stays there only in front ([`over_refused`]), and every click into the plug-in and every
/// read of it must bring the plug-in window in front of the page first — the page is not topmost,
/// so the activated plug-in window is drawn over it (docs/api/gui.md).
pub fn place(size: (i32, i32), min: (i32, i32), avoid: Rect, work: &[Rect]) -> Option<Placement> {
    if let Some(r) = beside_only(size, min, avoid, work) {
        return Some(Placement::Beside(r));
    }
    let home = work
        .iter()
        .filter(|a| a.overlap(&avoid) > 0)
        .max_by_key(|a| a.overlap(&avoid))
        .or_else(|| work.first())?;
    let (w, h) = (min.0.min(home.w), min.1.min(home.h));
    let corners = [
        Rect { x: home.right() - w, y: home.bottom() - h, w, h },
        Rect { x: home.right() - w, y: home.y, w, h },
        Rect { x: home.x, y: home.bottom() - h, w, h },
        Rect { x: home.x, y: home.y, w, h },
    ];
    // `min_by_key` keeps the first of equal ones: the order above is the order of preference.
    let rect = corners.into_iter().min_by_key(|r| r.overlap(&avoid))?;
    Some(Placement::Over { rect, covered: rect.overlap(&avoid) })
}

/// Where a page goes so that it does not cover `avoid`. At `size` first: on the monitor `avoid`
/// is mostly on, to its right, then its left, then below, then above — and failing all four,
/// centred on another monitor. Then smaller, on `avoid`'s own monitor: the side with the most
/// room, the page as large as that room allows up to `size`, and not smaller than `min`. Then
/// smaller on another monitor: as large as its work area allows up to `size`, not smaller than
/// `min`, centred — a small or portrait second screen. `None` when even that does not fit.
pub fn beside_only(size: (i32, i32), min: (i32, i32), avoid: Rect, work: &[Rect]) -> Option<Rect> {
    let fits = |r: &Rect, area: &Rect| area.contains(r) && !r.intersects(&avoid);
    let home = work
        .iter()
        .enumerate()
        .filter(|(_, a)| a.overlap(&avoid) > 0)
        .max_by_key(|(_, a)| a.overlap(&avoid))
        .map(|(i, _)| i);
    if let Some(i) = home {
        let area = work[i];
        if let Some(r) = beside(avoid, area, size.0, size.1).into_iter().find(|r| fits(r, &area)) {
            return Some(r);
        }
    }
    let elsewhere = work
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != home)
        .map(|(_, a)| (centred(size, a), a))
        .find(|(r, a)| fits(r, a))
        .map(|(r, _)| r);
    if elsewhere.is_some() {
        return elsewhere;
    }
    // Smaller, beside it on its own monitor: the room each side leaves, the page no larger
    // than `size` and no smaller than `min`, and the largest of the four.
    if let Some(i) = home {
        let area = work[i];
        let room = [
            (area.right() - avoid.right(), area.h),
            (avoid.x - area.x, area.h),
            (area.w, area.bottom() - avoid.bottom()),
            (area.w, avoid.y - area.y),
        ];
        let mut best: Option<Rect> = None;
        for (side, (rw, rh)) in room.into_iter().enumerate() {
            let (w, h) = (size.0.min(rw), size.1.min(rh));
            if w < min.0 || h < min.1 {
                continue;
            }
            let r = beside(avoid, area, w, h)[side];
            let larger = best.is_none_or(|b| i64::from(w) * i64::from(h) > i64::from(b.w) * i64::from(b.h));
            if fits(&r, &area) && larger {
                best = Some(r);
            }
        }
        if best.is_some() {
            return best;
        }
    }
    // Smaller, on another monitor: as large as its work area allows, centred on it.
    work.iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != home)
        .map(|(_, a)| (centred((size.0.min(a.w), size.1.min(a.h)), a), a))
        .find(|(r, a)| r.w >= min.0 && r.h >= min.1 && fits(r, a))
        .map(|(r, _)| r)
}

/// `size` centred in `area`.
pub fn centred(size: (i32, i32), area: &Rect) -> Rect {
    Rect { x: area.x + (area.w - size.0) / 2, y: area.y + (area.h - size.1) / 2, w: size.0, h: size.1 }
}

/// Which window a page is in: `host.gui.webview`'s `window` option. Chosen when the page is
/// described, not at each `show`, because it cannot change under a window that exists: Windows
/// wants a window hidden before its taskbar button is given or taken away, and a Mac's panel and
/// window are two classes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowKind {
    /// `"tool"`, the default: a page beside a plug-in, reached by a key the module offers, and in
    /// no list of windows, so the user's Alt+Tab and Command-Tab stay the DAW's (the research of
    /// 2026-09-28).
    #[default]
    Tool,
    /// `"app"`: a window like any program's — in the system's lists of windows, minimised,
    /// maximised and closed as any is — for a module whose page is an interface of its own rather
    /// than a companion of the plug-in window (the maintainer's decision of 2026-10-11, after W6).
    App,
}

impl WindowKind {
    /// The option's words, in the order the error names them.
    pub const NAMES: [&'static str; 2] = ["tool", "app"];

    /// The kind the option names, or `None`. Exactly these words, in lower case.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tool" => Some(WindowKind::Tool),
            "app" => Some(WindowKind::App),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            WindowKind::Tool => "tool",
            WindowKind::App => "app",
        }
    }

    /// What the window is on each platform.
    pub fn recipe(self) -> Recipe {
        match self {
            WindowKind::Tool => Recipe { listed: false, minimise: false, maximise: false, panel: true },
            WindowKind::App => Recipe { listed: true, minimise: true, maximise: true, panel: false },
        }
    }

    /// Whether a page leaving the front — hidden, closed by the user or by its module, or its
    /// module disabled or reloaded while it is in front — first gives the front back to the
    /// window it took it from. A tool page always: the module's key is the only way into it, so
    /// where the user was is where the page was brought from. An app page only when it has kept
    /// the front since it last took it (`kept`): the user reaches it with Alt+Tab or the taskbar
    /// as well, and one they came to from somewhere else is left as any program's window is —
    /// the system hands the front on.
    pub fn gives_front_back(self, kept: bool) -> bool {
        match self {
            WindowKind::Tool => true,
            WindowKind::App => kept,
        }
    }

    /// Whether a page of this kind can be described with `host.gui.webview`'s option `activates`
    /// (experimental; the maintainer's decision of 2026-10-11): a tool page either way — `false`
    /// asks a Mac for a panel that takes the keyboard without making the application the active
    /// one, so the DAW stays the active application, and changes nothing on Windows — and an app
    /// page only as `true`, its default: a window like any program's activates its application
    /// when it takes the keyboard, and one that did not would be in Command-Tab and the Dock under
    /// an application that never comes to the front.
    pub fn allows_activates(self, activates: bool) -> bool {
        activates || self == WindowKind::Tool
    }
}

/// What a page's window is, on both platforms: one decision, which `webview_win.rs` builds on
/// Windows and `backend/macos/page.rs` on a Mac, so the two cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// In the system's lists of windows. Windows: an Alt+Tab entry with the page's title, and a
    /// place on the taskbar — the application's own button, where Windows combines a program's
    /// windows, as Windows 11 does by default — no `WS_EX_TOOLWINDOW`. macOS: Command-Tab and the
    /// Dock list applications, not windows, and this application has no Dock icon; while such a
    /// page is shown, the application is a regular one, by a claim of its own through `dock.rs`,
    /// as the module manager's window makes one, under the setting `dock_while_open`.
    pub listed: bool,
    /// A minimise box (Windows), a miniaturise button (macOS).
    pub minimise: bool,
    /// A maximise box (Windows). A Mac's titled, resizable window has its zoom button either way;
    /// an app page's zooms it and never takes it into full screen (backend/macos/page.rs).
    pub maximise: bool,
    /// macOS: an `NSPanel` — a utility panel in no window list, floating while it has the keyboard
    /// and at the normal level otherwise — rather than an ordinary `NSWindow`. Either activates
    /// the application when it takes the keyboard (the maintainer's decision of 2026-10-11, the
    /// VPS Avenger control tool's way); a panel that does not is the experimental page option
    /// `activates = false` ([`WindowKind::allows_activates`]), which the panel is made with.
    pub panel: bool,
}

/// Whether a Mac's page waits for the keyboard before it goes where [`place`] put it. An app page
/// the user has open goes over `avoid` only once it is in front, so that one the system does not
/// bring there stays where the user left it (docs/api/gui.md); on Windows the answer to the request
/// for the front comes first, and the page is put there after it. On a Mac the answer comes later
/// (backend/macos/page.rs), so the page can be put while its request has no answer yet
/// (`front_asked`): a place over `avoid` then waits for the window to get the keyboard, kept with
/// the request and forgotten with it — not got, or a newer request made first. Anything else — a
/// tool page, a place beside `avoid`, a page whose request was answered — goes at once.
#[cfg(any(test, target_os = "macos"))]
pub fn place_when_key(kind: WindowKind, plan: &Placement, front_asked: bool) -> bool {
    kind == WindowKind::App && front_asked && matches!(plan, Placement::Over { .. })
}

/// Whether a Mac's page whose show did not get the front is hidden again: Windows' rule for a show
/// declined over `avoid` ([`over_refused`]), decided when the event says so (backend/macos/page.rs).
/// `over`: the show placed it over `avoid` — never a `focus`, which places nothing, so that a page
/// an earlier show left there is not hidden for it, as on Windows; `open`: it was shown before the
/// show. A tool page over `avoid` is hidden, and so is an app page the show brought there; an app
/// page the user has open is the user's window and stays.
#[cfg(any(test, target_os = "macos"))]
pub fn not_got_hides(kind: WindowKind, over: bool, open: bool) -> bool {
    over && (kind == WindowKind::Tool || !open)
}

/// A page's size: on Windows at 96 dots per inch, scaled by the monitor's; on a Mac in points,
/// which AppKit scales itself.
pub const DEFAULT_SIZE: (i32, i32) = (520, 640);

/// The smallest a page is made to fit beside the rectangle to avoid, in the units of
/// [`DEFAULT_SIZE`].
pub const MIN_SIZE: (i32, i32) = (320, 360);

/// The engine a page runs in, for the parts of its [`platform_shim`] that differ, and for the words
/// the log says a page's life in ([`Engine::words`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    /// WebView2, on Windows (`webview_win.rs`): the host's messages arrive as `chrome.webview`'s
    /// `message` events. Never made on a Mac, where no WebView2 page is built.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    WebView2,
    /// WebKit, on a Mac (`backend/macos/page.rs`): the host's messages arrive as JSON text through
    /// `window.__platformDeliver` ([`WEBKIT_DELIVER`]), and the keyboard is put back where it was
    /// in the page when its window takes the keyboard ([`WEBKIT_RESTORE_FOCUS`]). Built on a Mac only
    /// (and in the tests).
    #[cfg(any(test, target_os = "macos"))]
    WebKit,
}

impl Engine {
    /// This platform's: WebKit on a Mac.
    #[cfg(target_os = "macos")]
    pub fn here() -> Engine {
        Engine::WebKit
    }

    /// This platform's: WebView2 everywhere but a Mac — on Windows, and where no page is built at
    /// all and nothing asks.
    #[cfg(not(target_os = "macos"))]
    pub fn here() -> Engine {
        Engine::WebView2
    }

    /// The words `webview_lua.rs` says a page's life in, for this engine. Windows' are the lines
    /// stage 1 wrote, word for word (a test holds them to that), so the Windows probe's log reads
    /// as it did.
    pub fn words(self) -> Words {
        match self {
            Engine::WebView2 => Words {
                system: "Windows",
                env: "WebView2's environment",
                env_there: "WebView2's environment is there",
                env_failed: "WebView2's environment could not be created",
                building: "building in WebView2's environment — no module callback runs until WebView2 has made the page",
                front_taken: "shown and brought to the front (accepted)",
                focus_taken: "accepted",
                not_activating: "activates = false, which changes nothing on Windows: a window there takes the \
                                 keyboard and the front together",
            },
            #[cfg(any(test, target_os = "macos"))]
            Engine::WebKit => Words {
                system: "macOS",
                env: "the pages' rule list",
                env_there: "the pages' rule list is compiled",
                env_failed: "the pages' rule list could not be compiled",
                building: "building with WebKit — no module callback runs until the web view is made",
                front_taken: "shown, and the front asked for (the page's own lines say whether it came)",
                focus_taken: "asked for (the page's own lines say whether the front came)",
                not_activating: "activates = false: a panel that takes the keyboard without making this application \
                                 the active one (experimental)",
            },
        }
    }
}

/// What the log calls the parts of a page's life that differ between the engines ([`Engine::words`]).
/// On a Mac the part of a first build that is not waited for is the content rule list, where
/// Windows has WebView2's environment, and taking the front is asked for, not answered at once:
/// whether it came is an event, said by the page window itself (backend/macos/page.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Words {
    /// The system that decides whether a page comes to the front: `"Windows"`, `"macOS"`.
    pub system: &'static str,
    /// What a first build asks for and does not wait for.
    pub env: &'static str,
    /// That, once it is there: what a build queued by `show` or `prepare` waits for.
    pub env_there: &'static str,
    /// That, when it failed.
    pub env_failed: &'static str,
    /// The line before the part of a build the event loop waits for.
    pub building: &'static str,
    /// A `show` with `focus` that the system did not decline.
    pub front_taken: &'static str,
    /// A `focus` that the system did not decline.
    pub focus_taken: &'static str,
    /// A page described with `activates = false`, said when it is described: what that is here.
    pub not_activating: &'static str,
}

/// The script the host puts into every page before the page's own: `platform.post(value)` to
/// send the module a value, and `platform.onmessage = function (value) … end` to receive what
/// the module posts. It hides which engine carries the messages (WebView2's `chrome.webview` on
/// Windows, a function only the host calls on a Mac), so a page is written once.
///
/// Messages the module posts before the page has set `onmessage` wait for it, up to the same
/// cap the host keeps. A handler that throws does not stop the next message; the error is
/// rethrown on its own, where the page's console sees it. Main frame only: a page has no frames
/// (the policy forbids them), and a frame is not where a message may come from.
///
/// And `window.close()` does nothing: in WebView2 it takes the web view down and leaves its
/// window empty, and a page is closed by its module (`page:close()`), not by itself. Should a
/// page get round this, the host treats it as a page that failed (webview_win.rs).
///
/// Built from parts the two engines share, so the one cannot drift from the other; Windows' is
/// the very script stage 1 shipped, byte for byte (a test holds it to that), so no page of the
/// Windows probe sees a change. WebKit's adds two things:
///
/// - **The host's messages** come through `window.__platformDeliver(json)`, which only the host
///   calls ([`WEBKIT_DELIVER`]): a function the page cannot replace, with the message as an
///   argument and never as a script made from it.
/// - **The keyboard put back** where it was in the page when the page's window takes the keyboard:
///   the host calls `window.__platformRestoreFocus()` ([`WEBKIT_RESTORE_FOCUS`]) at AppKit's word
///   that the window became the key window, and the page's own window `focus` event calls it too —
///   whichever comes first, the second finds the work done. The element the keyboard was last on
///   gets it back if it is still in the page and shown; otherwise WebKit's own choice stands, which
///   is gui.md's "the element that had the focus last time, or the document itself". From then on
///   a guard watches, until the user's first key, pointer press or click in the page, or until the
///   keyboard moves to another element: the keyboard going to nothing — the document's body —
///   with the document still focused and no input meanwhile is put back once for each time it
///   goes, and said in the host's log ([`SHIM_NOTE`]). A move to another element is never undone:
///   with VoiceOver's "keyboard focus follows the VoiceOver cursor", the user's own navigation
///   moves the page's focus with no key reaching the page. Events only; no timer decides anything.
pub fn platform_shim(engine: Engine) -> String {
    let cap = MAX_QUEUED_POSTS;
    let mut s = String::from(SHIM_HEAD);
    match engine {
        Engine::WebView2 => {
            s.push_str(
                "  const ipc = window.ipc;\n  const channel = window.chrome && window.chrome.webview;\n  if (!ipc || !channel) return;\n",
            );
            s.push_str(SHIM_HANDLER);
            s.push_str(&format!(
                "  channel.addEventListener(\"message\", (event) => {{\n    if (handler) hand(event.data);\n    else if (waiting.length < {cap}) waiting.push(event.data);\n  }});\n"
            ));
        }
        #[cfg(any(test, target_os = "macos"))]
        Engine::WebKit => {
            s.push_str("  const ipc = window.ipc;\n  if (!ipc) return;\n");
            s.push_str(SHIM_HANDLER);
            s.push_str(&format!(
                "  const take = (value) => {{\n    if (handler) hand(value);\n    else if (waiting.length < {cap}) waiting.push(value);\n  }};\n"
            ));
            s.push_str("  Object.defineProperty(window, \"__platformDeliver\", { value: (text) => take(JSON.parse(text)) });\n");
            s.push_str(SHIM_WEBKIT_FOCUS);
        }
    }
    s.push_str(SHIM_TAIL);
    s
}

/// The start of every [`platform_shim`]: the top frame only, once, and `window.close()` made to do
/// nothing.
const SHIM_HEAD: &str = r#"(() => {
  "use strict";
  if (window.top !== window || window.platform) return;
  try {
    Object.defineProperty(window, "close", { value: () => {}, writable: false, configurable: false });
  } catch (err) {}
"#;

/// The handler `onmessage` sets, the messages waiting for it, and the call that keeps one
/// handler's error from stopping the next message.
const SHIM_HANDLER: &str = r#"  let handler = null;
  const waiting = [];
  const hand = (value) => {
    try { handler(value); } catch (err) { setTimeout(() => { throw err; }); }
  };
"#;

/// WebKit's: the keyboard put back in the page and the guard (see [`platform_shim`]). Its notes
/// go to the host's log through the page's own message channel, behind [`SHIM_NOTE`].
#[cfg(any(test, target_os = "macos"))]
const SHIM_WEBKIT_FOCUS: &str = r#"  const note = (text) => {
    try { ipc.postMessage("\u0001platform: " + text); } catch (err) {}
  };
  let last = null;
  let armed = false;
  let lost = false;
  const usable = (el) => !!el && el.isConnected && el !== document.body && el.getClientRects().length > 0;
  document.addEventListener("focusin", (event) => {
    if (event.target !== last) armed = false;
    last = event.target;
    lost = false;
  }, true);
  const disarm = (event) => {
    if (event.isTrusted) armed = false;
  };
  for (const type of ["keydown", "pointerdown", "click"]) document.addEventListener(type, disarm, true);
  document.addEventListener("focusout", (event) => {
    if (!armed || lost || event.target !== last || event.relatedTarget !== null) return;
    lost = true;
    queueMicrotask(() => {
      const nothing = document.activeElement === null || document.activeElement === document.body;
      if (!armed || !lost || !nothing || !document.hasFocus() || !usable(last)) return;
      last.focus();
      note("the keyboard went to nothing with no input in the page, and is back on the element that had it");
    });
  }, true);
  const restore = () => {
    armed = true;
    if (!usable(last)) return "where WebKit put it: no element had it before, or that one is gone or hidden";
    if (document.activeElement === last) return "on the element that had it";
    last.focus();
    return "back on the element that had it";
  };
  Object.defineProperty(window, "__platformRestoreFocus", { value: restore });
  window.addEventListener("focus", () => { restore(); });
"#;

/// The end of every [`platform_shim`]: the `platform` object itself, frozen.
const SHIM_TAIL: &str = r#"  const platform = Object.create(null);
  Object.defineProperties(platform, {
    post: {
      value: (value) => {
        ipc.postMessage(JSON.stringify(value === undefined ? null : value));
      },
    },
    onmessage: {
      get: () => handler,
      set: (fn) => {
        handler = typeof fn === "function" ? fn : null;
        while (handler && waiting.length) hand(waiting.shift());
      },
    },
  });
  Object.freeze(platform);
  Object.defineProperty(window, "platform", { value: platform });
})();"#;

/// What the host has WebKit run to hand a page one message: the body of an asynchronous function
/// whose one argument, `m`, is the JSON text the host encoded — data, never part of the script
/// (`callAsyncJavaScript`, backend/macos/page.rs). [`platform_shim`] defines the function it calls.
#[cfg(any(test, target_os = "macos"))]
pub const WEBKIT_DELIVER: &str = "return window.__platformDeliver(m);";

/// What the host has WebKit run when a page's window becomes the key window: the keyboard put
/// back where it was in the page ([`platform_shim`]). Its answer says where the keyboard is, for
/// the log; a document without the shim — none of the page's own — says so.
#[cfg(any(test, target_os = "macos"))]
pub const WEBKIT_RESTORE_FOCUS: &str =
    "return window.__platformRestoreFocus ? window.__platformRestoreFocus() : \"not a document of the page's own\";";

/// What a note of the WebKit shim's own begins with, on the page's message channel: a control
/// character no message of `platform.post` can begin with — that is always `JSON.stringify`'s
/// text — so the host tells the two apart ([`shim_note`]) and a note never reaches the module.
#[cfg(any(test, target_os = "macos"))]
pub const SHIM_NOTE: &str = "\u{1}platform: ";

/// The shim's note in a message the page's channel carried, or `None` for a message of the
/// page's own.
#[cfg(any(test, target_os = "macos"))]
pub fn shim_note(text: &str) -> Option<&str> {
    text.strip_prefix(SHIM_NOTE)
}

/// The folder WebView2 keeps its data in: beside the application, like its settings and its log.
pub const DATA_FOLDER: &str = "WebView2";

/// The folder beside the application a Mac keeps its page rules in ([`RULE_LIST`], compiled by
/// WebKit into a store of the host's own rather than WebKit's default one under `~/Library`), as
/// Windows keeps WebView2's data in [`DATA_FOLDER`].
#[cfg(any(test, target_os = "macos"))]
pub const RULE_FOLDER: &str = "WebKitRules";

/// The name the compiled [`RULE_LIST`] is kept under in its store. Compiled again at every start,
/// so a changed list replaces the one kept.
#[cfg(any(test, target_os = "macos"))]
pub const RULE_LIST_ID: &str = "pages";

/// The Mac's "403 for everything else": a WebKit content rule list every page's web view is built
/// with (backend/macos/page.rs), which blocks every load — a document, a script, an image, a
/// `fetch` — but the pages' own schemes ([`scheme_for`]: `mod-` and then lower-case letters,
/// digits and hyphens, served at `localhost`) and `data:` URLs, which the policy allows for images.
/// WebKit has no request filter of the kind WebView2's that answers 403 on Windows; this blocks
/// before a request leaves, whatever the page's policy lets through. Another module's scheme is
/// let past it and goes nowhere: a web view serves only its own module's. Like the 403, it does not
/// cover WebRTC or the engine's own look-ahead (TODO.md, stage 3). A list WebKit does not compile
/// is a page that is not built.
#[cfg(any(test, target_os = "macos"))]
pub const RULE_LIST: &str = r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^mod-[a-z0-9-]+://localhost/"},"action":{"type":"ignore-previous-rules"}},{"trigger":{"url-filter":"^data:"},"action":{"type":"ignore-previous-rules"}}]"#;

/// Where a page leaving the front gives it, on Windows: `back_to`, the window it took the front
/// from — or, when that is a page of the application's hidden since (the one this page was shown
/// in place of), the window that page came from, and so on: a module that shows one page in place
/// of another, the new one first, so the front goes from page to page and never through the
/// plug-in window, still lands in the plug-in window when the new one leaves. `page` answers for a
/// window that is a page: whether it is shown, and the window it came from (0 until it took the
/// front). Never `this` page, never a window none came from; at most `pages` steps, so pages that
/// came from each other end. Window ids are HWNDs, and 0 is none. A Mac's page follows the same
/// chain with each window's application as well ([`front_goes_to_app`]).
// Not called on a Mac, which has `front_goes_to_app`.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn front_goes_to(this: isize, back_to: isize, pages: usize, page: impl Fn(isize) -> Option<(bool, isize)>) -> isize {
    let mut to = back_to;
    for _ in 0..pages {
        match page(to) {
            Some((false, from)) if from != 0 && from != this => to = from,
            _ => break,
        }
    }
    to
}

/// A Mac's [`front_goes_to`], with `host.window`'s ids: the window and its application, as
/// `(window, pid)`. `page` answers for a window that is a page: whether it is shown, and where it
/// came from — `None` until it took the front. Where a page came from an application whose window
/// could not be named (window 0: accessibility did not answer), the chain ends on that application,
/// whose own choice of window then stands; stopping on the page before it instead would end on a
/// window of this application's that is hidden, and the front would go nowhere. Otherwise the same
/// chain: never through a page still shown, one that never took the front, or this page itself, and
/// at most `pages` steps.
#[cfg(any(test, target_os = "macos"))]
pub fn front_goes_to_app(
    this: isize,
    came: (isize, i32),
    pages: usize,
    page: impl Fn(isize) -> Option<(bool, Option<(isize, i32)>)>,
) -> (isize, i32) {
    let mut to = came;
    for _ in 0..pages {
        if to.0 == 0 {
            break;
        }
        match page(to.0) {
            Some((false, Some(from))) if from.0 != this => to = from,
            _ => break,
        }
    }
    to
}

/// An Edit command a Mac's page window sends to the first responder ([`MacKey::Edit`]), as an Edit
/// menu's item would — there is none: the host's menus have no Edit menu, and a tool page is in
/// an application whose menu bar is not shown.
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditCommand {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

#[cfg(any(test, target_os = "macos"))]
impl EditCommand {
    /// The action's selector, as AppKit names it: the one the page window sends
    /// (backend/macos/page.rs).
    pub fn selector(self) -> &'static std::ffi::CStr {
        match self {
            EditCommand::Cut => c"cut:",
            EditCommand::Copy => c"copy:",
            EditCommand::Paste => c"paste:",
            EditCommand::SelectAll => c"selectAll:",
            EditCommand::Undo => c"undo:",
            EditCommand::Redo => c"redo:",
        }
    }
}

/// What a key equivalent does in a Mac's page window once the page and WebKit have left it
/// unhandled ([`mac_key`]).
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacKey {
    /// Not one of the page's: AppKit goes on as it would — the main menu, then the system.
    Pass,
    /// Taken, and nothing done with it: Command-Q, Command-H and Option-Command-H in a tool page.
    Nothing,
    /// What the close box does: the page hidden, the front given back.
    Close,
    /// An Edit command sent to the first responder: the field the keyboard is in.
    Edit(EditCommand),
    /// The window miniaturised (an app page).
    Miniaturise,
    /// The application quits, by the host's own way out (an app page).
    Quit,
    /// The application hides, as any does (an app page).
    Hide,
    /// The other applications hide (an app page).
    HideOthers,
}

/// What a key equivalent does in a page of `kind` on a Mac, from the key's characters as AppKit
/// gives them without the modifiers (`charactersIgnoringModifiers`; Shift still applies) and the
/// modifiers held. Asked only after the page and WebKit have left the key unhandled, so a page's
/// own `keydown` handler sees every key first. Decided here, and not by whichever main menu
/// wxWidgets last installed — the module manager's, with Close and Quit, or none at all before the
/// manager was first opened — so a page's keys do not depend on whether the manager was ever
/// open:
///
/// - **Both kinds:** Command-W does what the close box does; Command-X, C, V, A and Z and
///   Shift-Command-Z cut, copy, paste, select all, undo and redo in the field the keyboard is in.
/// - **A tool page:** Command-Q, Command-H and Option-Command-H do nothing (the maintainer's
///   decision of 2026-10-11): a VoiceOver user in a companion panel reaches for the DAW's
///   shortcuts, and quitting the application by accident takes every overlay away.
/// - **An app page**, a window like any application's: Command-M miniaturises it, Command-Q quits
///   the application by the host's own way out, Command-H hides it and Option-Command-H hides the
///   others.
///
/// Every other key, and anything with Control held, is AppKit's as before.
#[cfg(any(test, target_os = "macos"))]
pub fn mac_key(kind: WindowKind, chars: &str, command: bool, shift: bool, option: bool, control: bool) -> MacKey {
    if !command || control {
        return MacKey::Pass;
    }
    let app = kind == WindowKind::App;
    let plain = !shift && !option;
    match chars.to_ascii_lowercase().as_str() {
        "w" if plain => MacKey::Close,
        "x" if plain => MacKey::Edit(EditCommand::Cut),
        "c" if plain => MacKey::Edit(EditCommand::Copy),
        "v" if plain => MacKey::Edit(EditCommand::Paste),
        "a" if plain => MacKey::Edit(EditCommand::SelectAll),
        "z" if !option => MacKey::Edit(if shift { EditCommand::Redo } else { EditCommand::Undo }),
        "m" if plain && app => MacKey::Miniaturise,
        "q" if !shift => {
            if app {
                MacKey::Quit
            } else {
                MacKey::Nothing
            }
        }
        "h" if !shift => match (app, option) {
            (false, _) => MacKey::Nothing,
            (true, false) => MacKey::Hide,
            (true, true) => MacKey::HideOthers,
        },
        _ => MacKey::Pass,
    }
}

/// [`mac_key`] for a key event as AppKit hands it over: by its characters without the modifiers
/// (`ignoring`, `charactersIgnoringModifiers`) first, and, where the table has nothing for those,
/// by the characters the key types with the modifiers held (`typed`, `characters`). A keyboard
/// layout whose letters are not Latin types Latin ones while Command is held — a Russian layout's
/// Command-Й is Command-Q — and AppKit matches a menu's key equivalents by those, so a tool page's
/// Command-Q is nothing under such a layout as well, and not the Quit item of whichever menu is
/// installed.
#[cfg(any(test, target_os = "macos"))]
pub fn mac_key_of(
    kind: WindowKind,
    ignoring: &str,
    typed: &str,
    command: bool,
    shift: bool,
    option: bool,
    control: bool,
) -> MacKey {
    match mac_key(kind, ignoring, command, shift, option, control) {
        MacKey::Pass => mac_key(kind, typed, command, shift, option, control),
        key => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: Where = Where { headless: false, supported: true };

    #[test]
    fn a_headless_run_and_an_unsupported_platform_answer_first() {
        let mut l = Lifecycle::default();
        let asked = std::cell::Cell::new(false);
        let step = l.on_show(Where { headless: true, supported: true }, || {
            asked.set(true);
            true
        });
        assert_eq!(step, ShowStep::Refuse(Refusal::Headless));
        assert_eq!(Refusal::Headless.reason(), "headless");
        // Headless wins over an unsupported platform: a headless Mac says headless.
        assert_eq!(
            l.on_show(Where { headless: true, supported: false }, || true),
            ShowStep::Refuse(Refusal::Headless)
        );
        let step = l.on_show(Where { headless: false, supported: false }, || true);
        assert_eq!(step, ShowStep::Refuse(Refusal::Unsupported));
        assert_eq!(Refusal::Unsupported.reason(), "not supported on this platform yet");
        assert!(!asked.get(), "the runtime is not asked where no page can be built");
        assert_eq!(l.phase(), Phase::Idle, "nothing was started");
    }

    /// Windows and macOS have a page window, and nothing else does: the platform Rust names, as
    /// `here` asks it.
    #[test]
    fn pages_are_shown_on_windows_and_macos() {
        assert!(Where::supports("windows"));
        assert!(Where::supports("macos"), "the Mac's page window is what host.gui.webview shows there");
        for other in ["linux", "freebsd", "ios", "Macos", ""] {
            assert!(!Where::supports(other), "{other}");
        }
        assert_eq!(Where::here(false).supported, cfg!(any(windows, target_os = "macos")));
        assert!(!Where::here(true).can_show());
    }

    /// Windows' words are stage 1's, word for word, so the probe's log reads as it did; WebKit's
    /// name neither WebView2 nor Windows, and say that taking the front is only asked for — a Mac's
    /// page has its answer later (backend/macos/page.rs).
    #[test]
    fn each_engine_says_a_page_s_life_in_its_own_words() {
        let w = Engine::WebView2.words();
        assert_eq!(w.system, "Windows");
        assert_eq!(w.env, "WebView2's environment");
        assert_eq!(w.env_there, "WebView2's environment is there");
        assert_eq!(w.env_failed, "WebView2's environment could not be created");
        assert_eq!(w.building, "building in WebView2's environment — no module callback runs until WebView2 has made the page");
        assert_eq!(w.front_taken, "shown and brought to the front (accepted)");
        assert_eq!(w.focus_taken, "accepted");
        // Said only for a page described `activates = false`, which stage 1 had not: no line of
        // its changes. On Windows it says the option does nothing there.
        assert!(w.not_activating.starts_with("activates = false") && w.not_activating.contains("nothing on Windows"));
        let k = Engine::WebKit.words();
        let all = [k.system, k.env, k.env_there, k.env_failed, k.building, k.front_taken, k.focus_taken, k.not_activating];
        for word in all {
            assert!(!word.contains("WebView2") && !word.contains("Windows"), "{word}");
        }
        assert_eq!(k.system, "macOS");
        assert!(k.front_taken.contains("asked for") && k.focus_taken.contains("asked for"));
        assert!(!k.front_taken.contains("accepted"), "{}", k.front_taken);
        assert!(k.not_activating.starts_with("activates = false") && k.not_activating.contains("experimental"));
        assert_eq!(Engine::here(), if cfg!(target_os = "macos") { Engine::WebKit } else { Engine::WebView2 });
    }

    #[test]
    fn a_missing_runtime_is_asked_again_at_every_show() {
        let mut l = Lifecycle::default();
        assert_eq!(l.on_show(HERE, || false), ShowStep::Refuse(Refusal::RuntimeMissing));
        assert_eq!(Refusal::RuntimeMissing.reason(), "WebView2 runtime not installed");
        assert_eq!(l.phase(), Phase::Idle);
        // Installed meanwhile: the next show builds.
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
    }

    #[test]
    fn the_first_show_builds_and_the_ones_after_wait_for_the_page() {
        let mut l = Lifecycle::default();
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
        assert_eq!(l.phase(), Phase::Building);
        let asked = std::cell::Cell::new(0);
        let count = || {
            asked.set(asked.get() + 1);
            true
        };
        assert_eq!(l.on_show(HERE, count), ShowStep::Refuse(Refusal::NotReady));
        assert_eq!(Refusal::NotReady.reason(), "not ready yet");
        l.built(true);
        assert_eq!(l.phase(), Phase::Loading);
        assert_eq!(l.on_show(HERE, || true), ShowStep::Refuse(Refusal::NotReady));
        assert_eq!(l.loaded(), Some(Vec::new()));
        assert_eq!(l.on_show(HERE, || true), ShowStep::Show);
        assert_eq!(asked.get(), 0, "no second build is started while one runs");
    }

    #[test]
    fn a_failed_build_is_said_once_and_the_next_show_tries_again() {
        let mut l = Lifecycle::default();
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
        l.built(false);
        assert_eq!(l.phase(), Phase::Failed);
        assert_eq!(l.on_show(HERE, || true), ShowStep::Refuse(Refusal::BuildFailed));
        assert_eq!(Refusal::BuildFailed.reason(), "could not be built");
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
    }

    #[test]
    fn messages_wait_for_the_page_in_order_and_then_go_at_once() {
        let mut l = Lifecycle::default();
        assert_eq!(l.post("1".into()), None);
        l.on_show(HERE, || true);
        assert_eq!(l.post("2".into()), None);
        l.built(true);
        assert_eq!(l.post("3".into()), None);
        assert_eq!(l.queued(), 3);
        assert_eq!(l.loaded(), Some(vec!["1".to_string(), "2".to_string(), "3".to_string()]));
        assert_eq!(l.queued(), 0);
        assert_eq!(l.post("4".into()), Some("4".to_string()));
        // A reload of the page reports loaded again, with nothing waiting.
        assert_eq!(l.loaded(), Some(Vec::new()));
    }

    #[test]
    fn the_oldest_waiting_message_gives_way_past_the_cap() {
        let mut l = Lifecycle::default();
        for i in 0..MAX_QUEUED_POSTS + 3 {
            l.post(i.to_string());
        }
        assert_eq!(l.queued(), MAX_QUEUED_POSTS);
        assert_eq!(l.take_dropped(), 3);
        assert_eq!(l.take_dropped(), 0, "said once");
        l.on_show(HERE, || true);
        l.built(true);
        let got = l.loaded().unwrap();
        assert_eq!(got.first().map(String::as_str), Some("3"));
        assert_eq!(got.last().map(String::as_str), Some(&*(MAX_QUEUED_POSTS + 2).to_string()));
    }

    #[test]
    fn a_closed_page_drops_what_waited_and_builds_again() {
        let mut l = Lifecycle::default();
        l.on_show(HERE, || true);
        l.post("x".into());
        l.closed();
        assert_eq!(l.phase(), Phase::Idle);
        assert_eq!(l.queued(), 0);
        // A build that finishes for the closed page changes nothing, and its load delivers
        // nothing.
        l.built(true);
        assert_eq!(l.phase(), Phase::Idle);
        assert_eq!(l.loaded(), None);
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
    }

    /// A first document that does not load — its file deleted after the page was described, say
    /// — is a build that failed: said once, then built again. A ready page that fails a later
    /// load (a link it followed, since refused) stays ready.
    #[test]
    fn a_failed_first_load_counts_as_a_failed_build() {
        let mut l = Lifecycle::default();
        l.on_show(HERE, || true);
        l.post("waiting".into());
        l.built(true);
        assert!(l.load_failed(), "the first document");
        assert_eq!(l.phase(), Phase::Failed);
        assert_eq!(l.queued(), 0, "what waited goes with the page");
        assert_eq!(l.loaded(), None, "a late load of the failed page delivers nothing");
        assert_eq!(l.on_show(HERE, || true), ShowStep::Refuse(Refusal::BuildFailed));
        assert_eq!(l.on_show(HERE, || true), ShowStep::Build);
        l.built(true);
        assert_eq!(l.loaded(), Some(Vec::new()));
        assert!(!l.load_failed(), "a ready page stays ready");
        assert_eq!(l.phase(), Phase::Ready);
        let mut idle = Lifecycle::default();
        assert!(!idle.load_failed(), "nothing is built");
        assert_eq!(idle.phase(), Phase::Idle);
    }

    #[test]
    fn every_refusal_has_its_own_words() {
        let all = [
            Refusal::Headless,
            Refusal::Unsupported,
            Refusal::NotReady,
            Refusal::RuntimeMissing,
            Refusal::BuildFailed,
            Refusal::Declined,
            Refusal::NoRoom,
        ];
        let mut words: std::collections::HashSet<&str> = all.iter().map(|r| r.reason()).collect();
        assert_eq!(words.len(), all.len());
        assert_eq!(Refusal::Declined.reason(), "declined by the system");
        assert_eq!(Refusal::NoRoom.reason(), "no room beside the rectangle to avoid");
        assert!(words.insert(OVER), "the note of a success is no refusal's words");
        assert_eq!(OVER, "over the rectangle to avoid");
    }

    #[test]
    fn a_scheme_is_a_host_label_and_differs_per_module() {
        let a = scheme_for("com.tool.html-probe");
        assert!(a.starts_with("mod-com-tool-html-probe-"), "{a}");
        assert_eq!(a, scheme_for("com.tool.html-probe"), "stable");
        assert_ne!(scheme_for("com.a-b"), scheme_for("com.a.b"));
        // The pair the review of 2026-09-28 found for the 32-bit hash in seconds: the same first
        // 40 readable characters, and once the same origin.
        let victim = scheme_for("com.example.a-very-long-module-name-for-presets");
        let attacker = scheme_for("com.example.a-very-long-module-name-for-x034lafx");
        assert_ne!(victim, attacker);
        assert!(victim.starts_with("mod-com-example-a-very-long-module-name-for-"), "{victim}");
        assert_eq!(victim.len(), "mod-com-example-a-very-long-module-name-for-".len() + 16, "{victim}");
        assert!(scheme_for(&"x".repeat(128)).len() == "mod-".len() + 40 + 1 + 16, "the longest is 61");
        // An id with nothing readable in it is the hash alone.
        let bare = scheme_for("...");
        assert!(bare.len() == 4 + 16 && bare[4..].chars().all(|c| c.is_ascii_hexdigit()), "{bare}");
        for id in ["com.tool.html-probe", &"x".repeat(128), "...", "Com.UPPER.case_9"] {
            let s = scheme_for(id);
            assert!(s.len() <= 63, "{s}");
            assert!(s.starts_with("mod-"));
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{s}");
            assert!(!s.contains("--"), "{s}");
        }
        assert_eq!(origin_for("mod-x-1"), "http://mod-x-1.localhost");
    }

    #[test]
    fn the_entry_url_encodes_each_part() {
        assert_eq!(entry_url("mod-x", "ui/index.html"), "mod-x://localhost/ui/index.html");
        assert_eq!(entry_url("mod-x", "a b/c#d.html"), "mod-x://localhost/a%20b/c%23d.html");
        assert_eq!(entry_url("mod-x", "ü.html"), "mod-x://localhost/%C3%BC.html");
    }

    #[test]
    fn a_request_path_is_the_part_after_the_page_host() {
        assert_eq!(request_path("mod-x://localhost/ui/a.js", "mod-x"), Some("ui/a.js"));
        assert_eq!(request_path("mod-x://localhost/ui/a.js?v=2#top", "mod-x"), Some("ui/a.js"));
        assert_eq!(request_path("MOD-X://LOCALHOST/a", "mod-x"), Some("a"));
        assert_eq!(request_path("mod-x://elsewhere/a", "mod-x"), None);
        assert_eq!(request_path("mod-y://localhost/a", "mod-x"), None);
        assert_eq!(request_path("mod-x://localhost", "mod-x"), None);
    }

    /// A fresh folder of this test's own, removed when dropped. Not `tempfile`, which
    /// `crates/macos-check` (it borrows this file, tests included) does not have.
    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A module folder with a page, a subfolder and a file beside the folder, canonical.
    fn module_folder(name: &str) -> (Scratch, PathBuf) {
        let dir = std::env::temp_dir().join(format!("webview-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("module");
        std::fs::create_dir_all(root.join("ui")).unwrap();
        std::fs::write(root.join("ui").join("index.html"), "<h1>x</h1>").unwrap();
        std::fs::write(root.join("ui").join("a b.js"), "x").unwrap();
        std::fs::write(dir.join("secret.txt"), "s").unwrap();
        let root = root.canonicalize().unwrap();
        (Scratch(dir), root)
    }

    #[test]
    fn only_a_file_inside_the_module_is_found() {
        let (_dir, root) = module_folder("resolve");
        assert_eq!(resolve(&root, "ui/index.html"), Some(root.join("ui").join("index.html")));
        assert!(resolve(&root, "ui/a%20b.js").is_some(), "decoded");
        for bad in [
            "",
            "ui",                   // a folder
            "ui/",                  // an empty part
            "ui//index.html",
            "./ui/index.html",
            "ui/./index.html",
            "../secret.txt",
            "ui/../../secret.txt",
            "%2e%2e/secret.txt",    // encoded dots
            "..%2fsecret.txt",      // an encoded slash makes one part `../secret.txt`
            "ui%2f..%2f..%2fsecret.txt",
            "ui\\..\\..\\secret.txt",
            "..%5csecret.txt",      // an encoded backslash
            "ui/index.html:stream", // an NTFS stream
            "C:/Windows/win.ini",
            "%00",
            "ui/index.html%00",
            "missing.html",
            "con",
            "ui/nul.txt",
            "%ff",                  // not UTF-8
        ] {
            assert_eq!(resolve(&root, bad), None, "{bad:?} must not be served");
        }
        let outside = root.parent().unwrap().join("secret.txt");
        let absolute = outside.to_string_lossy().replace('\\', "/");
        assert_eq!(resolve(&root, &absolute), None, "an absolute path");
        assert_eq!(resolve_file(&root, &absolute), None, "an absolute file option");
        // The module's own option is not URL-encoded: `%20` is three characters of a name.
        assert!(resolve_file(&root, "ui/a b.js").is_some());
        assert_eq!(resolve_file(&root, "ui/a%20b.js"), None);
        assert_eq!(resolve_file(&root, "ui\\index.html"), None, "a backslash is no separator here");
        let found = resolve_file(&root, "ui/index.html").unwrap();
        assert_eq!(url_path(&root, &found).as_deref(), Some("ui/index.html"));
        assert_eq!(url_path(&root, &outside), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_module_is_not_followed() {
        let (_dir, root) = module_folder("link");
        std::os::unix::fs::symlink(root.parent().unwrap().join("secret.txt"), root.join("ui").join("link.txt"))
            .unwrap();
        assert_eq!(resolve(&root, "ui/link.txt"), None);
    }

    #[test]
    fn every_response_carries_the_policy_and_only_files_are_served() {
        let (_dir, root) = module_folder("serve");
        let header = |s: &Served, name: &str| {
            s.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
        };
        let ok = serve(&root, "mod-x", "GET", "mod-x://localhost/ui/index.html?x=1");
        assert_eq!(ok.status, 200);
        assert_eq!(ok.body, b"<h1>x</h1>");
        assert_eq!(header(&ok, "Content-Type").as_deref(), Some("text/html; charset=utf-8"));
        let head = serve(&root, "mod-x", "HEAD", "mod-x://localhost/ui/index.html");
        assert_eq!((head.status, head.body.len()), (200, 0));
        let missing = serve(&root, "mod-x", "GET", "mod-x://localhost/../secret.txt");
        assert_eq!(missing.status, 404);
        let other = serve(&root, "mod-x", "GET", "mod-y://localhost/ui/index.html");
        assert_eq!(other.status, 404, "another module's scheme");
        let post = serve(&root, "mod-x", "POST", "mod-x://localhost/ui/index.html");
        assert_eq!(post.status, 405);
        // A file past the limit is not read: 413. (Extended rather than written, so the test
        // does not write 16 MiB.)
        let big = std::fs::File::create(root.join("ui").join("big.bin")).unwrap();
        big.set_len(MAX_FILE_BYTES + 1).unwrap();
        drop(big);
        let large = serve(&root, "mod-x", "GET", "mod-x://localhost/ui/big.bin");
        assert_eq!((large.status, large.body.as_slice()), (413, &b"too large"[..]));
        for s in [&ok, &head, &missing, &other, &post, &large] {
            assert_eq!(header(s, "Content-Security-Policy").as_deref(), Some(CSP));
            assert_eq!(header(s, "X-Content-Type-Options").as_deref(), Some("nosniff"));
            assert_eq!(header(s, "Cache-Control").as_deref(), Some("no-store"));
            assert_eq!(header(s, "X-DNS-Prefetch-Control").as_deref(), Some("off"));
        }
    }

    #[test]
    fn the_policy_allows_scripts_only_from_the_module() {
        let directive = |name: &str| {
            CSP.split(';')
                .map(str::trim)
                .find(|d| d.split_whitespace().next() == Some(name))
                .unwrap_or_else(|| panic!("no {name} in the policy"))
                .to_string()
        };
        assert_eq!(directive("script-src"), "script-src 'self'");
        assert!(!directive("script-src").contains("unsafe"), "no inline script, no eval");
        assert_eq!(directive("default-src"), "default-src 'none'");
        assert_eq!(directive("connect-src"), "connect-src 'self'");
        for none in ["frame-src", "child-src", "object-src", "form-action", "base-uri", "worker-src", "frame-ancestors"] {
            assert_eq!(directive(none), format!("{none} 'none'"));
        }
        assert!(!CSP.contains('\n') && !CSP.contains("  "), "one header line");
    }

    #[test]
    fn content_types_are_the_ones_webview2_needs() {
        assert_eq!(content_type(Path::new("a.js")), "text/javascript; charset=utf-8");
        assert_eq!(content_type(Path::new("a.mjs")), "application/javascript; charset=utf-8");
        assert_eq!(content_type(Path::new("a.svg")), "image/svg+xml; charset=utf-8");
        assert_eq!(content_type(Path::new("a.css")), "text/css; charset=utf-8");
        assert_eq!(content_type(Path::new("a.json")), "application/json; charset=utf-8");
        assert_eq!(content_type(Path::new("a.png")), "image/png");
        assert_eq!(content_type(Path::new("noext")), "application/octet-stream");
    }

    #[test]
    fn only_the_pages_own_origin_is_its_own() {
        let o = "http://mod-x-1.localhost";
        assert!(own_origin("http://mod-x-1.localhost/ui/index.html", o));
        assert!(own_origin("HTTP://MOD-X-1.LOCALHOST/", o));
        assert!(!own_origin("http://mod-x-1.localhost", o), "no path at all");
        assert!(!own_origin("http://mod-x-1.localhost.evil.example/", o));
        assert!(!own_origin("http://mod-x-1.localhostx/", o));
        assert!(!own_origin("https://mod-x-1.localhost/", o));
        assert!(!own_origin("http://mod-y-1.localhost/", o));
        assert!(!own_origin("about:blank", o));
        assert!(!own_origin("", o));
        assert!(!own_origin("http://mod-x-1.localhöst/", o), "a multi-byte character where the origin ends");
        assert_eq!(request_path("mod-ü://localhost/a", "mod-x"), None);
    }

    #[test]
    fn only_webview2s_variables_are_logged_with_what_they_do() {
        let vars = vec![
            ("PATH".to_string(), "C:\\x".to_string()),
            ("WEBVIEW2_USER_DATA_FOLDER".to_string(), "D:\\elsewhere".to_string()),
            ("webview2_additional_browser_arguments".to_string(), "--remote-debugging-port=9222".to_string()),
            ("WEBVIEW2_".to_string(), "empty name".to_string()),
        ];
        let lines = webview2_overrides(vars);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("WEBVIEW2_USER_DATA_FOLDER=D:\\elsewhere (replaces")));
        assert!(lines.iter().any(|l| l.contains("--remote-debugging-port=9222 (added to the browser's arguments)")));
        assert!(lines.iter().any(|l| l.contains("carries --remote-debugging-port, which opens a port")));
        assert!(webview2_overrides(Vec::new()).is_empty());
    }

    /// Found by the review of 2026-09-28: the value used to be cut at 200 characters, so harmless
    /// switches in front pushed a dangerous one out of the log, and a line break in it wrote a
    /// line of its own.
    #[test]
    fn no_length_hides_a_dangerous_switch_and_no_value_writes_a_line() {
        let padded = format!("{} \"--no-sandbox\" --renderer-cmd-prefix=cmd.exe", "--lang=en ".repeat(40));
        let lines = webview2_overrides(vec![("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS".to_string(), padded)]);
        assert!(lines.iter().any(|l| l.ends_with("--renderer-cmd-prefix=cmd.exe (added to the browser's arguments)")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("carries --no-sandbox, which turns the renderer's sandbox off")));
        assert!(lines.iter().any(|l| l.contains("carries --renderer-cmd-prefix, which starts every renderer")));
        assert_eq!(lines.len(), 3, "{lines:?}");
        // A switch that only begins like a dangerous one is not one.
        let near = webview2_overrides(vec![("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS".to_string(), "--no-sandboxing".to_string())]);
        assert_eq!(near.len(), 1, "{near:?}");
        let forged = webview2_overrides(vec![(
            "WEBVIEW2_USER_DATA_FOLDER".to_string(),
            "D:\\x\n[gui] no WEBVIEW2_* environment variable is set".to_string(),
        )]);
        assert_eq!(forged.len(), 1);
        assert!(!forged[0].contains('\n') && forged[0].contains("D:\\x\\n[gui]"), "{forged:?}");
    }

    const SCREEN: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };
    const FULL: (i32, i32) = (520, 640);
    const MIN: (i32, i32) = (320, 360);

    /// Where a page of the full size lands.
    fn at(x: i32, y: i32) -> Option<Rect> {
        Some(Rect { x, y, w: FULL.0, h: FULL.1 })
    }

    #[test]
    fn the_page_goes_beside_the_rectangle_right_first() {
        let plugin = Rect { x: 100, y: 100, w: 800, h: 600 };
        assert_eq!(beside_only(FULL, MIN, plugin, &[SCREEN]), at(900, 100));
        assert_eq!(place(FULL, MIN, plugin, &[SCREEN]), at(900, 100).map(Placement::Beside));
        // No room to the right: the left.
        let right = Rect { x: 1200, y: 100, w: 700, h: 600 };
        assert_eq!(beside_only(FULL, MIN, right, &[SCREEN]), at(680, 100));
        // Lined up with the plug-in's top, pushed up to stay on the monitor.
        let low = Rect { x: 100, y: 800, w: 800, h: 200 };
        assert_eq!(beside_only(FULL, MIN, low, &[SCREEN]), at(900, 400));
    }

    #[test]
    fn below_or_above_when_neither_side_has_room() {
        let wide = Rect { x: 0, y: 0, w: 1920, h: 300 };
        assert_eq!(beside_only(FULL, MIN, wide, &[SCREEN]), at(0, 300));
        let bottom = Rect { x: 0, y: 700, w: 1920, h: 340 };
        assert_eq!(beside_only(FULL, MIN, bottom, &[SCREEN]), at(0, 60));
    }

    #[test]
    fn another_monitor_when_the_plugin_fills_its_own() {
        let second = Rect { x: 1920, y: 0, w: 1280, h: 1000 };
        let full = Rect { x: 0, y: 0, w: 1920, h: 1040 };
        assert_eq!(place(FULL, MIN, full, &[SCREEN, second]), at(1920 + 380, 180).map(Placement::Beside));
    }

    /// A maximised plug-in window and a second monitor too small for the full page — a small or
    /// portrait screen at a high scaling: the page goes there, smaller, rather than over the
    /// plug-in; with no room for even the smallest there, over the plug-in.
    #[test]
    fn a_smaller_page_on_another_monitor_before_one_over_the_plugin() {
        let maximised = Rect { x: -8, y: -8, w: 1936, h: 1096 };
        let small = Rect { x: 1920, y: 0, w: 600, h: 500 };
        assert_eq!(
            place(FULL, MIN, maximised, &[SCREEN, small]),
            Some(Placement::Beside(Rect { x: 1960, y: 0, w: 520, h: 500 }))
        );
        let smaller = Rect { x: 1920, y: 100, w: 560, h: 400 };
        assert_eq!(
            place(FULL, MIN, maximised, &[SCREEN, smaller]),
            Some(Placement::Beside(Rect { x: 1940, y: 100, w: 520, h: 400 }))
        );
        let tiny = Rect { x: 1920, y: 0, w: 300, h: 300 };
        assert!(matches!(place(FULL, MIN, maximised, &[SCREEN, tiny]), Some(Placement::Over { .. })));
        // Room on its own monitor, if only for a smaller page, comes first.
        let wide = Rect { x: 200, y: 100, w: 1300, h: 800 };
        assert_eq!(beside_only(FULL, MIN, wide, &[SCREEN, small]), Some(Rect { x: 1500, y: 100, w: 420, h: 640 }));
    }

    /// A page over the plug-in window is kept only when it ends in front: without `focus`, or
    /// declined, it is hidden again and the show answers why.
    #[test]
    fn a_page_over_the_plugin_is_kept_only_in_front() {
        assert_eq!(over_refused(true, false), None, "in front: kept");
        assert_eq!(over_refused(true, true), Some(Refusal::Declined), "declined: hidden");
        assert_eq!(over_refused(false, false), Some(Refusal::NoRoom), "without the keyboard: never over it");
        assert_eq!(over_refused(false, true), Some(Refusal::NoRoom));
    }

    /// A large FX chain window on a single monitor: the page is made smaller to fit the side with
    /// the most room, never smaller than the minimum, and still beside it.
    #[test]
    fn a_smaller_page_where_the_full_one_has_no_room() {
        let wide = Rect { x: 200, y: 100, w: 1300, h: 800 };
        assert_eq!(beside_only(FULL, MIN, wide, &[SCREEN]), Some(Rect { x: 1500, y: 100, w: 420, h: 640 }));
        let left_room = Rect { x: 470, y: 0, w: 1450, h: 1040 };
        assert_eq!(beside_only(FULL, MIN, left_room, &[SCREEN]), Some(Rect { x: 0, y: 0, w: 470, h: 640 }));
        // Room below only, and less than a full page's height of it.
        let tall = Rect { x: 0, y: 0, w: 1920, h: 600 };
        assert_eq!(beside_only(FULL, MIN, tall, &[SCREEN]), Some(Rect { x: 0, y: 600, w: 520, h: 440 }));
        // Less than the minimum anywhere.
        let almost = Rect { x: 100, y: 0, w: 1600, h: 1040 };
        assert_eq!(beside_only(FULL, MIN, almost, &[SCREEN]), None);
    }

    /// The maintainer's FX window after the double click on its title bar: maximised, a little
    /// larger than the monitor on every side. The page is not refused — it goes over the plug-in
    /// window, at its smallest, in the bottom right corner of the monitor's work area.
    #[test]
    fn a_maximised_plugin_window_gets_the_page_in_a_corner_over_it() {
        let maximised = Rect { x: -8, y: -8, w: 1936, h: 1096 };
        assert_eq!(beside_only(FULL, MIN, maximised, &[SCREEN]), None);
        let corner = Rect { x: 1920 - 320, y: 1040 - 360, w: 320, h: 360 };
        assert_eq!(place(FULL, MIN, maximised, &[SCREEN]), Some(Placement::Over { rect: corner, covered: 320 * 360 }));
        assert_eq!(place(FULL, MIN, maximised, &[SCREEN]).unwrap().rect(), corner);
        // Room less than the minimum on the left: the corner that covers the least of it.
        let almost = Rect { x: 100, y: 0, w: 1820, h: 1040 };
        let got = place(FULL, MIN, almost, &[SCREEN]).unwrap();
        assert_eq!(got, Placement::Over { rect: Rect { x: 0, y: 680, w: 320, h: 360 }, covered: 220 * 360 });
        // The same with the room at the top: the bottom corners cover all, the top ones less.
        let low = Rect { x: 0, y: 200, w: 1920, h: 840 };
        let got = place(FULL, MIN, low, &[SCREEN]).unwrap();
        assert_eq!(got.rect(), Rect { x: 1600, y: 0, w: 320, h: 360 }, "top right");
        // A page larger than the monitor is made to fit it.
        let tiny = Rect { x: 0, y: 0, w: 300, h: 200 };
        let got = place((520, 640), (500, 600), Rect { x: 0, y: 0, w: 300, h: 200 }, &[tiny]).unwrap();
        assert_eq!(got.rect(), tiny);
    }

    #[test]
    fn only_no_monitor_at_all_leaves_the_page_unplaced() {
        let full = Rect { x: 0, y: 0, w: 1920, h: 1040 };
        assert_eq!(place(FULL, MIN, full, &[]), None);
        assert!(matches!(place(FULL, MIN, full, &[SCREEN]), Some(Placement::Over { .. })));
        assert!(matches!(
            place((2000, 640), (2000, 640), Rect { x: 0, y: 0, w: 10, h: 10 }, &[SCREEN]),
            Some(Placement::Over { .. })
        ));
        // A rectangle on no monitor at all: any monitor it does not touch.
        let off = Rect { x: -5000, y: 0, w: 100, h: 100 };
        assert_eq!(place(FULL, MIN, off, &[SCREEN]), at(700, 200).map(Placement::Beside));
    }

    #[test]
    fn a_page_beside_never_covers_the_rectangle_and_one_over_it_is_at_its_smallest_in_a_corner() {
        let monitors = [SCREEN, Rect { x: 1920, y: 0, w: 1280, h: 1000 }];
        for x in (0..1900).step_by(137) {
            for w in [200, 800, 1500, 1800] {
                for h in [300, 700, 1040] {
                    let avoid = Rect { x, y: 0, w, h };
                    if let Some(page) = beside_only(DEFAULT_SIZE, MIN_SIZE, avoid, &monitors[..1]) {
                        assert!(!page.intersects(&avoid), "{page:?} covers {avoid:?}");
                        assert!(SCREEN.contains(&page), "{page:?} is off the monitor");
                        assert!(page.w >= MIN_SIZE.0 && page.h >= MIN_SIZE.1, "{page:?} is too small");
                        assert!(page.w <= DEFAULT_SIZE.0 && page.h <= DEFAULT_SIZE.1, "{page:?} is too large");
                    }
                    if let Some(page) = beside_only(DEFAULT_SIZE, MIN_SIZE, avoid, &monitors) {
                        assert!(!page.intersects(&avoid), "{page:?} covers {avoid:?}");
                        assert!(monitors.iter().any(|m| m.contains(&page)), "{page:?} is off every monitor");
                    }
                    match place(DEFAULT_SIZE, MIN_SIZE, avoid, &monitors[..1]) {
                        Some(Placement::Beside(page)) => assert!(!page.intersects(&avoid)),
                        Some(Placement::Over { rect, covered }) => {
                            assert_eq!((rect.w, rect.h), MIN_SIZE, "{rect:?}");
                            assert!(SCREEN.contains(&rect), "{rect:?} is off the monitor");
                            assert_eq!(covered, rect.overlap(&avoid));
                            let corners = [(1600, 680), (1600, 0), (0, 680), (0, 0)];
                            assert!(corners.contains(&(rect.x, rect.y)), "{rect:?} is in no corner");
                            for (cx, cy) in corners {
                                let c = Rect { x: cx, y: cy, w: MIN_SIZE.0, h: MIN_SIZE.1 };
                                assert!(covered <= c.overlap(&avoid), "{c:?} covers less than {rect:?}");
                            }
                        }
                        None => panic!("never refused with a monitor"),
                    }
                }
            }
        }
    }

    #[test]
    fn a_window_kind_is_named_by_one_of_two_words_and_is_a_tool_window_by_default() {
        assert_eq!(WindowKind::default(), WindowKind::Tool);
        for kind in [WindowKind::Tool, WindowKind::App] {
            assert_eq!(WindowKind::parse(kind.name()), Some(kind));
            assert!(WindowKind::NAMES.contains(&kind.name()));
        }
        assert_eq!(WindowKind::NAMES, ["tool", "app"], "the order the error names them in");
        for other in ["", "App", "TOOL", "application", "window", " app", "panel"] {
            assert_eq!(WindowKind::parse(other), None, "{other:?}");
        }
    }

    /// The tool page of stage 1 is unchanged: no window list, no minimise or maximise box, and a
    /// panel on a Mac. The app page is the other way round in every point.
    #[test]
    fn a_tool_page_is_in_no_window_list_and_an_app_page_is_a_window_like_any() {
        let tool = WindowKind::Tool.recipe();
        assert!(!tool.listed && !tool.minimise && !tool.maximise && tool.panel, "{tool:?}");
        let app = WindowKind::App.recipe();
        assert!(app.listed && app.minimise && app.maximise && !app.panel, "{app:?}");
    }

    /// A tool page gives the front back whenever it leaves it; an app page only when it has kept
    /// the front since it took it — not when the user came to it some other way.
    #[test]
    fn an_app_page_gives_the_front_back_only_when_it_kept_it_since_it_took_it() {
        assert!(WindowKind::Tool.gives_front_back(true));
        assert!(WindowKind::Tool.gives_front_back(false));
        assert!(WindowKind::App.gives_front_back(true));
        assert!(!WindowKind::App.gives_front_back(false));
    }

    /// `activates = false` is a tool page's option alone: an app page is a window like any
    /// program's, which activates its application; `true`, the default, suits both.
    #[test]
    fn only_a_tool_page_can_be_one_that_does_not_activate_the_application() {
        assert!(WindowKind::Tool.allows_activates(true));
        assert!(WindowKind::Tool.allows_activates(false));
        assert!(WindowKind::App.allows_activates(true));
        assert!(!WindowKind::App.allows_activates(false));
    }

    /// On a Mac an app page the user has open goes over `avoid` only once it has the keyboard, as
    /// on Windows, though the request for the front is answered after the page was put: only that
    /// place waits. A tool page, a place beside `avoid` and a request answered at once go now.
    #[test]
    fn a_macs_open_app_page_goes_over_the_plugin_only_once_it_has_the_keyboard() {
        let r = Rect { x: 0, y: 0, w: 320, h: 360 };
        let over = Placement::Over { rect: r, covered: 1 };
        let beside = Placement::Beside(r);
        assert!(place_when_key(WindowKind::App, &over, true));
        assert!(!place_when_key(WindowKind::App, &over, false), "answered already: in front, put now");
        assert!(!place_when_key(WindowKind::App, &beside, true), "beside covers nothing: put now");
        assert!(!place_when_key(WindowKind::Tool, &over, true), "a tool page is put before it is shown");
    }

    /// A Mac's show that did not get the front hides a page it placed over `avoid` again, as a
    /// declined one is on Windows — a tool page, open or not, and an app page that was not open —
    /// and leaves an app page the user has open, and any page beside `avoid`, as it is.
    #[test]
    fn a_macs_page_over_the_plugin_without_the_keyboard_is_hidden_unless_the_user_has_it_open() {
        assert!(not_got_hides(WindowKind::Tool, true, false));
        assert!(not_got_hides(WindowKind::Tool, true, true));
        assert!(not_got_hides(WindowKind::App, true, false));
        assert!(!not_got_hides(WindowKind::App, true, true), "the user's window");
        for kind in [WindowKind::Tool, WindowKind::App] {
            for open in [false, true] {
                assert!(!not_got_hides(kind, false, open), "beside covers nothing");
            }
        }
    }

    /// The script WebView2's pages got from stage 1 until the shim was built from parts, as it
    /// was: the Windows shim is held to it byte for byte, so building the Mac's beside it changes
    /// nothing for a page on Windows.
    const WINDOWS_SHIM_OF_STAGE_1: &str = r#"(() => {
  "use strict";
  if (window.top !== window || window.platform) return;
  try {
    Object.defineProperty(window, "close", { value: () => {}, writable: false, configurable: false });
  } catch (err) {}
  const ipc = window.ipc;
  const channel = window.chrome && window.chrome.webview;
  if (!ipc || !channel) return;
  let handler = null;
  const waiting = [];
  const hand = (value) => {
    try { handler(value); } catch (err) { setTimeout(() => { throw err; }); }
  };
  channel.addEventListener("message", (event) => {
    if (handler) hand(event.data);
    else if (waiting.length < 256) waiting.push(event.data);
  });
  const platform = Object.create(null);
  Object.defineProperties(platform, {
    post: {
      value: (value) => {
        ipc.postMessage(JSON.stringify(value === undefined ? null : value));
      },
    },
    onmessage: {
      get: () => handler,
      set: (fn) => {
        handler = typeof fn === "function" ? fn : null;
        while (handler && waiting.length) hand(waiting.shift());
      },
    },
  });
  Object.freeze(platform);
  Object.defineProperty(window, "platform", { value: platform });
})();"#;

    #[test]
    fn the_windows_shim_is_the_one_stage_1_shipped() {
        assert_eq!(platform_shim(Engine::WebView2), WINDOWS_SHIM_OF_STAGE_1);
    }

    #[test]
    fn both_shims_name_the_two_calls_and_keep_the_hosts_cap() {
        for engine in [Engine::WebView2, Engine::WebKit] {
            let shim = platform_shim(engine);
            assert!(shim.starts_with("(() => {") && shim.ends_with("})();"), "{engine:?}: one script");
            assert!(shim.contains("post:") && shim.contains("onmessage:"), "{engine:?}");
            assert!(shim.contains("ipc.postMessage(JSON.stringify("), "{engine:?}");
            assert!(shim.contains(&format!("waiting.length < {MAX_QUEUED_POSTS}")), "{engine:?}: the page's cap is the host's");
            assert!(
                shim.contains(r#"defineProperty(window, "close", { value: () => {}"#),
                "{engine:?}: a page cannot take its own web view down"
            );
            assert!(shim.contains("if (window.top !== window || window.platform) return;"), "{engine:?}: the top frame, once");
        }
    }

    /// WebKit's shim takes the host's messages through a function the page cannot replace, and
    /// nothing of WebView2's is in it; it puts the keyboard back through a function the host calls,
    /// and its notes are what [`shim_note`] recognises.
    #[test]
    fn the_webkit_shim_delivers_through_a_fixed_function_and_puts_the_keyboard_back() {
        let shim = platform_shim(Engine::WebKit);
        assert!(!shim.contains("chrome.webview"));
        assert!(shim.contains(r#"Object.defineProperty(window, "__platformDeliver", { value: (text) => take(JSON.parse(text)) });"#));
        assert!(WEBKIT_DELIVER.contains("window.__platformDeliver(m)"), "the host calls what the shim defines");
        assert!(shim.contains(r#"Object.defineProperty(window, "__platformRestoreFocus", { value: restore });"#));
        assert!(WEBKIT_RESTORE_FOCUS.contains("window.__platformRestoreFocus()"));
        assert!(shim.contains(r#"window.addEventListener("focus", () => { restore(); });"#), "the page's own focus event as well");
        // The guard: armed by a restore, ended by the user's input or a move elsewhere, and a loss
        // to nothing only.
        assert!(shim.contains(r#"for (const type of ["keydown", "pointerdown", "click"])"#));
        assert!(shim.contains("if (event.isTrusted) armed = false;"));
        assert!(shim.contains("if (event.target !== last) armed = false;"), "a move to another element ends it");
        assert!(shim.contains("event.relatedTarget !== null) return;"), "a move to an element is never undone");
        assert!(!shim.contains("setInterval") && !shim.contains("requestAnimationFrame"), "no timer decides");
        // The note's prefix, as the script writes it, is the host's.
        assert_eq!(SHIM_NOTE, "\u{1}platform: ");
        assert!(shim.contains(r#"ipc.postMessage("\u0001platform: " + text)"#));
        // The shared parts are the same text in both.
        let windows = platform_shim(Engine::WebView2);
        for part in [SHIM_HEAD, SHIM_HANDLER, SHIM_TAIL] {
            assert!(shim.contains(part) && windows.contains(part));
        }
    }

    /// No message of the page's own is a note: `platform.post` sends `JSON.stringify`'s text, and
    /// that never begins with a control character.
    #[test]
    fn only_the_shims_notes_are_notes() {
        assert_eq!(shim_note("\u{1}platform: put back"), Some("put back"));
        for message in ["null", "\"text\"", "{\"a\":1}", "[1]", "12", "-1", "true", "false", "\"\\u0001platform: x\""] {
            assert_eq!(shim_note(message), None, "{message}");
        }
        assert_eq!(shim_note("platform: x"), None);
    }

    #[test]
    fn webkit_serves_the_scheme_itself_and_the_origin_is_the_pages_host() {
        let scheme = scheme_for("com.tool.html-probe");
        let origin = webkit_origin_for(&scheme);
        assert_eq!(origin, format!("{scheme}://localhost"));
        let entry = entry_url(&scheme, "ui/index.html");
        assert!(own_origin(&entry, &origin), "{entry} is the page's own");
        assert_eq!(request_path(&entry, &scheme), Some("ui/index.html"));
        assert!(!own_origin(&format!("{scheme}://localhostx/"), &origin));
        assert!(!own_origin(&format!("{scheme}://localhost"), &origin), "no path at all");
        assert!(!own_origin(&entry.replace(&scheme, "mod-other-1"), &origin), "another module's");
        assert!(!own_origin("https://localhost/ui/index.html", &origin));
        assert!(!own_origin("about:blank", &origin));
    }

    /// The rule list blocks everything first and lets two things past it afterwards; every scheme a
    /// page can have is one the second rule lets past, and `data:` is the third.
    #[test]
    fn the_rule_list_blocks_all_but_the_pages_schemes_and_data() {
        let rules: Vec<&str> = RULE_LIST
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
            .expect("a JSON array")
            .split("},{")
            .collect();
        assert_eq!(rules.len(), 3, "{rules:?}");
        assert!(rules[0].contains(r#""url-filter":".*""#) && rules[0].contains(r#""type":"block""#));
        assert!(rules[1].contains(r#""url-filter":"^mod-[a-z0-9-]+://localhost/""#));
        assert!(rules[2].contains(r#""url-filter":"^data:""#));
        for later in &rules[1..] {
            assert!(later.contains(r#""type":"ignore-previous-rules""#), "{later}");
        }
        assert!(!RULE_LIST.contains(char::is_whitespace), "one line, as WebKit is handed it");
        // Every module's scheme is what the second rule lets past: `mod-`, then only lower-case
        // letters, digits and hyphens.
        for id in ["com.tool.html-probe", "Com.UPPER.case_9", "...", &"x".repeat(128), "com.platform.page-check"] {
            let s = scheme_for(id);
            let rest = s.strip_prefix("mod-").expect("mod-");
            assert!(!rest.is_empty() && rest.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "{s}");
            assert!(entry_url(&s, "ui/a.js").starts_with(&format!("{s}://localhost/")));
        }
        assert_eq!(RULE_LIST_ID, "pages");
        assert!(!RULE_FOLDER.contains(['/', '\\']));
    }

    /// Moved here from webview_win.rs, beside the Mac's (the next test).
    /// A page leaving the front gives it to the window it came from — through a page it was shown
    /// in place of, hidden since, to the window that one came from; never through a page still
    /// shown, one that came from no window, or this page itself; and pages that came from each
    /// other end. With no window made: the pages are a table.
    #[test]
    fn a_page_shown_in_place_of_another_gives_the_front_to_where_that_one_came_from() {
        const THIS: isize = 1;
        const PLUGIN: isize = 100;
        // window -> (shown, came from)
        let goes_to = |back_to: isize, pages: &[(isize, bool, isize)]| {
            front_goes_to(THIS, back_to, pages.len(), |w| pages.iter().find(|p| p.0 == w).map(|p| (p.1, p.2)))
        };
        assert_eq!(goes_to(PLUGIN, &[(THIS, true, PLUGIN)]), PLUGIN, "not a page: the window itself");
        assert_eq!(goes_to(2, &[(THIS, true, 2), (2, false, PLUGIN)]), PLUGIN, "a page hidden since: where it came from");
        assert_eq!(goes_to(2, &[(THIS, true, 2), (2, true, PLUGIN)]), 2, "a page still shown: that page");
        assert_eq!(
            goes_to(3, &[(THIS, true, 3), (3, false, 2), (2, false, PLUGIN)]),
            PLUGIN,
            "two in place of each other, both hidden since"
        );
        assert_eq!(goes_to(2, &[(THIS, true, 2), (2, false, 0)]), 2, "a page that came from no window: left to the system");
        assert_eq!(goes_to(2, &[(THIS, true, 2), (2, false, THIS)]), 2, "never this page itself");
        // A loop ends, on one of its pages — hidden, so the system decides.
        assert!(matches!(goes_to(2, &[(THIS, true, 2), (2, false, 3), (3, false, 2)]), 2 | 3));
        assert_eq!(goes_to(0, &[(THIS, true, 0)]), 0, "none: none");
    }

    /// The Mac's: the same chain, with each window's application — and a page that came from an
    /// application whose window could not be named sends the front to that application, not to
    /// itself, hidden. With no window made: the pages are a table.
    #[test]
    fn a_macs_page_gives_the_front_to_the_application_a_page_came_from_when_its_window_had_no_name() {
        const THIS: isize = 1 << 40;
        const OURS: i32 = 10;
        const DAW: i32 = 20;
        const PLUGIN: (isize, i32) = (100, DAW);
        // window -> (shown, came from)
        type Page = (isize, bool, Option<(isize, i32)>);
        let goes_to = |came: (isize, i32), pages: &[Page]| {
            front_goes_to_app(THIS, came, pages.len(), |w| pages.iter().find(|p| p.0 == w).map(|p| (p.1, p.2)))
        };
        let a = THIS + 1;
        let b = THIS + 2;
        assert_eq!(goes_to(PLUGIN, &[(THIS, true, Some(PLUGIN))]), PLUGIN, "not a page: the window itself");
        assert_eq!(
            goes_to((a, OURS), &[(THIS, true, Some((a, OURS))), (a, false, Some(PLUGIN))]),
            PLUGIN,
            "through a page hidden since, with its application"
        );
        assert_eq!(
            goes_to((a, OURS), &[(THIS, true, Some((a, OURS))), (a, false, Some((0, DAW)))]),
            (0, DAW),
            "a page that came from the DAW whose window had no name: the DAW, never the hidden page"
        );
        assert_eq!(
            goes_to(
                (b, OURS),
                &[(THIS, true, Some((b, OURS))), (b, false, Some((a, OURS))), (a, false, Some((0, DAW)))]
            ),
            (0, DAW),
            "two pages, the first from the DAW unnamed"
        );
        let this_from_a = (THIS, true, Some((a, OURS)));
        assert_eq!(goes_to((a, OURS), &[this_from_a, (a, true, Some(PLUGIN))]), (a, OURS), "a page still shown: that page");
        assert_eq!(goes_to((a, OURS), &[this_from_a, (a, false, None)]), (a, OURS), "a page that never took the front");
        assert_eq!(goes_to((a, OURS), &[this_from_a, (a, false, Some((THIS, OURS)))]), (a, OURS), "never this page");
        assert_eq!(goes_to((0, DAW), &[(THIS, true, Some((0, DAW)))]), (0, DAW), "the application alone: itself");
        // A loop ends, on one of its pages.
        let looped = goes_to((a, OURS), &[this_from_a, (a, false, Some((b, OURS))), (b, false, Some((a, OURS)))]);
        assert!(looped == (a, OURS) || looped == (b, OURS), "{looped:?}");
        // The Windows chain, read with no application, is the same chain.
        let windows = [(THIS, true, 2), (2, false, 3), (3, false, PLUGIN.0)];
        let as_mac: Vec<Page> = windows.iter().map(|&(w, s, f)| (w, s, (f != 0).then_some((f, DAW)))).collect();
        assert_eq!(
            goes_to((2, DAW), &as_mac).0,
            front_goes_to(THIS, 2, windows.len(), |w| windows.iter().find(|p| p.0 == w).map(|p| (p.1, p.2)))
        );
    }

    /// The Mac's page keys: the same for both kinds where a page is a page, the application's own
    /// in an app page, and nothing in a tool page; Control, or no Command, is never the table's.
    #[test]
    fn a_macs_page_keys_are_the_hosts_and_a_tool_page_quits_and_hides_nothing() {
        use EditCommand::*;
        let (tool, app) = (WindowKind::Tool, WindowKind::App);
        let cmd = |kind, chars| mac_key(kind, chars, true, false, false, false);
        for kind in [tool, app] {
            assert_eq!(cmd(kind, "w"), MacKey::Close, "{kind:?}");
            assert_eq!(cmd(kind, "x"), MacKey::Edit(Cut));
            assert_eq!(cmd(kind, "c"), MacKey::Edit(Copy));
            assert_eq!(cmd(kind, "v"), MacKey::Edit(Paste));
            assert_eq!(cmd(kind, "a"), MacKey::Edit(SelectAll));
            assert_eq!(cmd(kind, "z"), MacKey::Edit(Undo));
            // Shift-Command-Z: AppKit gives the capital without the other modifiers.
            assert_eq!(mac_key(kind, "Z", true, true, false, false), MacKey::Edit(Redo));
            // Not the table's: AppKit's as before.
            assert_eq!(cmd(kind, "f"), MacKey::Pass);
            assert_eq!(cmd(kind, ""), MacKey::Pass);
            assert_eq!(cmd(kind, "ww"), MacKey::Pass);
            assert_eq!(mac_key(kind, "c", false, false, false, false), MacKey::Pass, "no Command");
            assert_eq!(mac_key(kind, "c", true, false, false, true), MacKey::Pass, "Control held");
            assert_eq!(mac_key(kind, "c", true, false, true, false), MacKey::Pass, "Option-Command-C");
            assert_eq!(mac_key(kind, "Q", true, true, false, false), MacKey::Pass, "Shift-Command-Q, the system's log out");
        }
        // A tool page: nothing for Quit and Hide, and Command-M is not its.
        assert_eq!(cmd(tool, "q"), MacKey::Nothing);
        assert_eq!(mac_key(tool, "q", true, false, true, false), MacKey::Nothing, "Option-Command-Q");
        assert_eq!(cmd(tool, "h"), MacKey::Nothing);
        assert_eq!(mac_key(tool, "h", true, false, true, false), MacKey::Nothing, "Option-Command-H");
        assert_eq!(cmd(tool, "m"), MacKey::Pass);
        // An app page: as any application.
        assert_eq!(cmd(app, "q"), MacKey::Quit);
        assert_eq!(cmd(app, "h"), MacKey::Hide);
        assert_eq!(mac_key(app, "h", true, false, true, false), MacKey::HideOthers);
        assert_eq!(cmd(app, "m"), MacKey::Miniaturise);
        let selectors: Vec<&str> =
            [Cut, Copy, Paste, SelectAll, Undo, Redo].iter().map(|c| c.selector().to_str().unwrap()).collect();
        assert_eq!(selectors, ["cut:", "copy:", "paste:", "selectAll:", "undo:", "redo:"]);
    }

    /// Under a layout whose letters are not Latin, the characters typed with Command held decide,
    /// as they do for AppKit's menus: a tool page's Command-Q is nothing there too. Where the
    /// characters without the modifiers are the table's, they decide, as before.
    #[test]
    fn a_macs_page_keys_hold_under_a_layout_whose_letters_are_not_latin() {
        let (tool, app) = (WindowKind::Tool, WindowKind::App);
        // A Russian layout: Command-Й types Command-Q, Command-Р Command-H, Command-Ц Command-W.
        assert_eq!(mac_key_of(tool, "й", "q", true, false, false, false), MacKey::Nothing);
        assert_eq!(mac_key_of(tool, "р", "h", true, false, false, false), MacKey::Nothing);
        assert_eq!(mac_key_of(tool, "р", "h", true, false, true, false), MacKey::Nothing, "Option-Command-H");
        assert_eq!(mac_key_of(app, "й", "q", true, false, false, false), MacKey::Quit);
        assert_eq!(mac_key_of(app, "р", "h", true, false, false, false), MacKey::Hide);
        for kind in [tool, app] {
            assert_eq!(mac_key_of(kind, "ц", "w", true, false, false, false), MacKey::Close);
            assert_eq!(mac_key_of(kind, "с", "c", true, false, false, false), MacKey::Edit(EditCommand::Copy));
            assert_eq!(mac_key_of(kind, "Я", "Z", true, true, false, false), MacKey::Edit(EditCommand::Redo));
            // A Latin layout: what it was.
            assert_eq!(mac_key_of(kind, "c", "c", true, false, false, false), MacKey::Edit(EditCommand::Copy));
            // Option-Command-C types a "ç": not the table's either way.
            assert_eq!(mac_key_of(kind, "c", "ç", true, false, true, false), MacKey::Pass);
            assert_eq!(mac_key_of(kind, "й", "q", false, false, false, false), MacKey::Pass, "no Command");
            assert_eq!(mac_key_of(kind, "й", "q", true, false, false, true), MacKey::Pass, "Control held");
            assert_eq!(mac_key_of(kind, "й", "й", true, false, false, false), MacKey::Pass, "no Latin letter at all");
        }
    }
}
