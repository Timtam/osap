# Building and running on macOS

*What a Mac needs to compile this, what to expect the first time, and what will and will
not work yet. The macOS backend was written without a Mac — see
[macos-port.md](macos-port.md) — so this page is also the list of things nobody has
watched happen.*

## Before you start: what works today

Be a little suspicious of a first run. As of this writing:

- The application builds, launches, and lives in the **menu bar** (no Dock icon).
- The log is written and contains a full account of the machine — permissions, displays,
  screen reader, versions.
- **One overlay activates: sforzando standalone.** That was not true when this page was
  written, and a tester disproved it on 2026-09-04 — his log shows the overlay coming up,
  announcing its first read-out, and answering Tab and Shift-Tab across all three of them in
  about 100 ms each. Everything else is still Windows-only: a module identifies its plugin by
  a window class (`NINormalWindow`, `Vst3PlugWindow`, `#32770`), and one whose matcher has no
  `macos = { … }` block correctly never matches. So most of what is installed sees windows and
  does nothing, and that is expected rather than a fault.
- Speech goes through the platform's **own voice** unless **Speak through VoiceOver** is
  ticked in the Application settings tab. Ticked, it is handed to VoiceOver and comes out
  the way VoiceOver says everything else. It is off until asked for: the first line through
  that path raises a macOS Automation consent dialog, and that should follow a request
  rather than a launch.

What is worth reporting is anything the log cannot explain.

## The one-command path

```bash
./bootstrap-macos.sh
```

Installs whatever is missing, builds, packages, and prints what to do next. Safe to run
again. Someone who is testing rather than developing needs that command and the tester briefing they
were sent, and nothing else on this page.

## Intel or Apple silicon

`cargo build` produces a binary for the Mac it runs on, and **the two are not
interchangeable**: an Apple-silicon binary will not start on an Intel Mac at all. Rosetta
translates Intel code so it runs on Apple silicon, never the other way round. So a build
someone else sends has to have been made for your architecture — `uname -m` says which you
have, `arm64` or `x86_64`. It is the strongest argument for building locally.

**The CI job builds a universal binary**, so what it hands you runs natively either way and
the question does not arise. It compiles both slices on one Apple-silicon runner —
`wxdragon-sys` cross-compiles wxWidgets on purpose, and nothing else in the macOS build is
architecture-specific — and refuses to publish a bundle that turns out to contain only one of
them.

Locally, one binary that runs on both is a *universal* binary — two builds joined with
`lipo`, which is what the CI does and what this does by hand:

```bash
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
./package-macos.sh --universal
```

Right for a release, where one download has to work anywhere. Not worth the doubled build
for a test build that one known person installs on one known machine.

## Prerequisites, if you would rather do it by hand

