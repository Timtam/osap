---
title: Test protocol — fourth macOS session
---

# Test protocol: the fourth macOS session

Written for **one hour on a borrowed MacBook**: new, probably on the newest macOS, its own
Retina display and no other screen, with Kontakt installed. Nobody here knows yet whether that
Kontakt is 7 or 8, or whether REAPER and sforzando are there too, so the steps say what to do in
each case.

The hour is short and the Mac is not yours, so this protocol is shorter than the last one and
ordered by value. **If time runs out, steps 3 and 4 are the session.** Step 5 comes next.
Everything after that is a bonus.

Each step has **what to do**, **what should happen**, and **what to write down**. "It said
nothing" and "it said the wrong thing" are different faults with different causes, and the
difference is invisible from here.

## Why this Mac matters

Three things have never happened on any Mac this application has run on, and this one does all
three at once:

- **A Retina display.** Both Macs so far drew one pixel per point. This one draws four per point,
  and every image search and every piece of text read off the screen has to get that right.
  Nothing for you to do about it: the log records it, and the Kontakt steps exercise it.
- **A new macOS.** The functions the application used to photograph the screen are marked as
  going away, so it now uses the newer way macOS offers (ScreenCaptureKit), and falls back to the
  old functions only where they still exist. This Mac is where that is tried for real.
- **Probably Kontakt 8.** Both of its overlays are new and built from what Kontakt 8 publishes
  on Windows, not from anything measured on a Mac.

## The keys

| What | Keys |
| --- | --- |
| Record the window in front (the probe) | **Command-Shift-F9** |
| Move between an overlay's controls | **Tab**, and **Shift-Tab** backwards |
| Put the keyboard back into a plugin window | **Command-Shift-F6** |
| Reload every module | **Command-Shift-F5** — keep Shift; Command-F5 alone switches VoiceOver off |

A MacBook's top row is media keys by default. If Command-Shift-F9 does nothing at all, hold
**fn** as well (Command-Shift-fn-F9), and write down that you had to.

---

## 1. Before the hour starts

**Do:** download the artifact **AutomationPlatform-macos-universal** from the newest **green**
run of the **macOS build** workflow on GitHub — a red or cancelled run leaves a file behind too,
and that one is missing what this session is about, so take a run with a tick. Do this on your
own computer, before the hour: it saves the student's time, and GitHub asks you to sign in
before it hands over the file. Bring it to the MacBook however is quickest — AirDrop, a USB
stick, a shared folder.

**Know before you start:** macOS asks for an **administrator password** when you grant the
permissions in step 3. On a borrowed Mac that is the student's, so have them nearby for the first
ten minutes.

---

## 2. Setting up

**Do:** it is a zip inside a zip: unzip twice, until you have a folder called `AutomationPlatform`
holding the `.app`, a `modules` folder and a `README.txt`. Move that whole folder somewhere you
can write to — the Desktop is fine.

Then, in Terminal, inside that folder, run
`xattr -dr com.apple.quarantine AutomationPlatform.app`. **On this Mac that is not optional.**
The artifact is not notarised, and since macOS 15 the old way round that — Control-click, Open —
is gone, so without the command the `.app` may not open from Finder at all. Where it does open,
macOS may run it from a hidden read-only copy of itself, where the `modules` folder is not there
— every overlay would then be silently absent, which sounds exactly like "nothing works". The
log says `translocated: YES` when that has happened.

**Write down:** nothing, unless something stopped you. The log records the macOS version by
itself.

---

## 3. The permissions, on a new macOS

**Do:** open `AutomationPlatform.app`.

**Should happen:** macOS raises its own **Accessibility** dialog, and because nothing has been
granted yet, our module manager **opens by itself** on the **Permissions** page and says why, in
the **system voice** rather than VoiceOver's. The announcement ends with: *"Grant Accessibility
first: the Screen Recording list shows this application only afterwards. On newer macOS that
list is called Screen and System Audio Recording."*

