# Speech through prism — design and measurements

*Decision document, 2026-09-01. Basis: a real from-source build of `ethindp/prism` v0.18.2 on
the development machine, a linked and executed test program, plus eight agents reading the
source of prism, of `tts-rs` and of this repository. Belongs to
[host-api-capability-catalog.md](host-api-capability-catalog.md).*

Speech is the whole output of this application. Everything below is shaped by one rule: a
failure that ends in silence is worse than a failure that says the wrong thing, because
silence is indistinguishable from "nothing happened".

## What is being replaced, and what is not

`crates/host/src/speech/mod.rs` uses exactly three calls from the `tts` crate:
`Tts::default()`, `speak(String, bool)` and `is_speaking()`. On Windows that crate routes
through **Tolk**, which loads `nvdaControllerClient64.dll` and `SAAPI64.dll` at run time —
the two files `package.ps1` ships beside the executable.

**Windows moves to prism. macOS does not, and will not.** Not only because
`speech/voiceover.rs` is better than prism's VoiceOver backend, but because prism's
*AVSpeech* backend asks for Personal Voice authorization inside `initialize()` and blocks on
it for up to 120 seconds — a permission dialog at start-up, in front of somebody who cannot
see it. And the measurement that justified the Windows change does not exist there: on a real
Mac (CI run 33620074214, macos-15-intel) `Speech::new` took **28 ms**, against 2872 on
Windows. The plan for macOS is in TODO.md: our own `objc2` wrapper, which can also reach
Personal Voice — asking at the moment somebody chooses it, rather than at start-up.

The VoiceOver half of that: prism's backend fires `NSAppleScript` on
`dispatch_get_main_queue()` — back onto the keyboard thread this project deliberately moved
it off — and coalesces non-interrupting lines with `" . "` after a 15 ms debounce, merging
exactly the name-then-value pair the overlays depend on. It also has no braille. So macOS
keeps `voiceover.rs`, with `tts` behind it as the plain voice.

## Three defects in what we ship today

These were found while measuring the replacement, and each is real independently of whether
prism ever lands.

**The application speaks to people who did not ask for speech.** `Tts::default()` tries Tolk,
and `tts-0.26.3/src/backends/tolk.rs:15` returns `None` when `detect_screen_reader()` finds
nothing — so `tts-rs` falls through to `Backends::WinRt` and talks out loud through the
speakers. A sighted user with no screen reader is addressed by every overlay control. The
probe even documents it: *"none detected (speech falls back to SAPI)"*.

**Nothing we say reaches a braille display on Windows.** The Tolk backend calls `Tolk_Speak`,
never `Tolk_Output`. `docs/host-api-capability-catalog.md` claimed braille; it has been
corrected.

**A failed line is discarded twice.** `tolk.rs:38` throws away the `bool` that `Tolk_Speak`
returns, and `speech/mod.rs:66` throws away the `Result`. If NVDA quits mid-session the
application goes silent with nothing in the log. The same backend declares `stop` as its only
feature, so `is_speaking()` is `Err(UnsupportedFeature)` and reads as **false** whenever a
screen reader is driving.

## Whose voice is it — the rule

Speech has two sources, and only one of them belongs to the application.

**Module speech** — `host.speech.output`, `crates/host/src/lib.rs:2939` — always speaks,
through whatever engine is available, screen reader or SAPI. A module was installed and
enabled deliberately; whoever did that asked for it. The overlay runtime is a module like any
other, so every overlay announcement is module speech.

**Application speech** is two sites and no more: the startup hint and the reload hotkey.
Both go through `Shared::announce`, and the rule there turned out simpler than it was
designed. It is not "speak when a screen reader is running, show otherwise" — it is **show,
and speak only where there is nothing to show it in**:

1. Show a notification. On Windows a screen reader reads the balloon out and everybody else
   can see it, so one channel serves both, and the application never speaks at all.
