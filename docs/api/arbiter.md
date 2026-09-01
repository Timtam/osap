---
title: "host.arbiter — one control, one owner"
sidebar_position: 17
toc_max_heading_level: 2
---

One control, one owner. Several overlays can match the same window at the same moment — Komplete Kontrol's chrome, the Kontakt inside it, the library loaded in that, and a modal over all three — and only one of them may hold the keyboard. A **slot** is the contested thing, named by a plain string; a **claim** is one overlay's standing bid for it, ranked by specificity; and the arbiter elects the most specific claim that currently matches, activating it and deactivating whoever held it before.

Almost every module gets this without touching the API: `O:attach` and `O:attachEmbedded` take `slot` and `specificity`, and the overlay runtime does the registering, the reporting and the tearing down. You come here to build something that competes for a slot without being an overlay, or to inspect one.

**The failure this design is most prone to is silent.** Slots are strings typed independently in modules that never see each other, so a typo does not error — it quietly creates a private slot where that overlay wins every time, or never competes at all. A slot with one participant is perfectly legal, so nothing else can notice. `participants` exists for exactly that, and every failure here is invisible to somebody who cannot see the screen.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["arbiter"]
```

See [what that list is and is not](./index.md#capabilities).

## host.arbiter.register(slot, specificity, onActivate, onDeactivate) {#host-arbiter-register}

**Signature:** `host.arbiter.register(slot: string, specificity: number, onActivate: () -> (), onDeactivate: () -> ()) -> handle`

Enters a claim for `slot` and returns the handle every other call here takes. The claim starts **not matching**: registering says only that this module wants the slot when its own conditions hold, and `setMatching` is what says they do.

`specificity` is the rank. Higher wins, and `O.layer` names the four the overlay runtime uses — `chrome` 10, `base` 20, `content` 30, `dialog` 40 — so a plug-in's generic header yields to whatever is loaded in it.

The two callbacks are the whole point: the arbiter drives them on every change of winner, so a module never has to work out that it has just lost.

```luau
-- What O:attach does underneath when it is given a slot.
self._claim = host.arbiter.register(
  self._slot,
  self._specificity,
  function() self:_activate() end,
  function() self:_deactivate() end
)
```

## host.arbiter.setMatching(slot, handle, matching) {#host-arbiter-setmatching}

**Signature:** `host.arbiter.setMatching(slot: string, handle, matching: boolean) -> nil`

Reports whether this claim's own conditions hold right now, and re-elects the slot. A change of winner runs the losing claim's `onDeactivate` and then the winner's `onActivate`, in that order, so the two never overlap.

Call it on every re-check, not only on a change — the arbiter keeps the last thing each claim reported and compares for itself.

```luau
-- Reported on every window/focus event and on the poll, if there is one.
host.arbiter.setMatching(self._slot, self._claim, matched)
```

## host.arbiter.unregister(slot, handle) {#host-arbiter-unregister}

**Signature:** `host.arbiter.unregister(slot: string, handle) -> nil`

Withdraws the claim and promotes whoever is next. The withdrawing claim's `onDeactivate` runs first if it was the one active, so a module that is being disabled or unloaded tears its own state down before the callbacks are freed.

```luau
-- A module being disabled gives its slot back rather than staying registered and inert.
if self._slot and self._claim then
  host.arbiter.unregister(self._slot, self._claim)
  self._claim = false
end
```

## host.arbiter.winner(slot) {#host-arbiter-winner}

**Signature:** `host.arbiter.winner(slot: string) -> string?`

The module id currently holding `slot`, or `nil` when nothing matches. Ask it to find out whether somebody else is in front of you rather than to decide whether you are active — your own `onActivate` already told you that.

```luau
-- Which overlay is holding the slot right now, in a line somebody can check against
-- what they are hearing.
local owner = host.arbiter.winner("com.platform.kontakt/plugin")
host.log.info("slot held by: " .. (owner or "nobody"))
```

## host.arbiter.winnerSpecificity(slot) {#host-arbiter-winnerspecificity}

**Signature:** `host.arbiter.winnerSpecificity(slot: string) -> number?`

The rank of whoever currently holds `slot`, or `nil` when nothing matches. Useful for the question "is what is showing more specific than me", which the overlay runtime asks to decide whether it has been outranked rather than simply not matched.

```luau
local rank = host.arbiter.winnerSpecificity(self._slot)
local outranked = rank ~= nil and rank > self._specificity
```

## host.arbiter.participants(slot) {#host-arbiter-participants}

**Signature:** `host.arbiter.participants(slot: string) -> { { module: string, specificity: number, matching: boolean, active: boolean }, … }`

Every claim on `slot`, for diagnosis. This is the only way to catch the failure the design is most prone to: a mistyped slot string is not an error, it is a second slot with one member that wins everything.

A slot that stays at one participant while its sibling shows several is the visible symptom, and it is visible only here.

```luau
for _, p in ipairs(host.arbiter.participants(SLOT)) do
  host.log.info(("  %s rank %d%s%s"):format(
    p.module, p.specificity, p.matching and " matching" or "", p.active and " ACTIVE" or ""))
end
```
