# Type-checks the macOS backend without a Mac, for both halves of the universal application.
#
#   .\check-macos.ps1            check it
#   .\check-macos.ps1 -Setup     install the two targets first (one-off), then check
#
# What this does and does not prove: `cargo check` runs the compiler front end and stops
# before linking, so it needs no Apple linker, no SDK, and no Mac — only rustup's
# aarch64-apple-darwin and x86_64-apple-darwin standard libraries, and a clang for one
# Objective-C file a build script compiles (below). It will catch every
# misremembered signature, missing feature flag and type error in code that has never been
# run. It cannot catch a wrong constant, an inverted coordinate axis, or a call that is legal
# and wrong. Treat a clean run as "this could work", never as "this works".
#
# Both targets, because the package is one binary with an Intel half and an Apple-silicon half,
# and a dependency can build for one and not the other: tract, measured for the neural text
# recogniser, needed a C compiler for either (tools/paddle-engines/README.md).
#
# See crates/macos-check/Cargo.toml for why the check goes through a separate crate rather
# than through `host` (wxdragon builds wxWidgets in its build script and cannot do that for
# macOS from here).
[CmdletBinding()]
param([switch]$Setup)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$targets = @("aarch64-apple-darwin", "x86_64-apple-darwin")

if ($Setup) {
  foreach ($target in $targets) {
    rustup target add $target
    if ($LASTEXITCODE -ne 0) { throw "could not add the $target target" }
  }
}

$installed = (rustup target list --installed) -split "`r?`n"
foreach ($target in $targets) {
  if ($installed -notcontains $target) {
    throw "the $target target is not installed — run:  .\check-macos.ps1 -Setup"
  }
}

# A C compiler for the Apple targets, for one file. wry, the web view under host.gui.webview,
# turns on objc2's `exception` feature, whose helper crate compiles one Objective-C file with
# the `cc` crate in its build script, and build scripts run under `cargo check` too. The file
# includes no header, so it needs no Apple SDK: only a clang that compiles for macOS, and an
# llvm-ar for the archive. Visual Studio's own LLVM has both (the installer's "C++ Clang
# Compiler for Windows"; its bin folder is what the Windows build's LIBCLANG_PATH usually names,
# README), and so does any LLVM for Windows. They are given to cargo as CC_<target> and
# AR_<target>, for this script's cargo calls only; one that is already set, in either of the two
# spellings cc reads, is used as it is, and only the files for those still unset are looked for.
$appleTools = [ordered]@{ CC = "clang.exe"; AR = "llvm-ar.exe" }

# The first folder that holds every file $need names (CC, AR or both), as a table from each to
# its path; $null when there is none.
function Find-AppleTools([string[]]$need) {
  $dirs = @()
  # The folder the Windows build was pointed at, first: somebody chose it.
  if ($env:LIBCLANG_PATH) { $dirs += $env:LIBCLANG_PATH }
  $vswhere = if (${env:ProgramFiles(x86)}) {
    Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  }
  if ($vswhere -and (Test-Path $vswhere -PathType Leaf)) {
    $dirs += & $vswhere -all -sort -products * -requires Microsoft.VisualStudio.Component.VC.Llvm.Clang -property installationPath |
      ForEach-Object { Join-Path $_ "VC\Tools\Llvm\x64\bin" }
  }
  # Where the installer puts it, for an installation vswhere does not list. The folders are named
  # by year up to 2022 and by version from 18 on, so the order of their names is not their age:
  # the newest clang first, by the version in its file. Any of them compiles the one file.
  foreach ($base in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if (-not $base) { continue }
    $dirs += Get-Item (Join-Path $base "Microsoft Visual Studio\*\*\VC\Tools\Llvm\x64\bin") -ErrorAction SilentlyContinue |
      Sort-Object -Descending {
        $version = $null
        $exe = Join-Path $_.FullName "clang.exe"
        if (Test-Path $exe -PathType Leaf) {
          [void][version]::TryParse((Get-Item $exe).VersionInfo.FileVersion, [ref]$version)
        }
        $version
      } | ForEach-Object { $_.FullName }
  }
  $onPath = Get-Command clang.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($onPath) { $dirs += Split-Path $onPath.Source }
  foreach ($dir in $dirs) {
    if (-not $dir) { continue }
    $found = @{}
    foreach ($tool in $need) {
      $file = Join-Path $dir $appleTools[$tool]
      if (Test-Path $file -PathType Leaf) { $found[$tool] = $file }
    }
    if ($found.Count -eq $need.Count) { return $found }
  }
  return $null
}

# The variables cc reads first for a target, in either of the two spellings it accepts.
function Test-ToolSet([string]$tool, [string]$target) {
  return [bool]([Environment]::GetEnvironmentVariable("${tool}_$target") -or
    [Environment]::GetEnvironmentVariable("${tool}_$($target -replace '-', '_')"))
}

$tools = $null
$wanted = @(foreach ($target in $targets) {
  foreach ($tool in $appleTools.Keys) { if (-not (Test-ToolSet $tool $target)) { "${tool}_$target" } }
})
if ($wanted) {
  $need = @(foreach ($tool in $appleTools.Keys) { if ($wanted -like "${tool}_*") { $tool } })
  $tools = Find-AppleTools $need
  if (-not $tools) {
    $files = ($need | ForEach-Object { $appleTools[$_] }) -join " and "
    $kinds = ($need | ForEach-Object { (@{ CC = "a clang"; AR = "an llvm-ar" })[$_] }) -join " and "
    throw ("no folder holds $files for the Apple targets, to compile the one Objective-C " +
      "file of objc2's exception helper, which wry needs. Install Visual Studio's `"C++ Clang Compiler for Windows`" " +
      "(Microsoft.VisualStudio.Component.VC.Llvm.Clang) or LLVM for Windows, or set LIBCLANG_PATH " +
      "to a folder that holds $(if ($need.Count -gt 1) { 'both' } else { 'it' }), or set " +
      (($wanted | ForEach-Object { $_ -replace '-', '_' }) -join ", ") + " to $kinds yourself")
  }
  Write-Host "For the Apple targets' one Objective-C file: $(($need | ForEach-Object { $tools[$_] }) -join ', ')"
}

# Its own target directory. Sharing one with the Windows build means every switch between
# the two rebuilds the world, which turns a 20-second check into several minutes.
$env:CARGO_TARGET_DIR = Join-Path $root "target-macos"

foreach ($target in $targets) {
  $before = @{}
  foreach ($tool in $appleTools.Keys) {
    if (Test-ToolSet $tool $target) { continue }
    $name = "${tool}_$($target -replace '-', '_')"
    $before[$name] = [Environment]::GetEnvironmentVariable($name)
    [Environment]::SetEnvironmentVariable($name, $tools[$tool])
  }
  Push-Location $root
  try {
    cargo check --target $target -p macos-check
  } finally {
    Pop-Location
    # Set for cargo, never for the shell this script runs in. Removed through the Env: drive:
    # [Environment]'s setter would be handed an empty string for $null, and leave the name set.
    foreach ($name in $before.Keys) {
      if ($null -eq $before[$name]) { Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue }
      else { [Environment]::SetEnvironmentVariable($name, $before[$name]) }
    }
  }
  if ($LASTEXITCODE -ne 0) { throw "the macOS backend does not compile for $target" }
}

Write-Host ""
Write-Host "The macOS backend compiles for $($targets -join ' and ')."
Write-Host "That is a front-end check only — nothing was linked and nothing was run."