2. `show_balloon` is Windows-only and answers false everywhere else, which is exactly the
   signal for step 3. On macOS there is no balloon and no Dock icon either, so somebody who
   cannot see the screen would otherwise have nothing to go on.
3. Speak — but only if a screen reader is actually listening. With none running the
   application says nothing, which is the right amount for an audience that is not there.

Headless has no tray icon, so step 1 does nothing there and the same rule applies.

No sound is used to report anything. A sound reaches the person it does not concern.

"Is a screen reader running" is answered by which prism backend was selected. That also
retired a startup line in `crates/host/src/backend/windows.rs`, which inferred it from
whether `nvdaControllerClient64` or `SAAPI64` was loaded in this process. The inference was
fair only while Tolk loaded those on demand; with speech going through prism nothing loads
either, and the very first run printed

```
[env] screen reader: none detected (speech falls back to SAPI)
[speech] speaking through NVDA
```

two lines apart. The second line is the better answer anyway — which backend *will* speak,
rather than which DLL happens to be in memory — so the first was removed rather than
repaired.

## Does it build

Yes. Measured on the development machine (Win 11 26220, VS 18.5, MSVC 14.50, 12 cores):

- End to end from nothing: **37.9 s** — shallow clone 3.5 s, CMake configure 8.1 s, Ninja
  build 26.3 s over 40 edges, install 1.1 s.
- Output: a **13.7 MB static `prism.lib`** built against the **dynamic** CRT when
  `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL` is passed, which is what Rust's MSVC target
  needs. Confirmed with `dumpbin -directives`: `/DEFAULTLIB:MSVCRT`.
- Zero compiler warnings across all 40 edges.
- **Configure downloads nothing.** All six third-party dependencies (fmt, highway, simdutf,
  concurrentqueue, dr_wav, moderncom) are vendored under `third_party/` as amalgamated
  source. CPM is included from one place only, `tests/CMakeLists.txt`, and tests default off.
  Proven by configuring with a dead proxy: exit 0 in 5.7 s, no `_deps/` produced.
- A real `/MD` executable was then linked against the installed library with plain `cl` and
  `link.exe` — no CMake — run, and it reported seven backends: NVDA, System Access, ZoomText,
  JAWS, OneCore, UIA, SAPI.

Tools required beyond what this machine already has for wxdragon: **`midl.exe`** from the
Windows SDK, and the Visual Studio **C++ ATL** component (`prism.lib` emits
`/DEFAULTLIB:atls.lib`). Python and Java are present but unused by this configuration.

### It compiles into our binary, with no new DLL

`target/release/automation-platform.exe` already imports `MSVCP140.dll`, `VCRUNTIME140.dll`
and their `_1` variants, because wxWidgets is statically linked in. It already imports
`rpcrt4.dll` — the library prism's NVDA backend speaks over — and `ole32`/`oleacc` for the COM
path to JAWS. `package.ps1` therefore loses two files rather than gaining any, and there is no
`prism.dll`: `BUILD_SHARED_LIBS` defaults to `ON` upstream and is set `OFF` explicitly.

