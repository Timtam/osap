---
title: Test protocol — third macOS session
---

# Test protocol: the third macOS session

Written for a **new Mac** — Apple silicon, a current macOS, and Kontakt or Komplete Kontrol
installed. If you are still on the 2015 Air, see [the last section](#if-you-are-still-on-the-old-air).

Each step has **what to do**, **what should happen**, and **what to write down**. The last one
is the part that matters: "it said nothing" and "it said the wrong thing" are different faults
with different causes, and the difference is invisible from here.

**If you only have twenty minutes once it is set up:** step 4 and step 6. Step 5 arrives on
its own during step 4. Step 1 is the build and takes longer than that on its own, so it is not
a thing to squeeze in — do it whenever, ahead of the session.

A great deal changed since your last session, and most of it because of what you reported.
Where a step exists to check one of those things, it says so.

## The keys

| What | Keys |
| --- | --- |
| Record the window in front (the probe) | **Command-Shift-F9** |
| Move between an overlay's controls | **Tab**, and **Shift-Tab** backwards |
| Put the keyboard back into a plugin window | **Control-Shift-Command-Option-F6** |
| Reload every module | **Control-Shift-Command-Option-F5** |

> **Superseded on 2026-09-10.** Both four-modifier chords sit on Control-Option, VoiceOver's
> own modifier, and never arrived on the Mac this protocol was run on. In builds after that
> date they are **Command-Shift-F6** and **Command-Shift-F5**. The table above is the protocol
> as it was sent.

---

## 1. Setting the machine up

**Order matters here and nowhere else.** Run this first, before anything is built:

```bash
./macos-signing-identity.sh
```

Half a minute, once, ever. macOS remembers permissions against an application's *code
identity*, and without this every rebuild is a different application that has to be granted
everything again. Doing it afterwards does not help: the grants you already made belong to the
old identity.

Then:

```bash
./bootstrap-macos.sh
```

The first build takes 10 to 25 minutes because it compiles wxWidgets. It is not stuck.

If `brew --version` says nothing after you install Homebrew, that is not a failed install —
on Apple silicon it lands in `/opt/homebrew/bin`, which is not on the default PATH. The script
finds it there by itself. To fix it permanently: `echo 'eval "$(/opt/homebrew/bin/brew
shellenv)"' >> ~/.zprofile`.

**Write down:** anything that stopped you, and roughly how long the build took.

---

## 2. The permissions, which the application now walks you through

**Do:** open `dist/AutomationPlatform/AutomationPlatform.app`.

**Should happen, in this order:** macOS raises its own **Accessibility** consent dialog, and
asks about **Screen Recording**, because the application requests both as it starts — those are
the system's dialogs, not ours. Then, because nothing has been granted yet, our module manager
**opens by itself** on a **Permissions** page and says why. That page is new and nobody has ever
seen it, which is what this step is for.

It opens with a line saying how many of the four are granted, then lists all four on every
visit — the granted ones too — each as one sentence: the permission's name, then its state,
then what stops working without it. Each has a button beside it that opens the right settings
pane. Near the bottom is a **Re-check now** button, and below that a reminder to quit and reopen
after granting. Keep arrowing past the button; the reminder is under it.

**Do:** work through it. After granting one, **quit and open the application again** — macOS
hands a new permission only to a process that started after it was granted, and that is the
commonest reason a grant looks like it did nothing.

**Write down:**

1. Did the window come up by itself, and did you land on something that told you what to do?
2. What does a row actually sound like, read out? Quote one back to us — that is more use
   than a yes or no, and it is the only way we learn what VoiceOver makes of it.
3. Do the buttons open the right pane?
4. Once everything is granted, does the page say so — and does the window stop appearing on its
   own?

The fourth permission, **Automation**, is not granted there: it is asked for when you tick
"Speak through VoiceOver" in step 3.

---

## 3. Does it speak, and through what

**Do:** with everything granted and VoiceOver running, start the application.

**Should happen:** a spoken line as it comes up.

**Then:** open the manager (menu-bar icon → **Show**), go to the **Application settings** tab —
it is the fourth of five — and tick the switch beginning **"Speak through VoiceOver…"**. macOS
should put up a permission dialog; allow it. Leave the switch on for the rest of the session.

**Then quit and start again**, with the switch already on.

That last part is the one thing here that has no evidence behind it. Last time you reported: on
a second launch, with the switch on, no announcement at all and the VoiceOver cursor "in
no-man's-land". We never had a log of that launch. The application now writes down which of
three things happened to every announcement — shown, spoken, or deliberately dropped — and says
which transport it chose, so if it happens again the log will explain it.

**Write down:** whether the first launch spoke, whether the second launch spoke, and where the
VoiceOver cursor ended up each time.

---

## 4. sforzando, and the menu that stopped tracking

**Do:** open **sforzando standalone** and click into its window so the overlay activates.

**Should happen:** without pressing anything, it names the first read-out — "Instrument" — and
reads its value. **Tab** gives "Polyphony", **Tab** again "Pitchbend range", a third **Tab**
comes back round.

**Then:** press **Return** on Pitchbend range and choose a value in sforzando's own menu the
way you normally would. **Enter should commit it**, as it does in the Polyphony menu.

Do not hurry, and do not deliberately dawdle either: the fix is a longer stopwatch, not an
unlimited one, and what we need to know is whether it now covers an ordinary use of that menu.
If Enter fails again, the useful thing to tell us is roughly how many seconds you had been in
it.

This is the fix for what you reported. It was not two menus behaving differently — it was two
durations. Nothing on that platform can see sforzando's menu, so the overlay let go of its keys
for a fixed 2.5 seconds and then took them back, whether the menu was still open or not. Under
that, your Enter reached the menu; over it, only VoiceOver's own VO+Space still worked. A
longer list read out by ear takes longer, which is why the longer menu was the one that broke.
It is 8 seconds now.

**Write down:** whether Enter commits in **both** menus, and if not, roughly how long you spent
in the one that failed.

**Then, with Pitchbend range set to 1:** press **Command-Shift-F9** over the sforzando window.
It should say "Recording…" before it starts, then a summary when it finishes.

---

## 5. The error window

**This comes to you**, about three seconds after the probe finishes speaking in step 4, and
**once per session on the first probe press only**. If you miss it, quit and start again.

1. Does it have a **Dock icon** and an entry in **Command-Tab**?
2. Does VoiceOver **read the message** when it opens?
3. Open the **module manager as well**, then close the manager. Does the error window **keep**
   its Dock icon?

The third is the one it was built for.

---

## 6. Kontakt or Komplete Kontrol — the press that decides an architecture

**This is the most valuable thing in the session**, and the old Mac could not do it at all.

**Do:** open **Kontakt** or **Komplete Kontrol** — the free Kontakt Player is enough — put its
window in front, and press **Command-Shift-F9**.

**Should happen:** it says "Recording. This can take a few seconds on a large plugin", then a
summary. This will take longer than sforzando did: it is by a distance the largest window this
has ever been pointed at.

**Why it matters:** on Windows we identify the parts of a plugin by the internal names Qt gives
them. Whether those names survive into what macOS publishes decides whether the whole
nested-overlay design — Kontakt inside a DAW, a library inside Kontakt — ports across, or has
to be rebuilt out of image matching. The probe states its own answer, in a line reading
something like *"N of the M element(s) returned publish an AXIdentifier"*.

**If you can, do it three times:** Kontakt standalone, Kontakt loaded inside a DAW, and
Komplete Kontrol. Those are three different windows and each needs its own answer.

**Write down:** nothing to listen for beyond the summary. Send the log and the pictures.

---

## 7. Back into a plugin window

**Careful:** this works by clicking just inside the plugin's panel, and only sforzando's corner
has ever been checked. **Use sforzando, in a scratch project you will not save.**

**Do:** in REAPER, open sforzando's plugin window, click into REAPER's **FX list**, and press
**Control-Shift-Command-Option-F6**.

Last time this worked for the first time — "Keyboard focus arrived! Hurray!" — and you noted
that the first control's announcement cut off the sentence saying where the keyboard had gone.
That announcement now waits its turn instead of interrupting.

**Write down:** whether you hear the whole sentence about where the keyboard went, and then the
control, rather than the second cutting off the first.

---

## 8. Personal Voice — for the first time on a Mac that has it

On the old Air this feature did not exist and the switch correctly did nothing. On macOS 14 and
later it is real.

**Do:** on the **Application settings** tab, tick **"Offer my Personal Voice to modules…"**.

**Should happen:** macOS puts up a permission dialog. If you have recorded a Personal Voice, it
should then appear among the voices a module can choose.

**Write down:** whether a dialog appeared, whether the application is still running afterwards,
and — if you have one recorded — whether it shows up.

---

## What the log answers on its own

Nothing to do — but they do not all come from the same place, and that matters if you go
looking for them. `arch:`, `rosetta:` and `backing scale:` are written **once per launch**, in
the block at the top of the log. `first screen capture:` is written once per launch too, by
whichever capture happens first. Only the last three arrive with **every** probe press.

- **`arch:` and `rosetta:`** — whether the Apple-silicon half of the build ran natively. It
  never has.
- **`backing scale:`** — Retina. The old display reported 1.00, where every coordinate agrees
  trivially. 2.00 is the first real test.
- **`first screen capture: … came back at N.NNx`** — the same question from the other side, and
  measured from the image rather than asked of the display.
- **`timers`**, **`ocr batching`**, **`window.active cost`** — all three now report what they
  measure rather than a verdict; read the sentence, not just the number.

## What to send

- **`automation-platform.log`**, whole, from the same folder as the `.app`.
- **Every `probe-N.png`**, from `modules/probe/`. They no longer overwrite each other across
  launches.
- Your notes.

Where something behaved oddly, the sentence that helps is what you **expected** and what you
**got** — not a guess at the cause.

## If you are still on the old Air {#if-you-are-still-on-the-old-air}

Skip **step 6** (Kontakt will not run on macOS 12) and **step 8** (Personal Voice needs macOS
14). The `arch:`, `rosetta:` and `backing scale:` lines will say the same as last time.
Everything else applies unchanged, and steps 3 and 4 are the ones carrying fixes for what you
reported.
