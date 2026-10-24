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
# Five shapes, because the host uses five. Missing one produces a false alarm, and a checker
# that cries wolf is turned off, which is worse than not having it.
#
# Forward slashes in the paths: this runs on a Linux CI runner as well as on Windows, and .NET
# accepts them on both. A backslash passes locally and fails in CI, which is the worst order to
# find that out in.
#
# gamepad_api.rs registers `host.gamepad` from a file of its own, so it is read beside lib.rs;
# without it every documented `host.gamepad.*` call would be reported as one the host lacks.
# timers.rs is the same for `host.timer`, webview_lua.rs for `host.gui`, element_lua.rs for
# `host.element`, and errors_lua.rs for `host.errors` — whose constants are named in errors.rs,
# one `Code::X => "X"` arm each, the fifth shape below.
$sources = @(
  (Get-Content (Join-Path $root "crates/host/src/lib.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/element_lua.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/gamepad_api.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/timers.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/webview_lua.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/errors_lua.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/errors.rs") -Raw),
  (Get-Content (Join-Path $root "crates/host/src/window_prelude.luau") -Raw)
) -join "`n"

$registered = [System.Collections.Generic.HashSet[string]]::new()
foreach ($pattern in @(
    '\.set\(\s*"([A-Za-z_][A-Za-z0-9_]*)"',      # the ordinary Rust binding
    '\(\s*"([A-Za-z_][A-Za-z0-9_]*)"\s*,\s*\d+\s*\)', # the UIA control-type tuples
    'function\s+[A-Za-z_][A-Za-z0-9_.]*\.([A-Za-z_][A-Za-z0-9_]*)\s*\(', # prelude functions
    '^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*function',  # table-literal functions
    'Code::[A-Za-z]+\s*=>\s*"([A-Z][A-Z0-9_]*)"'     # host.errors' constants (errors.rs)
  )) {
  foreach ($m in [regex]::Matches($sources, $pattern, 'Multiline')) {
    [void]$registered.Add($m.Groups[1].Value)
  }
}

# What hangs off `host` itself, derived rather than listed.
#
# It was a hand-kept list, justified as short and slow-changing. It then reported
# `host.calibrating` — which was real, registered at lib.rs, and had simply never been added to
# the list — as a name the host does not provide. A checker that cries wolf gets switched off,
# and every one of these is one regex from the source that registers it.
$hostTables = @()
foreach ($m in [regex]::Matches($sources, 'host\.set\(\s*"([A-Za-z_][A-Za-z0-9_]*)"')) {
  $hostTables += $m.Groups[1].Value
}
foreach ($m in [regex]::Matches($sources, 'function\s+host\.([A-Za-z_][A-Za-z0-9_]*)')) {
  $hostTables += $m.Groups[1].Value
}
# `host.overlay` is deliberately absent — it is a module, and a documented `host.overlay.*`
# call is a mistake worth reporting. So is `host.match`: it used to be added here by hand as
# "built by the prelude", but nothing builds it — a matcher is a plain table — and the hand
# entry let a documented `host.match { … }` pass as real.
$hostTables = $hostTables | Sort-Object -Unique

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

