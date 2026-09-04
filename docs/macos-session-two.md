---
title: The second macOS session
---

# A second session on the Mac

The first session answered more than we expected and changed a lot of code. This is the
list for the next one, in the order worth doing it, with what to expect beside each thing so
a difference is recognisable as one.

Everything about **getting it running, sending a log, and what not to reach for** is
unchanged — see [the first briefing](macos-tester-briefing.md). Nothing below repeats it.

## What changed since last time

- **The application has its own voice now.** It no longer borrows one; the speech path was
  rewritten from scratch for this platform, and **nothing in it has ever run on a Mac.**
- **The startup announcement was silent on every Mac** and is not any more. It asked whether
  we speak *through* VoiceOver when the question it needed was whether anybody is listening
  at all.
- **A fourth permission exists.** Speaking through VoiceOver is an Apple Event, which macOS
  refuses by default and refuses *silently*. It is asked for now, at the moment you tick the
  switch.
- **The frontmost-window question was the biggest stall in your log** — fourteen times, worst
  case 2.6 seconds. It asked an application three times over when it had already failed to
  answer once. That is fixed, and while an application is not talking the overlay is given
  the window it last described instead of being told there is none.
- **The sforzando pitchbend field read as nothing at the value 1.** Your probe explained it:
  the region we read started four points inside the value itself. Fine for "DEF", fatal for a
  single digit.
- **The probe measures the frontmost-window cost properly now.** It was asking forty times in
  a row and the host was answering thirty-nine of those out of a cache, so it was measuring
  almost nothing.

## Three things this Mac cannot answer, so they are not on the list

Not a limitation of the session — of the machine, and worth saying so nobody spends time on
them:

- **The arm64 half of the build.** A 2015 Air is Intel; it runs the other slice. Whether the
  Apple-silicon one launches needs a different machine.
- **Retina.** This display reports a backing scale of 1.00, so every coordinate agrees
  trivially. The interesting case is 2.00.
- **Personal Voice.** It needs macOS 14; this is 12.7.6. There is still something to check
  about it below, and it is the opposite of the feature.

## The list

### 1. Does it start, and does it speak

The whole speech path is new. Start it and listen.

- You should hear a spoken line when it comes up. Last time there was none — that was the
  bug above, not your Mac.
- Open **Settings** from the menu-bar item and read the list of switches. Two are new here:
  *"Speak through VoiceOver…"* and *"Offer my Personal Voice…"*.
- **Tick the Personal Voice one.** On this macOS the feature does not exist, and what we need
  to know is that it says so and carries on rather than taking the application with it. The
  honest answer here is "nothing happened, and it still works" — that is a pass.

Report: did anything speak at startup; did ticking Personal Voice do anything visible or
audible; is the application still running afterwards.

### 2. Speak through VoiceOver, and the permission that guards it

Tick *"Speak through VoiceOver"*. macOS should put a permission dialog on screen asking
whether this application may control VoiceOver.

- If a dialog appears: allow it, then use an overlay and say whether what it speaks now comes
  out in your voice, at your rate, rather than in a second voice talking over VoiceOver.
- If **no** dialog appears, that is just as useful an answer — say so. It means either the
  permission was already decided or the request never reached macOS.
- It also needs *"Allow VoiceOver to be controlled with AppleScript"* in VoiceOver Utility's
  General pane. Worth checking it is on.

### 3. The probe, over a Qt plugin — the one that decides an architecture

This is the highest-value single press in the session, and it needs no new code.

If you can install **Kontakt** or **Komplete Kontrol**, put its window in front and press
**Cmd+Shift+F9**. Everything else follows from what the probe writes.

The question it settles: Qt applications give their controls internal names, and on Windows
we match plugins by them. Whether those names survive into what macOS publishes decides
whether the whole nested-overlay design — Kontakt inside a DAW, a library inside Kontakt —
can be ported, or has to be rebuilt from images. sforzando could not answer it because it is
not a Qt application.

Nothing to look for by ear. Send the folder.

### 4. sforzando, and the field that read as nothing

Open **sforzando standalone**, put the overlay on it, and go to the pitchbend range field.

- Set it to **1**. It should now say "1". Last time it said nothing at this value while the
  polyphony field at 1 read fine.
- Also worth stepping through the other two fields once, since the same measurement moved.

Report what each field says, and especially any field that says *nothing* rather than
something wrong — those are different faults.

### 5. F6 into the plugin

Rewritten since your session, where five of five presses failed. In REAPER, with a plugin
window open, press **F6** and say where the keyboard ends up.

The log should say what it found. If it does nothing, that is still a result — the previous
diagnosis was that the focus stayed on the FX list, which is a different problem from the one
that was fixed.

### 6. The error window

At the end of a probe run it raises a real error on purpose, three seconds after the last
line. A window should appear. Three things, and the third is the one it was written for:

1. Does it have a **Dock icon** and an entry in **Cmd-Tab**?
2. Does VoiceOver **read the message** when it opens?
3. With the **module manager also open**, close the manager. Does the error window keep its
   Dock icon? (That is the bug this window exists to prevent.)

### 7. What the probe answers by itself

These come free with any probe run; they are in the log, not something to do.

- **`window.active cost`** — read the *tail*, not the median. This decides whether the
  frontmost-window question has to be moved off the thread that carries your keyboard, which
  is a large change nobody wants to make on a guess.
- **`timers`** — cadence, which every overlay deadline is written against.
- **`ocr batching`** — whether one wide read beats three small ones.

## One line to watch for in the log

If you see **"a key the overlay had claimed reached the application instead"**, say so and
quote the line. It means a hotkey you pressed was handed to the plugin instead of to the
overlay. We have reasoned that this can happen and have changed the code so it should not,
but we have never seen it — so an occurrence is worth more than the change was.

## What to send

The same as last time: the whole folder the probe writes, plus the log. See
[the first briefing](macos-tester-briefing.md#what-to-send).

Where something behaved oddly, the useful sentence is what you **expected** and what you
**heard** — not a diagnosis. Several of the last session's most useful findings came from
"this said nothing", which is a sentence we could act on.
