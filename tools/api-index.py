# -*- coding: utf-8 -*-
"""Give every API entry a stable anchor, and build the index of all of them.

Two problems, one pass.

**The anchors were not usable.** Docusaurus derives a heading's anchor from its text, and on a
heading like `O:addStaticText(label)` it produced `olabel` — the method name gone. Three
different entries collapsed to `oopts`, `oopts-1`, `oopts-2`, numbered in document order, so
adding an entry silently renumbered the ones after it and any link to them rotted. Headings
displayed correctly throughout, which is why nothing looked wrong. An explicit `{#anchor}` is
honoured verbatim, so every entry gets one, derived from its own name.

**There was no index.** The sidebar listed six pages with names like `speech-hotkey-keys-timer-log`,
so finding `host.timer.after` meant guessing which bundle it lived in. The index lists every
entry, grouped by namespace, with the one-line summary each page already carries.

    python tools/api-index.py            rewrite the anchors and regenerate docs/api/index.md
    python tools/api-index.py --check    fail if either is out of date (used by check-docs.ps1)

The index is generated rather than written, because a hand-kept list of 87 entries is a list
that is wrong within a month.
"""
import glob
import io
import os
import re
import sys

API = 'docs/api'
INDEX = os.path.join(API, 'index.md')

# The order namespaces appear in. Named rather than sorted: this is the order somebody learns
# them in, and it puts the two things every module touches first.
NS_ORDER = [
    'Overlay', 'host.window', 'host.screen', 'host.ocr', 'host.element', 'host.input',
    'host.keys', 'host.hotkey', 'host.gamepad', 'host.speech', 'host.sound', 'host.timer',
    'host.settings',
    'host.config', 'host.os', 'host.log', 'host.path', 'host.resource', 'host.json',
    'host.require',
    'host.tryRequire', 'host.include', 'host.epoch', 'host.now', 'host.inputEpoch',
    'host.arbiter', 'host.calibrating', 'Concepts',
]

NS_BLURB = {
    'Overlay': 'Controls a module defines over a plug-in window, walked with Tab and spoken '
              'aloud. A module, not a host namespace: `host.require("com.platform.overlay")`.',
    'host.window': 'Finding windows and the surfaces inside them, and reacting when the focus '
                   'moves.',
    'host.screen': 'Reading pixels, profiling a region, and finding an image within one.',
    'host.ocr': 'Recognising text in a screen region.',
    'host.element': 'Querying the accessibility tree an application publishes.',
    'host.input': 'Synthesising mouse and keyboard input.',
    'host.keys': 'Claiming keys before the focused application sees them.',
    'host.hotkey': 'Claiming a combination system-wide.',
    'host.gamepad': 'Watching game controllers — observed only, never taken from the game, and '
                    'delivered whichever window is in front.',
    'host.arbiter': 'Deciding which of several overlays owns a contested slot.',
    'host.timer': 'Waiting without blocking, and knowing when a cached reading went stale.',
    'host.settings': 'Typed, per-module settings, edited by the user in the module manager.',
    'host.log': 'Writing to the log file beside the application.',
    'host.json': 'Turning JSON text a module ships into Luau values.',
    'Concepts': 'The shapes and grammars the calls above are written in.',
}


def escaped(title):
    """A heading written so Markdown does not eat part of it.

    `## O:addStaticText(label)` renders correctly and is still wrong. A colon glued to a word
    is directive syntax, so `:addStaticText` is consumed before the heading's own value is
    taken — and that value is what feeds the table of contents and the anchor. The page showed
    the full name; its contents list showed `O(label)`, three different entries showed
    `O(opts)`, and `O:origin() / O:hwnd()` showed `O() / O()`. For somebody navigating the page
    by that list, half the reference had no usable names.

    Escaping the colon leaves the rendering identical and stops the parse.
    """
    return re.sub(r'(?<!\\):(?=[A-Za-z])', r'\\:', title)


def unescaped(title):
    return title.replace('\\:', ':')


def slug(heading):
    """A stable anchor, derived from the entry's own name rather than its punctuation."""
    h = heading.split('{')[0].strip()
    # `O:origin() / O:hwnd()` documents two calls under one heading; the first one names it.
    h = h.split(' / ')[0]
    h = re.split(r'\s+[—-]\s+', h)[0]          # `ocrLabel — reading a control's name` -> `ocrLabel`
    h = re.sub(r'\(.*', '', h)                  # arguments never belong in an anchor
    h = h.strip().lower()
    h = re.sub(r'[^a-z0-9]+', '-', h).strip('-')
    return h or 'entry'


