# Test session — builds up to 2026-09-01

Keyboard only throughout. Do the steps in order and send one log at the end.

Each step ends with a **TESTER FEEDBACK:** line — type your answer straight after it and
send this file back along with the log. Where a step mentions a log line, you do not need to
go looking for it: it is there for us, and sending the file is enough.

Steps 1 to 5 were written for the build of 28 August and have not been run yet, so they are
unchanged. Steps 6 to 8 are newer.


## Before you start

**You can download a build now instead of making one.** The macOS build runs on every push
again — it had been failing since 31 August for a billing reason on our side, not a code one —
so the Actions tab has a finished `.app` for the current commit, already carrying the current
modules. That saves the 10-to-25 minute first build entirely.

Building yourself still works and is still the surer path if anything looks wrong:

    cd ~/osap
    git pull
    ./bootstrap-macos.sh

Either way, the permissions are per code identity. A downloaded build is a different
application to macOS than one you built, so it asks for Accessibility and Screen Recording
again the first time.


## 1. Launching should not take your cursor

Note where your VoiceOver cursor is, then:

    open dist/AutomationPlatform/AutomationPlatform.app

You should hear "Automation Platform is running in the menu bar", and your cursor should
still be where you left it. Last time it landed in our application and VoiceOver said it had
no windows.

That turned out not to be our code: wxWidgets activates a menu-bar application at startup on
purpose — its own comment says it would otherwise receive no events — and it does so before
any of our code runs, so it cannot be prevented, only handed straight back. That is what now
happens.

TESTER FEEDBACK:


## 2. Closing and quitting — the window has a menu bar now

Open the manager from the menu-bar icon (VO-M twice), then:

  - Command-W
  - reopen it, then Command-Q

Both are real menu items under **File** now, so you can also walk the menu bar and read what
this window can do. Last time neither key worked, and the reason was structural rather than a
slip: a wxWidgets key event never reaches the window itself, only the control that has focus,
so the handler we had bound could never have fired.

TESTER FEEDBACK:


## 3. sforzando standalone — Shift+Tab, and the second digit

Open sforzando (Command-Space, type sforzando, Return).

  - Tab through all three read-outs, then Shift+Tab back through them, several times,
    reasonably quickly.
  - Set POLY. to 1 and read it.
  - Set PB RANGE to 1 and read it.

Last time Shift+Tab often produced "dimmed button" over our own announcement. It turns out to
be the same fault as the unreadable field. Shift+Tab from the start lands on PB RANGE, whose
reading returned nothing — and a reading that returns nothing is also the most expensive one
there is: four attempts, on the same thread that carries the keyboard. Long enough for macOS
to switch our key handling off, after which the next keystroke reached the plugin, which moved
its own focus, and VoiceOver announced where it had landed.

Both halves are fixed independently. Readings now have a time limit. And the reason the field
could not be read at all is that sforzando paints those values inside a black well set into
grey — so we had been measuring the well and handing that to the recogniser instead of the
digit.

TESTER FEEDBACK:


## 4. The plugin's own menus, by keyboard

Still in sforzando standalone: press Enter on PB RANGE, wait about a second, then choose a
different value with the arrows and Enter. The choice should stick.

Last time the arrows moved but nothing committed — Enter was being swallowed and simply
reopened the menu. Standalone, nothing could tell us a menu was open at all: subscribing to
the accessibility notifications used a quarter-second budget against an application we have
measured needing about seven hundred milliseconds to answer, so every subscription failed and
the refusal was recorded as final.

Try Instrument too if you like; it is the same mechanism.

TESTER FEEDBACK:


## 5. Getting into the plugin inside REAPER

Load sforzando as an FX in REAPER, open its window, then move the focus away — into the track
list or the FX list. Press Control-Shift-Command-Option-F6. Then press Tab.

You told us this spoke the right title but usually did not really move you: Tab kept walking
REAPER's own controls, and one Shift+Tab fixed it. That fits — bringing a window forward is
not the same as moving the keyboard into it, and REAPER's focus stayed on its FX list.

We now also ask the window to become the focused one, and measure what happened immediately
afterwards. So this step is worth doing **even if it still fails**: the log will say whether
the keyboard ended up in the window or still on one of REAPER's own controls, and that is the
number that decides what to do next.