# ---- What modules compare: a reason, a status word, a constant ---------------------------
# A failure is an error value, `{ code, message, detail? }`, and a module compares its code with a
# constant: `err.code == host.errors.NOT_READY` (docs/api/errors.md). Before the typed errors it was
# an English sentence, and code that still compares one — `why == "not ready yet"`, a table keyed by
# a reason, `why:find("denied")` — is never true and says nothing, which no test sees unless the
# failure happens in it; and `string.format("%s", err)` and `assert(ok, err)` raise for one, which
# no test sees either. So everywhere a module's code is written is read for those shapes: the Luau
# of modules/, tools/ and examples/, and every Luau block of docs/, the design notes included,
# since an example there is copied as readily. The names are the ones this project gives a reason
# (`why`, `err`, `reason` and what starts with them); a line that has to compare a word under such
# a name says so with the comment `-- lint: word`. A heuristic, backed by the tests: it reads
# names, not values.
#
# The words that stay words — a reading's `status`, onClose's and onEnd's `how`, host.keys.check's
# `reasons` (docs/api/errors.md, "Comparing codes") — have no constants, so a misspelt one is the
# same silent false. Each literal compared with one is held to the words its page documents. And a
# constant of `O.errors` or of a page's `platform.errors` that does not exist raises, or reads
# `undefined`, only when its line runs. The Luau of modules/, tools/ and examples/ is held to its
# constants by the host's tests (every_error_constant_a_module_names_is_one); the docs' Luau, and
# every page script, docs' and modules' alike, are held to them here.
$errorsRs = Get-Content (Join-Path $root "crates/host/src/errors.rs") -Raw
$errorsLuau = Get-Content (Join-Path $root "modules/overlay-runtime/src/errors.luau") -Raw
$hostConstants = @([regex]::Matches($errorsRs, 'Code::[A-Za-z]+\s*=>\s*"([A-Z][A-Z0-9_]*)"') | ForEach-Object { $_.Groups[1].Value })
$runtimeCodes = [regex]::Match($errorsLuau, '(?m)^\s*codes\s*=\s*\{([^}]*)\}')
$runtimeConstants = @([regex]::Matches($runtimeCodes.Groups[1].Value, '"([a-z_]+)"') | ForEach-Object { $_.Groups[1].Value.ToUpperInvariant() })
if ($hostConstants.Count -ne 19 -or $runtimeConstants.Count -ne 8) {
  Write-Host "check-docs.ps1 could not read the codes: $($hostConstants.Count) of host.errors in errors.rs (19 expected), $($runtimeConstants.Count) of the runtime's in errors.luau (8 expected)."
  exit 1
}

# The status words, and the page that documents each set. Every word here must be one its page
# names, as `word` or "word", so that the two cannot drift apart without a failure.
$words = [ordered]@{
  status = @{ Field = "a reading's status"; Page = "docs/api/ocr.md"; Words = @("text", "blank", "none", "failed", "stale") }
  close  = @{ Field = "onClose's how"; Page = "docs/api/gui.md"; Words = @("user", "failed") }
  ended  = @{ Field = "onEnd's how"; Page = "docs/api/overlay.md"; Words = @("ended", "done", "given_up") }
  check  = @{ Field = "host.keys.check's reasons"; Page = "docs/api/keys.md"; Words = @("parse", "reserved", "voiceover", "no_keycode", "tap", "altgr", "composes") }
}
$lint = @()
foreach ($set in $words.Values) {
  $page = Get-Content (Join-Path $root $set.Page) -Raw
  foreach ($w in $set.Words) {
    if (-not ($page.Contains('`' + $w + '`') -or $page.Contains('"' + $w + '"'))) {
      $lint += [pscustomobject]@{ What = "$($set.Field): '$w' is not a word $($set.Page) documents"; Where = "check-docs.ps1"; Line = "" }
    }
  }
}

$reason = '(why|err|reason)\w*'
$reasonShapes = @(
  @{ What = "a reason compared with a string"; Re = $reason + '\s*[=~]=\s*["'']' },
  @{ What = "two reasons compared"; Re = $reason + '\s*[=~]=\s*' + $reason + '\b' },
  @{ What = "a table looked up by a reason"; Re = '\[' + $reason + '\]' },
  @{ What = "a string function on a reason"; Re = 'string\.(find|match|gmatch|sub|lower|upper)\(\s*' + $reason },
  @{ What = "a string method on a reason"; Re = $reason + '\s*:(find|match|gmatch|sub|lower|upper)\(' }
)

# The `how` a hook is handed is onClose's or onEnd's only inside that hook: a function whose
# parameters name `how` on a line that names the hook, down to its `end` at that line's
# indentation, and any line that names the hook itself. A hook given a function by its name —
# `onClose = closed` — makes that function's `how` the hook's too, wherever it is defined in the
# file. Elsewhere `how` is a module's own word — VPS Avenger's acts answer "changed" or
# "unchanged" — which no page of the API documents. A `how` handed on to another function is not
# followed.
function Get-HowWords([string[]]$lines) {
  $at = @{}
  $named = @{}
  foreach ($l in $lines) {
    foreach ($m in [regex]::Matches($l, '\b(onEnd|onClose|onPageClose)\s*=\s*([A-Za-z_]\w*)\s*(?=,|\}|$)')) {
      $fn = $m.Groups[2].Value
      if ($fn -cin @("function", "nil", "true", "false")) { continue }
      $named[$fn] = if ($m.Groups[1].Value -ceq "onEnd") { $words.ended.Words } else { $words.close.Words }
    }
  }
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $l = $lines[$i]
    $these = @()
    if ($l -cmatch 'onEnd') { $these += $words.ended.Words }
    if ($l -cmatch 'onClose|onPageClose|pageClosed') { $these += $words.close.Words }
    $def = [regex]::Match($l, '\bfunction\s+(?:[\w.]+[.:])?([A-Za-z_]\w*)\s*\(')
    if ($def.Success -and $named.ContainsKey($def.Groups[1].Value)) { $these += $named[$def.Groups[1].Value] }
    if ($these.Count -eq 0) { continue }
    $at[$i] = @(@($at[$i]) + $these | Where-Object { $_ })
    $head = [regex]::Match($l, 'function\s*[\w.:]*\s*\(([^)]*)\)(.*)$')
    if (-not $head.Success -or $head.Groups[1].Value -notmatch '(^|[\s,])how([\s,]|$)') { continue }
    if ($head.Groups[2].Value -cmatch '\bend\b') { continue }   # the whole function on one line
    $indent = [regex]::Match($l, '^\s*').Length
    for ($j = $i + 1; $j -lt $lines.Count; $j++) {
      $e = [regex]::Match($lines[$j], '^(\s*)end\b')
      if ($e.Success -and $e.Groups[1].Length -le $indent) { break }
      $at[$j] = @(@($at[$j]) + $these | Where-Object { $_ })
    }
  }
  return $at
}