**Do, in this order — it is the order that worked last time:**

1. Grant **Accessibility**, then **quit and open the application again**.
2. Grant **Screen Recording**. Newer macOS versions call that list **Screen & System Audio
   Recording**; it is the same thing. Then quit and open the application once more — macOS hands
   a new permission only to a process that started after it was granted.
3. **Input Monitoring**, if the Permissions page still lists it as missing, the same way.

Leave **Automation** alone: it belongs to "Speak through VoiceOver", which stays off for this
session, so its row saying it cannot be determined is correct.

**Then, the first photograph of the screen, on purpose.** Once all three are granted and the
application has been opened again, bring a **Finder** window to the front (Command-Tab to
Finder, then Command-N) and press **Command-Shift-F9**. It says "Recording. This can take a few
seconds on a large plugin", then a summary ending in a number of words. About three seconds
later a window titled "Module error: com.tool.probe" opens and takes the keyboard, as it did
last time; Escape closes it, and there is nothing to answer about it — it comes once per launch.

That press does three jobs at once. It checks that the F-keys arrive on this keyboard — if
nothing happens, try it with **fn** held as well. Its word count says whether the photograph
showed Finder's window at all: a picture of a bare desktop reads almost no words. And it is the
moment a new macOS may show **a dialog of its own** about an application recording the screen,
or about bypassing a "window picker" — better now than in the middle of Kontakt. The application
now photographs the screen the newer way (ScreenCaptureKit), and nobody here has seen which
wording this macOS uses, or whether it asks at all. If a dialog appears, **allow it**, and press
Command-Shift-F9 once more. On some newer versions that dialog has been reported coming back
later, even after Allow; if it does, allow it again each time, and count how often.

**Write down:**

1. When did the application appear in the Screen Recording list — straight after granting
   Accessibility, only after opening the application again, or never?
2. Any system dialog about recording the screen: its wording, as near as you can, and what you
   had just done when it appeared.
3. Anything the Permissions page said that sounded wrong for this Mac.
4. The summary the Finder press spoke — above all its number of words — and whether it needed
   **fn**.

---

## 4. Kontakt on its own — the most valuable step

**Do:** open **Kontakt on its own**, not inside a DAW. First find out which one it is: VoiceOver
reads the window's title, and it is either **"Kontakt 7"** or **"Kontakt 8"**. Write that down;
the rest of this step differs slightly between the two.

If Kontakt opens with a **"What's New"** screen or an update notice over its window, close it
first — Escape, or its close button with VoiceOver. On Windows the overlay closes that screen by
itself; on a Mac it cannot yet.

Last time Kontakt did not answer the accessibility layer at all for about a minute after it came
to the front, and nothing tells you when that minute is over — so do not wait for it.
**Command-Tab to Finder** and straight back to Kontakt; coming to the front is what makes the
overlay look. If "Kontakt file menu, button" has not been spoken within about fifteen seconds, do
the Finder-and-back again, every half minute or so, until it is. Write down roughly how many
round trips it took.

**Should happen:** the overlay's first control is spoken — **"Kontakt file menu, button"**, then
its key in the Windows spelling. **Tab** then gives:

- on **Kontakt 7**: "Library on/off", "View menu", "Shop", then **"Kontakt controls, Tab to step
  through them"**;
- on **Kontakt 8**: straight to **"Kontakt controls, Tab to step through them"**. Kontakt 8 moved
  the other three into its menus, so two stops of ours are all there is.

The Tab after "Kontakt controls" goes *into* Kontakt, and comes back to "Kontakt file menu" only
when Kontakt's own controls have all been visited.

**If nothing is ever spoken on Kontakt 8:** that is a real answer, not a failure of yours — its
overlay has never met a Kontakt 8 on a Mac. Go straight to the probe press at the end of this
step; its recording is what the next build is made from.

**Do, in this order:**

