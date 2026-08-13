# Runs the platform with every REAL module — everything under modules/, which is the whole
# point of keeping the API demos in examples/ and the dev tools in tools/. Loading them by
# hand meant whichever module was not on the current command line went untested; sforzando
# sat unloaded through an entire refactor that way.
#
#   .\run-dev.ps1                 build if needed, then run every module in modules/
#   .\run-dev.ps1 -Calibrate      the same, with the calibration keys armed
#   .\run-dev.ps1 -Build          force a rebuild first
#   .\run-dev.ps1 -Only kontakt,cinematic-studio-strings
#                                 just those, by directory name
#   .\run-dev.ps1 -Examples       run the examples/ set instead
[CmdletBinding()]
param(
  [switch]$Calibrate,
  [switch]$Build,
  [switch]$Release,
  [switch]$Examples,
  [string[]]$Only
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
# Debug is the default, because that is what a build-and-try loop wants. But the image
# matcher is a tight pixel loop, and unoptimised it measured TWELVE SECONDS for a single
# full-region template match — so any judgement about performance has to be made on
# -Release, and a slow overlay is worth re-checking there before believing it.
$profileDir = "debug"
if ($Release) { $profileDir = "release" }
$exe = Join-Path $root "target\$profileDir\automation-platform.exe"

if ($Build -or -not (Test-Path $exe)) {
  # wxDragon needs these; setting them here rather than expecting a configured shell.
  if (-not $env:LIBCLANG_PATH) {
    $llvm = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\Llvm\x64\bin"
    if (Test-Path $llvm) { $env:LIBCLANG_PATH = $llvm }
  }
  Push-Location $root
  try {
    if ($Release) { cargo build --release } else { cargo build }
  } finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw "build failed" }
}

# Spelled out rather than a ternary: `? :` is PowerShell 7+ only, and this script has to
# run under whichever powershell.exe someone happens to have (5.1 is still the default).
$srcName = "modules"
if ($Examples) { $srcName = "examples" }
$srcDir = Join-Path $root $srcName
$dirs = Get-ChildItem $srcDir -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName "module.toml") } |
  Where-Object { -not $Only -or $Only -contains $_.Name } |
  ForEach-Object { $_.FullName }

if (-not $dirs) { throw "no modules found in $srcDir$(if ($Only) { " matching: $($Only -join ', ')" })" }

# A module the app is already running would hold the log file and its own hotkeys.
Get-Process -Name automation-platform -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

if ($Calibrate) { $env:AUTOMATION_PLATFORM_CALIBRATE = "1" }
else { Remove-Item Env:\AUTOMATION_PLATFORM_CALIBRATE -ErrorAction SilentlyContinue }

Write-Host "Starting the $profileDir build with $($dirs.Count) module(s) from $srcDir$(if ($Calibrate) { ' (calibrating)' }):"
$dirs | ForEach-Object { Write-Host "  $(Split-Path $_ -Leaf)" }

Start-Process -FilePath $exe -ArgumentList $dirs -WorkingDirectory $root
