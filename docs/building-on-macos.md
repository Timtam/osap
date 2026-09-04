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
again. Someone who is testing rather than developing needs that command and
[macos-tester-briefing.md](macos-tester-briefing.md), and nothing else on this page.

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
README — and a zip of the same. Read [macos-permissions.md](macos-permissions.md) before
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

## Checking macOS code from a Windows machine

Most of this port is developed on Windows. Two things make that possible, and it is worth
knowing which one catches what:

- `./check-macos.ps1` runs `cargo check --target aarch64-apple-darwin`. It is the compiler
  front end only — it never links — and it covers the **backend** and its two support
  files. It catches wrong signatures and missing feature flags within minutes.
- `.github/workflows/macos-build.yml` builds and links the **whole thing** on a real Mac
  runner — Intel by default, see above — and uploads the packaged app. That is the only place a missing framework, a bad
  `#[link]`, or an undefined Carbon symbol shows up — and the only place the GUI layer is
  compiled for macOS at all, since the check crate deliberately excludes it.

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
and rewrites it, and it carries four things across: `automation-platform.log`, its rotated
`.log.1`, `settings.toml`, and any `probe-*.png` the probe has written. Everything else there
is build output. It did not always: `git pull && ./bootstrap-macos.sh` used to throw away the
log of the session that prompted the fix, along with the settings and the screenshots a tester
had been asked to send.

## When something goes wrong

Send `automation-platform.log`. It sits beside the `.app` — or, if that folder was not
writable, in `~/Library/Application Support/AutomationPlatform/`; the log's own first lines
say which. For much more detail:

```bash
AUTOMATION_PLATFORM_TRACE=1 open AutomationPlatform.app
```

The first block of the log is an account of the machine as the application sees it. Most
questions we would ask are already answered there.
