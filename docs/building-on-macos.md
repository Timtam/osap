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

**The neural text recogniser** goes into the package only when it is asked for:
`./package-macos.sh --onnxruntime DIR`, where `DIR` is Microsoft's
`onnxruntime-osx-universal2-1.22.0.tgz` unpacked. `tools/onnxruntime-mac.txt` names that archive,
its address, its size and its SHA-256, and is the one place they are fixed; CI fetches it from
there, and refuses an archive whose SHA-256 differs. The script takes the folder as it is and
checks nothing of the archive, so compare the archive's `shasum -a 256` with `sha256=` there
before unpacking it. The script copies ONNX Runtime's dylib to `Contents/Frameworks/libonnxruntime.dylib`, strips
its local symbols (`strip -x`, both sizes printed) and signs it before the bundle, and copies
PaddleOCR's recognition model (7.8 MB) to `Contents/Resources/ppocr-rec.onnx`. The application
opens the dylib itself at run time — it is never linked, and the executable must not name it:
the script refuses one that does (`otool -L`) before it packages anything — after Vision's
warm-up, and says once in its log whether the recogniser is ready or why it is not. From then on
it reads every small region beside Vision and answers where Vision's accurate ladder read nothing,
by Windows' rule, and on an Intel Mac it checks the fast level's first pass as well (see
[`host.ocr.recognize`'s macOS section](api/ocr.md#macos)). The library
is built for macOS 13.3 and later, and the application does not open it on an older macOS; there,
where either file is missing, or where the library does not load, Vision reads alone, as in a
package without it. Every process that loaded it — the application at its quit, and each process
of `ocr-bench` — releases its session and ONNX Runtime's environment before it ends
(`release` in `backend/paddle_ocr.rs`): ONNX Runtime 1.22 deletes an environment still alive from
a static destructor that `exit()` runs after the mutex of its logging is gone, and macOS's libc++
then aborts the process ("mutex lock failed: Invalid argument", signal 6). Should something still
be inside ONNX Runtime half a second into that release, the process ends without `exit()`'s
teardown (`_exit`) and the same status. The application's ways of quitting that end it — the
tray's Quit, File > Quit and Command-Q — end the main loop, and the release runs at the end of
`run`. AppKit's own `terminate:` would call `exit()` without it, but wxWidgets first asks the
module window to close, which it refuses (it hides instead), so `terminate:` is cancelled for as
long as that window exists; so is the quit Apple Event of the Dock's Quit and of a logout, which
then ends nothing at all (TODO.md). Should an `exit()` begin without the release all the same, a
backstop ends it with `_exit(0)` before ONNX Runtime's teardown, with one line on standard error
— status 0 whatever `exit()` was given, which the backstop cannot know. Without
`--onnxruntime` the package reads with Vision alone, the fast model as the ladder's last rung,
as before.

Beside the `.app` the package also holds `licences/` — the application's GPL text, and with the
recogniser ONNX Runtime's MIT licence and Microsoft's third-party notices, the model's Apache-2.0
licence and where it comes from, and a `README.txt` that says which is which — and
`ocr-pictures/`, the real plug-in captures `ocr-bench` reads beside its own pictures
(`crates/host/bench-data/ocr/real/`, with their manifest and their `NOTICE`). The dylib makes the
zip larger: from about 17 MB to an estimated 53 to 79 MB, which the build job prints.

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
28 pictures the executable carries — fourteen layouts, each drawn for a standard display and for
a Retina one: imitations of sforzando's read-outs (a value in a black well set into grey, the
digits about 11 pixels tall at 1x as in sforzando's own fields, and its light Instrument field),
values with a sign or a decimal point (`-12`, `-0.5 dB`, `+3 ct`, `0.50`), four with ink that is
not text (a level meter, a speaker symbol, an empty well, a text caret), a word cut off at half
its height, and a line of words wider than 400 points — so a CI runner, a tester's Mac and
anybody else's read the same pixels. `--pictures` adds real captures from a folder. It opens no
window, captures nothing from the screen, needs no permission, never speaks and leaves the
application's log alone. Quit the application first: its own reads would compete for the
processor.

Each picture lists the answers that are right: its text, or nothing for the four without text,
where any text read is *invented*; a field whose lone dash a recogniser may drop accepts the dash
and nothing, and the half-clipped word accepts the word and nothing. A reading is compared after
runs of whitespace are made one space, case and all; one that is right only once spaces are
dropped and dash and quote forms made one (`0.00dB` for `0.00 dB`) is reported as right but for
spaces, not as right. Every picture but those that accept nothing must be read; on those that
accept text and nothing, any other text is read wrong.

```bash
AutomationPlatform.app/Contents/MacOS/automation-platform ocr-bench           # the full run
AutomationPlatform.app/Contents/MacOS/automation-platform ocr-bench --quick   # a shorter one
```

| Option | What it does |
|---|---|
| `--quick` | 3 passes a cell instead of 6, which is too few for a verdict; 2 pictures for the variants instead of 5; 3 pictures for the ways of reading instead of every one; one process per first-pass kind instead of three; blocks of 5 s instead of 30 under sustained load; 48 s of idle instead of 168 |
| `--quiet` | prints only where the file is and that it is done; everything else goes to the file alone, so that a screen reader does not read a hundred lines while the processor is meant to be measuring |
| `--long-idle` | adds one pass after 1, 2, 5 and 10 minutes with nothing to do, on the thread that read before and on a fresh one: 36 minutes more |
| `--capture-ms N` | counts the retry ladder's 250 ms budget from N ms before each pipeline read, standing in for the screen capture it does not make; 50 by default, the median capture on the tester's Intel Air; 0 to 10000 |
| `--pictures DIR` | also reads every picture `DIR/manifest.toml` lists, each `<name>@<scale>x.png` beside it, in the pipeline with the drawn ones. `crates/host/bench-data/ocr/real/` holds real Windows captures of value fields and control words, which `tools/ocr-fixtures/crop.py` cuts out of calibration shots (its `real.toml` says where each comes from, and how its answers were checked). A manifest with an unknown key, a missing file, a PNG of another size than its entry says, a label taken twice, or a picture that must be read and accepts nothing, is refused before anything runs |
| `--paddle` | the neural recogniser alone over every picture — on Windows, with what `Windows.Media.Ocr` reads and what Windows answers beside it; on a Mac, where the package carries ONNX Runtime — then how wide a line of words it reads right, four crops of the 2x pictures, and its scores on right and on invented readings. Each picture is read 23 times, the first 3 not timed (`--quick`: 6 and 1). About 20 s on an i7-8700K. `--quiet`, `--pictures` and `--out` work as above; the other options are accepted and change nothing. Exits with 1 when the recogniser cannot be made |
| `--paddle-probe` | only whether the neural recogniser loads and reads one picture (`lone-1@2x`): one line, `ok` with the runtime, the session's time and what it read, or `not available` with the reason; exits with 0 or 1, and writes no file. On Windows as on a Mac |
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
   application's own warm-up (one accurate pass over a line of printed words, the picture
   `line@1x`, on a thread of its own), after the warm-up it made until 2026-10 (the same pass
   over six dark bars), after one over a small field instead, and with none — then the first real
   pass on another thread, and the pass after it; the fast level's first pass in a process with
   no warm-up; the first passes in a second language (`de-DE`) after a warm-up in Vision's
   default; and, where the package carries it, the neural recogniser's first two recognitions
   after its session alone, after its warm-up, and after its warm-up made beside Vision's, with the
   process's peak memory before and after. One process comes first and is not counted, so that
   the counted ones all find the file cache warm, and the order of the kinds is turned by one each
   round.
3. **Probes**: each variant that calls what no Mac has run for this application yet — request
   revision 2 or 3, a compute device, the neural recogniser — runs one pass in a process of its
   own first. One that dies there, or that is refused, is left out of everything below, with the
   reason. The neural recogniser is then made in the benchmark's own process, only once its probe
   went through: the line `paddle | the neural recogniser in this process: …` gives the session's
   time and the runtime, and `paddle | the neural recogniser is not measured in this run: …` the
   reason it is not.
4. **The pipeline**: every picture read as `host.ocr.recognize` reads a region once its capture
   is in hand — the content crop, the blank guard, the enlargement, the retry ladder, as the
   recognise thread climbs it with nothing waiting behind the read, and without the fast pass the
   application makes there only for the second recogniser's counts — under each
   of twelve ways of reading a small region, the *strategies*: `prod` (the application's own on
   this Mac: with the neural recogniser loaded, Windows' rule over the accurate ladder, on an Intel
   Mac with the fast level checked by it first — a line `pipeline | prod reads as the application
   does on this Mac: …` says which; without it, as `old`), `old` (the ladder the application read
   with before the recogniser answered on a Mac, the fast model as its last rung), `rev2`
   (every request of every rung under revision 2), `paddle` (the neural recogniser alone),
   `acc+paddle` and `acc+paddle+rest` (Windows' rule after the first accurate pass alone: the
   recogniser's text when that pass read nothing, without and with `old`'s rungs after it), `rev2-tight` (the first
   pass under revision 2), `rev3>rev2` (revision 2 over the same crop after a first pass that read
   nothing, instead of the whole region), `fast=paddle-checked`, `fast=paddle-strict` and
   `fast=paddle` (the fast level first, believed only where the recogniser reads the same; they
   differ in what they do when fast reads nothing: the accurate ladder and never the
   recogniser's text alone, an accurate pass and then the recogniser's text, or the recogniser's
   text at once), and `fast>acc` (the fast level first, unchecked). Six of them are *candidates*,
   other ways the application may come to read small regions by: `acc+paddle+rest`, `rev2-tight`
   and `rev3>rev2` for the lone digit, and the three `fast=paddle` ones. `old`, `rev2`, `paddle`,
   `acc+paddle` and `fast>acc` are measured *for comparison* only: they read as the application
   did, speak a value nothing checked, or leave out or change more than a candidate would. The run
   prints what each one does, and which it is. A strategy that needs what this Mac lacks — a
   revision, the recogniser — says why and is left out (`prod` never is: it needs nothing), and so
   is `old` where `prod` reads as it does, without the recogniser; one of the others that needs
   the recogniser reads a region whose ink is not one line, or too wide, as `old` does. A picture the blank guard answers is read by the pipeline alone, as
   a read would be. The few pictures read for speed are read six times after a first one kept
   apart, every other small picture twice, for whether it is read right, and every row once a
   round in an order that changes from round to round. Each row says what a read cost, the Vision
   passes and the recogniser's runs it made, who answered (`as captured`, `fast first`, `tight`,
   `tight again`, `whole`, `enlarged`, `fast last`, `Paddle`, `nobody`), how often fast and the
   recogniser read the same, and the time that was not Vision's. After the tables one line says
   what the application's reading read wrong, invented or was refused on, whatever the verdicts —
   `pipeline | the application's reading (prod) …` — and one more the same of the ladder before it,
   `pipeline | the ladder before it (old) …`; the run's summary page has both, after the line
   saying which reading `prod` was on that Mac, and CI adds the two to the run's notes.
