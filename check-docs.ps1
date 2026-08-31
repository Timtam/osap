# Every host.* name the documentation calls, checked against what the host actually registers.
#
# Why this exists: a documented example that raises is worse than no example at all. It gets
# copied, it fails, and the reader — who came to the reference precisely because they did not
# know the API — has no way to tell whether the platform is broken or their own typing is.
#
# It was written after finding seven such lines in one file: `host.window.focused()`, a binding
# that has never existed, twice; and `host.log("...")` five times, where `host.log` is a table
# carrying only `info`, so the call raises "attempt to call a table value". Nothing had noticed,
# because prose and code rot in exactly the same silence.
#
#   .\check-docs.ps1          the API reference under docs/api/ — where correctness is a promise
#   .\check-docs.ps1 -All     every document, including the design notes
#
# The design notes deliberately describe APIs that do not exist yet — they are proposals — so
# they are excluded unless asked for.

[CmdletBinding()]
param([switch]$All)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

# ---- What the host registers ------------------------------------------------------------
# Four shapes, because the host uses four. Missing one produces a false alarm, and a checker
# that cries wolf is turned off, which is worse than not having it.
#
# Forward slashes in the paths: this runs on a Linux CI runner as well as on Windows, and .NET
# accepts them on both. A backslash passes locally and fails in CI, which is the worst order to
# find that out in.
$sources = @(
  (Get-Content (Join-Path $root "crates/host/src/lib.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/window_prelude.luau") -Raw)
) -join "`n"

$registered = [System.Collections.Generic.HashSet[string]]::new()
foreach ($pattern in @(
    '\.set\(\s*"([A-Za-z_][A-Za-z0-9_]*)"',      # the ordinary Rust binding
    '\(\s*"([A-Za-z_][A-Za-z0-9_]*)"\s*,\s*\d+\s*\)', # the UIA control-type tuples
    'function\s+[A-Za-z_][A-Za-z0-9_.]*\.([A-Za-z_][A-Za-z0-9_]*)\s*\(', # prelude functions
    '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*function'   # table-literal functions
  )) {
  foreach ($m in [regex]::Matches($sources, $pattern, 'Multiline')) {
    [void]$registered.Add($m.Groups[1].Value)
  }
}

# The tables hanging off `host` itself. Named rather than derived: this list is short, it
# changes rarely, and spelling it out means an entirely new table shows up as an error
# instead of being silently accepted.
$hostTables = @('os', 'window', 'screen', 'ocr', 'input', 'sound', 'speech', 'hotkey', 'keys',
  'timer', 'log', 'path', 'resource', 'settings', 'config', 'require', 'include', 'uia',
  'overlay', 'match', 'now', 'epoch', 'inputEpoch', 'arbiter', 'tryRequire')

# ---- What the documentation calls -------------------------------------------------------
$docs = Join-Path $root "docs"
$files = if ($All) { Get-ChildItem $docs -Recurse -Filter *.md } else { Get-ChildItem (Join-Path $docs "api") -Filter *.md }

$problems = @()
foreach ($file in $files) {
  $text = Get-Content $file.FullName -Raw
  $lines = $text -split "`n"
  $inBlock = $false
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^```luau\s*$') { $inBlock = $true; continue }
    if ($line -match '^```\s*$') { $inBlock = $false; continue }
    if (-not $inBlock) { continue }
    if ($line -match '^\s*--') { continue }   # a comment cannot raise
    foreach ($m in [regex]::Matches($line, '\bhost\.([A-Za-z_][A-Za-z0-9_.]*)')) {
      $parts = $m.Groups[1].Value -split '\.'
      $why = $null
      if ($hostTables -notcontains $parts[0]) {
        $why = "there is no host.$($parts[0])"
      }
      else {
        foreach ($seg in $parts[1..($parts.Count - 1)]) {
          if (-not $registered.Contains($seg) -and $hostTables -notcontains $seg) {
            $why = "'$seg' is not registered anywhere"
            break
          }
        }
      }
      if ($why) {
        $rel = $file.FullName.Substring($root.Length + 1).Replace('\', '/')
        $problems += [pscustomobject]@{
          Name = "host.$($m.Groups[1].Value)"; Why = $why
          Where = "${rel}:$($i + 1)"; Line = $line.Trim()
        }
      }
    }
  }
}

if ($problems.Count -eq 0) {
  $scope = if ($All) { "docs/" } else { "docs/api/" }
  Write-Host "Every host.* name used in $scope is one the host registers."
  exit 0
}

Write-Host "$($problems.Count) documented call(s) the host does not provide:`n"
foreach ($p in $problems) {
  Write-Host "  $($p.Name)  -- $($p.Why)"
  Write-Host "      $($p.Where)   $($p.Line)"
}
Write-Host "`nAn example that raises is worse than no example: it is copied, it fails, and the"
Write-Host "reader cannot tell whether the platform or their own typing is at fault."
exit 1