def namespace(heading):
    # Unescaped first. The escape that fixes the contents list also breaks this if it is not
    # undone here: `O\\:addStepper` does not start with `O:`, so every overlay method was
    # silently reclassified as a "concept" — twenty-five of them, in a group meant for four.
    h = unescaped(heading.split('{')[0].strip())
    m = re.match(r'(host\.[a-zA-Z]+)', h)
    if m:
        return m.group(1)
    if h.startswith('O:') or h.startswith('O.') or h.startswith('Bindings'):
        return 'Overlay'
    return 'Concepts'


def summarise(body):
    """The first sentence of prose after the heading, with the signature line skipped."""
    for line in body.split('\n'):
        t = line.strip()
        if not t or t.startswith('```') or t.startswith('|') or t.startswith('#'):
            continue
        # A line that is nothing but a signature in backticks describes the shape, not the job.
        if re.fullmatch(r'`[^`]+`', t) or re.match(r'\*\*Signatures?:\*\*', t):
            continue
        t = re.sub(r'^\(prelude\)\s*', '', t)
        # A summary is lifted out of its own page, so any relative link in it would resolve
        # against the index instead and point at nothing. Keep the words, drop the link.
        t = re.sub(r'\[([^\]]+)\]\([^)]*\)', r'\1', t)
        # One sentence is enough for an index; the entry itself carries the rest.
        s = re.split(r'(?<=[.!?])\s', t)[0].strip().rstrip(':')
        # And one sentence can still be a paragraph. Cut at the first clause boundary past
        # the point where a row stops being scannable — a table read aloud, cell by cell, is
        # the case this is for.
        if len(s) > 130:
            head = re.split(r'\s+[—-]\s+|:\s+|;\s+', s)[0].strip()
            s = head if 20 < len(head) <= 160 else s[:127].rsplit(' ', 1)[0] + '…'
        return s
    return ''


# The contents list should be the list of entries, and nothing else. Forty-seven of the
# fifty-one level-3 headings in the reference are the per-OS blocks, so without this a page
# reads out as "Windows, macOS, Windows, macOS" six times over with nothing to say which call
# each pair belongs to — noise for a sighted reader, and for somebody navigating by that list,
# the page's structure buried in it.
TOC_LIMIT = 'toc_max_heading_level: 2'


def limit_toc(paths, dry):
    """Every reference page declares the depth of its own contents list."""
    changed = []
    for path in paths:
        text = open(path, encoding="utf-8").read()
        if TOC_LIMIT in text or not text.startswith("---\n"):
            continue
        end = text.index("\n---\n", 4)
        text = text[:end] + "\n" + TOC_LIMIT + text[end:]
        changed.append(path)
        if not dry:
            io.open(path, "w", encoding="utf-8", newline="\n").write(text)
    return changed


# Page furniture, not an entry. Every feature page carries one, so it is eighteen headings
# with the same name — they are not functions and must not reach the index or be slugged from
# their text (which would collide eighteen ways).
FURNITURE = {'What to declare'}


def collect():
    entries = []
    for path in sorted(glob.glob(os.path.join(API, '*.md'))):
        if os.path.basename(path) == 'index.md':
            continue
        page = os.path.splitext(os.path.basename(path))[0]
        text = open(path, encoding='utf-8').read()
        parts = re.split(r'^(## .+)$', text, flags=re.M)
        for i in range(1, len(parts), 2):
            head = parts[i][3:]
            if unescaped(head.split('{')[0].strip()) in FURNITURE:
                continue
            entries.append({
                'page': page,
                'path': path,
                'raw': parts[i],
                'title': unescaped(head.split('{')[0].strip()),
                'anchor': slug(head),
                'ns': namespace(head),
                'summary': summarise(parts[i + 1]),
            })
    return entries