1. From "Kontakt file menu", **Tab round once** to "Kontakt controls", and write down the stops
   as you heard them.
2. On **"Kontakt controls"**, press **Tab** again. The keyboard should go *into Kontakt*, and
   **VoiceOver** — not our voice — should announce each of Kontakt's own controls. Keep pressing
   Tab until "Kontakt file menu" comes round again. Write down roughly how many stops there were
   and a few of their names; the log counts them too. "Button" on its own is an expected answer.
   **Do not press Return or Space while inside Kontakt's controls** — nobody knows what its
   unnamed buttons do.
3. Go to **"Kontakt file menu"** and press **Return**. You hear "Kontakt file menu, activated",
   and Kontakt's file menu should open: use the arrows — **does VoiceOver read the menu's items?**
   Do not choose anything. Press **Escape**, count one, and press **Tab**.
4. **Only on Kontakt 7:** go to **"Library on/off"** and press **Return once**. It shows or hides
   Kontakt's preset browser, which nobody can hear; the probe press below shows which. Press it
   once more afterwards, to put Kontakt back as it was.
5. **Then press Command-Shift-F9** over Kontakt. It says "Recording …", then a summary. If a
   system dialog about screen recording appears now after all, allow it, write that down, and
   press Command-Shift-F9 once more.

**Write down:** which Kontakt; items 1 to 3; for item 3, whether VoiceOver said anything at all
when you pressed the arrows (even just "menu"), and whether Tab after Escape moved on the first
press; the summary the probe spoke.

---

## 5. Kontakt inside REAPER — only if REAPER is installed

If REAPER is not on this Mac, skip to step 6.

**Do:** quit the standalone Kontakt from step 4. In REAPER, put **Kontakt** on a track and open its
FX window. Then press **Command-Shift-F6** to get the keyboard into Kontakt's panel. It says the
window's title, then "on" and the name of the control that took the keyboard — "on an unnamed
control" is a normal answer. If F6 does nothing, click into the panel with VOCR, press
**Command-Shift-F6** once more, and carry on.

**Should happen:** the overlay activates, and its only sign is one phrase shortly after F6's own
sentence: **"Load instrument, button"**. The overlay never says its own name, so if you hear F6's
sentence and then nothing, it did not come up.

On **Kontakt 8** that phrase is the whole question of this step: whether a Kontakt 8 inside a DAW
publishes anything a Mac can find. On Windows it publishes nothing at all. If nothing comes up,
press **Command-Shift-F9** over the FX window and move on to step 6; the log already says why in
one line.

**Do, in this order, and only these:**

1. **Tab through all the controls once** and write down what you heard, starting with the very
   first words before you pressed anything.
2. Go to **"Load instrument"** and press **Return**. It opens Kontakt's file menu, **reads the
   menu off the screen**, and clicks the row that begins with "Load". Kontakt's own file dialog
   should open; close it with Escape. If you hear **"Menu item not found"** instead, do not press
   Escape — the overlay already has. On this Mac that sentence matters more than before: reading
   a menu off a Retina screen has never been done, and the log keeps what was read.
3. **Tab past everything else without pressing it.** Several of those controls click at points
   measured on Windows, and some of Kontakt's rows put those points close to controls that
   overwrite or delete things.

**If anything says "something else is covering …":** the application refused to click because
another window was over the spot. Write down which control said it.

**Then:** press **Command-Shift-F9** over the FX window.

**Write down:** which Kontakt, everything from 1 and 2, and whether the overlay activated at all.

---

## 6. Reload — two minutes

**Do:** press **Command-Shift-F5** — and keep the Shift held. **Command-F5 on its own is macOS's
own switch for turning VoiceOver off.** If you hear "VoiceOver off", press Command-F5 again to
bring it back and repeat the step.

**Should happen:** after a moment, a spoken count — "12 modules reloaded", or whatever number this
Mac loads. If a module does not rebuild, the sentence names it: "11 reloaded, 1 failed: Kontakt".

