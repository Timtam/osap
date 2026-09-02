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

# What has to be there, by name. Checked rather than copied blindly, because a missing one
# fails SILENTLY: without DirectML the second OCR engine is gone and small text stops being
# read, which does not look like a missing file to whoever is testing.
#
# The two speech client DLLs used to be here. They are not any more: prism reaches NVDA over
# raw RPC with stubs compiled into the binary, and is linked statically, so there is nothing
# beside the executable for speech at all.
$required = @(
  @{ Name = "automation-platform.exe";      Why = "the application" },
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

# The licences. This is not paperwork for its own sake: the application is GPL-3.0-or-later
# and links prism, which is MPL-2.0, and MPL-2.0 section 3.2 requires that whoever receives
# the executable is told how to get the source of the covered files. prism's own NOTICE is
# not a sufficient attribution list — it omits highway (Apache-2.0, whose section 4(d) has a
# real propagation requirement) and NVGT (Zlib) — so its whole LICENSES tree is shipped
# rather than a summary of it.
$licences = Join-Path $stage "licences"
New-Item -ItemType Directory -Path $licences -Force | Out-Null
Copy-Item (Join-Path $root "LICENSE") (Join-Path $licences "automation-platform-GPL-3.0.txt")

$vendor = Join-Path $root "crates\prism-sys\vendor"
if (Test-Path (Join-Path $vendor "LICENSE")) {
  Copy-Item (Join-Path $vendor "LICENSE") (Join-Path $licences "prism-MPL-2.0.txt")
  Copy-Item (Join-Path $vendor "NOTICE")  (Join-Path $licences "prism-NOTICE.txt")
  Copy-Item (Join-Path $vendor "LICENSES") (Join-Path $licences "prism") -Recurse
  Push-Location $vendor
  $prismSha = (git rev-parse HEAD).Trim()
  # A tag is a nicety; its absence must not fail the packaging. `actions/checkout` fetches
  # submodules without tags, so on CI this finds nothing, leaves $LASTEXITCODE at 128 — and
  # because it is the last native command in this script, PowerShell then reports the whole
  # step as failed AFTER it has successfully written the zip. Reset deliberately, not
  # accidentally: the value belongs to a lookup that was allowed to come up empty.
  $prismTag = (git describe --tags 2>$null)
  if (-not $prismTag) { $prismTag = "no tag on this checkout" }
  $global:LASTEXITCODE = 0
  Pop-Location
} else {
  Write-Warning "crates\prism-sys\vendor is empty (git submodule update --init) — shipping without prism's licences"
  $prismSha = "unknown"
  $prismTag = "unknown"
}

@"
Licences
========

Automation Platform is free software under the GNU General Public License, version 3 or
later. The full text is in automation-platform-GPL-3.0.txt. The source is at
https://github.com/Timtam/osap

Speech on Windows goes through PRISM, which is used under the Mozilla Public License 2.0
(prism-MPL-2.0.txt). MPL-2.0 section 3.2 asks that you be told where its source is:

    https://github.com/ethindp/prism
    commit $prismSha  ($prismTag)

That commit is what this build was compiled from. prism in turn carries the libraries
whose licences are in the prism\ folder beside this file — fmt 12.2.1, highway 1.4.0,
simdutf 9.0.0, concurrentqueue, dr_wav, moderncom, djinni, NVGT, and NV Access's NVDA
controller RPC definitions — and its own NOTICE is in prism-NOTICE.txt.
"@ | Set-Content (Join-Path $licences "README.txt") -Encoding UTF8

# A note for whoever unpacks it. Short on purpose: the two things that actually go wrong are
# extracting somewhere unwritable and expecting a console window.
@"
Automation Platform — test build

To run: extract this folder somewhere you can write to (Documents or the Desktop —
NOT Program Files) and start automation-platform.exe.

There is no console window. The application runs in the system tray, and writes its
log to automation-platform.log beside the executable. Settings go to settings.toml in
the same place, so the whole folder is portable and can be deleted to reset.

Speech goes through whichever screen reader is running — NVDA, JAWS or ZoomText —
and through the system voice when there is none. Nothing has to be installed for
that: it is all inside the executable.

Licences are in the licences folder, including where to get the source.

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