# The arguments of the call whose `(` is at `$open` in `$text`, as written, to its matching `)` —
# strings, brackets and nesting kept whole. Each caller gathers them with @( ), a list however
# many there are.
function Get-CallArgs([string]$text, [int]$open) {
  $out = @(); $cur = ""; $depth = 0; $quote = $null
  for ($k = $open + 1; $k -lt $text.Length; $k++) {
    $ch = $text[$k]
    if ($quote) {
      $cur += $ch
      if ($ch -eq '\' -and $k + 1 -lt $text.Length) { $k++; $cur += $text[$k] }
      elseif ($ch -eq $quote) { $quote = $null }
      continue
    }
    if ($ch -eq '"' -or $ch -eq "'") { $quote = $ch; $cur += $ch; continue }
    if ($ch -eq '(' -or $ch -eq '{' -or $ch -eq '[') { $depth++ }
    elseif ($ch -eq ')' -or $ch -eq '}' -or $ch -eq ']') {
      if ($depth -eq 0) { $out += $cur.Trim(); return $out }
      $depth--
    }
    elseif ($ch -eq ',' -and $depth -eq 0) { $out += $cur.Trim(); $cur = ""; continue }
    $cur += $ch
  }
  return $out
}

# What raises for an error value that a sentence would pass: `%s` and `%q` of string.format take a
# string only, and so does Luau's `assert` for its message (docs/api/errors.md, "Error values").
# The format string is read where the call has it — `string.format("…", …)`, `("…"):format(…)`,
# its string on the line before — and each argument matched with its specifier. A sentence under a
# reason's name is passed through `tostring` too, so that the two never look alike.
function Test-Formats([string]$where, [string[]]$lines, [int[]]$numbers) {
  $found = @()
  $kept = for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i].Contains("-- lint: word")) { "" } else { [regex]::Replace($lines[$i], '--.*$', '') }
  }
  $text = $kept -join "`n"
  $lineAt = { param($pos) ([regex]::Matches($text.Substring(0, $pos), "`n")).Count }
  $calls = @(
    # The literal is the whole format, not a part of one put together: its specifiers are the call's.
    @{ Re = 'string\.format\(\s*(["''])((?:\\.|(?!\1).)*)\1\s*,'; Fmt = 2; Open = 'string.format(' },
    @{ Re = '\(\s*(["''])((?:\\.|(?!\1).)*)\1\s*\)\s*:\s*format\('; Fmt = 2; Open = ':format(' }
  )
  foreach ($c in $calls) {
    foreach ($m in [regex]::Matches($text, $c.Re)) {
      $fmt = $m.Groups[$c.Fmt].Value
      $open = if ($c.Open -eq ':format(') { $m.Index + $m.Length - 1 } else { $text.IndexOf('(', $m.Index) }
      $all = @(Get-CallArgs $text $open)
      # @( ): a list of one would be its one string, indexed by character.
      $given = @(if ($c.Open -eq ':format(') { $all } else { $all | Select-Object -Skip 1 })
      $specs = @([regex]::Matches($fmt, '%(?:%|[-+ #0]*\d*(?:\.\d+)?([A-Za-z*]))') | Where-Object { $_.Value -ne '%%' } | ForEach-Object { $_.Groups[1].Value })
      for ($k = 0; $k -lt $given.Count -and $k -lt $specs.Count; $k++) {
        if ($given[$k] -match ('^' + $reason + '$') -and $specs[$k] -cin @("s", "q")) {
          $n = & $lineAt $m.Index
          $found += [pscustomobject]@{ What = "%$($specs[$k]) of '$($given[$k])', which raises for an error value: write %* or tostring($($given[$k]))"; Where = "${where}:$($numbers[$n])"; Line = $lines[$n].Trim() }
        }
      }
    }
  }
  foreach ($m in [regex]::Matches($text, '(?<![\w.:])assert\(')) {
    $all = @(Get-CallArgs $text ($m.Index + $m.Length - 1))
    if ($all.Count -ge 2 -and $all[1] -match ('^' + $reason + '$')) {
      $n = & $lineAt $m.Index
      $found += [pscustomobject]@{ What = "assert with '$($all[1])' as its message, which raises for an error value: write tostring($($all[1]))"; Where = "${where}:$($numbers[$n])"; Line = $lines[$n].Trim() }
    }
  }
  # host.keys.check's words as a table's keys: a table looked up by a word of a check's `reasons`
  # has its literal's keys held to the words, as a comparison with one is.
  $vars = @([regex]::Matches($text, 'for\s+\w+\s*,\s*([A-Za-z_]\w*)\s+in\s+ipairs\(\s*host\.keys\.check\([^\n]*?\)\s*\.reasons\s*\)') | ForEach-Object { $_.Groups[1].Value })
  foreach ($v in ($vars | Sort-Object -Unique)) {
    foreach ($t in @([regex]::Matches($text, '([A-Za-z_]\w*)\[\s*' + [regex]::Escape($v) + '\s*\]') | ForEach-Object { $_.Groups[1].Value } | Sort-Object -Unique)) {
      $lit = [regex]::Match($text, '(?<![\w.])(?:local\s+)?' + [regex]::Escape($t) + '\s*=\s*\{')
      if (-not $lit.Success) { continue }
      $body = @(Get-CallArgs $text ($lit.Index + $lit.Length - 1))
      foreach ($field in $body) {
        $key = [regex]::Match($field, '^(?:([A-Za-z_]\w*)|\[\s*"([^"]*)"\s*\])\s*=')
        if (-not $key.Success) { continue }
        $w = if ($key.Groups[1].Success) { $key.Groups[1].Value } else { $key.Groups[2].Value }
        if ($words.check.Words -cnotcontains $w) {
          $n = & $lineAt $lit.Index
          $found += [pscustomobject]@{ What = "'$w', a key of $t, is not a word of $($words.check.Field) ($(($words.check.Words | Sort-Object) -join ', '))"; Where = "${where}:$($numbers[$n])"; Line = $lines[$n].Trim() }
        }
      }
    }
  }
  return $found
}