def rewrite_anchors(entries, dry):
    """Stamp `{#anchor}` on every heading, and repoint links that used the old ones."""
    changed = []
    by_path = {}
    for e in entries:
        by_path.setdefault(e['path'], []).append(e)
    # Old explicit anchors have to keep resolving from inside the docs, so every link is
    # rewritten to the new name in the same pass.
    renames = {}
    for e in entries:
        m = re.search(r'\{#([A-Za-z0-9_-]+)\}', e['raw'])
        if m and m.group(1) != e['anchor']:
            renames[m.group(1)] = e['anchor']

    for path, es in by_path.items():
        text = open(path, encoding='utf-8').read()
        for e in es:
            want = '## ' + escaped(e['title']) + ' {#' + e['anchor'] + '}'
            if e['raw'].rstrip() != want:
                text = text.replace(e['raw'].rstrip(), want, 1)
        for old, new in renames.items():
            text = text.replace('(#' + old + ')', '(#' + new + ')')
        # Anchors that were guessed at the old scheme, in any page.
        for e in entries:
            guessed = re.sub(r'[^a-z0-9]+', '', e['title'].lower())
            if guessed and guessed != e['anchor']:
                text = text.replace('(#' + guessed + ')', '(#' + e['anchor'] + ')')
        original = open(path, encoding='utf-8').read()
        if text != original:
            changed.append(path)
            if not dry:
                io.open(path, 'w', encoding='utf-8', newline='\n').write(text)
    return changed


