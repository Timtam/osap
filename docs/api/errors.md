---
title: "host.errors — error values and their codes"
sidebar_position: 0.5
toc_max_heading_level: 2
---

When a call cannot do what it was asked, it answers an **error value**: a read-only table with a `code` to compare, a `message` for the log and an optional `detail`. A mistake in the call itself — a wrong type, an unknown key — raises instead, as it always has. This page is the rule every call follows, and `host.errors` holds the codes. Each call's own entry lists the error values it gives, under **Errors** ([Reading an Errors table](#reading-an-errors-table)).

```luau
-- In a hotkey's callback, a handler, where the read may wait for the screen.
host.hotkey.register("Ctrl+Alt+Shift+F9", function()
  local win = host.window.active()
  if not win then return end                            -- no window in front
  local c, err = host.screen.pixel({ window = win, fraction = { 0.5, 0.5 } })
  if err then
    host.log.info("not read: " .. err)                  -- the message, word for word
    if err.code == host.errors.HIDDEN then return end   -- its client area is empty: read it later
    return
  end
  host.log.info("the middle of the window is " .. c.hex)
end)
```

**Which calls answer one.** [`host.gui`](./gui.md): a page's `show`, `prepare`, `focus` and `post`, `onClose`'s `err`, and the page's own `platform.post`. [`host.window.focus`](./window.md#host-window-focus). Every question of [`host.element`](./element.md), which is every call there but `pending` and `type`. Every call of [`host.input`](./input.md) that acts, which is every call there but `cursorPos` and `held`. [`host.keys.capture`](./keys.md#host-keys-capture) and [`host.hotkey.register`](./hotkey.md#host-hotkey-register). Every call of [`host.screen`](./screen.md) but `size`, `predicate` and `pending`, and a snapshot's `crop`. [`host.ocr`](./ocr.md): a reading's `error`, and `resolveLanguage`. [`host.speech.use`](./speech.md#host-speech-use), [`host.sound.play`](./sound.md#host-sound-play), and [`host.gamepad`](./gamepad.md)'s `list`, `state` and `on`. The [overlay runtime](./overlay.md) answers them too, since its version 0.11, with eight codes of its own ([below](#the-overlay-runtime-s-codes)). No other call answers one: it raises for a mistake, and its entry says what its `nil` or `false` means.

**Every call on this page can also raise** the guard's `stopped by the host: …`, in a module that ran too long ([the limits](../module-runtime-and-lifecycle.md#limits)). Nothing here needs a capability, so the capability gate never raises here.

:::warning A value and an error together
**An error value beside a first value that is not `false`, `nil` or `{}` means that part of what was asked was done.** The first value is that part. The error says what was not done. Four calls answer that way — `page:show`, the two dumps and `host.hotkey.register`:

- `page:show` answers `true, err`: the page is in front, but over the rectangle it was to avoid.
- The two dumps of the accessibility tree, `host.element.dump` and `rawDump`, answer `list, err`: the elements read before it stopped.
- `host.hotkey.register` answers `id, err`: the claim is made and kept, but not held right now.

So test the error to know whether *everything* was done, and the first value to know what there is. `if not ok then` alone takes a partial answer for a whole one. See [Partial answers](#partial-answers). Each of the four says so in its own Returns: [`page:show`](./gui.md#page-show-returns), the dumps [`host.element.dump`](./element.md#host-element-dump-returns) and [`rawDump`](./element.md#host-element-rawdump-returns), and [`host.hotkey.register`](./hotkey.md#host-hotkey-register-returns).
:::

## What to declare {#declare}

Nothing — `host.errors` is there for every module, as `host.os` is. See [what the capability list is and is not](./index.md#capabilities).

## Error values {#error-values}

An error value is a table with three fields:

- `code` — a lower-case snake_case string: one of [the host's codes](#host-errors), or one of [the overlay runtime's](#the-overlay-runtime-s-codes), or a library's own. This is what a module compares.
- `message` — the sentence for the log: what went wrong, in words, with names and numbers in it. It may change between versions; the code does not.
- `detail` — an optional string for the log, with more than the message says: why a page's build failed, which of a page's processes ended, what an overlay's hook raised. Absent is `nil`. Never compared.

**It reads as its message.** `tostring(err)` is the message, and `..` gives a string with the error on either side: `"not read: " .. err`, `err .. " (retrying)"`, `err .. err`. A number on the other side is fine. Anything else raises: `err .. {}` raises `attempt to concatenate an error value with a table value`. `host.log.info(err)` logs the message and `host.speech.output(err)` speaks it. Neither says the detail, which a module logs itself where it wants it (`host.log.info(err .. " — " .. (err.detail or ""))`).

**It is read-only.** Assigning a field raises (`attempt to modify a readonly table`), and so does `setmetatable` on it. **A field it does not have raises too**, rather than reading as `nil`: `err.cod` raises `an error value has code, message and detail, not 'cod'`. So does a method call such as `err:find("x")`: the error value is not a string, and a typo should say where it is. `detail` is the exception: it reads `nil` when there is none.

**What else it is.** `type(err)` is `"table"`. `pairs(err)` gives its two or three fields. `table.clone(err)` gives a writable copy with the same metatable. `host.json.encode(err)` gives `{"code":…,"message":…,"detail":…}`, so `page:post({ ok = false, error = err })` hands a page the shape it compares with `platform.errors`. What `host.json.decode` reads back is a plain table, compared the same way. An error value a callback is handed behaves as one a call returns. A module makes its own with [`host.errors.new`](#host-errors-new).

**Luau's string functions take strings only.** Every `string.*` and `utf8.*` function raises for an error value: `string.len(err)` raises `invalid argument #1 to 'len' (string expected, got table)`. So does `table.concat`, which takes strings and numbers: `table.concat({ "a", err })` raises `invalid value (table) at index 2 in table for 'concat'`. Hand them `tostring(err)`. Two are easy to miss:

- `string.format` reads `%s` as a string and raises `invalid argument #2 to 'format' (string expected, got table)`. Use `%*` or `tostring(err)`.
- Luau's `assert` takes a string message only, so `assert(ok, err)` raises `invalid argument #2 to 'assert' (string expected, got table)` about its own argument instead of the error. Write `assert(ok, tostring(err))`, or `if not ok then error(err) end`.

`error(err)` raises the value itself, and its message is what the log and the error dialog show. `#err` is `0`, a table's length, not the message's.

```luau
local ok, err = page:show({ focus = true })
if not ok then
  host.speech.output(err)                                    -- spoken as its message
  host.log.info(string.format("[my-module] page: %*", err))  -- %*, not %s
  return
end
```

## Partial answers {#partial-answers}

A call answers no error value when it did everything it was asked, and an error value when it did not. Its first value then says what there is:

- `false`, `nil` or `{}` — the main result is missing.
- Anything else — **the part that was done**. This is unconventional, and it is the one case a module has to handle on purpose.

The calls in this table are the only ones that answer this way. No other call of the host does. The overlay runtime makes none of its own, but hands one on: an overlay page's `onShown` is handed `page:show`'s `true, err` as it came ([`O:addPage`](./overlay.md#o-addpage)).

| call | answers | the first value | the error value says |
|---|---|---|---|
| [`page:show`](./gui.md#page-show-returns), with `focus` | `true, err` | `true`: the page is shown and in front | it is over part of `avoid`, there being no room beside it: `too_large`, `over the rectangle to avoid` |
| [`host.element.dump`](./element.md#host-element-dump-returns) | `list, err` | the elements listed before it stopped | the tree holds more than the 2000 elements a dump lists (`too_large`), a Mac's walk took longer than 5 seconds (`timeout`), or the program stopped answering on the way (`timeout`, `denied` or `gone`) |
| [`host.element.rawDump`](./element.md#host-element-rawdump-returns) | `list, err` | the elements read before the walk stopped | the walk met its bound — 4000 elements on Windows, 2000 on a Mac (`too_large`), or 5 seconds (`timeout`) — or the program stopped answering on the way (`timeout`, `denied` or `gone`) |
| [`host.hotkey.register`](./hotkey.md#host-hotkey-register-returns) | `id, err` | the hotkey's id: the claim is made, and it is tried again whenever it can be held | why it is not held now: another module or another program holds it, or the system refused it (`taken`); a macOS letter the current keyboard layout does not type (`not_supported`); the Mac's hotkey handler could not be installed (`failed`) |

The same calls can also answer with the first value missing. A dump that read nothing answers `{}, err`. A combination the system keeps, one with no key code here, or one of the application's own keys, is not claimed at all: `host.hotkey.register` answers `nil, err`. A `show` not done as asked answers `false, err`, and its Errors table says where the page is then.

So read the two values for what each says, rather than reading the first as "it worked":

```luau
-- The claim stands even when it is not held now: keep the id, and say why.
local id, err = host.hotkey.register("Ctrl+Shift+F9", function() host.speech.output("F9") end)
if id and err then
  host.log.info("[my-module] Ctrl+Shift+F9 is claimed, but not held now: " .. err)
elseif err then
  host.log.info("[my-module] Ctrl+Shift+F9 is not claimed: " .. err)
end
local hotkeyId = id                  -- nil only when it was not claimed

-- What a dump read is worth having, all of it or not.
local win = host.window.active()
if win then
  local list, err = host.element.dump(win.id)
  for _, e in ipairs(list) do host.log.info(e.name) end
  if err then host.log.info("[my-module] not the whole tree: " .. err) end
end
```

`if not ok then … end` alone treats a partial answer as complete, and `if err then return end` throws away the part that was done. A module that answers this way itself — a library that returns what it found so far — does the same with [`host.errors.new`](#host-errors-new), and says so at its own function.

## Return or raise {#return-or-raise}

**A call raises when the arguments alone decide**, so the same call fails whatever the state: a wrong type, an unknown key, a value outside its fixed range, corners past a fixed size limit, a key spec that does not parse. That is a mistake to fix, and it keeps its sentence: the log line and the error dialog name the call and what was wrong with it.

**A call returns an error value when the moment decides**: the window, the screen, what is held, other modules, the system, the user. That is a normal outcome a module plans for.

**The same limit can be either.** Corners past a call's fixed size limit raise — a snapshot of more than 40 million pixels, say — since the corners alone decide. A window region past the same limit is answered `too_large`, since the window's size at that moment decides. The snapshot and template budgets are answered for every region, since what is already held decides, and other modules' snapshots count toward the application's budget. The overlay runtime keeps the same rule: `O:chooseMenuItem` raises on an overlay with no menu tests, since the overlay's declaration decides, not the moment.

**Files.** A file inside the module's own package that is missing or broken raises when a call names it: `host.resource.read`, `host.require`, `host.include`, and a template or sound path inside the module's folder. The package is the module's own, so a missing file there is a mistake in it. A template or a sound outside the package, and a template file that changed after it was first read, is returned: `not_found`, `invalid` or `failed`. `host.resource.read` raises for a file outside the package too, and `host.include` refuses a path outside it.

**Two raises any call can meet**, said once at the top of each page rather than at every entry: the capability gate, for a namespace the module did not declare ([What to declare](./index.md#capabilities)), and the guard's `stopped by the host: …`, when the module has run too long ([the limits](../module-runtime-and-lifecycle.md#limits)). **A call that waits** names a third at its own entry, under **Raises where it cannot wait**: called without a callback where it cannot wait, it raises rather than answering ([Where it waits](./screen.md#where-it-waits)).

## Where an error value arrives {#where-an-error-value-arrives}

Always in one of five places, which each Errors table names with these words:

- `return` — the second value of the call: `local v, err = call()`.
- `callback` — the second argument of the call's callback: `cb(value, err, info)`.
- `both` — the second value without a callback, the callback's second argument with one.
- `field` — a field of a result: a text reading's `error`, beside its `status = "failed"`.
- `hook <name>` — an argument of a hook the module wrote into a spec: a page's `onClose(how, err)`; an overlay's `onDone`, `onGiveUp`, `onBack` or `onShown`, or a hand-over's `onEnd`.

**A call that hands its work on answers when it has handed it on.** What fails after that reaches only its callback or its hook. A click with `via = "noactivate"` answers `true` once the click is handed to the host's window-style thread, and what fails there reaches the callback only, or the log alone without one ([`host.input.click`](./input.md#host-input-click)). `O:chooseMenuItem` answers `true` once the menu's opener is clicked, and how the choice ended comes to its `onDone` ([`O:chooseMenuItem`](./overlay.md#o-choosemenuitem)).

**A status word is not an error value.** Where a call answers one of a closed set of outcomes that are not failures, it is a plain lower-case word, compared as a word: a reading's `status`, `"text"`, `"blank"`, `"none"`, `"failed"` or `"stale"`; onClose's `"user"` or `"failed"`. Where the word says that something failed, an error value comes beside it. A reading's `"stale"` is the reading's form of `replaced`, and has none.

## Reading an Errors table {#reading-an-errors-table}

Every call's entry has two sections after its prose and examples, before its Windows and macOS blocks:

- **Returns** — every form the answer takes: the value, a partial answer where there is one, the missing value with an error (`nil, err`, `false, err`, `{}, err`), and what a callback is handed.
- **Errors** — every error value it can give, in a table:

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | The window's client area is empty: a minimised window, or one not laid out yet. | `the window's client area is empty (…x…)` | both |

- **code** — the code. Or a link that stands for several rows at once: a group of reasons several calls share ("any of the capture errors", on [the screen page](./screen.md#failure-reasons)), or another call's errors, which a call that hands them on links to.
- **arrives** — one of the five words above.
- **when** — what made it, and the state afterwards.
- **message** — what the log shows; `…` stands for a part filled in, such as a size or a name. A message may change; the code does not.
- **platform** — `both` (Windows and macOS), `Windows`, `macOS`, or `other` (the build for other systems, which does little). A row may name two: `both, other`.

Under the table, **Raises** lists the mistakes in the call that raise instead, and a call that waits adds **Raises where it cannot wait**. A call that never fails says "None. It never answers an error value." Every table is checked against the host's own list of reasons, and the overlay runtime's, in both directions: a row the host never gives fails the documentation check, and so does a reason with no row.

## Comparing codes {#comparing-codes}

**Compare `err.code` with a constant**: `err.code == host.errors.NOT_READY`. Never the message, which may change; never the error value with a string — `err == "not ready yet"` is always false; never `detail`.

```luau
-- `plugin` is the window the page is to stay beside, as host.window.active() answered it.
local function showBeside(plugin)
  local ok, err = page:show({ focus = true, avoid = plugin.bounds })
  if err and err.code == host.errors.NOT_READY then
    return  -- onReady comes when the page has loaded
  elseif not ok then
    host.speech.output("Page not shown: " .. err)
  elseif err and err.code == host.errors.TOO_LARGE then
    -- `true, err`: a partial answer. The page is in front, but over part of the plug-in.
    host.log.info("page shown, but " .. err)
  end
end

-- Several codes, one answer each: a table keyed by the code.
local SAY = {
  [host.errors.DENIED] = "this application lacks a right; see the log",
  [host.errors.UNAVAILABLE] = "something it needs is not installed or switched off",
}
local function said(err) return SAY[err.code] or tostring(err) end
```

**Code written against sentences stops matching, silently.** Before the conversion a call answered a sentence, such as `false, "not ready yet"`, and a module compared `why == "not ready yet"`. Now the second value is a table, and that comparison is false without a word. Code that only tests it (`if why then`), logs it or puts it in a sentence (`"x: " .. why`) keeps working. `string.find(why, …)`, `why:find(…)`, `table.concat` over it and the rest of the string functions raise ([Error values](#error-values)). A module that depends on the overlay runtime is not protected by its version, since a dependency is a lower bound only.

**A page compares the same codes**, under `platform.errors`: the module posts `{ ok = false, error = err }`, and the page tests `m.error.code === platform.errors.NOT_READY` ([`platform.errors`](./gui.md#platform-errors)). `platform.errors` holds the host's 19 codes, not the overlay runtime's.

## host.errors {#host-errors}

**Signature:** `host.errors: { [string]: string, new: function }`

The codes, as constants: `host.errors.NOT_READY` is `"not_ready"`. A read-only table holding the 19 codes below and [`new`](#host-errors-new), the same in every module. Reading a name it does not have raises and lists the codes — `host.errors has no 'NOT_REDY'; the codes are NOT_READY, NOT_SUPPORTED, …` — so a misspelt constant fails where it is written, not as a comparison that is never true.

Three codes tell a module what the user can do: `not_supported` (nothing, here), `unavailable` (install it or switch it on) and `denied` (grant this application a right). The others describe the moment or the target. A code never names a platform; the message may.

| code | constant | meaning |
|---|---|---|
| `not_ready` | `NOT_READY` | It is being prepared; ask again later. |
| `not_supported` | `NOT_SUPPORTED` | This platform, system version, keyboard layout, build or run mode cannot do it, for any target. |
| `unavailable` | `UNAVAILABLE` | Something it needs is missing or switched off now — not installed, not running, switched off, or in use by other programs — and the user can change that. |
| `denied` | `DENIED` | This application lacks a right the system grants: higher rights, Accessibility, the secure desktop. |
| `refused` | `REFUSED` | This target will not do it: it was asked and said no, or it offers no way to — an element with no press, this application's own window. |
| `busy` | `BUSY` | Something of the same kind is under way, so this one was not started. |
| `too_many` | `TOO_MANY` | Too many requests of this kind are waiting. |
| `too_large` | `TOO_LARGE` | The request is larger than a limit, or than the room at this moment allows. |
| `replaced` | `REPLACED` | A newer request with the same key replaced this one. |
| `timeout` | `TIMEOUT` | No answer came within the time allowed. |
| `gone` | `GONE` | What it was asked about no longer exists. |
| `hidden` | `HIDDEN` | The window is not shown now: minimised, hidden, off screen, or its client area is empty. |
| `outside` | `OUTSIDE` | The point or region is not where it has to be: outside the window, on its frame, outside a snapshot, or with no area. |
| `covered` | `COVERED` | Something else is drawn over the point. |
| `not_found` | `NOT_FOUND` | There is no such thing to act on: no such element, engine or file, or nothing held. |
| `taken` | `TAKEN` | Something outside this module holds it: the system, another program or another module. |
| `invalid` | `INVALID` | Data read at run time is not in the form needed: a PNG that does not decode, a key spec that does not parse. |
| `cancelled` | `CANCELLED` | It was ended before it was answered: the application is closing, the request was cancelled, or the user ended it. |
| `failed` | `FAILED` | The system or the host's own code failed. The message says how, and the log has more. |

```luau
local ok, err = host.speech.use("sapi")
if not ok and err.code == host.errors.NOT_READY then
  host.timer.after(500, function() host.speech.use("sapi") end)
end
```

### Returns {#host-errors-returns}

The table itself. In a module and its code dependencies it is the same table every time; another module has its own, with the same constants. Each constant is a string, the code in lower case. `pairs(host.errors)` gives the 19 of them and `new`.

### Errors {#host-errors-errors}

None. It never answers an error value.

**Raises** for a mistake in the code that reads it: a name it does not have (`host.errors has no 'NOT_REDY'; the codes are NOT_READY, NOT_SUPPORTED, …`, every code named); and any assignment to it, or to its metatable (`attempt to modify a readonly table`).

### Windows {#host-errors-windows}

The same codes. A message may name the part of Windows that failed — WebView2, desktop duplication, UI Automation, a window message — and carry its error number, `(error 5)` or `0x80070005`.

### macOS {#host-errors-macos}

The same codes. A message may name the part of macOS that failed or the right that is missing — Accessibility, Screen Recording, Vision, a CoreGraphics call.

## host.errors.new(code, message, detail?) {#host-errors-new}

**Signature:** `host.errors.new(code: string, message: string, detail: string?)` → `Error`

Makes an error value of the module's own: for a library or a runtime whose functions answer as the host's do, so the module that calls them compares `err.code` and logs `err` the same way. The value behaves in every way as one of the host's: the same read-only table, the same message on `tostring` and `..`, the same raise for a field it does not have.

`code` may be any lower-case snake_case word, not only one of `host.errors`'s: the overlay runtime has codes of its own ([below](#the-overlay-runtime-s-codes)). Use a host code where one says it, so a caller that knows the host's codes understands yours. The message is for the log, a sentence; the detail, if any, is more for the log.

```luau
-- A library of the module's own, answering as the host does.
local M = {}
function M.preset(index, name)
  local p = index.byName[name]
  if p == nil then
    return nil, host.errors.new(host.errors.NOT_FOUND, "the index has no preset '" .. name .. "'")
  end
  return p
end

local p, err = M.preset(index, "Warm Pad")
if err and err.code == host.errors.NOT_FOUND then host.speech.output(err) end
```

Costs one table, made on the event loop at the call, like any table.

### Returns {#host-errors-new-returns}

One value: a new error value, `{ code, message, detail? }` as given, read-only. It has no `detail` field when `detail` is `nil`. Never `nil`.

### Errors {#host-errors-new-errors}

None. It never answers an error value.

**Raises** for a mistake in the call, with a message that begins `host.errors.new: `:

- a `code` that is not a string: `the code must be a string, not a nil`;
- a `code` that is not lower-case snake_case, a letter and then letters, digits and `_`: `the code 'Not_Ready' is not lower-case snake_case: a letter, then letters, digits and _`;
- a `message` that is not a string: `the message must be a string, not a number`;
- an empty `message`: `the message is empty`;
- a `detail` that is neither `nil` nor a string: `the detail must be a string or nil, not a table`.

It raises too for a `code`, `message` or `detail` that is not valid UTF-8, in the words of the conversion that failed. A fourth argument is ignored.

### Windows {#host-errors-new-windows}

The same on every platform.

### macOS {#host-errors-new-macos}

The same on every platform.

## The overlay runtime's codes {#the-overlay-runtime-s-codes}

The [overlay runtime](./overlay.md) answers its own failures with error values too, since its version 0.11. It makes them with `host.errors.new`, from its own reasons in `modules/overlay-runtime/src/errors.luau`, and adds eight codes of its own: `not_in_front`, `moved_on`, `unplaced`, `closed`, `no_menu`, `no_submenu`, `unfit_submenu` and `clash`. What each means is in the table of [`O.errors`](./overlay.md#o-errors). `O.errors` holds them, and also answers every host code, so `O.errors.BUSY` is `host.errors.BUSY`; it raises for a name that is neither. An error of the host's that the runtime hands on — a page's, a capture's, a focus's, a hotkey registration's — is the host's own table, `rawequal` to what the host answered. The runtime's page says what each of its calls answers.