function Test-Luau([string]$where, [string[]]$lines, [int[]]$numbers) {
  $found = @(Test-Formats $where $lines $numbers)
  $how = Get-HowWords $lines
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $raw = $lines[$i]
    if ($raw.Contains("-- lint: word")) { continue }
    $line = [regex]::Replace($raw, '--.*$', '')   # a comment compares nothing
    $here = "${where}:$($numbers[$i])"
    foreach ($shape in $reasonShapes) {
      if ([regex]::IsMatch($line, $shape.Re)) {
        $found += [pscustomobject]@{ What = $shape.What; Where = $here; Line = $raw.Trim() }
        break
      }
    }
    $checks = @(
      @{ Re = '\.status\s*[=~]=\s*"([^"]*)"|"([^"]*)"\s*[=~]=\s*[\w.\[\]]*\.status\b'; Set = $words.status.Words; Field = $words.status.Field },
      @{ Re = 'table\.find\(\s*[\w.]*reasons\s*,\s*"([^"]*)"|reasons\[[^\]]*\]\s*[=~]=\s*"([^"]*)"'; Set = $words.check.Words; Field = $words.check.Field }
    )
    if ($how.ContainsKey($i)) {
      $checks += @{ Re = '(?<![\w.])how\s*[=~]=\s*"([^"]*)"|"([^"]*)"\s*[=~]=\s*how\b'; Set = $how[$i]; Field = "the hook's how" }
    }
    foreach ($c in $checks) {
      foreach ($m in [regex]::Matches($line, $c.Re)) {
        $w = if ($m.Groups[1].Success) { $m.Groups[1].Value } else { $m.Groups[2].Value }
        if ($c.Set -cnotcontains $w) {
          $found += [pscustomobject]@{ What = "'$w' is not a word of $($c.Field) ($(($c.Set | Sort-Object -Unique) -join ', '))"; Where = $here; Line = $raw.Trim() }
        }
      }
    }
    foreach ($m in [regex]::Matches($line, '(?<![\w.])O\.errors\.([A-Za-z_][A-Za-z0-9_]*)')) {
      $k = $m.Groups[1].Value
      if ($hostConstants -cnotcontains $k -and $runtimeConstants -cnotcontains $k) {
        $found += [pscustomobject]@{ What = "O.errors has no $k"; Where = $here; Line = $raw.Trim() }
      }
    }
  }
  return $found
}