5. **The engine**: one Vision pass with today's request (`prod`) and with variants that change
   one thing each: the fast level; request revision 2 or 3; a minimum text height of 1/32 or
   0.25; one request reused; the ink enlarged toward 48 pixels instead of 64; the request pinned
   to the CPU, the GPU or the Neural Engine, where Vision offers it (macOS 14 and later); the
   language en-US; and the neural recogniser instead of Vision, over the whole small region
   (`paddle-raw`) or over the content crop a read makes (`paddle-crop`). `prod-b` is `prod` again
   under another name, the control. Every cell (a variant over a picture) runs once a round, in an
   order that changes from round to round, so that whatever a switch between models costs falls on
   every variant alike; the first pass of each cell is kept apart. A pass is timed as the
   application spends it: making the request, the handler, `performRequests` and reading the
   results out. A variant this Mac cannot run says why. Then the **closing lines** (below).
6. **Threads**: the first pass on a fresh thread once the process is warm, one thread alone
   against two at once, `prod` in the engine against `prod` pass after pass, which says what
   switching between variants cost, the accurate and the fast level each beside the neural
   recogniser at user-initiated and at utility priority against alone, and four blocks of 30
   seconds, alternately Vision alone and Vision beside the recogniser at the measuring thread's own
   priority, the way a read of the application asks it.