# What every page's "What to declare" box links to. Said once, here, rather than repeated
# sixteen times — and said honestly, because the alternative is a reader believing the list is
# a boundary it is not.
CAPABILITIES = [
    '## What to declare in module.toml {#capabilities}',
    '',
    'Each page says which name to put in its manifest:',
    '',
    '```toml',
    '[capabilities]',
    'require = ["window", "screen", "speech"]',
    '```',
    '',
    '**These are all the names:** `window`, `screen`, `ocr`, `element`, `input`, `keys`, '
    '`hotkey`, `gamepad`, `speech`, `sound`, `timer`, `settings` (which also covers '
    '`host.config`), `log`, `path`, `resource` and `arbiter` — one per namespace, spelt as the '
    'namespace. There are no finer-grained names: `"window.read"` or `"screen.imagesearch"` '
    'unlock nothing. **Names are not checked** when the module loads, so a misspelt or invented '
    'one loads without a word, and the first use of the namespace it was meant for raises. '
    '`path`, `resource` and `log` are gated like the rest: a module that only reads its own '
    'data files still declares `path` and `resource`, and `log` if it logs.',
    '',
    '**It is enforced.** A namespace you have not declared is not on your `host` table, and '
    'reaching for it raises an error naming your module and the capability it needs rather '
    'than evaluating to `nil` three frames from anything that could explain it. Declaring more '
    'than you use is legal and harmless; declaring less is a failure at the moment you first '
    'need it.',
    '',
    '**Permission follows the module that wrote the code, not the one running it.** A '
    '`code_module` dependency is evaluated inside its dependent\'s VM, and what it may reach '
    'is decided by its own manifest: the overlay runtime declares `ocr`, so its code may read '
    'text on behalf of a module that never declared `ocr` itself. The reverse also holds — a '
    'dependency cannot reach something merely because one of its dependents declared it.',
    '',
    'Ownership is unaffected, and deliberately so. A hotkey or a timer that a dependency\'s '
    'code registers still belongs to the module whose VM it ran in, so disabling that module '
    'takes it away. A `code_module` also runs in a VM of its own, so its top level runs once '
    'there and once in every dependent — see [`host.require`](require.md#host-require).',
    '',
    '**A module that attaches an overlay declares `window`**, even when its own file never '
    'writes `host.window`. The host delivers every foreground and focus change through the '
    'module\'s own `host.window`, so without `window` the module\'s triggers are never seen and '
    'its overlay never activates. A gate or matcher the module supplied is its own code as '
    'well, and needs `window` if it calls `host.window`.',
    '',
    'The same delivery reaches every enabled module, overlay or not. So a module that lacks '
    '`window`, loaded beside one that watches windows, has an error logged on every foreground '
    'and focus change and shown once in a dialog. That is a bug, listed in `TODO.md`; until it '
    'is fixed, such a module declares `window` too.',
    '',
    'Nothing is gated on `host.os`, `host.require`, `host.tryRequire`, `host.include`, '
    '`host.epoch`, `host.now`, `host.inputEpoch`, `host.calibrating` or `host.json`. A clock, a '
    'counter, a platform name, a way to reach a declared dependency and a parser of strings the '
    'module already holds are not worth asking permission for, and gating them would mean every '
    'manifest names them — which is the same as naming none.',
    '',
    'The list is also shown to the user once, before installing a module from GitHub; nothing '
    'is granted or refused per capability, and a module copied into `modules/` by hand is not '
    'reviewed at all. It is not a security boundary on its own — a module still runs arbitrary '
    'Luau, and the declaration is the module\'s own word — but it is now the word the platform '
    'holds it to.',
    '',
    'The overlay is the exception in shape rather than degree: it is a **module**, so it goes '
    'under `dependencies` rather than here.',
    '',
    '## Paths {#paths}',
    '',
    '**Every relative path resolves against the root of the module whose source contains the '
    'call.** For your own code that is your module\'s folder. For a `code_module` it is the '
    'dependency\'s own folder, even while its code runs in your VM: a runtime whose code calls '
    '`host.screen.imageSearch("images/x.png")` looks in the runtime\'s `images/`, whichever '
    'game module it is working for. So hand a shared runtime something that cannot be misread — '
    'an absolute path from your own `host.path`, a `host.screen.template` handle, or the '
    'decoded data itself — never a relative path. The rule holds for `host.path`, '
    '`host.resource.read` and `exists`, `host.screen` (search templates, `template{ file = … }`, '
    '`save`, `saveMarked`), `host.sound.play` and `host.include`, and settings are stored '
    'under the id of the module whose code defines them.',
    '',
    '**Paths are not confined to the module.** Every call above except `host.include` joins the '
    'path onto the root as written: an absolute path is used as it stands and `..` is not '
    'refused, so a path can name any file the user can read. `host.include` checks that the '
    'path stays inside the module; [its page](include.md#host-include) says where that check '
    'falls short.',
    '',
    '## Coordinates {#coordinates}',
    '',
    'Every coordinate the platform takes or returns — window bounds, regions, clicks, hits — is '
    'a screen coordinate with its origin at the top left of the primary display. On **Windows** '
    'it is a physical device pixel, because the application is per-monitor DPI aware; on '
    '**macOS** it is a point. The two agree only at 100 % scaling, and a Retina Mac is exactly '
    'half of the same panel on Windows at 200 %. Coordinates measured by a tool that is not '
    'DPI-aware, on a Windows display scaled above 100 %, have to be multiplied by the scale '
    'factor. A fractional coordinate is cut toward zero without an error. Regions are '
    '`{ x1, y1, x2, y2 }` with `x2` and `y2` exclusive — see '
    '[Region form](screen.md#region-form).',
    '',
    '## Coming from another tool {#coming-from}',
    '',
    'Where habits from other tools mislead, and the section that says what happens here '
    'instead:',
    '',
    '- **A score-based image matcher** (OpenCV and the like). There is no score or threshold: '
    'every template pixel that is not transparent must be within `tolerance` on each colour '
    'channel, and the first position in row order wins — '
    '[`imageSearch`](screen.md#host-screen-imagesearch). For several templates against one '
    'frame, [`imageSearchEach`](screen.md#host-screen-imagesearcheach). Template files are PNG '
    'only; [`host.screen.template`](screen.md#host-screen-template) builds one from bytes.',
    '- **A reader that keeps a frame and reads many points from it.** On Windows every '
    '[`pixel`](screen.md#host-screen-pixel) call is a screen read of its own; on macOS only '
    'reads close together in place and time share one. No call reads many points from one '
    'capture.',
    '- **Tesseract or other OCR language codes.** `lang` is each platform engine\'s own '
    'identifier — [Recognition language](ocr.md#recognition-language) — and OCR is synchronous.',
    '- **AutoHotkey.** `ahk_class` belongs inside the `windows` block of a '
    '[matcher](window.md#matchers); `SetTimer` is '
    '[`host.timer.every`](timer.md#host-timer-every), which cannot be stopped; `Send` is '
    '[`host.input.send`](input.md#host-input-send), virtual keys only; `ImageSearch`\'s second '
    'corner is exclusive here.',
    '- **A keyboard hook that only listens.** A [capture](keys.md#host-keys-capture) takes the '
    'key away; there is no listen-only mode for ordinary keys, only the `"<modifier> tap"` '
    'form watches without taking.',
    '- **A script host with threads or async.** Every callback runs on one thread, and nothing '
    'interrupts one that does not return — see the '
    '[module lifecycle](../module-runtime-and-lifecycle.md).',
    '- **A manifest that names its target window.** `module.toml` has no window or process '
    'field; matching is Luau — see the [manifest](../module-package-format.md).',
    '',
]