Measured on the linked release binary rather than assumed, three imports **are** new, and none
of them is a file to ship: `rpcns4.dll` and `xmllite.dll` are Windows itself (the RPC name
service prism's NVDA path uses, and XML for the speech engines), and
`msvcp140_atomic_wait.dll` belongs to the same Visual C++ redistributable as the `MSVCP140`
the binary has always needed. The three vendor DLLs appear where they must:

```
delay load dependencies: ZDSRAPI_x64.dll, byctrl-x64.dll, PCTKUSR.dll
```

and in the ordinary import table, not at all — which is what lets the application start on a
machine that has none of those screen readers.

### The trap that would have shipped as silence

Prism's backends register themselves through **static initializers in per-backend object
files**. An ordinary static link discards every one of them: the first test executable
compiled, linked, ran, exited 0 — and reported **0 backends**. No error anywhere, at any
stage. Prism's own CMake hides this behind `$<LINK_LIBRARY:WHOLE_ARCHIVE,prism>` on its
interface target, which a Rust build script never reads.

`build.rs` must emit `cargo:rustc-link-lib=static:+whole-archive=prism`. Relinking with
`/WHOLEARCHIVE` took the test executable from 231,936 to 880,640 bytes and from zero backends
to seven. **A smoke test asserting the backend names — not a count — belongs beside the
crate**, because nothing else can catch a regression here: the failure mode is a clean build
and a mute application.

The second trap is `midl.exe`. `cmake/PrismCodegen.cmake` requires it unconditionally on
Windows, and under the Ninja generator prism's own search never finds it — it works only
because `vcvars` puts the versioned SDK directory on `PATH`. `build.rs` must locate it and
pass `-DPRISM_MIDL_COMPILER`, so that someone else's `FATAL_ERROR` becomes our own clear
message.

## What prism changes for the user

- **Braille on Windows for the first time**, through `prism_backend_output`. Deferred to its
  own step and its own switch; the overlays interrupt on every Tab, and a display that
  flashes 70 lines a minute is not an improvement.
- **A per-call error**, so NVDA quitting mid-session becomes a spoken sentence and a log line
  instead of silence.
- **Four screen readers Tolk cannot reach**: ZDSR, PC-Talker, Boy PC Reader and Sense
  Reader. Untestable here, which is a reason to be careful with them, not to withhold them.
- **Two shipped DLLs disappear**, and Tolk's LGPL-3 static-link obligation with them.
- **Dolphin SuperNova is lost**: Tolk has a driver, prism has no backend.
- **Narrator is not gained.** Prism reaches it only through the UIA backend, which is
  compiled out for returning a success code when nobody is listening. A backend that cannot
  distinguish speaking from silence is not worth a screen reader we do not have a user for.

## Design

### One thread, and it is not optional

`prism_init`, backend creation and every later call live on one dedicated thread, exactly as
`voiceover.rs` does. Four separate problems collapse into that one decision.

*Apartment.* `Speech::new()` runs as the second statement of `Manager::new`, before anything
else initializes COM. Measured: `prism_init` then makes the **main thread STA**, and the later
`CoInitializeEx(MULTITHREADED)` at `backend/uia.rs:32` returns `RPC_E_CHANGED_MODE`, which
that line discards. No UIA slowdown was measurable from this — but OneCore's
`is_current_thread_sta()` then becomes true and every OneCore call marshals and blocks on an
`INFINITE` wait on the GUI thread.

*Shutdown.* `prism_shutdown` calls `CoUninitialize` on whichever thread called `prism_init`.
On the main thread that is one init/shutdown pair per process, under a cached `IUIAutomation`
and under wxWidgets' own OLE init.

*Thread safety.* Prism documents that a backend handle is not safe for concurrent use even
across logically independent calls. One owning thread satisfies that by construction.

*Blast radius.* `source/prism.cpp` has one `catch` in its whole `extern "C"` block.
`prism_backend_speak`, `_name`, `_get_features` and `_is_speaking` have neither try/catch nor
null checks. A `std::runtime_error` thrown out of OneCore under handle exhaustion would unwind
into a Rust frame and abort the process. prism-sys null-checks every pointer on our side, and
the thread keeps such an abort from being provokable from the event loop mid-announcement.

### Backends are chosen by id, never by "best"

`prism_registry_create(ctx, PRISM_BACKEND_NVDA)`, then JAWS, then the non-screen-reader
engine. Not `acquire_best`, which caches the instance and hands the same dead backend back
forever, defeating prism's own documented recovery. And not `create_best`, for two measured
reasons:

- **OneCore's `initialize()` costs 2.9 seconds** — 2887 / 2950 / 2812 ms across three separate
  process runs. `onecore.cpp:187` unconditionally synthesizes a space to a stream and reads it
  back to cache an audio format we never use. `create_best` walks priority order and lands
  there whenever no screen reader is up.
- **The UIA backend returns success for silence.** It initializes on any window our own
  process owns — wxWidgets always provides one — and "speaks" by raising a UI Automation
  notification, audible only if a screen reader is listening. With NVDA gone it returns
  `PRISM_OK` and makes no sound. A success code for silence is the one thing this design
  cannot have, so the UIA backend is compiled out.

The 2.9 s stall is not a reason to drop SAPI and OneCore, which are wanted for module speech
where no screen reader is present. It is a reason to **initialize the non-screen-reader engine
lazily, on the speech thread**, where a slow start blocks nothing and is finished long before
a module says anything.

### The backend set

On: `nvda`, `jaws`, `zoom_text`, `sapi`, `onecore`, and the four readers this project has no
way to test but no reason to withhold — `zdsr`, `pc_talker`, `boy_pc_reader`, `sense_reader`.

Off: `uia` (a success code for silence, above), `system_access` and `window_eyes`. Those two
are the only backends prism marks `LEGACY`, so with both off
`PRISM_ENABLE_LEGACY_BACKENDS=OFF` and the legacy path is gone as well. Dropping System Access
is a deliberate, stated regression: `package.ps1` ships `SAAPI64.dll` today precisely for it.

### Keeping the four means the delay-load flags are not optional

`zdsr`, `pc_talker` and `boy_pc_reader` reference vendor DLLs — `ZDSRAPI_x64.dll`,
`PCTKUSR.dll`, `byctrl-x64.dll` — that ship with those screen readers and exist on nobody
else's machine. Compiled in without `/DELAYLOAD`, the executable **does not start at all**:
exit `0xC0000135`, `STATUS_DLL_NOT_FOUND`, before `main`. That is every user, for the sake of
four readers almost none of them run.

Prism's own CMake passes those flags, but its **exported** interface does not reach a static
Rust consumer: `prism-targets.cmake` keeps the `/delayload:` options, while the five generated
import libraries come through as empty `$<LINK_ONLY:>` because they were wrapped in
`$<BUILD_INTERFACE:>`. So `build.rs` emits the `/delayload:` arguments itself and links
`ZDSR.lib`, `PCTalker.lib` and `byctrl.lib` from the install prefix by hand.

The smoke test for this is not subtle and must exist: **the built executable has to start.**

This also brings prism issue #109 along — an intermittent access violation in
`prism_shutdown` after `ZDSRAPI_x64.dll` has been unloaded, open since 2026-08-29. It can only
fire on a machine that actually ran ZDSR, and only at shutdown. Worth trying `/DELAYLOAD`
without prism's `/DELAY:unload`, which is what creates the unload path in the first place.

### The plain voice is prism too, on a second thread

Dropping `tts` from the Windows build was nearly abandoned on a half-measurement. Opening a
prism speech engine costs seconds — OneCore 3.5 s, SAPI 2.0 s — and the moment the fallback
is needed is the moment a screen reader has just gone, which is exactly when the user has to
be told what happened. Against that, `tts` looked instant and already there.

It is neither. Asked what it actually costs, the answer was **2872 ms of blocking start-up in
the running application**, for a voice that on a machine with a screen reader never says a
word, and **567 ms more** for its first line if it ever does. It was not cheaper than prism;
it was more expensive, and it charged everybody at every start rather than one person once.

So the fallback is a second prism worker, in `speech/fallback.rs`, with its own library on
its own thread. It opens in the background and queues whatever arrives meanwhile, so none of
that time is start-up time: `Speech::new` now measures **0 ms**. Two threads and two
libraries rather than one, and that is the point — the screen-reader path is usually lost by
the stall deadline, so its thread is wedged inside a call that will never return, and
anything sharing that thread would be wedged with it.

The screen-reader worker therefore stopped opening speech engines at all. It is screen
readers or nothing; the plain voice is somebody else's job.

One thing fell out of removing the crate, worth naming because nothing would otherwise have
caught it: the build broke on `OcrResult::Lines`. That call needs the `Foundation_Collections`
feature of the `windows` crate, which this project used without declaring — `tts` happened to
enable it on the same crate, and cargo unified the features. A borrowed feature is a
dependency you do not know you have until the lender leaves.

### Opening a library repeatedly is the thing that breaks

Everything unusual in the shape of the engine list comes from one property, found three
separate times before it was recognised: **a prism library is safe to open once and keep, and
unsafe to open and close over and over.**

The three findings, in the order they arrived:

1. Asking OneCore whether it is there kills the process the *second* time. Its `get_features`
   calls WinRT's `ApiInformation::IsTypePresent`, and those statics do not survive the
   `CoUninitialize` that closing a library performs. Documented as legal without
   `initialize`, and it is — once. `prism-sys` refuses to probe that one backend, and prism
   master still crashes on it.
2. Answering the question on the **caller's** thread left a test process unable to exit after
   every one of its tests had passed. Opening a library fixes that thread's COM apartment and
   closing it unfixes it, and the caller here is the event loop.
3. Answering on a fresh thread per call hung under a handful of concurrent queries.

So the question goes to a worker that already holds a library and cannot wedge — the plain
voice, not the screen reader, whose thread can be stuck inside an RPC call that never
returns. It answers into a shared slot; the caller reads a snapshot. Measured: **1.9 µs** to
read, against 35 ms for a fresh look when idle and 2.7 s while an engine is opening.

The same property decides what a *chosen* engine is. Not something a caller opens for the
duration of a call, but another long-lived worker with its own library, created once and kept
for the life of the application. Two engines are then two queues — which is exactly what
`interrupt` has always meant, since it only ever applied to the engine being addressed. A
module speaking through its own voice does not cut off what the overlay is saying through the
user's, in the same way another application talking over a screen reader does not.

### Losing the screen reader is not permanent

One strike takes the screen-reader path out of service; it does not keep it out. Every few
seconds the event loop starts a fresh worker that tries only the screen readers — never a
speech engine, because the fallback is already speaking and the only reason to be there is to
stop needing it. If one opens, it takes over from the next line onwards; if not, the worker
exits without a word, because something that runs every few seconds must not write to the log.

Two things make this safe rather than a way of adopting a corpse.

**prism refuses a backend whose reader is not running.** Measured on a machine with NVDA up:
JAWS, ZoomText, ZDSR, PC-Talker, Boy PC Reader and Sense Reader all came back
`BACKEND_NOT_AVAILABLE` (16), and NVDA opened with `features=0xb5`. So "it opened" really is
evidence that the reader is there. Because everything rests on that, `prism-sys`'s smoke tests
assert the invariant: any backend that opens must also claim `IS_SUPPORTED_AT_RUNTIME`.

**A retry starts unhealthy and is promoted only on success.** Starting it healthy would open a
window — short, but real — in which lines were handed to a worker about to find nothing, and
each of those would reach the user late, through the fallback, for no reason.

**One searcher, not one per attempt.** Measured: a sweep of the seven screen-reader backends
with only NVDA present costs 86 ms, of which **62 ms is `prism_init`** — the library, not the
looking. The four readers nobody here can test are most of the rest, at 5–6.5 ms each, because
each goes through the process list; JAWS and ZoomText answer in under 100 µs. So the searcher
opens the library once and sleeps between sweeps: every 3 seconds for the first minute, every
30 after, which costs about 0.8% of one core while somebody is plausibly mid-restart. It stops
when the setting is switched off, because that is an explicit answer and looking anyway would
be work done against it, and it stops when its channel closes, so a replaced searcher does not
sit there holding a library nobody can reach.

The first version keyed the eager tier off when the *worker* had started rather than when the
path was *lost*, so on an application that had been running for more than a minute the first
attempt waited thirty seconds. It worked, and from a keyboard that is indistinguishable from
not working. Two tests now cover it: one simulates the loss and requires the screen reader to
be picked up within five seconds, the other that asking twice does not send a second searcher.

**Losing the screen reader is noticed by the deadline, not by an error.** The first live loss
showed that quitting NVDA leaves `prism_backend_speak` *blocked* rather than returning a
failure — so the 300 ms deadline is the normal way this is found out, not a corner case, and
the abandoned thread is the normal cost of it. A line was added to log the moment such a call
finally returns, if it ever does, because whether that thread is parked for ever or merely for
a long time is not something the design can assume.

The old worker is dropped when the new one replaces it, which closes its channel and lets it
finish — except a wedged one, which stays parked with its `Context`. That leak is deliberate:
a parked thread costs less than a wedged application.

### One strike, with two measured exceptions

`voiceover.rs:136-152` is the model: on failure, mark unhealthy, log once, and send the
original text down the refusal channel so `pump()` speaks it through the other path. One
strike is right here for the same reason — prism's documentation says the NVDA endpoint
binding is fixed for the instance's lifetime and never retried, so the second error is
guaranteed by the first.

Two things must not count as a strike:

**The empty string is rejected as invalid UTF-8.** Measured: `speak("")` returns
`PRISM_ERROR_INVALID_UTF8`, while `" "` and German text return 0 — `nvda.cpp:188` treats
"converted to zero UTF-16 units" as invalid input. And an empty string is reachable today:
`modules/daw-hosts/src/main.luau:77` is `host.speech.output(w.title or "plugin window")`, and
Lua's `or` does not fall through on `""`, so an untitled plugin window passes an empty string.
Under `tts` that is a harmless no-op. Under a naive one-strike rule it would permanently
demote the screen-reader path the first time somebody opened an untitled plugin. So prism-sys
returns early for empty or whitespace-only text, and `INVALID_UTF8` never demotes anything.

**`is_speaking` is not implemented.** Measured on this machine: the NVDA backend's feature
bits have the `IS_SPEAKING` bit clear and the call returns `Not implemented`. It is gated on
the running NVDA exposing a newer RPC interface. So it is never called: `Speech::is_speaking()`
is driven from our own pending counter, as `voiceover.rs` already does. A one-strike rule with
that call in the loop would demote a perfectly healthy NVDA on the first `wait_for_speech`
iteration.

### A deadline, because a wedged call cannot be killed

A hung `osascript` can be killed; a synchronous RPC call into a wedged NVDA cannot — it blocks
with no timeout, in our own address space. The deadline therefore lives with the caller:
`pump()` tracks the oldest unanswered utterance and treats anything past **300 ms** as a
strike — demote, speak the outstanding text the other way, hand the worker nothing further,
and have the worker re-check health after its call returns so a late answer never speaks a
stale line. The parked thread is leaked deliberately; that is cheaper than a wedged
application. 300 ms is the `LowLevelHooksTimeout` budget the event loop is already written
around.

When healthy, a call costs 0.11 ms.

## The crate

```
crates/prism-sys/
  Cargo.toml
  build.rs
  src/lib.rs          -- #![cfg(windows)]
  vendor/             -- git submodule, pinned to a SHA
  tests/smoke.rs
```

**Vendor as a non-recursive submodule**, pinned to a **SHA that has been smoke-tested here**,
not to a tag. Precedent: prism issue #49 — from v0.16.2, the NVDA backend's `initialize()`
returned success when NVDA was not running, so a dead backend was selected and every later
call failed silently. That shipped in four consecutive releases over 23 days. Being on the
latest tag would not have helped.

**bindgen at build time**, allowlisted to `prism_.*`, `Prism.*`, `PRISM_.*`, run against the
install prefix rather than the source tree, with `-DPRISM_STATIC`. This is the answer to the
objection that a hand-written `extern "C"` block is a declaration and not a check: generated
from the header we actually compiled, a changed signature becomes a compile error rather than
a wrong calling convention at run time. `LIBCLANG_PATH` is already a prerequisite here for
wxdragon. There is no checked-in fallback — a missing libclang panics naming the variable.

Two measured caveats: `PrismBackendFeature` is **not** fixed-width while
`prism_backend_get_features` returns `uint64_t`, so the bits we use are declared as `u64`
constants by hand; and `PRISM_ERROR_COUNT` moves between versions and is never matched on.

**Gating.** `build.rs` returns early on non-Windows *before* touching `vendor/`, because the
workspace is `members = ["crates/*"]` and `macos-build.yml` runs `cargo build --release` at
the root without checking out submodules. `src/lib.rs` is `#![cfg(windows)]`. In
`crates/host/Cargo.toml`, `prism-sys` goes under the existing `cfg(windows)` target table and
`tts` moves out of the unconditional dependencies into two per-target entries — macOS as
today, Windows without the `tolk` feature.

**Linking.** `static:+whole-archive=prism`, plus `rpcrt4`, `delayimp`, `onecore`,
`uiautomationcore` and `PowrProf`, which prism declares only as CMake interface properties
that a Rust link never reads. Everything else is already claimed by the object files.

## Steps

Each is a separate commit and independently revertible. Steps 1 to 3 change nothing anyone can
hear.

1. **The crate skeleton and the build** (3–4 h). Submodule, empty `#![cfg(windows)]` crate,
   `build.rs` with the midl lookup, the CRT define, the explicit backend list, a
   post-configure assertion that the built backend set is exactly what was asked for, and the
   non-Windows early return. CMake treats an unknown option as a warning and exits 0, so the
   assertion is the only thing that catches an upstream rename.
2. **Bindings, link flags, smoke tests** (3–4 h). The tests assert backend **names**, that
   `speak("")` does not reach our caller as an error, that `NOT_IMPLEMENTED` from `is_speaking`
   is not an error, and that `initialize` is fast — so a `create_best` creeping back in fails
   as an assertion rather than as a three-second startup stall.
3. **`speech/prism.rs`** (4–6 h). The sink, the worker thread, one-strike demotion, the
   deadline, the rearm path. Same four methods as `voiceover.rs`, deliberately *not* a shared
   trait: `voiceover.rs` is borrowed standalone by `crates/macos-check`, and a trait from
   `speech/mod.rs` would break `check-macos.ps1`.
4. **Wire it in** (1–2 h). **User test 1:** does NVDA speak the Kontakt overlay at all — Tab
   through the CSS library, open an untitled plugin window, and check that speech survives it.
5. **The settings switch** (1 h), in `appcfg::SWITCHES`, **default on** — the owner is
   already testing this build live, so the switch exists to turn prism *off* when something
   goes wrong, not to opt into it.
   **User test 2:** the switch is readable with NVDA and takes effect both ways.
   **User test 3:** quit NVDA mid-session with the overlay open — one spoken sentence, then
   the other voice, no gap and no freeze; restart NVDA and re-tick to re-arm.
6. **Application speech separated from module speech** (2–3 h). The reload path gains a tray
   balloon; both application sites gate on a screen reader being present. Symmetrically on
   macOS, where `voiceover::is_running()` already answers the same question.
7. **Packaging, licensing, docs** (2–3 h). The two DLLs leave `package.ps1`. This repository
   has **no LICENSE file** while every crate declares `GPL-3.0-or-later`; MPL-2.0 §3.2
   requires telling recipients how to obtain the source, so the GPL-3 text goes to the root and
   prism's `LICENSES/` tree into the ZIP. Prism's own `NOTICE` is not a sufficient attribution
   list — it omits highway (Apache-2.0) and NVGT (Zlib). Cargo sees prism-sys as having no
   dependencies, so nothing will ever tell us about a CVE in the seven vendored libraries;
   record their versions next to the pinned SHA.
8. **Remove `tts` from the Windows build** (1 h), once the trial has held. Its own commit,
   doing nothing else.
9. **Braille** (2–3 h), deferred, behind its own switch. **User test 4:** does a display show
   the announcements without thrashing during rapid Tab navigation.

## What can still go wrong, worst first

**Both paths go silent.** Every route to it found so far has a measured cause and a named
mitigation: the empty-string strike cascade, `is_speaking` counted as a strike, a refused
interrupting line whose text is not handed back, or the `tolk` feature left enabled so that
the fallback is the same dead NVDA.

**The GUI thread wedges.** A blocking RPC call into a hung NVDA cannot be killed. Mitigated by
the dedicated thread and the 300 ms deadline; the parked thread is leaked on purpose.

**The process aborts**, from a C++ exception crossing the FFI boundary or a null backend
pointer. Mitigated by null checks and by compiling OneCore's throwing paths out of reach of
the event loop — not fully eliminable, because prism does not guard its own boundary.

**A whole-archive regression ships**: zero backends, exit 0, total silence, no diagnostic.
Only the name-asserting smoke test can catch it, and today that test runs only where somebody
runs it.

**An upstream behavioural regression on a version bump.** Issue #49 is the precedent, in our
backend and our failure mode. Also open: #68 (`prism_backend_output` rejecting valid UTF-8 by
byte-length parity — the braille call), #97 (SAPI silently dropping any string that starts
with `<`), #96 (text corrupted on some CPUs by the vendored simdutf). The `<` bug is worth a
guard in prism-sys regardless.

