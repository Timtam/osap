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
    'Overlay', 'host.window', 'host.screen', 'host.ocr', 'host.uia', 'host.input',
    'host.keys', 'host.hotkey', 'host.speech', 'host.sound', 'host.timer', 'host.settings',
    'host.config', 'host.os', 'host.log', 'host.path', 'host.resource', 'host.require',
    'host.tryRequire', 'host.include', 'host.epoch', 'host.now', 'host.inputEpoch',
    'host.arbiter', 'host.calibrating', 'Concepts',
]

NS_BLURB = {
    'Overlay': 'The self-voicing control tree: what a module builds, and how it is bound to a '
               'window. `local O = host.require("com.platform.overlay")`.',
    'host.window': 'Finding windows and their controls, and reacting when the focus moves.',
    'host.screen': 'Reading pixels, and finding a picture within them.',
    'host.ocr': 'Reading text that exists nowhere but on the screen.',
    'host.uia': 'Asking the accessibility layer what a window contains.',
    'host.input': 'Driving the mouse and keyboard.',
    'host.keys': 'Claiming keys before the application sees them.',
    'host.hotkey': 'Claiming a combination system-wide.',
    'host.arbiter': 'Deciding which of several overlays owns a contested slot.',
    'host.timer': 'Waiting without blocking, and knowing when a cached reading went stale.',
    'host.settings': 'The few choices a module should not make on the user\'s behalf.',
    'host.log': 'The log is evidence: the tester is blind, remote, and often on the platform '
                'none of us can run.',
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
    '**It is not enforced.** Nothing in the host gates a call on this list — every module '
    'receives the whole `host` table whichever names it declares, and a name nobody recognises '
    'loads without complaint. What the list actually does is two things: it is written to the '
    'log when the module loads, and it is shown to the user before installing a module from '
    'GitHub. That second one is the only reason to keep it accurate — it is the only thing '
    'somebody is told about a stranger\'s module before it runs.',
    '',
    'Which also means it cannot be a security claim: it is a self-report from exactly the party '
    'a reader has no reason to trust. Treat it as a declaration of intent, and expect it to '
    'become load-bearing later.',
    '',
    'The overlay is the exception in shape rather than degree — it is a **module**, so it goes '
    'under `dependencies` rather than here.',
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
           'Luau VM, and only fires while that module is **enabled**.', '']
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
    'host.match',      # the matcher constructor, documented as the "Matchers" grammar instead
    'host.overlay',    # gone; the overlay is a module reached through host.require
}


def public_names():
    """Every `host.*` a module can call, from the Rust bindings and the Luau prelude."""
    lib = open(os.path.join('crates', 'host', 'src', 'lib.rs'), encoding='utf-8').read()
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


def main():
    check = '--check' in sys.argv
    entries = collect()

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
        if gaps:
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
