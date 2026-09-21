# In-memory templates for image search — design

*Decision document, 2026-09-21. Basis: a design proposal, an adversarial critique of it that
checked every claim against the code, and the build of steps 1, 2 and 4–7 below. Belongs to
[screen-frame-sharing-design.md](screen-frame-sharing-design.md), whose rules on screen
touches it follows. For what exists, read the [`host.screen` reference](api/screen.md): it is
written from the code, and this document records why it is shaped the way it is.*

Until this change a template could only be a PNG file inside the module. Two kinds of module
need more. A converted game-menu reader carries hundreds of small signatures as data, written
by a converter, not as files. A module that learns what a control looks like at run time
needs to keep a piece of the screen as a template without saving it to disk first. And a
screen that shows one of several states needs every state checked against one frame, which
`imageSearchAsync` with a list cannot do: it stops at the first hit.

## What was built

- **`host.screen.template(spec)`** returns a `Template` handle from exactly one source:
  `rgba` or `rgb` bytes with `w` and `h` (a string or a Luau `buffer`), `capture = region`, or
  `file = path`. An optional `name` travels into every hit. It returns `nil` only for a failed
  capture. Every other refusal is a mistake in the code that wrote the spec, and it raises.
- **Every search takes a handle** wherever it took a path: `imageSearch`, `imageSearchAll`,
  `imageSearchMulti` and `imageSearchAsync`, in lists as well.
- **`host.screen.imageSearchEach(entries, opts, cb)`**: one capture, and an answer for every
  entry: a hit or `false` per slot, or `nil` when the region could not be captured. An entry
  may have its own `within` region and `name`.
- **One hit shape** everywhere: `{ x, y, w, h, n, name }`.
- **`host.json.decode`**, bundled here because it was the missing half of shipping signatures
  as data. It is not gated: like `os` and `now`, it reads nothing but its argument.

## Rules, and why

**Handles are local to the VM, and pixels cross from Luau once.** The worker only ever sees an
`Arc<Decoded>`, so a handle collected while a search is in flight costs nothing. Handles
cannot be exported through data exports or settings.

**Each VM has a budget of 32 MiB for templates.** Luau's collector sees a handle as a few
dozen bytes, not the pixels behind it, so it has no reason to hurry. The budget belongs to the
Lua state and is created on first use there. It is not tied to one `install_host_api` call,
because a code dependency's host is installed into the same state once per dependency. When a
reservation does not fit, the budget collects garbage twice, then refuses with a message that
says to build templates once. It is a constant, not a setting.

**Handles have size limits; files do not.** A handle's side may be at most 4096 pixels, and
its area at most 1,048,576 pixels. The repository already ships a 1209x987 PNG, and a PNG has
always loaded at any size. The limits are checked before the budget and before a capture, so
an oversized spec is told it is too large, and no compositor frame is spent to be told no.

**Specs are strict.** An unknown field is refused. A misspelt `rbga` or a `tolerance` this
version does not take would otherwise be a template that does something other than what its
author reads in it. It also means the step 3 fields can be added later without changing what
any existing spec does.

**A `capture` template is a live read, always.** It is counted in the observation log like
`pixel`, and its alpha is forced to 255: a template made only of wildcards matches
everywhere. It is never served from a frame cache. A template learned from a stale frame is a
template of something that is no longer there. It is read through the module's capture
source, like `host.screen.save`; duplication answers with the most recently composed frame,
which is still a read made now. That does not by itself cut it from the picture the module's
searches see: in a module that declares `[screen] capture = "duplication"`, a `capture` made
before duplication has opened — at load, where nothing opens it, or while the prewarm of a
window trigger is still opening it — is read the standard way, or is `nil` under
`fallback = "none"`, and the template keeps that picture for the session. The API page says
so; whether to make such a capture wait for, or refuse, the opening is open (TODO.md).

**An async answer belongs to the VM that asked.** Bindings are scoped to the identity of the
module whose code runs, which for a code dependency is the dependency. The VM, its callbacks
and its lifetime belong to the module that loaded it. So each VM is tagged with
`VmOwner { idx, gen }` before any of its code runs, and when an answer arrives it meets one of
three fates:

- **Delivered**: the owner is enabled and is still the VM that asked.
- **Held**: the owner is disabled. The search is sent again when the owner is enabled, and
  the callback gets that fresh answer.
- **Dropped**: the VM is gone, reloaded or rolled back.