TESTER FEEDBACK:


## 6. One probe per plugin — it now asks more than it used to

Put a plugin window in front and press **Command-Shift-F9**, as before. Nothing about how you
do it has changed, and it still costs one keypress.

What has changed is what it writes down. It used to record what the window *is* — its
identity, its geometry, its accessibility tree, a picture of it, what OCR reads in it. It now
also puts four questions to macOS itself, because they are questions a machine can answer and
you should never have been asked to:

  - **whether the two coordinate systems agree** — the pointer's own position against the
    window's accessibility frame. If those disagree by a factor of two, every click we ever
    send on a Retina screen lands in the wrong place, and nothing else would have shown it;
  - **whether anything is drawn over the plugin**, and whether the check that is supposed to
    notice that works on your machine at all;
  - **whether the text recogniser invents words.** It finds a patch of the window that is
    provably one flat colour, reads it, and writes down anything that comes back. Nothing
    should. On Windows this failure was real — six different blank areas all came back as the
    same invented string. We have since put the same guard on the Mac, written without one to
    try it on, so this is also how we find out whether it was needed and whether it works;
  - **how long a tenth of a second really is** on your machine. Every wait we do is built on
    that number.

All four are read-only. The probe does not click, drag, scroll or type anything, and it never
has — it runs over your real plugin and you cannot see what it did, so that is a rule rather
than a preference.

**One optional extra.** The first question is only meaningful if the mouse pointer happens to
be resting over the plugin window, and there is no reason it would be. If parking it there is
easy for you — VO-Command-F5 moves the mouse to the VoiceOver cursor — do that first and the
log settles one more thing. If it is any trouble at all, skip it: the probe says which case it
got, and an unanswered question is fine.

Please do it once per plugin you would want an overlay for. The files are numbered.

TESTER FEEDBACK:


## 7. Nothing to arrange for the keyboard

There is no step here, and that is the point — it is worth knowing what the log now settles on
its own so you do not go looking.

The first time a keystroke is actually taken by our overlay, the log says so by name. That one
line answers the failure we cannot otherwise see: **Input Monitoring not granted leaves an
event tap that is created successfully, reports itself as enabled, and never fires.** Until
now that produced a log identical to a working session. So if steps 3 and 4 go well, the line
will be there; and if the overlay seems dead, its absence says where to look.

TESTER FEEDBACK:


## 8. If a module refuses to load, the log now says why

Nothing to do — this is one to recognise if you see it.

A module has to declare what it uses (`[capabilities] require` in its `module.toml`), and until
this week nothing checked. Now it does: a module that reaches for something it did not declare
stops with a line naming **the module and the missing capability**. If one of them fails to
load, that line is the whole diagnosis, and the others carry on without it.

We fixed every module we could reach from Windows, and every one loads on the build machine's
Mac. What we cannot reach from here is a path that only runs on macOS — a branch behind
`host.os.is("macos")` — so if a module drops out on your machine and not on ours, that is the
most likely reason and the log will say so exactly.

TESTER FEEDBACK:


## 9. Send the log

`automation-platform.log`, in the folder beside the application, together with this file and
any `probe-*.png`.

It now records by name: who had the front when the application started and whether we gave it
back; every accessibility subscription that was refused, and why; any key we had claimed and
let through anyway; any reading we abandoned rather than keep the keyboard waiting; the first
key the tap ever suppressed; for every wait, whether the thing it was waiting for changed or
the wait simply ran out; and any module that would not load, with the reason.


## Two things worth knowing

The Dock icon while the manager window is open is **on by default** now, as you suggested.

And nothing in this round asks you to reproduce anything with tracing switched on. Every
question above is answered by lines the log writes on its own.


## What this round deliberately does not ask

Some of what we have built has never run on a Mac and **cannot** be run there yet, so there is
no point asking. Scrolling a control, dragging one, and the two mechanisms that decide when a
text field keeps the keyboard are used by exactly one module, which is Windows-only — no
module that loads on your machine can reach that code however willing you are. They come back
onto this list when a plugin you actually have uses them, and TODO.md says so in the meantime.