7. **Idle**: one pass after 2, 10 and 30 seconds with nothing to do, each once on the thread that
   read before and once on a fresh one, and the fast level and the neural recogniser after the
   same pauses; `--quick` waits 2 and 10 seconds.

Each section ends with its table in Markdown and with how long it took (`… | section took N s`),
and the last line says `done`. A full run is estimated at 20 to 30 minutes on the CI's Intel
Mac, less on Apple silicon, 168 seconds of it waiting on purpose; `--pictures` with the 13 real
captures lengthens the pipeline by an estimated two fifths. The first CI run's section times are what
the rounds are set by.

**The closing lines** weigh the pipeline's candidates by one rule, printed with them: a way of
reading is named when it reads every picture with text right on every read (nothing counts as
right where a picture accepts it), reads nothing on every picture without text, is clearly faster
than `prod` on at least one picture and clearly slower on none; of those, the cheapest by its
medians over the pictures read for speed. Any strategy that is faster but not right is said as
such, never named, with `(prod too)` after a picture `prod` does not read right either. When
`prod` itself reads a picture wrong, or invents text, a further line says which, and names the
cheapest candidate that reads the rest right. And a strategy for comparison that would pass the
rule is said in a line of its own — `for comparison only, never a way the application may read`
— and never named. A `--quick` run, and one whose control came out "clearly" different, names
none.