The first version delivered by the scoped index. Disabling the overlay runtime then dropped
every library's landmark callback. The landmark gate clears its in-flight flag only in that
callback, so it never searched again. The second version delivered by the owner but dropped
answers for a disabled owner, which moved the same wedge to disabling a library. Holding the
search keeps the promise `host.timer.every` makes: a poll survives a disable.

**The worker survives its own panics.** A panic used to end the thread for the session, and
every later callback silently never fired. Now each entry is caught on its own and answered
"no match". A panic in a batch as a whole, such as in the capture, answers every search in it
with `nil` ("could not look"). Both are caught through `logging::contain`. The application's
panic hook writes nothing for these, and the worker logs the 1st, 2nd, 4th, 8th and so on,
with place and message. Otherwise a template that panicked on every 500 ms poll would write a
line per poll.

**Scaled variants are cached, within bounds.** Scaled variants are cached per template, keyed
by the scaled size (1.24 and 1.26 of a 40-pixel template are the same 50 pixels). The cache
holds at most 16 entries and 16 MiB, and the oldest entry goes first. The size is checked
against the searched area before anything is resized, so `scales = {40}` never allocates to
find out it cannot fit. Scaled needles get probe pixels again. Choosing them is linear in the
pixel count, because a variant too large for the cache is rebuilt on every call.

**Every template is resolved before the capture.** Before this change, `imageSearchMulti`
opened each file after the capture. A bad path late in the list stayed dormant until every
earlier template happened to miss.

**The call-level `tolerance` is unchanged.** A value outside 0–255 still silently becomes 0,
an exact match. Changing that changes what existing modules get, so it is a separate step.
Every search reads the value through one helper, so that step is one edit.

## Where the code is

- `crates/host/src/template.rs`: the matcher. It is pure code, with no Lua and no platform
  switch. Its test module keeps a verbatim copy of the old matcher, and an equivalence test
  plants templates, masks them and scales them in 600 seeded cases. The copy is from
  09c5640.
- `crates/host/src/image_search.rs`: everything that knows about Lua, modules or threads.
  That covers handles, the budget, owners and fates, the worker, delivery, and the six search
  bindings.
- `crates/host/src/json.rs`: `host.json.decode`.
- `lib.rs` keeps only one-line registrations. It also keeps three calls: `register_vm` in
  `populate_vm`, `purge_pending_images` in `purge_module`, and `resume_held_images` in
  `apply_enabled`.
- `backend::CaptureFn` names the capture pointer the worker is handed. The desktop duplication
  feature changed that one alias to take several regions and a `CaptureSource`: every search
  captures through its VM's source (`capture_source::read_source`), an `ImageTask` carries
  it, and the worker's frame key is the region and the source together.

## Steps

| Step | What | State |
|---|---|---|
| 1 | Move the matcher unchanged; add characterisation tests | built |
| 2 | Variant cache bounded by the haystack; `catch_unwind` on the worker | built; the before/after `match_ms` measurement is open |
| 3 | Point templates, `lo`/`hi` ranges, `outside` checks, template slack, `maxMiss`, ±2 px refinement | waits for the developer's answer (below) |
| 4 | Bindings, handle, budget, `VmOwner` | built |
| 5 | `capture` and `file` sources; alpha forced in the Windows capture | built |
| 6 | `imageSearchEach` and the hit shape | built |
| 7 | Reference docs, index, checks | built |
| 8 | Live self-test on the manager window, needing nobody to set up a screen | open; a headless run stood in for it |
| 9 | Commit after the maintainer confirms | open |

## Open

**What is one cell of a signature?** Step 3 depends on the answer: a pixel at a fixed place,
or the mean of a block.

- If a cell is a pixel, step 3 is built as designed. Point templates take only integer scales
  2 to 8. The slack comes from the point, then the template, then the call, then 0; once a
  level states a value, the call's tolerance is not added. `outside` checks never take the
  call's slack.
- If a cell is a mean, the first version is `host.screen.cells{ region, cols, rows }`, which
  reduces a region to block means, compared in Luau. The colour test gets a `kind` byte so a
  channel-difference predicate can follow without an API change.

`template::Body` has one variant, so a sparse body would be an addition.

**Follow-ups:**
- `host.resource.readBytes`, so packed binary templates can be shipped as files.
- The call-level `tolerance` clamp.
- A capture-probe measurement of whether macOS downsampling from backing pixels to points is
  a box average. Until then, the docs make no claim about it.

A time-based in-flight stamp in the overlay runtime is no longer needed for disable and
re-enable, but it would still guard against a callback lost some other way.
