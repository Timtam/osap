# Builds a self-contained, distributable copy — everything a tester needs and nothing else.
#
#   .\package.ps1                 build and stage into dist\, then zip it
#   .\package.ps1 -Version 0.3.0  name the zip for that version instead of the date
#   .\package.ps1 -NoZip          stage only, for looking at what would ship
#   .\package.ps1 -NoBuild        package whatever is already in target\release
#
# The layout is the one the app already expects when it is started with no arguments: it
# looks for `modules` NEXT TO THE EXECUTABLE (registry::modules_dir), so a tester runs the
# .exe and gets every module without a command line. Settings and logs are written beside
# the executable too, which is why the folder has to be somewhere writable — extracting into
# Program Files will not do.
[CmdletBinding()]
param(
  [string]$Version,
  [switch]$NoZip,
  [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$rel = Join-Path $root "target\release"

if (-not $NoBuild) {
  # wxDragon needs this; set here rather than expecting a configured shell, exactly as
  # run-dev.ps1 does.
  if (-not $env:LIBCLANG_PATH) {
    $llvm = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\Llvm\x64\bin"
    if (Test-Path $llvm) { $env:LIBCLANG_PATH = $llvm }
  }
  # A running copy holds its own .exe open, and cargo reports that as a link error that
  # looks nothing like "the app is running".
  Get-Process -Name automation-platform -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 500
  Push-Location $root
  try { cargo build --release } finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw "build failed" }
}

# What has to be there, by name. Checked rather than copied blindly, because each one fails
# SILENTLY and differently if it is missing: without the speech clients the overlay runs and
# says nothing, and without DirectML the second OCR engine is gone and small text stops being
# read — neither looks like a missing file to whoever is testing.
$required = @(
  @{ Name = "automation-platform.exe";      Why = "the application" },
  @{ Name = "nvdaControllerClient64.dll";   Why = "speech through NVDA" },
  @{ Name = "SAAPI64.dll";                  Why = "speech through System Access" },
  @{ Name = "DirectML.dll";                 Why = "the second OCR engine (small text)" }
)

$stage = Join-Path $root "dist\AutomationPlatform"
if (Test-Path (Join-Path $root "dist")) { Remove-Item (Join-Path $root "dist") -Recurse -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

$missing = @()
foreach ($item in $required) {
  $src = Join-Path $rel $item.Name
  if (Test-Path $src) { Copy-Item $src $stage }
  else { $missing += "$($item.Name)  ($($item.Why))" }
}
if ($missing) {
  throw "not in target\release, so the package would be broken:`n  " + ($missing -join "`n  ")
}

# The modules, minus what only a developer needs. `calibration` is the big one: 123 captured
# screenshots and 66 MB, all of them evidence for coordinates rather than anything the app
# reads at runtime.
$skipDirs = @("calibration")
$modulesOut = Join-Path $stage "modules"
New-Item -ItemType Directory -Path $modulesOut -Force | Out-Null

$shipped = 0
Get-ChildItem (Join-Path $root "modules") -Directory |
  Where-Object { Test-Path (Join-Path $_.FullName "module.toml") } |
  ForEach-Object {
    $dest = Join-Path $modulesOut $_.Name
    Copy-Item $_.FullName $dest -Recurse
    foreach ($skip in $skipDirs) {
      $junk = Join-Path $dest $skip
      if (Test-Path $junk) { Remove-Item $junk -Recurse -Force }
    }
    $shipped++
  }
if ($shipped -eq 0) { throw "no modules found under $root\modules" }

# The documentation, made to work from the folder rather than from a web server — see
# docs-offline.ps1 for why that is a conversion and not a copy. Skipped rather than fatal when
# the site has never been built: a tester without docs still has a working application, and
# failing the whole package over them would be the wrong trade.
$docsOut = Join-Path $stage "docs"
if (Test-Path (Join-Path $root "docs-site\build")) {
  & (Join-Path $root "docs-offline.ps1") -Out $docsOut
} else {
  Write-Host "No built docs at docs-site\build — packaging without them."
  Write-Host "  Build them once with:  cd docs-site; npm run build"
}

# A note for whoever unpacks it. Short on purpose: the two things that actually go wrong are
# extracting somewhere unwritable and expecting a console window.
@"
Automation Platform — test build

To run: extract this folder somewhere you can write to (Documents or the Desktop —
NOT Program Files) and start automation-platform.exe.

There is no console window. The application runs in the system tray, and writes its
log to automation-platform.log beside the executable. Settings go to settings.toml in
the same place, so the whole folder is portable and can be deleted to reset.

Speech goes through NVDA or System Access if one of them is running.

The documentation is in docs\index.html — open it in a browser. It works from
this folder; no internet connection and no server are needed.

$shipped module(s) included.
"@ | Set-Content (Join-Path $stage "README.txt") -Encoding UTF8

$label = if ($Version) { $Version } else { Get-Date -Format "yyyy-MM-dd" }
$size = "{0:N1} MB" -f ((Get-ChildItem $stage -Recurse -File | Measure-Object Length -Sum).Sum / 1MB)
Write-Host "Staged $shipped module(s) into $stage  ($size)"

if (-not $NoZip) {
  $zip = Join-Path $root "dist\AutomationPlatform-$label.zip"
  Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
  $zipSize = "{0:N1} MB" -f ((Get-Item $zip).Length / 1MB)
  Write-Host "Wrote $zip  ($zipSize)"
}