def build_index(entries):
    groups = {}
    for e in entries:
        groups.setdefault(e['ns'], []).append(e)
    order = [n for n in NS_ORDER if n in groups] + \
            [n for n in sorted(groups) if n not in NS_ORDER]

    out = ['---', 'title: All functions', 'sidebar_position: 0', '---', '',
           '# All functions', '',
           'Every call the platform offers a module, in one place. ' +
           str(len(entries)) + ' entries.', '',
           'A module reaches the host through the global `host` table, which is always there. ' +
           'The overlay is a module like any other and is imported: ' +
           '`local O = host.require("com.platform.overlay")`.', '',
           'Every callback registered through any of these runs in the calling module\'s own ' +
           'Luau VM, and fires only while that module is **enabled** — with two exceptions: ' +
           'a [`host.settings.onChange`](settings.md#host-settings-onchange) callback fires ' +
           'whenever its setting changes, the module manager\'s Settings dialog included, and ' +
           'an arbiter claim that holds its slot when the module is disabled has its ' +
           '`onDeactivate` called, so an overlay can tear down. A disabled module is ' +
           'still loaded — its entry file has run, and its recurring timers and listeners ' +
           'are kept — and its other callbacks stop; see the ' +
           '[module lifecycle](../module-runtime-and-lifecycle.md).', '']
    out += CAPABILITIES + ['']
    for ns in order:
        out.append('## ' + ns)
        out.append('')
        if ns in NS_BLURB:
            out.append(NS_BLURB[ns])
            out.append('')
        out.append('| | |')
        out.append('|---|---|')
        for e in sorted(groups[ns], key=lambda x: x['title']):
            link = '[`' + e['title'] + '`](' + e['page'] + '#' + e['anchor'] + ')'
            out.append('| ' + link + ' | ' + (e['summary'] or '') + ' |')
        out.append('')
    return '\n'.join(out).rstrip() + '\n'


# ── The other direction: a public name with no entry ────────────────────────────────
#
# `host.set("<ns>", <var>)` names a namespace and the local holding it; `<var>.set("<fn>", …)`
# names a call inside it. That is every binding the host registers, plus what the Luau prelude
# adds on top. Anything here without an entry is a call a module author can make and cannot
# read about — which is how `host.os.pick`, `host.arbiter`, `host.now`, `host.inputEpoch`,
# `host.calibrating` and `host.resource.exists` went undocumented while a checker that only
# looked the other way reported everything as fine.

# Names the host registers that are not module surface. Kept short and justified, because an
# allowlist is where a checker goes to die.
NOT_SURFACE = {
    'host.overlay',    # gone; the overlay is a module reached through host.require
}


def without_test_modules(src):
    """`src` with every `#[cfg(test)] mod … { … }` block removed.

    A test may build a table named after a namespace and put anything it likes on it — the
    capability-gate tests hang a deliberately leaky function on a stand-in `log` table — and
    the scraper below would read that as a call a module author can make. It cannot, and
    telling them to document it would be telling them to document a fixture.

    Cut on a closing brace in COLUMN ZERO rather than by counting braces: a test that embeds
    Luau (these do) has braces inside string literals, and a counter that does not also parse
    Rust string syntax would land in the wrong place. rustfmt puts a top-level `mod`'s closing
    brace at column zero, and everything inside it is indented.
    """
    lines = src.splitlines(keepends=True)
    out, skipping = [], False
    for i, line in enumerate(lines):
        # Only a `#[cfg(test)]` that introduces a MODULE opens a block to skip. On a `use` or
        # a single item there is no brace to close, and skipping to the next column-zero one
        # would swallow the real code in between.
        if not skipping and line.startswith('#[cfg(test)]'):
            following = lines[i + 1] if i + 1 < len(lines) else ''
            if following.startswith('mod '):
                skipping = True
                continue
        if skipping:
            if line.rstrip() == '}':
                skipping = False
            continue
        out.append(line)
    return ''.join(out)


# The Rust files that register `host.*` bindings. lib.rs holds nearly all of them; a namespace
# big enough to deserve a file of its own is listed here, or its calls would be invisible to the
# check below — a call a module can make and nobody would be told is undocumented.
BINDING_SOURCES = [
    os.path.join('crates', 'host', 'src', 'lib.rs'),
    os.path.join('crates', 'host', 'src', 'gamepad_api.rs'),
]


