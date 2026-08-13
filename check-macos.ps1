# Type-checks the macOS backend without a Mac.
#
#   .\check-macos.ps1            check it
#   .\check-macos.ps1 -Setup     install the target first (one-off), then check
#
# What this does and does not prove: `cargo check` runs the compiler front end and stops
# before linking, so it needs no Apple linker, no SDK, and no Mac — only rustup's
# aarch64-apple-darwin standard library. It will catch every misremembered signature,
# missing feature flag and type error in code that has never been run. It cannot catch a
# wrong constant, an inverted coordinate axis, or a call that is legal and wrong. Treat a
# clean run as "this could work", never as "this works".
#
# See crates/macos-check/Cargo.toml for why the check goes through a separate crate rather
# than through `host` (wxdragon builds wxWidgets in its build script and cannot do that for
# macOS from here).
[CmdletBinding()]
param([switch]$Setup)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$target = "aarch64-apple-darwin"

if ($Setup) {
  rustup target add $target
  if ($LASTEXITCODE -ne 0) { throw "could not add the $target target" }
}

$installed = (rustup target list --installed) -split "`r?`n"
if ($installed -notcontains $target) {
  throw "the $target target is not installed — run:  .\check-macos.ps1 -Setup"
}

# Its own target directory. Sharing one with the Windows build means every switch between
# the two rebuilds the world, which turns a 20-second check into several minutes.
$env:CARGO_TARGET_DIR = Join-Path $root "target-macos"

Push-Location $root
try {
  cargo check --target $target -p macos-check
} finally { Pop-Location }
if ($LASTEXITCODE -ne 0) { throw "the macOS backend does not compile" }

Write-Host ""
Write-Host "The macOS backend compiles for $target."
Write-Host "That is a front-end check only — nothing was linked and nothing was run."
