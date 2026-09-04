---
title: Test protocol — second macOS session
---

# Test protocol: the second macOS session

Self-contained. Each step has **what to do**, **what should happen**, and **what to write
down**. The last one is the part that matters: "it said nothing" and "it said the wrong thing"
are different faults with different causes, and the difference is invisible from here.

You are not asked to diagnose anything. What you expected and what you got is the whole job.

**If you only have twenty minutes:** do step 1, step 4 and step 6. Those three answer the most.

## Before you start

- **Ad-hoc signed builds lose their permissions on every rebuild**, so macOS may ask again for
  Accessibility and Screen Recording. That is not a fault.
- **The application lives in the menu bar**, not the Dock. Its icon's menu has **Show** and
  **Quit**; Quit is the only thing that exits it, and closing the manager window only puts it
  away. The list inside the manager announces itself as "Installed modules".
- **The log** is `automation-platform.log`, in the same folder as `Automation Platform.app` —
  beside it, not inside it. If that folder is not writable (the app was moved into
  `/Applications`, or is still quarantined), it goes to
  `~/Library/Application Support/AutomationPlatform/` instead.
- **The probe's pictures do NOT sit next to the log.** They land in the probe's own folder,
  `modules/probe/probe-1.png`, `probe-2.png` and so on. The log names each one on a line
  beginning `capture:`. If in doubt, search the application's folder for `probe-` and send
  whatever comes back.

## The keys used below

| What | Keys |
| --- | --- |
| Record the window in front (the probe) | **Command-Shift-F9** |
| Move between an overlay's controls | **Tab**, and **Shift-Tab** backwards |
| Change a focused slider or stepper | **Left** and **Right** |
| Switch an overlay's tabs | **Left** and **Right** on the tab strip, or **Control-Tab** from anywhere in the overlay |
| Put the keyboard back into a plugin window | **Control-Shift-Command-Option-F6** |
| Reload every module | **Control-Shift-Command-Option-F5** |

sforzando's overlay has three read-outs and no tabs and no sliders, so in step 3 the only keys
that belong to it are **Tab** and **Shift-Tab**. Arrow keys there go to sforzando itself.

---

## 1. Does it start, and does it speak

The whole speech path was rewritten since your session. Everything else depends on this.

**Do:** start the application with VoiceOver running.

**Should happen:** you hear a spoken line as it comes up.

Last time there was silence, and that was a real bug: it asked whether we speak *through*
VoiceOver, when the question it needed was whether anybody is listening at all.

**Write down:** did anything speak, and did it come out in VoiceOver's voice or a second one.

---

## 2. The manager window

Open the menu-bar icon and choose **Show**. The window has four tabs: **Installed**,
**Browse**, **Updates**, **Application settings**.

### 2a. The Installed list

Nobody using a screen reader has ever read this list on a Mac. It is a different control there
than on Windows, chosen precisely so VoiceOver can reach it, and whether that worked is
unknown.

**Do:** on the **Installed** tab, arrow down the list.

**Write down:**

1. Does one press move you one row, and does that one announcement carry **both** the module's
   name **and** whether it is ticked — or do you have to do something extra to hear the tick?
2. Press **Space** on one module in the middle to untick it, then arrow over its neighbours.
   Does each row report **its own** state?
3. Press **Space** again to put it back. Does it come back?

**Careful:** unticking a module really does turn it off. If you leave sforzando unticked, step
3 will not work.

### 2b. Speak through VoiceOver

**Do:** go to the **Application settings** tab. It is a scrolling page, so keep arrowing past
the bottom of what fits — the two you want are at the end. Tick the one whose label begins
**"Speak through VoiceOver…"**.

**Should happen:** macOS puts up a permission dialog asking whether this application may
control VoiceOver. Allow it.

This is a **fourth** permission, in its own pane, refused by default and refused *silently* —
which is why it is now asked for the moment you tick the switch instead of being discovered as
a setting that does nothing.

**Write down:** whether a dialog appeared at all. If none did, say so — that is just as useful.

Then leave the switch on for the rest of the session. From step 3 onward, say whether what the
overlay speaks arrives in your voice and at your rate, and — this is the part only an ear on a
Mac can settle — whether a new line **cuts off** the one before it or waits its turn.

It also needs **"Allow VoiceOver to be controlled with AppleScript"** in VoiceOver Utility's
General pane. Worth checking that before deciding this failed.

---

## 3. sforzando, and the field that read as nothing

**Do:** open **sforzando standalone**. Click into its window so the overlay activates, then
press **Tab**.

**Should happen:** it says "Instrument". Tab again for "Polyphony", again for "Pitchbend
range". **Shift-Tab** goes back.

**Then:** set **Pitchbend range to 1** and read it again. It should say **"1"**.

Last time it said *nothing* at that value, while Polyphony at 1 read fine. Your own probe
explained it: the region we read started four points inside the value itself — harmless for a
wide word like "DEF", fatal for a single digit. The region has moved since.

