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

The same pass stamps an entry's own level-3 headings (its Returns, Errors and OS blocks) from
its anchor, and `--check` also holds docs/api/errors.md's codes to crates/host/src/errors.rs, and
every page's Errors tables to the reasons the host and the overlay runtime can give, both ways.
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
    'host.keys', 'host.hotkey', 'host.gamepad', 'host.speech', 'host.sound', 'host.gui',
    'host.timer', 'host.settings',
    'host.config', 'host.errors', 'host.os', 'host.log', 'host.path', 'host.resource', 'host.json',
    'host.require',
    'host.tryRequire', 'host.include', 'host.epoch', 'host.now', 'host.inputEpoch',
    'host.arbiter', 'Concepts',
]

NS_BLURB = {
    'Overlay': 'Controls a module defines over a plug-in window, walked with Tab and spoken '
              'aloud. A module, not a host namespace: `host.require("com.platform.overlay")`.',
    'host.window': 'Finding windows and the surfaces inside them, and reacting when the focus '
                   'moves.',
    'host.screen': 'Reading pixels, profiling a region, reducing one to a grid of cells, and '
                   'finding an image within one — from the screen, or from a snapshot of it '
                   'read several times.',
    'host.ocr': 'Recognising text in a screen region.',
    'host.element': 'Querying the accessibility tree an application publishes.',
    'host.input': 'Synthesising mouse and keyboard input.',
    'host.keys': 'Claiming keys before the focused application sees them, and the key spec every '
                 'call that takes a key reads.',
    'host.hotkey': 'Claiming a combination system-wide.',
    'host.gamepad': 'Watching game controllers — presses, button combinations, sticks, pads '
                    'coming and going. Observed only, never taken from the game, and not '
                    'filtered by the window in front.',
    'host.arbiter': 'Deciding which of several overlays owns a contested slot.',
    'host.gui': 'An HTML page of the module\'s own, in a window beside the plug-in. Experimental: '
                'the API may change after the Windows probe and the Mac\'s first VoiceOver tests.',
    'host.timer': 'Waiting without blocking, and knowing when a cached reading went stale.',
    'host.settings': 'Typed, per-module settings, edited by the user in the module manager.',
    'host.log': 'Writing to the log file beside the application.',
    'host.errors': 'What a call answers when something it was asked to do was not done: an error '
                   'value, its code, and the constants to compare the code with.',
    'host.json': 'Turning JSON text a module ships into Luau values, and Luau values into JSON text.',
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
    # The methods of the objects a call hands out belong with the call: the trip O:inPlugin
    # hands an act and the binding `O.window` makes, with the overlay; the page
    # `host.gui.webview` returns, and the page's own `platform`, with host.gui; a snapshot, with
    # host.screen. Without these, `Snapshot:crop` and `platform.post` were filed under Concepts.
    for prefix, ns in (('trip', 'Overlay'), ('Binding', 'Overlay'), ('page', 'host.gui'),
                       ('platform', 'host.gui'), ('Snapshot', 'host.screen')):
        if h.startswith(prefix + ':') or h.startswith(prefix + '.'):
            return ns
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
                'body': parts[i + 1],
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

    texts = {}
    level3 = {}
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
        texts[os.path.normpath(path)], level3[os.path.normpath(path)] = stamp_level3(text)

    # A level-3 anchor that changed is linked from other pages too, the guides included.
    docs = {os.path.normpath(p) for p in glob.glob(os.path.join('docs', '**', '*.md'), recursive=True)}
    for path in sorted(set(texts) | docs):
        original = open(path, encoding='utf-8').read()
        text = repoint(path, texts.get(path, original), level3)
        if text != original:
            changed.append(path)
            if not dry:
                io.open(path, 'w', encoding='utf-8', newline='\n').write(text)
    return changed


# ── An entry's own level-3 headings ─────────────────────────────────────────────────────────
#
# Every call entry has the same few level-3 headings — its OS blocks, and from the typed errors
# on its Returns and Errors — so Docusaurus numbered their anchors down the page: `#macos-1` was
# page:show's macOS block until an entry above it grew one, and then every link after it pointed
# at the wrong call, silently, as the entry anchors once did. Each is stamped from its entry's
# anchor instead, `{#page-show-macos}`. Any other level-3 heading keeps the anchor it has: it is
# named by what it says, and other pages link to it by that.
STAMPED = ('Returns', 'Errors', 'Windows', 'macOS', 'Both')


def headings(text):
    """(line number, level, text) of every heading outside fenced code, in page order."""
    out, fenced = [], False
    for i, line in enumerate(text.split('\n')):
        if re.match(r'^(```|~~~)', line):
            fenced = not fenced
            continue
        m = None if fenced else re.match(r'^(#{1,6}) (.+?)\s*$', line)
        if m:
            out.append((i, len(m.group(1)), m.group(2)))
    return out


def explicit(head):
    m = re.search(r'\{#([A-Za-z0-9_-]+)\}\s*$', head)
    return m.group(1) if m else None


def bare(head):
    return unescaped(re.sub(r'\s*\{#[^}]*\}\s*$', '', head).strip())


def stamp_level3(text):
    """`text` with each entry's STAMPED headings stamped, and the anchors they had before: the
    numbered ones Docusaurus gave them (github-slugger's: the second `macOS` of a page is
    `macos-1`; a heading with an anchor of its own takes no number) or an explicit one that
    differs."""
    lines = text.split('\n')
    renames, seen, entry = {}, {}, None
    for i, level, head in headings(text):
        own = explicit(head)
        if level == 2:
            entry = own
        before = own
        if own is None:
            base = re.sub(r'[^\w\- ]', '', bare(head).lower()).replace(' ', '-')
            n = seen.get(base, 0)
            seen[base] = n + 1
            before = base if n == 0 else base + '-' + str(n)
        if level == 3 and entry and bare(head) in STAMPED:
            want = entry + '-' + bare(head).lower()
            if own != want:
                lines[i] = '### ' + bare(head) + ' {#' + want + '}'
                renames[before] = want
    return '\n'.join(lines), renames


def repoint(path, text, level3):
    """`text`, the page at `path`, with every link to a level-3 anchor that changed repointed:
    `(#macos-1)` on the page itself, `(gui.md#macos-1)` and `(api/gui.md#windows-1)` from
    another."""
    here = os.path.dirname(path)

    def one(m):
        target, anchor = m.group(1), m.group(2)
        page = os.path.normpath(os.path.join(here, target)) if target else os.path.normpath(path)
        new = level3.get(page, {}).get(anchor)
        return '](' + target + '#' + new + ')' if new else m.group(0)

    return re.sub(r'\]\(([^()\s#]*)#([A-Za-z0-9_-]+)\)', one, text)


# How a call fails, said once on the index, before what to declare: whether it answers or
# raises, the error value, the partial answers a module must read on purpose, and where each
# page lists them. docs/api/errors.md has the rule in full; this is what a reader of the index
# must not miss.
FAILURES = [
    '## When a call fails {#failures}',
    '',
    'A call that cannot do what it was asked tells the module in one of two ways: it **answers '
    'an error value**, or it **raises**. Which of the two is a rule, the same for every call. '
    '[host.errors](errors.md) has the rule in full, and the codes.',
    '',
    '### Return or raise {#return-or-raise}',
    '',
    '- **When the moment decides, the call answers an error value.** The moment is the window, '
    'the screen, the system, or what another module or program holds. It is an outcome to plan '
    'for. The call answers its usual value for "nothing" — `nil`, `false` or `{}` — and the '
    'error value after it: `nil, err`.',
    '- **When the arguments alone decide, the call raises.** A wrong type, an unknown option, a '
    'value outside its fixed range, a key spec that does not parse, a file missing from the '
    'module\'s own package. It is a mistake to fix. The log and the error dialog name the call '
    'and what was wrong.',
    '',
    'Any call can also raise for two reasons that are not its own. The capability gate raises '
    'for a namespace the module did not declare ([What to declare](#capabilities)). The guard '
    'raises `stopped by the host: …` in a module that ran too long ([the '
    'limits](../module-runtime-and-lifecycle.md#limits)). And a call that waits raises where it '
    'cannot wait, when it is given no callback ([Where it waits](screen.md#where-it-waits)).',
    '',
    '### The error value {#the-error-value}',
    '',
    'An error value is a read-only table:',
    '',
    '- `code` — what kind of failure it is, a lower-case word such as `not_ready`. This is what '
    'a module compares.',
    '- `message` — a sentence for the log. It may change between versions; the code does not.',
    '- `detail` — more for the log, or `nil`. Never compared.',
    '',
    'It reads as its message: `tostring(err)` gives it, and so does `"not read: " .. err`. '
    '`host.log.info(err)` logs it and `host.speech.output(err)` speaks it. It is not a string, '
    'though. The string functions raise for it, and so does a field it does not have, such as '
    '`err.cod` ([Error values](errors.md#error-values)).',
    '',
    'Compare its code with a constant of [`host.errors`](errors.md#host-errors), never its '
    'message:',
    '',
    '```luau',
    'local saved = nil   -- the id of a window to come back to',
    '',
    'local function comeBack()',
    '  if not saved then return end',
    '  local ok, err = host.window.focus(saved)',
    '  if not ok then',
    '    host.log.info("[my-module] not focused: " .. err)      -- the message, for the log',
    '    if err.code == host.errors.GONE then saved = nil end   -- the code, to decide by',
    '  end',
    'end',
    '```',
    '',
    '### A value and an error together {#partial-answers}',
    '',
    '**Read both values.** Mostly an error value comes with nothing beside it: `nil, err`, '
    '`false, err` or `{}, err`. Four calls can answer a result **and** an error value at once: '
    '`page:show`, the two dumps and `host.hotkey.register`. '
    'The result is the part that was done, and the error value says what was not. This is '
    'unusual, so each of the four says so in its Returns.',
    '',
    '| call | answers | what it means |',
    '|---|---|---|',
    '| [`page:show`](gui.md#page-show-returns) with `focus` | `true, err` | The page is shown '
    'and in front, but over part of `avoid`: there was no room beside it. `err.code` is '
    '`too_large`. An overlay page\'s `onShown` is handed the same two values. |',
    '| [`host.element.dump`](element.md#host-element-dump-returns) and '
    '[`rawDump`](element.md#host-element-rawdump-returns) | `list, err` | The elements read '
    'before the dump stopped. `err` says why it stopped: the tree was larger than a dump reads, '
    'the walk took too long, or the program stopped answering. |',
    '| [`host.hotkey.register`](hotkey.md#host-hotkey-register-returns) | `id, err` | The claim '
    'is made and kept, but not held right now. Its callback fires once it is held. `err` says '
    'why it is not. |',
    '',
    '`if not ok then` alone takes such an answer for a whole one, and `if err then return end` '
    'throws away the part that was done. Test each value for what it says:',
    '',
    '```luau',
    'local id, err = host.hotkey.register("Ctrl+Shift+F9", function() host.speech.output("F9") end)',
    'if id and err then',
    '  host.log.info("[my-module] Ctrl+Shift+F9 is claimed, but not held now: " .. err)',
    'elseif err then',
    '  host.log.info("[my-module] Ctrl+Shift+F9 is not claimed: " .. err)',
    'end',
    'local hotkeyId = id   -- kept even when it is not held now: the claim is tried again',
    '```',
    '',
    '[Partial answers](errors.md#partial-answers) has the three in full.',
    '',
    '### Where it arrives {#where-it-arrives}',
    '',
    '- **The call\'s second value**, most often: `local v, err = call()`.',
    '- **The callback\'s second argument**, for a call given a callback: `cb(value, err, '
    'info)`.',
    '- **A field of the answer**: a text reading has `status = "failed"` and its `error`.',
    '- **An argument of a hook** the module wrote into a spec, such as a page\'s `onClose(how, '
    'err)` or a menu choice\'s `onDone`.',
    '',
    'A call that hands its work on answers once it has handed it on. What fails after that '
    'reaches only its callback or hook. A click with `via = "noactivate"` is one '
    '([`host.input.click`](input.md#host-input-click)). An overlay\'s '
    '[`O:chooseMenuItem`](overlay.md#o-choosemenuitem), whose `onDone` hears how the choice '
    'ended, is another.',
    '',
    '### Returns and Errors, on every page {#returns-and-errors}',
    '',
    'Every call\'s entry has the same two sections, after its description and examples, and '
    'before its Windows and macOS blocks:',
    '',
    '- **Returns** says every form its answer takes: the value, the missing value with an error '
    'value, and a partial answer where there is one.',
    '- **Errors** is a table of every error value the call can give: its code, where it '
    'arrives, when it comes, its message, and the platform. Under the table, **Raises** lists '
    'the mistakes that raise instead.',
    '',
    'A call that never fails says "None. It never answers an error value." [Reading an Errors '
    'table](errors.md#reading-an-errors-table) explains the columns. Each table is checked '
    'against the host\'s own list of reasons, and the overlay runtime\'s, in both directions: a '
    'row the host never gives fails the documentation check, and so does a reason with no row.',
    '',
    'These calls answer error values: [`host.gui`](gui.md)\'s pages, '
    '[`host.window.focus`](window.md#host-window-focus), the questions of '
    '[`host.element`](element.md), [`host.input`](input.md)\'s `click`, `post` and `send`, '
    '[`host.keys.capture`](keys.md#host-keys-capture), '
    '[`host.hotkey.register`](hotkey.md#host-hotkey-register), every call of '
    '[`host.screen`](screen.md) but `size`, `predicate` and `pending`, a snapshot\'s `crop`, '
    '[`host.ocr`](ocr.md)\'s readings and `resolveLanguage`, '
    '[`host.speech.use`](speech.md#host-speech-use), '
    '[`host.sound.play`](sound.md#host-sound-play), [`host.gamepad`](gamepad.md)\'s `list`, '
    '`state` and `on`, and the [overlay runtime](overlay.md), with codes of its own in '
    '[`O.errors`](overlay.md#o-errors). Every other call answers none: it raises for a mistake, '
    'and its entry says what its `nil` or `false` means.',
    '',
]

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
    '`host.config`), `log`, `path`, `resource`, `arbiter` and `gui` (experimental) — one per '
    'namespace, spelt as the namespace. There are no finer-grained names: `"window.read"` or `"screen.imagesearch"` '
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
    '**Receiving window events needs no declaration.** The host delivers foreground and focus '
    'changes to the callbacks registered with `host.window.onTrigger` and `onFocus` through a '
    'handle of its own, not through the module\'s `host` table. So a module that attaches an '
    'overlay declares `window` only if its own code calls `host.window`: the overlay runtime '
    'registers the triggers with its own manifest\'s permission, and a module that registered '
    'none is skipped. A gate or a matcher\'s `where` function the '
    'module supplies is its own code, though, and needs `window` if it calls `host.window`.',
    '',
    'Nothing is gated on `host.os`, `host.require`, `host.tryRequire`, `host.include`, '
    '`host.epoch`, `host.now`, `host.inputEpoch`, `host.json` or `host.errors`, nor on '
    '`host.keys.normalize`, `describe` and `check` — the rest of `host.keys` needs `keys`. A '
    'clock, a counter, a platform name, a way to reach a declared dependency and a parser of '
    'strings the module already holds are not worth asking permission for, and gating them would '
    'mean every manifest names them — which is the same as naming none. `host.keys.check` reads '
    'one thing besides its string: for a Ctrl+Alt chord on Windows or an Option chord on a Mac, '
    'the character the keyboard layout in use types with it, unless it is asked with '
    '`layout = false`.',
    '',
    'The list is also shown to the user before installing from GitHub — for the module chosen '
    'and for every dependency the install adds — and again before an update that asks for a '
    'capability the installed version did not, or starts using a module it did not (see [the '
    'module manager](../module-manager.md#browse-tab)). Nothing is granted or refused per '
    'capability, and a module copied into `modules/` by hand is not reviewed at all. It is not a '
    'security boundary on its own — a module still runs arbitrary '
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
    'path stays inside the module; [its page](include.md#host-include) says how.',
    '',
    '## Coordinates {#coordinates}',
    '',
    'Every coordinate the platform takes or returns — window bounds, regions, clicks, hits — is '
    'a screen coordinate with its origin at the top left of the primary display. On **Windows** '
    'it is a physical device pixel, because the application is per-monitor DPI aware; on '
    '**macOS** it is a point. The two agree only at 100 % scaling, and a Retina Mac is exactly '
    'half of the same panel on Windows at 200 %. Coordinates measured by a tool that is not '
    'DPI-aware, on a Windows display scaled above 100 %, have to be multiplied by the scale '
    'factor. A fractional coordinate is cut toward zero without an error, except by the calls '
    'that read regions strictly — the cells calls and `host.ocr.recognize` — which raise for a '
    'corner that is not a whole number. Regions are `{ x1, y1, x2, y2 }` with `x2` and `y2` '
    'exclusive, and every `host.screen` and `host.ocr` call that takes a region also takes one '
    'as fractions of a window\'s client area, `{ window = w, fraction = { x1, y1, x2, y2 } }` — '
    'and `host.screen.pixel` a point, `{ window = w, fraction = { x, y } }` — which follows the '
    'window across sizes, scalings and platforms. A region table that is neither form raises; '
    'only a region left out is the whole primary screen — see [Region form](screen.md#region-form). '
    'An [overlay](overlay.md) control\'s `region` and `ocrLabel` options take only corners, '
    'relative to the overlay\'s origin, or a function answering them; an overlay with '
    '[`O:scale`](overlay.md#o-scale) scales them, and every other authored coordinate but a '
    '`rawOrigin` control\'s, by a factor it asks for at each use, and rounds the result to whole '
    'pixels on Windows and whole points on macOS.',
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
    '- **A reader that compares grid signatures by similarity** (a region cut into blocks, each '
    'the share of pixels passing a colour test). That is '
    '[`host.screen.matchCells`](screen.md#host-screen-matchcells): its colour test is pasted as '
    'written, its regions are window fractions, its signatures are hex, and the similarity '
    'and the runner-up come back for the module\'s own thresholds.',
    '- **A reader that keeps a frame and reads many points from it.** Keep one: '
    '[`host.screen.snapshot`](screen.md#host-screen-snapshot) takes a picture that every screen '
    'call given `{ snapshot = s }` reads, and [`pixels`](screen.md#host-screen-pixels) reads many '
    'points from as few captures as they allow. Without them, on Windows every '
    '[`pixel`](screen.md#host-screen-pixel) call is a screen read of its own; on macOS only '
    'reads close together in place and time share one.',
    '- **A reader that captures one frame per poll and runs every test on it.** Take one picture '
    'per poll with [`snapshot`](screen.md#snapshot-with-a-callback) given a callback, off the event '
    'loop and able to wait until part of the screen changes — a poll that waits in `snapshot` '
    'without one keeps the keys of its module waiting — and pass it '
    'to every test, '
    '[`host.ocr.recognize`](ocr.md#host-ocr-recognize) included: they all read that one picture. Without '
    'one, [`matchCells`](screen.md#host-screen-matchcells) and the image searches share a '
    'picture only when they ask for the same region, read the same way, and '
    'fall into the same batch of the image worker, and two `host.ocr.recognize` calls only when they '
    'ask for the same thing before the first is taken; otherwise detecting a state and reading '
    'its text are two pictures, taken at two moments.',
    '- **Tesseract or other OCR language codes.** `lang` is a language tag such as `"de"` or '
    '`"de-DE"`, matched against what the platform\'s recogniser reads. A three-letter code such '
    'as `"eng"` is a well-formed tag that no recogniser lists, so it does not raise: every '
    'region is answered `"failed"`, with an `error` of code `unavailable` whose message names '
    'the languages that are there. Write `"en"` — '
    '[Recognition language](ocr.md#recognition-language). '
    '[`host.ocr.recognize`](ocr.md#host-ocr-recognize) recognises off the event loop: given a '
    'callback it answers there, and without one it waits in its handler, holding only its own '
    'module, and raises where it cannot wait ([Where it waits](ocr.md#where-it-waits)).',
    '- **AutoHotkey.** `ahk_class` belongs inside the `windows` block of a '
    '[matcher](window.md#matchers); `SetTimer` is '
    '[`host.timer.every`](timer.md#host-timer-every), stopped with '
    '[`host.timer.cancel`](timer.md#host-timer-cancel); `Send` is '
    '[`host.input.send`](input.md#host-input-send), virtual keys only; `ImageSearch`\'s second '
    'corner is exclusive here.',
    '- **Exceptions, or an error code to fetch afterwards** (AutoHotkey\'s `throw` and '
    '`ErrorLevel`, Windows\' `GetLastError`). A failure of the moment is not thrown and sets '
    'nothing global: the call answers an error value as its second value, and only a mistake '
    'in the call raises — [When a call fails](#failures).',
    '- **A keyboard hook that only listens.** A [capture](keys.md#host-keys-capture) takes the '
    'key away; there is no listen-only mode for ordinary keys, only the `"<modifier> tap"` '
    'form watches without taking.',
    '- **A script host with threads or async.** Every callback runs on one thread; one that runs '
    'past 2 seconds of processor time or 10 seconds in all is stopped, with its module '
    '([the limits](../module-runtime-and-lifecycle.md#limits)), and below that it holds '
    'everything while it runs. The slow work of '
    '[`host.ocr.recognize`](ocr.md#host-ocr-recognize) and of every '
    '[`host.screen`](screen.md#with-a-callback) call given a callback — '
    '[`imageSearchEach`](screen.md#host-screen-imagesearcheach) among them — runs on threads of '
    'the host\'s own and answers in a callback. Without one, a text read answers the handler that waits '
    'for it, and so do [`snapshot`](screen.md#where-it-waits), `pixel`, `pixels`, `profile`, '
    '`cells`, `matchCells`, `template{ capture }`, a snapshot\'s `crop`, the searches without a '
    'callback, `save` and `saveMarked`: only their own module waits, and each of them raises '
    'where it cannot wait ([Where it waits](screen.md#where-it-waits)). `template` from bytes or a '
    'file checks its arguments on the loop and answers at once, its picture made on the image '
    'worker; a search checks a template path on the loop by its PNG header, the first time the '
    'path is named, and the image worker opens it. Which call runs where: '
    '[Threads](../module-runtime-and-lifecycle.md#threads).',
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
    out += FAILURES + CAPABILITIES + ['']
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
# read about — which is how `host.os.pick`, `host.arbiter`, `host.now`, `host.inputEpoch` and
# `host.resource.exists`, among others, went undocumented while a checker that only looked the
# other way reported everything as fine.

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
    os.path.join('crates', 'host', 'src', 'element_lua.rs'),
    os.path.join('crates', 'host', 'src', 'gamepad_api.rs'),
    os.path.join('crates', 'host', 'src', 'webview_lua.rs'),
    os.path.join('crates', 'host', 'src', 'errors_lua.rs'),
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


# ── Returns and Errors: what each page says, held to the Reason table ───────────────────────
#
# Every call entry says what it returns and which errors it gives, under `### Returns` and
# `### Errors` (docs/api/errors.md, "Reading an Errors table"). An Errors table is compared with
# the reasons the host can give — crates/host/src/errors.rs, the one table every reason is made
# from — and with the overlay runtime's, in both directions: a row with no reason behind it
# fails, and so does a reason of the entry with no row. That is the only way a wrong or
# incomplete Errors table can fail CI, so the table is read from the source, not restated here.
#
# The pages were revised one at a time, and a CONVERTED set named those whose entries followed
# the template. It ended when it named every page: the checks below run for every page, so a new
# page is held to the template from its first line.

# Entries that are not calls, by anchor. A field says what it holds under Returns and has no
# Errors; a concept needs neither. Every other entry is a call, so a new heading is told to grow
# both. Named rather than guessed from the title's punctuation: `O.contentSize(origin)` and
# `O.calibrating()` read as calls and are fields, `O.menuTests` reads as a field and is a concept.
FIELDS = {
    'page-window', 'host-os-current', 'host-errors', 'host-element-type',
    'o-layer', 'o-state', 'o-away', 'o-calibrating', 'o-contentsize', 'o-stategeneration', 'o-errors',
}
CONCEPTS = {
    'error-values', 'partial-answers', 'return-or-raise', 'where-an-error-value-arrives',
    'reading-an-errors-table', 'comparing-codes', 'the-overlay-runtime-s-codes',
    'asking-without-waiting', 'button-combinations', 'button-and-axis-names',
    'what-a-page-is-served-and-what-it-may-not-do', 'key-spec-string-format',
    'recognition-language', 'where-a-hook-runs', 'ocrlabel', 'o-menutests', 'bindings',
    'plugin-base-library-overlays', 'a-runtime-shared-by-game-modules',
    'which-picture-a-read-sees', 'failure-reasons', 'where-it-waits', 'with-a-callback',
    'reading-a-snapshot', 'region-form', 'a-slow-callback', 'table-shapes', 'matchers',
}

ERRORS_RS = os.path.join('crates', 'host', 'src', 'errors.rs')
ERRORS_LUAU = os.path.join('modules', 'overlay-runtime', 'src', 'errors.luau')
ERRORS_PAGE = os.path.join(API, 'errors.md')
# The page of the overlay runtime's entries, whose reasons are errors.luau's.
RUNTIME_PAGE = 'overlay'

# The fixed words of an Errors table's "arrives" and "platform" cells.
ARRIVES = re.compile(r'(return|callback|both|field|hook [A-Za-z][A-Za-z0-9]*)')
PLATFORMS = {'both', 'Windows', 'macOS', 'other'}


def host_table():
    """errors.rs as the tool reads it: the codes in order, every Reason (`code`, `message`,
    `entries`), and the groups. Each Reason is one `const` or `static`, of any visibility,
    `NAME: Reason = Reason { … };` with its message on one line. Every Reason literal outside a
    comment is counted, so one written another way — or anywhere else in the file — is reported
    rather than skipped."""
    src = open(ERRORS_RS, encoding='utf-8').read()
    src = src[:src.index('\nmod tests {')] if '\nmod tests {' in src else src
    src = re.sub(r'^[ \t]*//.*$', '', src, flags=re.M)
    codes = dict(re.findall(r'Code::(\w+) => "([a-z][a-z0-9_]*)"', src))
    lists = {m.group(1): re.findall(r'"([^"]+)"', m.group(2))
             for m in re.finditer(r'const (\w+): &\[&str\] = &\[(.*?)\];', src, re.S)}
    reasons = []
    for m in re.finditer(r'^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:const|static)[ \t]+(\w+): Reason = Reason \{\s*'
                         r'code: Code::(\w+),\s*message: "((?:[^"\\]|\\.)*)",\s*'
                         r'entries: (&\[(.*?)\]|\w+),?\s*\};', src, re.S | re.M):
        name, code, message, entries, inline = m.groups()
        reasons.append({
            'source': ERRORS_RS,
            'name': name,
            'code': codes[code],
            'message': message.replace('\\"', '"').replace('\\\\', '\\'),
            'entries': re.findall(r'"([^"]+)"', inline) if entries.startswith('&[') else lists[entries],
        })
    declared = len(re.findall(r'(?<!\w)Reason\s*\{\s*(?:code|message|entries)\b', src))
    unread = declared - len(reasons)
    block = src[src.index('pub static GROUPS'):]
    block = block[:block.index('\n];')]
    groups = {m.group(1): re.findall(r'"([^"]+)"', m.group(2))
              for m in re.finditer(r'\(\s*"([a-z0-9-]+)",\s*&\[(.*?)\],?\s*\)', block, re.S)}
    order = [codes[c] for c in re.findall(r'Code::(\w+),', src[src.index('pub const ALL'):src.index('];', src.index('pub const ALL'))])]
    return order, reasons, groups, unread


def runtime_reasons():
    """The overlay runtime's reasons, from its errors.luau, in the same shape: one
    `key = { code = "…", message = "…", entries = { "…", … } },` each — and how many `code = "…"`
    outside a comment are not read so, which a reason written another way would be. None until
    the runtime has the file."""
    if not os.path.exists(ERRORS_LUAU):
        return [], 0
    src = open(ERRORS_LUAU, encoding='utf-8').read()
    src = re.sub(r'--.*$', '', src, flags=re.M)
    reasons = [{'source': ERRORS_LUAU, 'name': m.group(1), 'code': m.group(2), 'message': m.group(3).replace('\\"', '"'),
                'entries': re.findall(r'"([^"]+)"', m.group(4))}
               for m in re.finditer(r'(\w+)\s*=\s*\{\s*code\s*=\s*"([a-z][a-z0-9_]*)",\s*'
                                    r'message\s*=\s*"((?:[^"\\]|\\.)*)",\s*entries\s*=\s*\{(.*?)\}\s*,?\s*\}',
                                    src, re.S)]
    return reasons, len(re.findall(r'\bcode\s*=\s*"', src)) - len(reasons)


def runtime_lists():
    """errors.luau's `codes`, in order, and its `passes`: for each runtime entry, the host entries
    whose errors it hands on as they came — `["o-…"] = { "host-…", … },` each."""
    if not os.path.exists(ERRORS_LUAU):
        return [], {}
    src = re.sub(r'--.*$', '', open(ERRORS_LUAU, encoding='utf-8').read(), flags=re.M)
    m = re.search(r'\bcodes\s*=\s*\{(.*?)\}', src, re.S)
    codes = re.findall(r'"([a-z][a-z0-9_]*)"', m.group(1)) if m else []
    passes = {}
    m = re.search(r'\bpasses\s*=\s*\{(.*?)\n  \},', src, re.S)
    if m:
        for k, v in re.findall(r'\["([a-z0-9-]+)"\]\s*=\s*\{(.*?)\}', m.group(1), re.S):
            passes[k] = re.findall(r'"([^"]+)"', v)
    return codes, passes


# The groups of errors.rs: a group's own section, where its reasons are listed once, is a
# concept like any other.
GROUP_NAMES = set()


def kind(e):
    if e['anchor'] in CONCEPTS or e['anchor'] in GROUP_NAMES:
        return 'concept'
    return 'field' if e['anchor'] in FIELDS else 'call'


def sections(text):
    """An entry's level-3 sections, in order: (heading, its text)."""
    out, fenced, current = [], False, None
    for line in text.split('\n'):
        if re.match(r'^(```|~~~)', line):
            fenced = not fenced
        m = None if fenced else re.match(r'^### (.+?)\s*$', line)
        if m:
            current = [bare(m.group(1)), []]
            out.append(current)
        elif current:
            current[1].append(line)
    return [(h, '\n'.join(body)) for h, body in out]


ERRORS_HEADER = ['code', 'arrives', 'when', 'message', 'platform']
# A group's own section lists its reasons once, for every call that gives them: no "arrives".
GROUP_HEADER = ['code', 'when', 'message', 'platform']


def table_rows(text, header=ERRORS_HEADER):
    """The rows of every table in `text` whose header is `header`, the Errors template's unless
    told otherwise. A table inside fenced code is an example, not a row of the entry's."""
    rows, inside, fenced = [], False, False
    for line in text.split('\n'):
        if re.match(r'^\s*(```|~~~)', line):
            fenced = not fenced
            inside = False
            continue
        if fenced:
            continue
        cells = [c.strip() for c in line.strip().strip('|').split('|')] if line.lstrip().startswith('|') else None
        if cells is None:
            inside = False
            continue
        if not inside:
            inside = [c.lower() for c in cells] == header
            continue
        if set(''.join(cells)) <= set('-: '):
            continue
        rows.append(cells)
    return rows


def entry_problems(e, body, reasons, groups, codes, pages, passes):
    """What is wrong with one entry. `codes` are the codes a row may name, the host's and the
    runtime's; `pages` maps every entry's anchor to its page; `passes` maps a runtime entry to the
    host entries whose errors it hands on (errors.luau's `passes`)."""
    problems = []
    k = kind(e)
    if k == 'concept':
        return problems
    heads = [h for h, _ in sections(body)]
    os_at = min([i for i, h in enumerate(heads) if h in ('Windows', 'macOS', 'Both')] or [len(heads)])
    if 'Returns' not in heads[:os_at]:
        problems.append('no `### Returns` before its OS blocks')
    mine = reasons_of(e['anchor'], reasons, groups)
    # A field says what it holds; one that can raise or fail may say so under Errors too, and
    # that table is held to the reasons like a call's. One that a reason names must have it, or
    # the reason is compared with nothing.
    if k == 'field' and 'Errors' not in heads and not mine:
        return problems
    if 'Errors' not in heads[:os_at]:
        problems.append('no `### Errors` before its OS blocks')
        return problems
    if 'Returns' in heads and heads.index('Errors') < heads.index('Returns'):
        problems.append('`### Errors` comes before `### Returns`')
    text = dict(sections(body))['Errors']
    rows = table_rows(text)
    handed = set()
    if not rows and 'None. It never answers an error value.' not in text:
        problems.append('an Errors section with neither the table nor "None. It never answers an error value."')
    # What raises instead is said under the same lead everywhere, so that it can be found.
    if k == 'call' and '**Raises' not in text and 'It never raises' not in text:
        problems.append('an Errors section that says neither "**Raises** …" nor "It never raises."')
    covered = set()
    for row in rows:
        if len(row) != 5:
            problems.append('a row without five cells: ' + ' | '.join(row))
            continue
        code, arrives, _when, message, platform = row
        if not ARRIVES.fullmatch(arrives):
            problems.append('arrives `' + arrives + '` is not return, callback, both, field or hook <name>')
        if not all(p.strip() in PLATFORMS for p in platform.split(',')):
            problems.append('platform `' + platform + '` is not both, Windows, macOS or other')
        link = re.search(r'\]\(([^)#]*)#([A-Za-z0-9_-]+)\)', code)
        if link and not re.fullmatch(r'`[a-z_]+`', code):
            target = link.group(2)
            # A link stands for a set of reasons, and nothing beside it is read: a code written
            # next to it would be compared with nothing.
            if not re.fullmatch(r'(?:(?:any of(?: the)?|the rest of) )?\[[^\]]*\]\([^)]*\)', code):
                problems.append('a code cell with more than "any of", "any of the" or "the rest of" beside its link: ' + code)
            if target in groups:
                if e['anchor'] not in groups[target]:
                    problems.append('the group ' + target + ' is not one this entry gives (errors.rs, GROUPS)')
                covered |= {id(r) for r in reasons if target in r['entries']}
            elif target.endswith('-errors'):
                entry = target[:-len('-errors')]
                page = os.path.splitext(os.path.basename(link.group(1)))[0] if link.group(1) else e['page']
                if pages.get(entry) != page:
                    problems.append('a code cell that links to ' + (link.group(1) or '') + '#' + target
                                    + ', the errors of no entry there')
                    continue
                theirs = reasons_of(entry, reasons, groups)
                # The overlay runtime hands on the host's errors — a click's, a capture's, a
                # page's — and errors.luau lists only its own (its head comment says so), so a
                # runtime entry's row for a host entry's errors stands for them as they are. Every
                # other link must bring only reasons of this entry.
                passed_on = e['page'] == RUNTIME_PAGE and pages.get(entry) != RUNTIME_PAGE
                if passed_on:
                    handed.add(entry)
                extra = [] if passed_on else [r for r in theirs if id(r) not in {id(m) for m in mine}]
                for r in extra:
                    problems.append('the row for ' + target + ' brings `' + r['message'] + '`, which is no reason of this entry')
                covered |= {id(r) for r in theirs}
            else:
                problems.append('a code cell that links to ' + target + ', which is neither a group nor an entry\'s errors')
            continue
        m = re.fullmatch(r'`([a-z_]+)`', code)
        if not m:
            problems.append('a code cell that is neither a code nor a link: ' + code)
            continue
        if m.group(1) not in codes:
            problems.append('the row `' + m.group(1) + '` names no code of host.errors or the runtime\'s')
            continue
        # A message is a code span; one that holds a backtick itself — the runtime's "its `…`
        # function raised" — is written between double backticks, as Markdown allows.
        messages = [a or b for a, b in re.findall(r'``\s?(.+?)\s?``|`([^`]+)`', message)]
        if not messages:
            problems.append('the row `' + m.group(1) + '` has no message in backticks to compare')
        for msg in messages:
            hit = [r for r in mine if r['code'] == m.group(1) and r['message'] == msg]
            if not hit:
                problems.append('the row `' + m.group(1) + '` `' + msg + '` is no reason of this entry')
            covered |= {id(r) for r in hit}
    for r in mine:
        if id(r) not in covered:
            problems.append('the reason `' + r['code'] + '` `' + r['message'] + '` has no row')
    # What the runtime hands on is held to errors.luau's `passes`, both ways: a host entry's
    # errors the entry hands on with no row, or a row for a host entry it does not hand on.
    if e['page'] == RUNTIME_PAGE:
        want = set(passes.get(e['anchor'], ()))
        for a in sorted(want - handed):
            problems.append('hands on ' + a + '\'s errors (errors.luau, passes) and has no row for them')
        for a in sorted(handed - want):
            problems.append('a row for ' + a + '\'s errors, which errors.luau\'s passes does not say it hands on')
    return problems


def reasons_of(anchor, reasons, groups):
    """The reasons an entry gives: those naming it, and those naming a group it is in."""
    return [r for r in reasons
            if anchor in r['entries'] or any(g in groups and anchor in groups[g] for g in r['entries'])]


def reason_problems(entries):
    """Checks 1 to 3 of the typed-errors plan, for every page, and each group's own section; that
    each reason and group names entries that exist, and calls or fields rather than concepts,
    since a reason naming anything else is in no entry's table and would never be compared with
    one; and that errors.luau's `passes` names runtime entries and host entries."""
    order, reasons, groups, unread = host_table()
    GROUP_NAMES.update(groups)
    runtime, runtime_unread = runtime_reasons()
    _, passes = runtime_lists()
    reasons = reasons + runtime
    codes = set(order) | {r['code'] for r in runtime}
    pages = {e['anchor']: e['page'] for e in entries}
    kinds = {e['anchor']: kind(e) for e in entries}
    problems = []
    if unread:
        problems.append(ERRORS_RS + ': ' + str(unread) + ' Reason(s) not written the way this tool reads one')
    if runtime_unread:
        problems.append(ERRORS_LUAU + ': ' + str(runtime_unread) + ' reason(s) not written the way this tool reads one')
    for r in reasons:
        for a in r['entries']:
            if a not in pages and a not in groups:
                problems.append(r['source'] + ': ' + r['name'] + ' names ' + a + ', which is neither an entry nor a group')
            elif a in pages and kinds[a] == 'concept':
                problems.append(r['source'] + ': ' + r['name'] + ' names ' + a + ', a concept, which has no Errors to compare it with')
    for a, theirs in passes.items():
        if pages.get(a) != RUNTIME_PAGE or kinds[a] == 'concept':
            problems.append(ERRORS_LUAU + ': passes names ' + a + ', which is no call or field of the runtime\'s')
        for t in theirs:
            if t not in pages or pages[t] == RUNTIME_PAGE:
                problems.append(ERRORS_LUAU + ': passes says ' + a + ' hands on ' + t + ', which is no host entry')
    for g, members in groups.items():
        for a in members:
            if a not in pages:
                problems.append(ERRORS_RS + ': the group ' + g + ' names ' + a + ', which is no entry')
    for e in entries:
        for p in entry_problems(e, e['body'], reasons, groups, codes, pages, passes):
            problems.append(e['page'] + '.md#' + e['anchor'] + ': ' + p)
    return problems + group_problems(reasons, groups)


DUPLICATION_TAIL = re.compile(r'^- `(.+?)` — `([a-z_]+)`(?::.*)?$')


def group_problems(reasons, groups):
    """Each group of errors.rs against its own section, `{#<group>}` on a page of docs/api, in
    both directions — as an entry's Errors table is held to its reasons, since a row that links to
    a group stands for these. A section lists its reasons as a table (code, when, message,
    platform), or, where every message is one sentence and a tail, as `` - `tail` — `code` ``
    bullets under the sentence's start, written "the whole sentence `…`"."""
    problems = []
    texts = {p: open(p, encoding='utf-8').read() for p in sorted(glob.glob(os.path.join(API, '*.md')))}
    for g in groups:
        heading = re.compile(r'^#{2,6} .*\{#' + re.escape(g) + r'\}\s*$')
        where = [(p, t) for p, t in texts.items() if any(heading.match(l) for l in t.split('\n'))]
        if len(where) != 1:
            problems.append('the group ' + g + ' has ' + ('no section' if not where else 'more than one section')
                            + ' {#' + g + '} in docs/api')
            continue
        path, text = where[0]
        lines = text.split('\n')
        start = next(i for i, l in enumerate(lines) if heading.match(l))
        level = len(re.match(r'^(#+)', lines[start]).group(1))
        body, fenced = [], False
        for l in lines[start + 1:]:
            if re.match(r'^\s*(```|~~~)', l):
                fenced = not fenced
            if not fenced and re.match(r'^#{1,' + str(level) + r'} ', l):
                break
            body.append(l)
        body = '\n'.join(body)
        here = os.path.basename(path) + '#' + g
        theirs = [r for r in reasons if g in r['entries']]
        listed = []
        for row in table_rows(body, GROUP_HEADER):
            if len(row) != 4:
                problems.append(here + ': a row without four cells: ' + ' | '.join(row))
                continue
            code, _when, message, platform = row
            if not all(p.strip() in PLATFORMS for p in platform.split(',')):
                problems.append(here + ': platform `' + platform + '` is not both, Windows, macOS or other')
            m = re.fullmatch(r'`([a-z_]+)`', code)
            msgs = [a or b for a, b in re.findall(r'``\s?(.+?)\s?``|`([^`]+)`', message)]
            if not m or not msgs:
                problems.append(here + ': a row that is not a code and a message in backticks: ' + ' | '.join(row))
                continue
            listed += [(m.group(1), msg) for msg in msgs]
        tails = [(l, DUPLICATION_TAIL.match(l)) for l in body.split('\n') if l.startswith('- `')]
        if tails:
            start_of = re.search(r'the whole sentence `([^`]+)`', body)
            if not start_of:
                problems.append(here + ': tails with no "the whole sentence `…`" that they end')
            for l, t in tails:
                if t is None:
                    problems.append(here + ': a bullet that is not "- `tail` — `code`": ' + l)
                elif start_of:
                    listed.append((t.group(2), start_of.group(1) + t.group(1)))
        covered = set()
        for code, msg in listed:
            hit = [r for r in theirs if r['code'] == code and r['message'] == msg]
            if not hit:
                problems.append(here + ': `' + code + '` `' + msg + '` is no reason of the group (errors.rs)')
            covered |= {id(r) for r in hit}
        for r in theirs:
            if id(r) not in covered:
                problems.append(here + ': the reason `' + r['code'] + '` `' + r['message'] + '` is not listed')
    return problems


def codes_problems():
    """errors.md explains every host code, and nothing else, in errors.rs's order."""
    order, _, groups, _ = host_table()
    GROUP_NAMES.update(groups)
    text = open(ERRORS_PAGE, encoding='utf-8').read()
    start = text.index('{#host-errors}')
    rows = []
    for line in text[start:].split('\n')[1:]:
        if line.startswith('## '):
            break
        m = re.match(r'^\| `([a-z_]+)` \| `([A-Z_]+)` \|(.*)\|\s*$', line)
        if m:
            rows.append(m.groups())
    problems = []
    if [c for c, _, _ in rows] != order:
        problems.append('errors.md\'s table of codes is not errors.rs\'s Code::ALL, in order: '
                        + ', '.join(c for c, _, _ in rows) + ' against ' + ', '.join(order))
    for c, k, meaning in rows:
        if k != c.upper():
            problems.append('errors.md: the constant of `' + c + '` is `' + k + '`')
        if not meaning.strip():
            problems.append('errors.md: the code `' + c + '` has no meaning')
    # And the runtime's: overlay.md's O.errors explains errors.luau's codes, and nothing else, in
    # its order.
    runtime, _ = runtime_lists()
    if runtime:
        text = open(os.path.join(API, RUNTIME_PAGE + '.md'), encoding='utf-8').read()
        body = text[text.index('{#o-errors}'):]
        rows = []
        for line in body.split('\n')[1:]:
            if line.startswith('## ') or line.startswith('### '):
                break
            m = re.match(r'^\| `([a-z_]+)` \| `([A-Z_]+)` \|(.*)\|\s*$', line)
            if m:
                rows.append(m.groups())
        if [c for c, _, _ in rows] != runtime:
            problems.append(RUNTIME_PAGE + '.md: the table of O.errors is not errors.luau\'s codes, in order: '
                            + ', '.join(c for c, _, _ in rows) + ' against ' + ', '.join(runtime))
        for c, k, meaning in rows:
            if k != c.upper():
                problems.append(RUNTIME_PAGE + '.md: the constant of `' + c + '` is `' + k + '`')
            if not meaning.strip():
                problems.append(RUNTIME_PAGE + '.md: the code `' + c + '` has no meaning')
    return problems


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
    # And within a page, with the level-3 anchors stamped from the entries': `foo-errors` is the
    # Errors heading of `foo` and must not be an entry of its own.
    for path in sorted({e['path'] for e in entries}):
        stamped, _ = stamp_level3(open(path, encoding='utf-8').read())
        ids = [e['anchor'] for e in entries if e['path'] == path]
        ids += [explicit(h) for _, level, h in headings(stamped) if level >= 3 and explicit(h)]
        twice = sorted({a for a in ids if ids.count(a) > 1})
        if twice:
            print('anchor collisions on ' + path.replace(os.sep, '/') + ': #' + ', #'.join(twice))
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

    errors = codes_problems() + reason_problems(entries)
    if errors:
        print('Returns and Errors that do not match crates/host/src/errors.rs:')
        for p in errors:
            print('  ' + p)

    if check:
        if gaps or undeclared or errors:
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