def public_names():
    """Every `host.*` a module can call, from the Rust bindings and the Luau prelude."""
    lib = '\n'.join(
        without_test_modules(open(path, encoding='utf-8').read()) for path in BINDING_SOURCES
    )
    pre = open(os.path.join('crates', 'host', 'src', 'window_prelude.luau'),
               encoding='utf-8').read()

    names = set()
    # host.set("ns", var) — a namespace, and the local that holds it.
    ns_var = {}
    for m in re.finditer(r'host\.set\(\s*"([A-Za-z_][A-Za-z0-9_]*)"\s*,\s*([A-Za-z_][A-Za-z0-9_]*)',
                         lib):
        ns_var[m.group(2)] = m.group(1)
        names.add('host.' + m.group(1))
    # var.set("fn", …) — a call inside one of them.
    for var, ns in ns_var.items():
        for m in re.finditer(re.escape(var) + r'\.set\(\s*"([A-Za-z_][A-Za-z0-9_]*)"', lib):
            names.add('host.' + ns + '.' + m.group(1))
    # The prelude adds its own, on host and on the window table.
    for m in re.finditer(r'function\s+host\.([A-Za-z0-9_.]+)\s*\(', pre):
        names.add('host.' + m.group(1))
    for m in re.finditer(r'function\s+W\.([A-Za-z0-9_]+)\s*\(', pre):
        names.add('host.window.' + m.group(1))
    # A leading underscore is this project's mark for ''internal, do not call'': the window
    # trigger dispatchers are reached by the prelude, never by a module.
    return {n for n in names
            if n not in NOT_SURFACE and not n.split('.')[-1].startswith('_')}


def undocumented(entries):
    """Public names no entry covers, by the name each entry leads with."""
    documented = set()
    for e in entries:
        m = re.match(r'(host\.[A-Za-z0-9_.]+)', e['title'])
        if m:
            documented.add(m.group(1).rstrip('.'))
        # `host.config.get / host.config.set / …` documents several in one heading.
        for extra in re.findall(r'host\.[A-Za-z0-9_.]+', e['title']):
            documented.add(extra.rstrip('.'))
    missing = []
    for name in sorted(public_names()):
        if name in documented:
            continue
        # A namespace is covered when any of its calls is.
        if any(d.startswith(name + '.') for d in documented):
            continue
        missing.append(name)
    return missing


def undeclared_pages():
    """Reference pages missing the box that says what module.toml must list.

    Every page answers the same question for the namespace it documents, and a page that
    forgets to is one an author reads without ever learning whether it costs them a
    declaration.
    """
    missing = []
    for path in sorted(glob.glob(os.path.join(API, '*.md'))):
        if os.path.basename(path) == 'index.md':
            continue
        if '{#declare}' not in open(path, encoding='utf-8').read():
            missing.append(path)
    return missing


def main():
    check = '--check' in sys.argv
    entries = collect()

    undeclared = undeclared_pages()
    if undeclared:
        print('reference page(s) with no "What to declare" box — an author cannot tell from')
        print('these what module.toml needs:')
        for f in undeclared:
            print('  ' + f.replace(os.sep, '/'))

    dupes = {}
    for e in entries:
        dupes.setdefault(e['anchor'], []).append(e['title'])
    clashes = {a: t for a, t in dupes.items() if len(t) > 1}
    if clashes:
        print('anchor collisions — two entries would answer to the same link:')
        for a, t in clashes.items():
            print('  #' + a + ': ' + ', '.join(t))
        return 1

    changed = rewrite_anchors(entries, dry=check)
    changed += limit_toc(sorted({e['path'] for e in entries}), dry=check)
    entries = collect() if not check else entries
    index = build_index(entries)
    current = open(INDEX, encoding='utf-8').read() if os.path.exists(INDEX) else None

    gaps = undocumented(entries)
    if gaps:
        print('public name(s) with no entry — a module author can call these and cannot read')
        print('about them:')
        for g in gaps:
            print('  ' + g)

    if check:
        if gaps or undeclared:
            return 1
        stale = changed + ([INDEX] if current != index else [])
        if stale:
            print('out of date, run `python tools/api-index.py`:')
            for f in stale:
                print('  ' + f.replace(os.sep, '/'))
            return 1
        print(str(len(entries)) + ' entries: anchors stamped and the index is current.')
        return 0

    io.open(INDEX, 'w', encoding='utf-8', newline='\n').write(index)
    print(str(len(entries)) + ' entries across ' +
          str(len({e['ns'] for e in entries})) + ' namespaces')
    print('anchors rewritten in ' + str(len(changed)) + ' file(s); index written to ' + INDEX)
    return 0


if __name__ == '__main__':
    sys.exit(main())