**Then, with the value still at 1:** press **Command-Shift-F9** over the sforzando window.

That records what is actually on screen there, which is the only way to tell a bad region from
a bad reading. Your ear cannot separate those two, and neither can the log on its own.

**Write down:** what each of the three read-outs says, and especially any that says *nothing*
rather than something wrong.

---

## 4. The error window

**This comes to you.** About three seconds after the probe finishes speaking in step 3, it
raises a real error on purpose so that the window can be looked at.

**It happens once per session, on the first probe press only.** Pressing F9 again will not
bring it back — the application deliberately reports the same fault once. If you miss it, quit
and start again.

Three questions, and the third is the one it was built for:

1. Does it have a **Dock icon**, and an entry in **Command-Tab**?
2. Does VoiceOver **read the message** when it opens?
3. Open the **module manager as well**, then close the manager. Does the error window **keep**
   its Dock icon?

That third one is the bug this window exists to prevent: the application normally has no Dock
icon at all, so a window with no way back to it cannot be reached.

**Write down:** the answer to each of the three.

---

## 5. Back into a plugin window

**Careful with this one:** it works by clicking just inside the plugin's panel, and only
sforzando's corner has ever been checked. On another plugin that click could press a control.
**Use sforzando, in a scratch project you will not save.**

**Do:** in REAPER, open sforzando's plugin window, then click into REAPER's **FX list** (that
specific place — not another application), and press
**Control-Shift-Command-Option-F6**.

**Should happen:** the keyboard lands in the plugin window and you are told so.

Five of five presses failed in your session. It has been rewritten since. Note that the
shortcut tells you where it thinks the keyboard went, and that sentence is the thing under
test — so the useful check is whether **Tab** then walks sforzando's overlay, which is what
proves the keyboard really arrived.

**Write down:** what it said, and whether Tab afterwards actually reached the overlay.

---

## 6. The probe over a Qt plugin — the single most valuable press

One keypress, no new code, and it decides an architecture.

**Do:** if you have **Kontakt** or **Komplete Kontrol** installed, put its window in front and
press **Command-Shift-F9**.

**Why it matters:** on Windows we identify the parts of a plugin by the internal names Qt gives
them. Whether those names survive into what macOS publishes decides whether the whole
nested-overlay design — Kontakt inside a DAW, a library inside Kontakt — can be ported, or has
to be rebuilt out of image matching. sforzando cannot answer it: it is not a Qt application.

The probe states the answer itself now, in a line reading something like *"N of M elements
publish an AXIdentifier"*.

**Write down:** nothing to listen for. Send the log and the picture.

---

## 7. Personal Voice — last, and expected to do nothing

**Do:** on the **Application settings** tab, tick the switch beginning **"Offer my Personal
Voice to modules…"**.

**Should happen:** nothing you can hear. Personal Voice needs macOS 14 and this Mac is on
12.7.6, so the feature is genuinely absent; the application should notice that and carry on.
The log gets a line saying so.

This is last on purpose. A review found that this exact switch would have **taken the
application down** on your macOS — it asked the system a question that does not exist there —
and that is fixed. This step is the check that the fix holds.

**Write down:** whether the application is still running afterwards, and whether anything
audible happened. If it quits, say so first: that is the most important sentence in the whole
session.

---

## What the probe answers on its own

Nothing to do; these are written into the log by any probe press.

- **`window.active cost`** — what it costs to ask which window is in front. This was the
  biggest stall in your last log: fourteen of them, the worst 2.6 seconds, during ordinary use
  rather than while pressing anything. Fixed twice since, and this line decides whether a much
  larger change is needed.
- **`timers`** — how long a timer really takes here, which every overlay deadline is written
  against.
- **`ocr batching`** — whether one wide text read beats three small ones.

## One line to look for afterwards

Not something you will hear — it is only written to the log. If **"a key the overlay had
claimed reached the application instead"** appears anywhere in it, that is worth pointing at.
It means a key you pressed went to the plugin rather than to the overlay. We reasoned it could
happen and changed the code so it should not, but it has never been seen — so one sighting is
worth more than the change was.

## Three things this Mac cannot answer

Not on the list. They need different hardware, not a different session:

- **The Apple-silicon half of the build.** This is a 2015 Intel Air; it runs the other half.
- **Retina.** This display reports a scale of 1.00, so coordinates agree here trivially. The
  interesting case is 2.00.
- **Personal Voice itself.** It needs macOS 14. Step 7 checks the opposite — that its absence
  is survived.

## What to send

- **`automation-platform.log`**, whole, not an excerpt. The interesting lines are rarely the
  ones near the failure.
- **Every `probe-N.png`**, from the `modules/probe` folder.
- Your notes from the steps above.

Where something behaved oddly, the sentence that helps is what you **expected** and what you
**got** — not a guess at the cause. Several of the last session's most useful findings came
from "this said nothing", which is a sentence we could act on immediately.
