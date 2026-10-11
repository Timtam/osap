//! `host.gui.webview`'s rules, with no window in them: which answer `show` gives and when, what
//! happens to a message posted before the page is there, which file a request is served and with
//! which headers, which navigations and messages are let through, which window a page is in and
//! where it goes, and the page's own `platform` object. The window itself is `webview_win.rs` on Windows; the Luau side
//! is `webview_lua.rs`.
//!
//! **Experimental.** Stage 1 of the plan the maintainer decided on 2026-09-28: a page a module
//! shows beside a plug-in window, for the Windows probe (`tools/html-probe`) to measure with NVDA.
//! The API may change after the probe.
//!
//! Pure on purpose: everything here decides something, and a decision about a browser window is
//! only testable if nothing opens one. `crates/macos-check` borrows the file, so the answer a Mac
//! gives today — "not supported on this platform yet" — is compiled for the Mac as well.

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
    /// A platform the host cannot show a page on yet: macOS until wry releases its fix for the
    /// activation on every web view (#1866), and any platform with no web view at all.
    Unsupported,
    /// The page is being built, or has not finished loading. `onReady` says when it has.
    NotReady,
    /// Windows without the WebView2 runtime. Asked at every `show` that would start a build, so
    /// installing the runtime needs no restart.
    RuntimeMissing,
    /// The last build failed; the log says why. Said once, and the next `show` tries again.
    BuildFailed,
    /// The page is on screen, and Windows refused to bring it to the front — or, where it would
    /// have been over `avoid` ([`over_refused`]), it is hidden again.
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
        Where { headless, supported: cfg!(windows) }
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
fn escape_controls(text: &str) -> String {
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

/// A rectangle in screen coordinates — physical pixels on Windows, as `host.window` gives them.
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
}

/// What a page's window is, on both platforms: one decision, which `webview_win.rs` builds on
/// Windows and the Mac's page will build on (stage 4, TODO.md), so the two cannot drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// In the system's lists of windows. Windows: an Alt+Tab entry with the page's title, and a
    /// place on the taskbar — the application's own button, where Windows combines a program's
    /// windows, as Windows 11 does by default — no `WS_EX_TOOLWINDOW`. macOS: Command-Tab and the
    /// Dock list applications, not windows, and this application has no Dock icon; while such a
    /// page is shown, the application is a regular one, by the claim the module manager's window
    /// makes (`gui::dock`).
    pub listed: bool,
    /// A minimise box (Windows), a miniaturise button (macOS).
    pub minimise: bool,
    /// A maximise box (Windows). A Mac's titled, resizable window has its zoom button either way.
    pub maximise: bool,
    /// macOS: an `NSPanel` that does not activate the application when it takes the keyboard, so
    /// the DAW stays the active application; otherwise an `NSWindow`, which activates it.
    pub panel: bool,
}

/// A page's size, at 96 dots per inch: scaled by the monitor's.
pub const DEFAULT_SIZE: (i32, i32) = (520, 640);

/// The smallest a page is made to fit beside the rectangle to avoid, at 96 dots per inch.
pub const MIN_SIZE: (i32, i32) = (320, 360);

/// The script the host puts into every page before the page's own: `platform.post(value)` to
/// send the module a value, and `platform.onmessage = function (value) … end` to receive what
/// the module posts. It hides which engine carries the messages (WebView2's
/// `chrome.webview` here, WebKit's message handler on a Mac later), so a page is written once.
///
/// Messages the module posts before the page has set `onmessage` wait for it, up to the same
/// cap the host keeps. A handler that throws does not stop the next message; the error is
/// rethrown on its own, where the page's console sees it. Main frame only: a page has no frames
/// (the policy forbids them), and a frame is not where a message may come from.
///
/// And `window.close()` does nothing: in WebView2 it takes the web view down and leaves its
/// window empty, and a page is closed by its module (`page:close()`), not by itself. Should a
/// page get round this, the host treats it as a page that failed (webview_win.rs).
pub const PLATFORM_SHIM: &str = r#"(() => {
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

/// The folder WebView2 keeps its data in: beside the application, like its settings and its log.
pub const DATA_FOLDER: &str = "WebView2";

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

    #[test]
    fn this_platform_is_supported_on_windows_only() {
        assert_eq!(Where::here(false).supported, cfg!(windows));
        assert!(!Where::here(true).can_show());
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

    /// The tool page of stage 1 is unchanged: no window list, no minimise or maximise box, a
    /// panel that does not activate the application on a Mac. The app page is the other way
    /// round in every point.
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

    #[test]
    fn the_shim_is_one_script_that_names_the_two_calls() {
        assert!(PLATFORM_SHIM.contains("post:"));
        assert!(PLATFORM_SHIM.contains("onmessage:"));
        assert!(PLATFORM_SHIM.contains("chrome.webview"));
        assert!(PLATFORM_SHIM.contains("JSON.stringify"));
        assert!(PLATFORM_SHIM.contains(&MAX_QUEUED_POSTS.to_string()), "the page's cap is the host's");
        assert!(
            PLATFORM_SHIM.contains(r#"defineProperty(window, "close", { value: () => {}"#),
            "a page cannot take its own web view down"
        );
    }
}