**Write down:** the sentence, word for word, or that there was none.

---

## 7. If minutes are left: sforzando

Only if **sforzando** is installed, and only if there are ten minutes left before step 8.

**Do:** open **sforzando standalone** and click into its window. It should name "Instrument" and
read its value, and **Tab** should give "Polyphony", then "Pitchbend range". Press **Return** on
Pitchbend range, choose a value, press **Return**, count one, press **Tab**.

**Write down:** whether "Pitchbend range" read a sensible value, and whether that Tab moved on
the first press.

---

## 8. Leaving the Mac as you found it — the last five minutes

Keep these five minutes, whatever else did not get done.

1. **Send the files first** (see below): the log lives inside the folder you are about to delete.
2. Quit the application from its **menu-bar icon's** menu. If there is no icon — newer macOS
   versions can hide one — quit it in Terminal with `pkill -x automation-platform`.
3. In **System Settings → Privacy & Security**, remove **AutomationPlatform** from
   **Accessibility**, **Screen Recording** (or **Screen & System Audio Recording**) and **Input
   Monitoring**: select it in each list and press the minus button. This needs the student's
   password again.
4. Delete the `AutomationPlatform` folder.

**Left out on purpose:** **Personal Voice**. It is the student's own voice, on the student's
Mac, and not ours to ask for.

---

## What the log answers on its own

Nothing to do. Lines worth knowing about, most of them new:

- **`[host] os macos … — macOS NN.N`** — the version this session ran on.
- **`[macos] screen capture paths on this macOS: ScreenCaptureKit captureImageInRect present,
  CGWindowListCreateImage …, CGDisplayCreateImageForRect …`** — which ways of photographing the
  screen this macOS still has. If the last two say `absent`, the old way is gone from this
  version, and the new one is all there is.
- **`[macos] screen captures are coming from ScreenCaptureKit captureImageInRect`** — which way
  actually served. On a CI Mac running macOS 15.7 it answered in 61 ms the first time and 11 ms
  after that.
- **`[macos] first capture for reading text: … points came back as … px — 2.00x`** — the Retina
  measurement: the factor every word read off the screen is divided by. Every Mac so far said
  1.00x. (The `first screen capture … N.NNx` line belongs to the pixel path and may say 1.00x even
  here.)
- **`'FILE': text centre …, element centre … — off by …`** — from each probe press: where a word
  was read, against where the button carrying that name says it is. A few points apart is
  normal. A gap that grows the further the word is from the window's corner is the Retina mistake
  this session exists to catch.
- **`[macos] reading text in the … part of a … region`** — a window hung off the bottom of this
  laptop's screen, and only the part on screen was read.
- **`[macos] ScreenCaptureKit refused a … capture …`** — with the reason macOS gave; usually the
  Screen Recording permission.
- **`[macos] ScreenCaptureKit did not answer …`** — it never did that on the CI Mac. Here it would
  be the most important line in the log.
- **`[macos] ocr: first recognition of a … pt region took N ms`** — the first text read off a
  Retina screen.
- **`[kontakt] Kontakt 8 panel in 'FX: …': Kontakt File Menu at …`** or **`[kontakt] 'FX: …'
  publishes no button named Kontakt File Menu`** — step 5 on Kontakt 8, whichever way it went.
- **`focus_step(…): the ring has N focusable stop(s)`** — step 4's first step into Kontakt's own
  controls.
- **`translocated: YES`**, with **`no modules to load`** under it — macOS ran a read-only copy of
  the `.app`, and nothing else in the session can be judged.

## What to send

- **`automation-platform.log`**, whole, from the same folder as the `.app`.
- **Every `probe-N.png`**, from `modules/probe/`.
- Your notes, with **expected** and **got** where something behaved oddly — and which Kontakt it
  was, and whether REAPER and sforzando were there.
