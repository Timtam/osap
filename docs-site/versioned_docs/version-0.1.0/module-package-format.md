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

[capabilities]                  # Default-deny, see study §7
require = ["window.read", "input.click", "screen.imagesearch", "ocr", "speech"]
# "ffi.native" only if native/ is used — highest level, signature-required

[native]                        # optional, only with ffi.native
"windows-x64" = "native/windows-x64/foo.dll"
"macos-arm64" = "native/macos-arm64/libfoo.dylib"
```

TOML, because it is declarative and **readable without code execution** — the host can check capabilities/trust *before* Luau runs.

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
