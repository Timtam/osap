# Module Package Format (Draft)

*Status: Draft, 2026-06-21. Open points at the end. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md).*

## Concept

A module is a **self-contained package** (ZIP container) that bundles everything needed to run it: Luau code (multiple files, fixed entry point), sound assets, images for the image search, optional native libs — plus a manifest. In contrast to ReaHotkey (global `Images/`, `Sounds/` folders), **each module carries its own resources** → relocatable, individually versionable and signable.

## Layout

```
serum2.<ext>            (ZIP; in dev mode an unpacked directory)
├── module.toml         Manifest (mandatory)
├── src/
│   ├── main.luau       Entry point (default; overridable in the manifest)
│   ├── overlays/serum2.luau
│   └── lib/coords.luau
├── assets/
│   ├── images/serum2/preset.png
│   └── sounds/focus.ogg
└── native/             (only if the FFI capability is used)
    ├── windows-x64/foo.dll
    └── macos-arm64/libfoo.dylib
```

## Manifest (`module.toml`)

```toml
id         = "com.example.serum2"
name       = "Serum 2"
version    = "1.2.0"
engine_api = ">=1.0, <2.0"      # API version range, see study §6
entry      = "src/main.luau"    # Default, optional
license    = "MIT"

# Other modules loaded first; their exports become reachable via host.require(id).
# Each entry is a module id plus an OPTIONAL space-separated semver requirement,
# verified at load (a bare id accepts any version). Auto-discovered among sibling
# modules / fetched on install.
dependencies = ["com.example.daw-hosts", "com.platform.kontakt >= 1.2, < 2"]
code_module  = false            # true = this module's CODE (its functions) is
                                # loaded into each dependent's VM, reachable via
                                # host.require — not just serialized data. Set it on
                                # base / library modules that others build on.

# Operating systems this module is written for. OMITTING IT MEANS EVERYWHERE — see below.
supported_os = ["windows", "macos"]

[capabilities]                  # Default-deny, see study §7
require = ["window.read", "input.click", "screen.imagesearch", "ocr", "speech"]
# "ffi.native" only if native/ is used — highest level, signature-required

[native]                        # optional, only with ffi.native
"windows-x64" = "native/windows-x64/foo.dll"
"macos-arm64" = "native/macos-arm64/libfoo.dylib"

[screen]                        # optional: which picture host.screen and host.ocr read
capture  = "duplication"        # "standard" (default) | "duplication" (Windows only)
fallback = "none"               # with "duplication": "standard" (default) | "none"
```

TOML, because it is declarative and **readable without code execution** — the host can check capabilities/trust *before* Luau runs.

### `supported_os`, and why omitting it is the safe default

A module is already gated at runtime by its window matchers: one whose matcher has no block
for the current platform never matches, and the module sits there inert. So this field is not
what makes a module correct. It is what lets the application say something useful *before*
running it — warn before installing something that cannot work here, and not pay for loading
it.

What loading an inert module costs is small but not nothing: a VM, a matcher evaluated on
every focus change, any timer it starts, and any global shortcut it registers — which on the
wrong platform claims a key combination for something that can never happen, and shows the
user a conflict dialog about it.

Against that stands the fact that a manifest is a **claim**, and claims rot. Sforzando was
Windows-only one day and worked on both the next; a manifest still saying `["windows"]` would
have excluded a module that had just started working, and the reason would have been a line
in a file nobody reads. So:

- **Omitted means no claim, and no claim means every platform.** Every module written before
  this field existed goes on loading exactly as it did.
- A module that names platforms and not this one is **not loaded**, and says so in the log at
  every start, by name, with the reason.
- `AUTOMATION_PLATFORM_IGNORE_SUPPORTED_OS=1` loads it anyway — for the case where the
  manifest is simply behind the code.
- The install review says *"it can be installed, but it will not be loaded here"* rather than
  refusing. Refusing an install on the strength of one line in a text file is a stronger
  claim than that line can carry.

Names are `windows`, `macos`, `linux` — Rust's `std::env::consts::OS` values — matched
case-insensitively, because the file is written by hand and `"Windows"` is what a person
types.

### `[screen]`, and why it is declared per module

