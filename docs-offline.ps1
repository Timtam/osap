# Turns the built documentation site into something that works from a FOLDER, with no server.
#
#   .\docs-offline.ps1 -Out dist\AutomationPlatform\docs
#   .\docs-offline.ps1 -Out … -Build     build the site first (needs npm)
#
# Why this exists rather than copying docs-site\build: that build is made for GitHub Pages and
# every page carries ~25 references beginning `/operating-system-automation-platform/`. Opened
# from a folder those resolve against the drive root, so nothing loads — no stylesheet, no
# links, nothing. Measured, not assumed.
#
# What saves it is that Docusaurus PRE-RENDERS: the prose is already in the HTML, and only the
# paths are wrong. So this rewrites them to relative, points directory links at their
# index.html (a browser will not guess that for a file:// URL the way a server does), and drops
# the scripts.
#
# Dropping the scripts is deliberate rather than lazy. The bundle exists to hydrate a
# single-page router, and a router is exactly the part that cannot work from a folder; without
# it each page stays what it already is — semantic HTML with the site's own stylesheet. For a
# screen-reader user that is not a downgrade: no client-side navigation to fight, no focus
# handling of ours to trust.
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$Out,
  [switch]$Build
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$site = Join-Path $root "docs-site"
$built = Join-Path $site "build"

if ($Build) {
  Push-Location $site
  try {
    & npm run build
  } finally { Pop-Location }
  if ($LASTEXITCODE -ne 0) { throw "docs build failed" }
}
if (-not (Test-Path $built)) {
  throw "no built docs at $built — run with -Build, or build docs-site once by hand"
}

# The base every absolute reference starts with, read from the config rather than written down
# twice: if someone changes it there, this must follow or it silently rewrites nothing.
$configText = Get-Content (Join-Path $site "docusaurus.config.js") -Raw
if ($configText -notmatch "baseUrl:\s*'([^']+)'") { throw "could not read baseUrl from docusaurus.config.js" }
$base = $Matches[1]

if (Test-Path $Out) { Remove-Item $Out -Recurse -Force }
New-Item -ItemType Directory -Path $Out -Force | Out-Null
Copy-Item (Join-Path $built "*") $Out -Recurse

$pages = 0
$rewritten = 0
Get-ChildItem $Out -Recurse -Filter *.html | ForEach-Object {
  $file = $_
  # How far this page sits below the docs root decides what "up" means for it.
  $rel = $file.DirectoryName.Substring($Out.Length).TrimStart('\', '/')
  $depth = if ($rel -eq "") { 0 } else { ($rel -split '[\\/]').Count }
  $up = if ($depth -eq 0) { "./" } else { ("../" * $depth) }

  $html = Get-Content $file.FullName -Raw
  $before = $html

  # A link to a PAGE ends without a file extension; from a folder that is a directory, and a
  # browser will not serve its index.html on its own.
  $html = [regex]::Replace($html, [regex]::Escape($base) + '([A-Za-z0-9_\-/\.]*)', {
    param($m)
    $path = $m.Groups[1].Value
    if ($path -eq "") { return $up + "index.html" }
    $leaf = ($path -split '/')[-1]
    if ($leaf -notmatch '\.') { return $up + $path.TrimEnd('/') + "/index.html" }
    return $up + $path
  })

  # The scripts only exist to hydrate the router; see the header.
  $html = [regex]::Replace($html, '<script[^>]*>.*?</script>', '', 'Singleline')
  $html = [regex]::Replace($html, '<script[^>]*/>', '')

  if ($html -ne $before) { $rewritten++ }
  Set-Content $file.FullName $html -NoNewline -Encoding UTF8
  $pages++
}

# The JavaScript bundle is dead weight once the script tags are gone, and it is most of the
# site's size: 1.4 MB of assets against 1.2 MB of everything else. Removing it also removes the
# only remaining copies of the absolute paths, so nothing in the folder still points at a server
# that is not there.
foreach ($dead in @("assets\js", "sitemap.xml")) {
  $path = Join-Path $Out $dead
  # The sitemap goes for the same reason: it lists server URLs and means nothing in a folder.
  if (Test-Path $path) { Remove-Item $path -Recurse -Force }
}

# Docusaurus puts the real front page under the baseUrl, and leaves a redirect stub at the root
# that only makes sense on a server. Replace it with the actual overview page.
$stub = Join-Path $Out "index.html"
$real = Join-Path $Out "intro\index.html"
if ((Test-Path $real) -and (Test-Path $stub)) {
  $probe = Get-Content $stub -Raw
  if ($probe -match 'baseUrl' -and $probe.Length -lt 4000) { Copy-Item $real $stub -Force }
}

$size = "{0:N1} MB" -f ((Get-ChildItem $Out -Recurse -File | Measure-Object Length -Sum).Sum / 1MB)
Write-Host "Docs: $pages page(s), $rewritten rewritten, into $Out  ($size)"