```bash
xcode-select --install          # clang, and the libclang bindgen needs
brew install cmake ninja        # ninja is optional; cmake is not
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Unlike Windows, `LIBCLANG_PATH` does not need setting — the Command Line Tools put
libclang where bindgen looks.

## Building

```bash
cargo build --release
```

The **first** build compiles wxWidgets from source: the `wxdragon-sys` build script
downloads a ~100 MB pinned wxWidgets 3.3.2 tarball and builds it with CMake. Expect
several minutes and a network connection. Later builds reuse it.

Then, for something a person can actually run:

```bash
./package-macos.sh
```

That produces `dist/AutomationPlatform/` with the `.app`, the modules beside it, and a
README — and a zip of the same. When the documentation site has been built
(`cd docs-site && npm ci && npm run build`) and PowerShell 7 (`pwsh`) is installed, the
documentation goes beside them as `docs/`: pages that `docs-offline.ps1` converts to work from
the folder, without a server. Without either, the package is made without it, and the script
says which is missing. Read [macos-permissions.md](macos-permissions.md) before
launching it, because two of the three ways this can fail look nothing like permissions.

## Running it without packaging

`cargo run --release` works and is the fast loop, with one macOS-specific annoyance worth
knowing before it costs you an hour:

**A loose binary is a different application every time you build it.** macOS records
Accessibility permission against the executable's identity, and an unsigned binary's
identity is its path and its content — so every `cargo build` invalidates the grant and you
are asked again. There is no way around it short of building a bundle with a stable
identifier, which is what `package-macos.sh` does. If the development loop becomes
unbearable, package once and replace only the binary inside the bundle.

A loose binary also gets a Dock icon and a regular application's menu bar, because
wxWidgets checks whether it is inside a `.app` and gives up on the agent behaviour when it
is not. Nothing is broken; it just looks different from the shipped shape.

## Running the management commands

Inside a bundle there is no `automation-platform` on the path. The executable is at:

```bash
AutomationPlatform.app/Contents/MacOS/automation-platform list
```

`list`, `search`, `install`, `update` and `uninstall` all work from there and print to the
terminal.

## Measuring text recognition {#measuring-text-recognition}

`ocr-bench` measures what Apple Vision's text recognition costs on the Mac it runs on. It reads
ten pictures the executable carries — five layouts, each drawn for a standard display and for a
Retina one: imitations of sforzando's read-outs (a value in a black well set into grey, the
digits about 11 pixels tall at 1x as in sforzando's own fields) and a line of words wider than
400 points — so a CI runner, a tester's Mac and anybody else's read the same pixels. It opens no
window, captures nothing from the screen, needs no permission, never speaks and leaves the
application's log alone. Quit the application first: its own reads would compete for the
processor.

```bash
AutomationPlatform.app/Contents/MacOS/automation-platform ocr-bench           # the full run
AutomationPlatform.app/Contents/MacOS/automation-platform ocr-bench --quick   # a shorter one
```

| Option | What it does |
|---|---|
| `--quick` | 3 passes a cell instead of 6, which is too few for a verdict; 2 pictures for the variants instead of 5; one process per warm-up instead of three; 24 s of idle instead of 84 |
| `--quiet` | prints only where the file is and that it is done; everything else goes to the file alone, so that a screen reader does not read a hundred lines while the processor is meant to be measuring |
| `--long-idle` | adds one pass after 1, 2, 5 and 10 minutes with nothing to do, on the thread that read before and on a fresh one: 36 minutes more |
| `--capture-ms N` | counts the retry ladder's 250 ms budget from N ms before each pipeline read, standing in for the screen capture it does not make; 50 by default, the median capture on the tester's Intel Air; 0 to 10000 |
| `--out FILE` | writes to `FILE` instead of `ocr-bench-N.txt` beside the `.app` (the first N not taken; the fallback folder when that one is read-only) |
| `--summary FILE` | appends each section's table to `FILE` in Markdown as soon as the section is done; CI passes `$GITHUB_STEP_SUMMARY` |

A download carries `measure-text-recognition.command` beside the `.app`, which runs it with
`--quiet` and passes on whatever else it is given: `zsh measure-text-recognition.command` in
Terminal (a double-click opens it in Terminal too, where Gatekeeper may ask first). It does not
start while the application is running, which it tells by `pgrep -f` on the executable's path.
It ends with the Glass sound when the run finished and the Basso sound when it did not, and then
shows the file in Finder, selected.

What it does, in this order. Every line starts with `OCR BENCH:` and is written to the file the
moment it is known, so a pass that kills the process keeps everything before it:

1. **The machine**: model, processor, cores, memory, the macOS version, the thermal state, Low
   Power Mode, the load average, the power source and whether VoiceOver runs (these again at the
   end); the text recognition revisions this macOS has and the one a new request uses; the
   compute devices Core ML lists and the ones Vision offers text recognition, per stage (both
   from macOS 14 on).
2. **First passes**, each in a process of its own that the benchmark starts: after the
   application's own warm-up (one accurate pass over six dark bars, on a thread of its own),
   after a warm-up over a line of words, and with none — then the first real pass on another
   thread, and the pass after it. One process comes first and is not counted, so that the counted
   ones all find the file cache warm, and the order of the three is turned by one each round.
3. **Probes**: each variant that calls what no Mac has run for this application yet — request
   revision 2 or 3, a compute device — runs one pass in a process of its own first. One that dies
   there, or that Vision refuses, is left out of everything below, with the reason.
4. **The pipeline**: every picture read as `host.ocr.recognize` reads a region once its capture
   is in hand — the content crop, the blank guard, the enlargement, the retry ladder — and how
   many Vision passes each read made. Then the same again with every request of every rung asking
   for revision 2, where this macOS has it and its probe went through.
5. **The engine**: one Vision pass with today's request (`prod`) and with variants that change
   one thing each: the fast level; request revision 2 or 3; a minimum text height of 1/32 or
   0.25; one request reused; the ink enlarged toward 48 pixels instead of 64; the request pinned
   to the CPU, the GPU or the Neural Engine, where Vision offers it (macOS 14 and later); and the
   language en-US. `prod-b` is `prod` again under another name, the control. Every cell (a
   variant over a picture) runs once a round, in an order that changes from round to round, so
   that whatever a switch between models costs falls on every variant alike; the first pass of
   each cell is kept apart. A pass is timed as the application spends it: making the request,
   the handler, `performRequests` and reading the results out. A variant this Mac cannot run says
   why.
6. **Threads**: the first pass on a fresh thread once the process is warm, one thread alone
   against two at once, and `prod` in the engine against `prod` pass after pass, which says what
   switching between variants cost.
7. **Idle**: one pass after 2, 10 and 30 seconds with nothing to do, each once on the thread that
   read before and once on a fresh one; `--quick` waits 2 and 10 seconds.

Each section ends with its table in Markdown, and the last line says `done`.

**How a variant is judged.** Against `prod`, picture by picture: "clearly faster" when an exact
two-sided Mann-Whitney test between the two sets of warm passes gives p < 0.01 *and* the median
is at least 10 % and 5 ms lower; "clearly slower" the other way round; otherwise "no clear
difference". Three passes against three cannot reach p < 0.01, so `--quick` gives no verdicts.
If `prod-b` comes out "clearly" anything, the run says it is too noisy for verdicts. Whether a
variant read each picture right is reported beside its speed, never folded into it.

The timings are the whole machine's at that moment, and those of a command-line process rather
than the application: every thread that times a pass asks for user-initiated quality of service,
as the application's recognise thread does (the application's event loop reads at
user-interactive), and the run says which class each section's thread actually had; nothing
captures the screen. Compare a variant with `prod` from the same run; milliseconds from two
machines say as much about the machines as about the settings.

**In CI**, the `ocr-bench` job of `.github/workflows/macos-build.yml` runs the full benchmark
from the downloaded zip on the `macos-15` and `macos-26` runners (arm64 virtual machines) and on
`macos-15-intel`, the one runner that executes the x86_64 slice. It runs when the run built the
executable, and in every run started by hand. The job's log has every line, the run's summary
page the tables, and each leg's whole output is kept as the artifact `ocr-bench-<runner>.txt`,
with the runner image on its last line. The job fails only when the benchmark did not finish; a
picture today's request read wrong, or one Vision refused, is a warning. It also runs the
tester's `.command` with `--help`, the one path that is harmless there.

## Checking macOS code from a Windows machine

Most of this port is developed on Windows. Two things make that possible, and it is worth
knowing which one catches what:

- `./check-macos.ps1` runs `cargo check --target aarch64-apple-darwin`. It is the compiler
  front end only — it never links — and it covers the **backend** and its two support
  files. It catches wrong signatures and missing feature flags within minutes.
- `.github/workflows/macos-build.yml` builds and links the **whole thing** on a real Mac
  runner — both slices, on Apple silicon, see above — and uploads the packaged app. That is the only place a missing framework, a bad
  `#[link]`, or an undefined Carbon symbol shows up — and the only place the GUI layer is
  compiled for macOS at all, since the check crate deliberately excludes it.
  It does not run on its own: every push to `main` that touches code runs the **Build** workflow
  (`.github/workflows/build.yml`), which runs it beside the Windows build, so a run that
  succeeds holds both downloads — `automation-platform-<version>-<commit>-macos.zip` and
  `…-windows`. The macOS one is the zip `ditto` made, uploaded as it is, so one unpacking
  gives the folder with the `.app`, the modules, the documentation and the README.