**The application does not launch**, because a delay-load flag was dropped when the backend
list changed. Loud and immediate rather than silent, and caught by the smoke test that starts
the executable.

**Two CRTs in one process**, signalled only by an `LNK4098` buried in cargo output. Prism's
default is a generator expression that makes a debug build `/MTd`. Removed entirely by passing
`CMAKE_MSVC_RUNTIME_LIBRARY` explicitly — and by not trusting the `cmake` crate's
`.static_crt(false)`, which emits a `cc`-crate flag that prism's cache variable then overrides.

**A contributor cannot build it**: missing ATL, missing midl outside a developer shell, an
uninitialised submodule. Loud failures, but they land on somebody who did not sign up for
them. `build.rs` finds midl itself and names the submodule command in its panic; ATL joins
`LIBCLANG_PATH` and Ninja in the README.

**Build time**: about 30 s once per clean build, and roughly 5 s added to every relink of the
39 MB binary because of the whole-archived 14 MB library.

## Open questions

- **Windows CI.** There is none today. Prism's own workflow runs on `windows-2025-vs2026`,
  not a standard hosted label, and prism needs C++23 including `<expected>` and `<flat_set>`.
  Whether a stock `windows-latest` can compile it is unknown and settleable with a throwaway
  workflow in about twenty minutes. Note that `Swatinem/rust-cache` will not cache a workspace
  crate's `OUT_DIR`, so without an explicit cache key on the pinned SHA every run pays the
  full build.
- **Whether an NVDA-only configuration drops the ATL prerequisite.** Settled by configuring
  with only NVDA and checking `dumpbin -directives` for `atls.lib`.
- **`crates/macos-check` does not borrow `speech/mod.rs`**, only `voiceover.rs` — so
  `check-macos.ps1` cannot see the file this change edits. Worth closing while the file is
  being touched anyway.
- The relicensing of `idl/nvdaController.idl` rests on prism's author stating that permission
  was given, while the file still carries NV Access's LGPL-2.1 header. Practical exposure is
  low, since LGPL-2.1 §3 permits GPL redistribution either way, but it is the one licence
  claim that rests on a third party's word rather than on a readable file.