function Test-PageScript([string]$where, [string[]]$lines, [int[]]$numbers) {
  $found = @()
  for ($i = 0; $i -lt $lines.Count; $i++) {
    foreach ($m in [regex]::Matches($lines[$i], '(?<![\w.])platform\.errors\.([A-Za-z_][A-Za-z0-9_]*)')) {
      if ($hostConstants -cnotcontains $m.Groups[1].Value) {
        $found += [pscustomobject]@{ What = "platform.errors has no $($m.Groups[1].Value)"; Where = "${where}:$($numbers[$i])"; Line = $lines[$i].Trim() }
      }
    }
  }
  return $found
}

function Get-Rel([string]$path) { return $path.Substring($root.Length + 1).Replace('\', '/') }

foreach ($dir in @("modules", "tools", "examples")) {
  foreach ($f in Get-ChildItem (Join-Path $root $dir) -Recurse -File -Include *.luau, *.js, *.html) {
    if ($f.FullName -match '[\\/]node_modules[\\/]') { continue }
    $lines = @((Get-Content $f.FullName -Raw) -split "`n")
    $numbers = @(1..$lines.Count)
    if ($f.Extension -eq ".luau") { $lint += Test-Luau (Get-Rel $f.FullName) $lines $numbers }
    else { $lint += Test-PageScript (Get-Rel $f.FullName) $lines $numbers }
  }
}
foreach ($f in Get-ChildItem $docs -Recurse -Filter *.md) {
  $lines = @((Get-Content $f.FullName -Raw) -split "`n")
  $kind = $null; $block = @(); $numbers = @()
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $open = [regex]::Match($lines[$i], '^```(\w+)\s*$')
    if (-not $kind -and $open.Success) { $kind = $open.Groups[1].Value; $block = @(); $numbers = @(); continue }
    if ($kind -and $lines[$i] -match '^```\s*$') {
      if ($block.Count -gt 0) {
        if ($kind -eq "luau") { $lint += Test-Luau (Get-Rel $f.FullName) $block $numbers }
        elseif ($kind -in @("js", "javascript", "html")) { $lint += Test-PageScript (Get-Rel $f.FullName) $block $numbers }
      }
      $kind = $null
      continue
    }
    if ($kind) { $block += $lines[$i]; $numbers += ($i + 1) }
  }
}

$scope = if ($All) { "docs/" } else { "docs/api/" }
if ($problems.Count -eq 0) {
  Write-Host "Every host.* name used in $scope is one the host registers."
}
else {
  Write-Host "$($problems.Count) documented call(s) the host does not provide:`n"
  foreach ($p in $problems) {
    Write-Host "  $($p.Name)  -- $($p.Why)"
    Write-Host "      $($p.Where)   $($p.Line)"
  }
  Write-Host "`nAn example that raises is worse than no example: it is copied, it fails, and the"
  Write-Host "reader cannot tell whether the platform or their own typing is at fault.`n"
}

if ($lint.Count -eq 0) {
  Write-Host "No reason is compared as a sentence, every status word compared is a documented one, and every error constant named exists."
}
else {
  Write-Host "$($lint.Count) comparison(s) that are never true, or constant(s) that do not exist:`n"
  foreach ($p in $lint) {
    Write-Host "  $($p.What)"
    Write-Host "      $($p.Where)   $($p.Line)"
  }
  Write-Host "`nCompare err.code with a constant of host.errors or O.errors (docs/api/errors.md). A line"
  Write-Host "that has to compare a word under a reason's name carries the comment -- lint: word."
}

if ($problems.Count -gt 0 -or $lint.Count -gt 0) { exit 1 }
exit 0