If you are changing macOS code, assume the check is necessary and the CI run is the proof.

## Two things about the folder it builds into

**The log accumulates, and only the last block is about this run.** It is opened in append
mode, so every launch adds another block to the END of the file — the top of it is the first
time the application was ever started, before anything was granted. Reading `head` of it after
granting a permission reports the opposite of the truth. The last block is the one that counts:

```bash
grep '\[env\]' dist/AutomationPlatform/automation-platform.log | tail -20
```

**A rebuild keeps the evidence, and only the evidence.** `package-macos.sh` empties `dist/`
and rewrites it, and it carries five things across: `automation-platform.log`, its rotated
`.log.1`, `settings.toml`, any `probe-*.png` the probe has written, and any `ocr-bench-*.txt`
the [text recognition measurement](#measuring-text-recognition) has. Everything else there is
build output. It did not always: `git pull && ./bootstrap-macos.sh` used to throw away the
log of the session that prompted the fix, along with the settings and the screenshots a tester
had been asked to send.

## When something goes wrong

Send `automation-platform.log`. It sits beside the `.app` (beside the original one when macOS
runs a translocated copy, see [below](#translocation)) — or, if that folder was not writable, in
`~/Library/Application Support/AutomationPlatform/`; the log's own first lines say which. For
much more detail:

```bash
AUTOMATION_PLATFORM_TRACE=1 open AutomationPlatform.app
```

The first block of the log is an account of the machine as the application sees it. Most
questions we would ask are already answered there. It is written when the application starts,
at the top of the file that session began in. After 8 MB the log goes on in a new file whose
first line is `log continued`, and the earlier part — that first block with it — is moved to
`automation-platform.log.1` beside it; so send that file too when it is there. After a second
8 MB the first block is gone: quitting and reopening the application writes it again.

Its second line, after `session start`, is the build, which is how a report is matched to the
download it came from:

```text
[host] version 0.1.0, build 6c95c8b
```

The same build is at the end of the Modules window's title
(`Automation Platform — Modules (0.1.0, build 6c95c8b)`) and near the top of the package's
`README.txt`, where `package-macos.sh` writes it (`--commit`, or this checkout's commit when
left out). It is the commit the executable was compiled from, with `-modified` when the working
tree had uncommitted changes, and in a download it is the commit in the download's name.

When the CI job reused an earlier run's executable because no Rust had changed, the executable
knows only that earlier commit. The job then writes a `build-info.txt` beside the `.app` naming
the download's commit, the log gives that one and adds `(binary built from <commit>)`, and the
README's `Build:` line says both. No other package has the file. A reused `.app` that cannot
find it, because it has been moved away from its folder or macOS is running a translocated copy
whose original it could not find (the log's `translocated` line says so, see
[below](#translocation)), names only its executable's commit.

The application uses the file only when its `binary=` line names the executable beside it. One
left in the folder by an older download that a newer one was unpacked over (to keep
`settings.toml` and the log) names another executable, or none, and would otherwise pass the new
build off as the old one; it is not used, the log says so on the line after the build, and the
build named is the executable's own.

### Opened with the quarantine flag still set {#translocation}

A download opened without removing its quarantine flag — with Open on the Finder's context
menu, or Open Anyway in Privacy & Security, instead of the `xattr` line in the README — is run by
macOS from a read-only copy of the `.app` alone, at a random path under
`/private/var/folders/…/AppTranslocation/`. This is App Translocation, and macOS makes a new copy
at every launch until the flag is removed. The folder around the copy holds nothing else.

So the application asks the Security framework where the original `.app` is, before it reads
anything from its folder, and uses the folder around the original for its modules,
`settings.toml` and log — the log, as anywhere, only when that folder can be written, which the
`app folder` line says. The header says what happened, on the line after the executable (and the
`[env]` block repeats the `translocated` line):

```text
[host] executable /private/var/folders/…/AppTranslocation/…/d/AutomationPlatform.app/Contents/MacOS/automation-platform
[host] translocated: YES — macOS is running a read-only copy of the .app from …, because it was opened with its download quarantine flag still set. The original is /Users/…/AutomationPlatform/AutomationPlatform.app, and the folder around it is the application's folder (the log's `app folder` line says whether it can be written). …
[host] app folder /Users/…/AutomationPlatform (writable)
[host] modules folder /Users/…/AutomationPlatform/modules: 12 folders
```

An `.app` that runs where it is says `translocated: no`; a loose binary out of `target/` says
nothing. The two functions that answer, `SecTranslocateIsTranslocatedURL` and
`SecTranslocateCreateOriginalPathForURL`, are not in the SDK's public headers and are looked up by
name when the application starts rather than linked, so a macOS without them still starts. When
they are missing, or cannot find the original, the line says `YES` and why, and the application
uses the folder around the copy: no module loads, settings changes are not saved (the log says
`cannot save settings to …` at the first one), and the log goes to
`~/Library/Application Support/AutomationPlatform/` because the copy's folder cannot be written.
The same happens when they answer "not translocated" for an `.app` that runs from under
`AppTranslocation`: the path is not overruled by an answer that contradicts it. Removing the flag
is the clean way in every case; the line repeats how.

**An `.app` moved out of its folder** — dragged into Applications on its own, say — reads the
`modules` folder beside it in its new place, where there is none. The header's `modules folder`
line names the folder it looked in and says `does not exist`, and the `no modules to load` line
names it again and says what that means. The header line says the same in a start with module
folders on the command line, which does not read that folder at all. Put the `.app` back beside
its `modules` folder, or move the whole folder.