`[screen]` chooses the picture every `host.screen` and `host.ocr` read of the module sees. The
default, `standard`, is how every module has always read the screen. `duplication` reads it
through DXGI Desktop Duplication on Windows, for an application whose picture the standard
path reads frozen or black; `fallback = "none"` makes a read that duplication cannot answer
fail instead of returning the standard picture, for an application where that picture is
wrong rather than slow. The API reference has the details:
[Which picture a read sees](api/screen.md#which-picture-a-read-sees).

It is a declaration in the manifest, not a per-call option and not something the host
detects, for three reasons:

- **Whether the standard picture is wrong is a property of the target application**, and the
  module's author is the one who has tested that application. The person using the module —
  often blind — cannot see a frozen picture to report it.
- **One declaration covers every call.** A template cut from one picture is never matched
  against the other, which matters because the two can differ (HDR is converted for
  duplication, for one).
- **A frozen frame cannot be told from a still screen**, and a menu waiting for input is still
  by design. A host that switched sources by guessing would make the same template match and
  stop matching for no reason anyone could hear.

**Who decides for a VM:** the module's own `[screen]` if it has one; otherwise the nearest
`code_module` dependency that declares one — smallest depth first, then the earliest in
manifest order (`dependencies` before `optional_dependencies`) — counting only dependencies
that themselves require `screen` or `ocr`; otherwise `standard`. So a runtime that every game
module is built on declares it once, and a single module can still override it. Resolved each
time the VM is built, from the manifests as they are on disk then: reloading a module after
editing its table applies it, and reloading a runtime after editing ITS table applies it to
every module that lists the runtime under `dependencies`, because those are rebuilt with it. A
module that has the runtime only under `optional_dependencies` is not rebuilt with it; reload
that module itself. Unknown values are named in the log and read as `standard` (for `capture`) or the standard fallback (for
`fallback`); the module still loads. Hosts that predate the table ignore it.

On macOS the table is accepted and ignored. A switch in the Application settings tab ("Let
modules that ask for it read the screen through the graphics card", on by default) turns
duplication off for every module on a Windows machine where it misbehaves.

## Multi-file code (fixed entry point, no mono-file)

- A fixed entry point (`entry`) that pulls in any number of files via `require`.
- `require` is **package-relative and sandboxed**: `require("@self/overlays/serum2")` or `require("./lib/coords")`. Resolution ONLY within the package (+ host-provided std libs). `../` traversal is normalized/rejected.

## Resource resolution (the core of your requirement)

**Principle: Modules address resources via logical, package-relative paths; the host owns the mapping `logical path → real bytes / real path`.** The module never knows where it is physically stored.

How the host knows the calling module: Each Luau VM, i.e. host API binding, is **bound to its package**. When module code calls `host.path("assets/...")`, the host already knows the caller's package root — no ambiguity, no path discovery in the module.

Two modes:

1. **Capabilities accept logical paths directly** (the normal case):
   `overlay:addGraphicalButton{ image = "assets/images/serum2/preset.png" }` — the host resolves internally against the package root and loads the bytes. The module never gets to see a real path.
2. **Escape hatch `host.path(rel) -> string`**: returns a real absolute path when an *external* consumer (native FFI lib, external tool) strictly requires a real file.

Plus `host.read(rel) -> string|buffer` for direct byte reads (config/JSON from the package).

## Loading/runtime model

- **Extract-on-install** into a content-addressed cache directory (`<cache>/<id>/<version>-<hash>/`). Reason: native libs MUST be real files (`dlopen`/`LoadLibrary`); image search/sound/OCR benefit as well. The logical addressing stays identical — the host prepends the cache root to the relative paths; the module is unaware of this.
- **Dev mode:** unpacked directory instead of ZIP, same logical addressing → live edit without repacking.

## Security

- Resource resolution is anchored at the package root → a module **cannot break out of its package** via the resource API (part of the sandbox model, study §7).
- `require` is package-scoped; path traversal is blocked.
- Resource read = low capability (own package) → allowed by default. Native libs = `ffi.native`, signature-required + out-of-process sandbox.

## Cross-references

- `engine_api` ↔ API versioning (study §6).
- The entire package (manifest + content hash) is **Ed25519-signed** for the trust store (study §7/§11).
- Versioned web docs (TODO) are generated from manifest + host API.

## Decided (2026-06-21)

- **Manifest = TOML** (declarative, verifiable without code execution — trust/capability check before Luau start).
- **Resource addressing = logical, package-relative paths + `host.path()` escape hatch** — the module stays completely location-independent.
- **Loading model = extract-on-install** into a content-addressed cache (`<cache>/<id>/<version>-<hash>/`).

## Still open

- Container: **ZIP** (recommended, random access, universal) vs. tar.zst.
- Package extension/name (depends on the still-open product name).
- Enforcing convention folders for assets (`assets/images`, `assets/sounds`) vs. leaving them free.
- Multiple overlays per package (e.g. the Dubler2 family) — probably yes: one entry point registers multiple overlays, resources under `assets/<overlay>/`.