**How a variant is judged.** Against `prod`, picture by picture: "clearly faster" when an exact
two-sided Mann-Whitney test between the two sets of warm passes gives p < 0.01 *and* the median
is at least 10 % and 5 ms lower; "clearly slower" the other way round; otherwise "no clear
difference". Three passes against three cannot reach p < 0.01, so `--quick` gives no verdicts.
If `prod-b` comes out "clearly" anything, the run says it is too noisy for verdicts. Whether a
variant read each picture right is reported beside its speed, never folded into it.

The timings are the whole machine's at that moment, and those of a command-line process rather
than the application: every thread that times a pass asks for user-initiated quality of service,
as the application's recognise thread does, and the run says which class each section's thread
actually had; nothing
captures the screen. Compare a variant with `prod` from the same run; milliseconds from two
machines say as much about the machines as about the settings.

**In CI**, the `ocr-bench` job of `.github/workflows/macos-build.yml` runs the full benchmark
from the downloaded zip, with `--pictures ocr-pictures`, on the `macos-15` and `macos-26` runners
(arm64 virtual machines) and on `macos-15-intel`, the one runner that executes the x86_64 slice.
It runs when the run built the executable, and in every run started by hand, for at most 45
minutes a leg. The job's log has every line, the run's summary page the tables and the closing
lines (each of them also as an annotation of the run), and each leg's whole output is kept as the
artifact `ocr-bench-<runner>.txt`, with the runner image on its last line. A leg fails when the
benchmark did not finish, when the neural recogniser did not load although the bundle carries
it, and when a process that loaded it read and then ended by a signal or with a status other
than 0: the run itself after its report, one of its children (the report says `died of signal 6
at exit after reading`, or `hung at exit after reading` for one the two-minute limit ended, apart
from a recogniser that did not load; a child of the neural recogniser's first passes fails the
leg too when a signal ended it before its answer, where a Vision probe that dies is one of the
findings the probes are there for), or any process whose standard error holds the exit
backstop's line. Standard error goes into the log as it comes, through `tee`, so it is there
even when the 45 minutes run out. A picture today's request read wrong or Vision refused (the
engine table's `prod` row), and whatever the line `pipeline | the application's reading (prod) …`
names, are warnings — text the recogniser invents where Vision reads nothing among it, as on
Windows. On `macos-15` two
short runs (`--quick`) follow, on copies of the bundle signed again ad hoc: one without ONNX
Runtime, one with only its x86_64 half, which that Mac cannot load; each has to finish and say
that the recogniser is not available, and why, and end with status 0. A third copy has only its
library flagged as a browser's download (`com.apple.quarantine`), and `--paddle-probe` there has
to answer within two minutes, or a warning says so; an `ok` followed by a signal or another
status is an error, and so is a libc++ abort after either answer ("not available" ends with 1
by design). These steps run even when the measurement went red, and the three
copies' output is kept as the artifact `ocr-bench-macos-15-copies`. The job also runs the tester's
`.command` with `--help`, the one path that is harmless there.

The build job fetches ONNX Runtime by `tools/onnxruntime-mac.txt` (cached by its version and
checksum), checks its size and its SHA-256 — an archive that differs from the pin is refused, and
so is a pin file without one, whose error names the archive's own — and refuses a library
without both slices, or one whose slices import Core ML's `MLComputePlan` or
`MLOptimizationHints` strongly (macOS 14.4 and later, where the
library promises 13.3); it prints each slice's `minos`, and warns when one is not the 13.3 the
application keeps as its floor. `package-macos.sh` refuses an executable that links ONNX Runtime
(`otool -L`: the application only ever opens it) before it packages anything; after the upload
the job asks the bundle again, and refuses one without the library or the model. A zip larger
than 80 MB is a warning. The `newer-macos` job asks `ocr-bench --paddle-probe` on macOS 14 and
26, where "not available" is an error, and so is an `ok` from a probe that then ends by a signal
or with another status than 0; it refuses a download whose signature does not verify
(`codesign --verify --deep --strict`) in a last step of its own, so that its other steps still
run. On Windows, the build job runs `ocr-bench --paddle` over the drawn pictures and the real
captures, for at most ten minutes, and keeps the output as
`automation-platform-<version>-<commit>-ocr-bench-windows-paddle.txt`; a picture that must be read
and that the recogniser reads wrong is a warning there.

## Checking macOS code from a Windows machine

Most of this port is developed on Windows. Two things make that possible, and it is worth
knowing which one catches what:

- `./check-macos.ps1` runs `cargo check` for `aarch64-apple-darwin` and for
  `x86_64-apple-darwin`, the two halves of the universal application (`-Setup` installs both
  targets). It is the compiler front end only — it never links — and it covers the **backend**
  and its two support files. It catches wrong signatures and missing feature flags within
  minutes.
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
