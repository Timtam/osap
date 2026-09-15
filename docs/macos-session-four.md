---
title: Test protocol — fourth macOS session
---

# Test protocol: the fourth macOS session

Written for **one hour on a borrowed MacBook**: new, probably on the newest macOS, its own
Retina display and no other screen, with **Kontakt, REAPER and sforzando** installed. Nobody here
knows yet whether that Kontakt is 7 or 8, so the Kontakt steps say what to do for each.

The hour is short and the Mac is not yours, so the steps are ordered by value, with a rough
number of minutes on each:

| Step | What | Minutes |
| --- | --- | --- |
| 2 | Setting up | 5 |
| 3 | Permissions, and the first photograph of the screen | 10 |
| 4 | Kontakt on its own | 12 |
| 5 | Kontakt inside REAPER | 12 |
| 6 | sforzando: the menu, and F6 | 8 |
| 7 | Reload | 2 |
| 8 | Personal Voice | 3 |
| 9 | Leaving the Mac as you found it | 5 |

**If time runs out, steps 3 and 4 are the session**, then 5, then 6. Step 8 only when at least
eight minutes are left, because of what it can hold up (see there). **Step 9 always.**

Each step has **what to do**, **what should happen**, and **what to write down**. "It said
nothing" and "it said the wrong thing" are different faults with different causes, and the
difference is invisible from here.

## Why this Mac matters

Three things have never happened on any Mac this application has run on, and this one does all
three at once:

- **A Retina display.** Both Macs so far drew one pixel per point. This one draws four per point,
  and every piece of text read off the screen and every picture searched for has to get that
  right. Nothing for you to do about it: the log records it. Text is read in steps 3 to 6; the
  only picture search is on Kontakt 7 inside REAPER, in step 5.
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

**A MacBook's top row is media keys by default**, and on a current one F5 without fn is Dictation
and F6 is Do Not Disturb. So step 3 finds out with the one harmless key: press Command-Shift-F9;
if nothing at all happens, press **Command-Shift-fn-F9**. **If you needed fn, hold it for every
F-key in this protocol from then on** — Command-Shift-fn-F6, Command-Shift-fn-F5, and fn on the
VoiceOver key in step 7 too. Never try F6 or F5 without fn first. If Do Not Disturb or Dictation
switches on anyway, switch it back off and write it down.

---

## 1. Before the hour starts

**Do:** download the artifact **AutomationPlatform-macos-universal** from the newest **green**
run of the **macOS build** workflow on GitHub, a run with a tick. A red run can still carry a
file, but red can mean the check on macOS 26 failed, and that is probably what this Mac runs. Do
this on your own computer, before the hour: it saves the student's time, and GitHub asks you to
sign in before it hands over the file. Bring it to the MacBook however is quickest — AirDrop, a
USB stick, a shared folder.

**Know before you start:**

- macOS asks for an **administrator password** when you grant the permissions in step 3, and
  again when you take them away in step 9. On a borrowed Mac that is the student's, so have them
  nearby until the permissions are done — about the first quarter of an hour — and for the last
  five minutes.
- **Tell the student what gets sent.** Every probe press records the text of the window it is
  pressed over and the titles of windows lying underneath it, and the log names the Mac's user
  folder. Ask them to quit mail, messages and their browser before you begin.

---

## 2. Setting up

**Do:** it is a zip inside a zip: unzip both in Finder, until you have a folder called
`AutomationPlatform` holding the `.app`, a `modules` folder and a `README.txt`. Move that whole
folder into the **home folder** (in Finder, Command-Shift-H opens it). **Not** the Desktop,
Documents or Downloads: macOS guards those three with a permission dialog of its own, both for
the application — which reads its modules and writes its log beside itself — and for Terminal.

Then, in Terminal, run:

```bash
cd ~/AutomationPlatform && xattr -dr com.apple.quarantine AutomationPlatform.app
```

**On this Mac that is not optional.** The artifact is not notarised, and since macOS 15 the old
way round that — Control-click, Open — is gone, so without the command the `.app` may not open
at all. Where it does open, macOS may run it from a hidden read-only copy of itself, where the
`modules` folder is not there — every overlay would then be silently absent, which sounds exactly
like "nothing works".

Keep that Terminal window open: steps 3 and 9 use it.

**Write down:** anything that stopped you, and any dialog asking whether Terminal or
AutomationPlatform may access a folder (allow it). The log records the macOS version by itself.

---

## 3. The permissions, on a new macOS

**How to quit and reopen**, which this step needs twice: closing the manager window is **not**
quitting — it only hides it, and opening the `.app` while the old process still runs just brings
that process forward, without the new permission. Quit from the menu-bar icon: **VO-M twice**
reaches the menu-bar extras; on Automation Platform's menu choose **Quit**. If you cannot find the
icon, run `pkill -f AutomationPlatform.app` in the Terminal window. To open it again, run
`open AutomationPlatform.app` in that same window. If System Settings offers **Quit & Reopen**
after you switch a permission on, choosing that does the same.

**Do:** open it: `open AutomationPlatform.app` in Terminal.

**Should happen:** up to three things at once. macOS raises its own **Accessibility** dialog, very
likely a second one about **recording the screen** (the application asks for both as it starts),
and, because nothing has been granted yet, our module manager **opens by itself** on the
**Permissions** page and says why, in the **system voice** rather than VoiceOver's. The
announcement ends with: *"Grant Accessibility first: the Screen Recording list shows this
application only afterwards. On newer macOS that list is called Screen and System Audio
Recording."* On each macOS dialog choose **Open System Settings**, not Deny.

**Do, in this order — it is the order that worked last time:**

1. Grant **Accessibility**, then **quit and open the application again**.
2. Grant **Screen Recording**. Newer macOS versions call that list **Screen & System Audio
   Recording**; it is the same thing. If the application is listed as **Automation Platform**,
   with a space, that is it. Then quit and open it once more — macOS hands a new permission only
   to a process that started after it was granted.
3. **Input Monitoring**, only if its row on the Permissions page says it is **NOT granted**. If it
   says it cannot be determined, leave it alone; that is normal. If it is missing from the Input
   Monitoring list, press the **+** button there, choose `AutomationPlatform.app` in the home
   folder's AutomationPlatform folder, switch it on, and quit and reopen as in item 1.

Leave **Automation** alone: it belongs to "Speak through VoiceOver", which stays off for this
session, so its row saying it cannot be determined is correct.

**How you know it is finished:** on the launch where nothing is missing, the manager does **not**
open by itself, and the system voice says only *"Automation Platform is running in the menu bar.
Open its menu to manage modules."* If the manager opens by itself again instead, its sentence
names what is still missing.

**Then, the first photograph of the screen, on purpose.** In Terminal, type `open .` and press
Return: a Finder window of our own folder comes to the front — not one of the student's. Press
**Command-Shift-F9**. It says "Recording. This can take a few seconds on a large plugin", then a
summary: "Probe N written. … surfaces, … accessibility elements, … words." **Press nothing until
that summary has been spoken** — while it records, the application cannot look at the keyboard.
About eight seconds after the summary, a window titled "Module error: com.tool.probe" opens and
takes the keyboard, as it did last time; Escape closes it, and there is nothing to answer about
it — it comes once per launch.

That press does three jobs at once:

- **The F-keys.** If nothing happens, press Command-Shift-**fn**-F9 (see "The keys"). If it is
  silent **both** ways, do not blame the keyboard yet: open the manager from the menu-bar icon
  (**Show module manager**) and listen to the list of installed modules. If it is empty, macOS is
  running a read-only copy after all: quit, run the `xattr` command again, and reopen. If instead
  F9 says *"Nothing is focused, so there is nothing to probe"*, the key arrived but Accessibility
  is not in effect yet: quit and reopen.
- **Whether the photograph saw anything.** Its last number is the word count. A picture of a bare
  desktop reads almost no words.
- **A dialog of macOS's own**, about an application recording the screen or about bypassing a
  "window picker". The application now photographs the screen the newer way (ScreenCaptureKit),
  and nobody here has seen which wording this macOS uses, or whether it asks at all — better now
  than in the middle of Kontakt. If one appears, **allow it**, and press Command-Shift-F9 once
  more. On some newer versions that dialog has been reported coming back later, even after Allow;
  if it does, allow it again each time, and count how often.

**Write down:**

1. When did the application first appear in the Screen Recording list — already at the first
   launch, straight after granting Accessibility, only after opening it again, or never?
2. Any system dialog about recording the screen: its wording, as near as you can, and what you
   had just done when it appeared.
3. Anything the Permissions page said that sounded wrong for this Mac.
4. The summary the Finder press spoke — above all its number of words — and whether it needed
   **fn**. If the error window talked over the number, say so rather than pressing again: the log
   keeps it.

---

## 4. Kontakt on its own — the most valuable step

**Do:** open **Kontakt on its own**, not inside REAPER. Listen to the window's title as VoiceOver
reads it and **write it down word for word**. It is probably **"Kontakt 7"** or **"Kontakt 8"**;
the overlay comes up only on a window titled exactly one of those, and nobody has seen a Kontakt 8
window on a Mac, so any other title is itself the answer to this step.

Somebody else's Kontakt can show things first. A **"What's New"** screen or an update notice:
close it with Escape, or its close button with VoiceOver — on Windows the overlay closes that
screen by itself, on a Mac it cannot yet. An **activation or "Run Demo"** notice: choose Run Demo,
and never sign in to anyone's Native Access account. An **audio or MIDI setup** dialog: close it,
nothing needs choosing. Write down which appeared.

Last time Kontakt did not answer the accessibility layer at all for a minute or more after it
came to the front, and nothing tells you when that is over — so do not wait for it.
**Command-Tab to Finder** and straight back to Kontakt; coming to the front is what makes the
overlay look. If "Kontakt file menu, button" has not been spoken within about fifteen seconds, do
the Finder-and-back again, every half minute or so. **If after four minutes it still has not been
spoken, stop:** press Command-Shift-F9 over Kontakt, write down how long you tried, and go on to
step 5.

**Should happen:** the overlay's first control is spoken — **"Kontakt file menu, button"**, and
nothing after it. From then on each Tab stop says its name, "button", and its key in the Windows
spelling ("Alt+L" and so on). **Tab** gives:

- on **Kontakt 7**: "Library on/off", "View menu", "Shop", then **"Kontakt controls, Tab to step
  through them"**;
- on **Kontakt 8**: straight to **"Kontakt controls, Tab to step through them"**. Kontakt 8 moved
  the other three into its menus, so two stops of ours are all there is.

The Tab after "Kontakt controls" goes *into* Kontakt, and comes back to "Kontakt file menu" only
when Kontakt's own controls have all been visited.

**If nothing is ever spoken on Kontakt 8:** that is a real answer, not a failure of yours — its
overlay has never met a Kontakt 8 on a Mac. Go straight to the probe press, item 5; its recording
is what the next build is made from.

**Do, in this order:**

1. From "Kontakt file menu", **Tab round once** to "Kontakt controls", and write down the stops
   as you heard them.
2. On **"Kontakt controls"**, press **Tab** again. The keyboard should go *into Kontakt*, and
   **VoiceOver** — not our voice — should announce each of Kontakt's own controls. Keep pressing
   Tab until "Kontakt file menu" comes round again. Write down roughly how many stops there were
   and a few of their names; the log counts them too. "Button" on its own is an expected answer.
   **Do not press Return or Space while inside Kontakt's controls** — nobody knows what its
   unnamed buttons do. If the very first Tab after "Kontakt controls" already says "Kontakt file
   menu", nothing inside Kontakt took the keyboard: write that down, wait ten seconds, and try
   once more.
3. Go to **"Kontakt file menu"** and press **Return**. You hear "Kontakt file menu, activated",
   and Kontakt's file menu should open: use the arrows — **does VoiceOver read the menu's items?**
   Do not choose anything. Press **Escape**, count one, and press **Tab**. If no menu seems to
   open at all, write that down; the log says whether the button was found by name.
4. **Only on Kontakt 7:** go to **"Library on/off"** and press **Return once**. It shows or hides
   Kontakt's preset browser, which nobody can hear. **Do item 5 now** and wait for its summary —
   that photograph is what shows which — then come back and press "Library on/off" once more, to
   put Kontakt back as it was.
5. **Press Command-Shift-F9** over Kontakt (on Kontakt 7 you have just done this in item 4). It
   says "Recording …", then a summary. **Over Kontakt the summary can take up to half a minute.
   Press nothing until you hear it** — not Tab, not Return, not the probe key again: a key pressed
   while it records goes straight to Kontakt. If no summary has come after a minute, write that
   down and carry on. If a system dialog about screen recording appears now after all, allow it,
   write that down, and press Command-Shift-F9 once more.

**Write down:** the window's title; items 1 to 3; for item 3, whether VoiceOver said anything at
all when you pressed the arrows (even just "menu"), and whether Tab after Escape moved on the first
press; the summary the probe spoke.

---

## 5. Kontakt inside REAPER

**Do:** quit the standalone Kontakt from step 4. In REAPER, choose **File → New project tab**, so
the student's own project is not touched. Then **Insert → Virtual instrument on new track**, and
choose Kontakt 7 or Kontakt 8 in the list. REAPER names the track after it, and **the overlay
recognises Kontakt only by that name in the FX window's title** (`FX: Track 1 "Kontakt 8"`) — so
never add it to a track that already exists. If REAPER shows its evaluation reminder first, its
button only works after a few seconds.

With Kontakt's FX window open, press **Command-Shift-F6**. It answers with one of these, and which
one is worth writing down:

- the window's title, then "on" and a control's name — "on an unnamed control" is normal: the
  keyboard was handed to one of Kontakt's own elements;
- the window's title alone: the keyboard was already inside, or a click just inside the panel's
  top-left corner put it there;
- **"could not move the keyboard into …"** or **"could not tell whether the keyboard is in …"**:
  press Command-Shift-F6 once more; if it says the same again, press Command-Shift-F9 over the FX
  window and go on to step 6;
- **"could not find a plugin window"**: the FX window is closed or REAPER did not answer — open it
  and press again.

**Should happen:** the overlay activates, and its only sign is one phrase shortly after F6's own
sentence: **"Load instrument, button"**. The overlay never says its own name, so if you hear F6's
sentence and then nothing, it did not come up.

On **Kontakt 8** that phrase is the whole question of this step: whether a Kontakt 8 inside a DAW
publishes anything a Mac can find. On Windows it publishes nothing at all. If nothing comes up,
write down exactly what F6 said, press **Command-Shift-F9** over the FX window, and move on to
step 6.

**Do, in this order, and only these:**

1. **Tab through all the controls once** and write down what you heard, starting with the very
   first words before you pressed anything. Roughly what to expect, each with "button" and its
   key:
   - **Kontakt 7:** "Load instrument", "Save multi as", "Reset multi", "Instrument editor",
     "Previous instrument", "Next instrument", "Snapshot menu", "Previous snapshot", "Next
     snapshot", "Side pane", "Info pane", "Keyboard panel", "Plugin size" with a width and a
     height, "Increase plugin height", "Decrease plugin height", "Library on/off".
   - **Kontakt 8:** "Load instrument", "Save multi as", "Reset multi", then either "Switch to
     classic view" or "Switch to play view", then "Side pane", "Info pane", "Keyboard panel",
     "Plugin size" — and, after "Switch to play view", the instrument, multi and snapshot controls
     as well.

   **"Increase plugin height" and "Decrease plugin height" are the session's only picture
   search.** After each name listen for either nothing or **"not found"**, and write down which:
   it is the Retina answer, not your mistake.
2. Go to **"Load instrument"** and press **Return**. It opens Kontakt's file menu, finds the row
   that begins with "Load" — by name if the menu publishes names, otherwise **by reading the menu
   off the screen** — and presses it. Kontakt's own file dialog should open; close it with Escape.
   If you hear **"Menu item not found"** instead, do not press Escape — the overlay already has.
   If about two seconds after Return you hear neither a dialog nor that sentence, press Escape
   once yourself and write down "Load instrument: silent".
3. **Tab past everything else without pressing it.** Several of those controls click at points
   measured on Windows, and some of Kontakt's rows put those points close to controls that
   overwrite or delete things.

**Then:** press **Command-Shift-F9** over the FX window and press nothing until its summary. Then
**close that FX window** before step 6 — F6 takes the first FX window it finds, and with two open
it may take Kontakt's.

**Write down:** which Kontakt, F6's sentence, everything from 1 and 2, and whether the overlay
activated at all.

---

## 6. sforzando: the menu, and F6

Two things from last time, both changed since: "Tab doesn't work right after" leaving a menu, and
F6's title being cut off by the control's announcement.

### 6a. sforzando on its own, and its menu

**Do:** open **sforzando standalone**. Coming to the front is what makes the overlay look; you do
not need to click into it. If nothing is said within about ten seconds, **Command-Tab to Finder
and straight back**, as in step 4.

**Should happen:** it says "Instrument, button" and the instrument's value. **Tab** gives
"Polyphony, button" and its value, and Tab again "Pitchbend range, button" and its value. **Note
the value Pitchbend range reads now** — it is the student's, and you will put it back.

**Then:** press **Return** on Pitchbend range, choose a different value the way you normally would,
and press **Return** to commit it. Then count a full second — "one-and" — and press **Tab**. The
keys come back between a third and two-thirds of a second after the Return, so that Tab should
move on at the first press, and say **"Instrument"**: the ring wraps round from Pitchbend range.
If it said nothing, press it once more and tell us the first one was swallowed — the log cannot
show that, only you can.

Last time, the overlay let go of its keys for a fixed eight seconds whenever a menu might be open,
because nothing on that Mac could see sforzando's menu. Now a Return or Escape pressed in the menu
is noticed, and a menu that is a window of its own is seen the moment it opens.

**Then, Escape:** press **Return** on Pitchbend range once more, press **Down** once, and press
**Escape**. Count a full second, press **Tab** (it should say "Instrument"), then **Shift-Tab** back
to Pitchbend range and listen: if Escape cancelled the menu, it still reads the value you chose a
moment ago, not the one below it.

**Then:** press **Command-Shift-F9** over the sforzando window, and press nothing until its summary.

**Finally, put Pitchbend range back** to the value you noted, the same way as before, and quit
sforzando.

**Write down:** the three values read on the first Tab round; whether the Tab a second after
Return moved on at the first press; what Pitchbend range read after Escape.

### 6b. Back into a plugin window with F6

**Do:** in REAPER, in the same new project tab, **Insert → Virtual instrument on new track** and
choose **sforzando**; REAPER names the track after it, and the overlay finds sforzando by that
name in the window's title. Its FX window should be the one VoiceOver reads as `FX: Track 2
"sforzando"`, with REAPER's FX list on its left; if the window that opened has no FX list, close
it and open the track's FX chain instead. Click into the **FX list** and press
**Command-Shift-F6**.

**Should happen:** it says the window's title once the keyboard is inside — `FX: Track 2
"sforzando"`, or whatever number the track has; if it names the Kontakt track instead, that window
was still open: close it and press again — and, about a third of a second later, sometimes more,
the first control of the sforzando overlay. Through the system voice the second line waits for the
first, so **the title should be heard to its end**.

**Write down:** what it said, in order; whether the title was heard to its end or cut off.

---

## 7. Reload — two minutes

**Do:** press **Command-Shift-F5** (with fn if you needed it) — and keep the Shift held.
**Command-F5 on its own is macOS's own switch for turning VoiceOver off.** If you hear "VoiceOver
off", press the same keys again without Shift — with fn too, if you were holding it — to bring it
back, and repeat the step.

**Should happen:** after a moment, a spoken count — "12 modules reloaded", or whatever number this
Mac loads. If a module does not rebuild, the sentence names it: "11 reloaded, 1 failed: Kontakt".
If every one fails it says "Reload failed:" and names them.

**Write down:** the sentence, word for word, or that there was none.

---

## 8. Personal Voice — last, and only with eight minutes left

Last time: "the permissions dialog doesn't show, but the checkbox gets checked. Nothing happens
otherwise." The log then claimed you had refused a dialog you never saw. That sentence is gone.

**Do:** open the menu-bar icon's menu (VO-M twice). If its item says **"Show module manager"**,
choose it. If it says **"Close the module window"**, the manager is already open somewhere: choose
that, open the menu again, and choose "Show module manager". Go to the **Application settings**
tab — the fourth of five — and tick **"Offer my Personal Voice to modules — asks macOS for
permission…"**. It comes straight after "Speak through VoiceOver…": **do not tick that one**, or
anything else on this tab.

**Should happen, one of these:**

- **macOS shows its own permission dialog.** If you allow it, nothing of ours follows; the
  system's dialog is the feedback. If you refuse it, a few sentences follow in the system voice,
  beginning "macOS answered 'denied'" — that is correct, not a fault.
- **A dialog of ours opens, and the application itself says nothing.** VoiceOver reads the
  dialog's text, which has the focus. This is what happens when macOS had already answered before
  we asked. The text says what macOS answered and, if the answer was "denied", where the switch
  for it is (System Settings → Accessibility → Personal Voice).
- **No dialog at all, and a few sentences in a system voice — not VoiceOver's**, beginning either
  "macOS answered 'denied' without showing a dialog" or "macOS reports that this Mac does not
  support Personal Voice". This is the case where macOS was asked for the first time and answered
  without showing a dialog.
- **Nothing at all for thirty seconds** can also be correct: a grant macOS gives without asking.
  The log tells that apart from the old fault.

**Why last:** the request and every spoken line go through one worker thread, in order, and the
request waits up to two minutes for macOS to answer. If a dialog does appear and takes a moment,
everything the application would have said meanwhile waits behind it. At the end of the session
that costs nothing; before the Kontakt steps it would have cost the session.

**Write down:** which one happened and, if it was ours, its first few words — the log keeps the
whole text. If macOS asked and you allowed it, say so: the student should know.

---

## 9. Leaving the Mac as you found it — the last five minutes

Keep these five minutes, whatever else did not get done.

1. **Quit the application** from its menu-bar icon's menu (**Quit**), or `pkill -f
   AutomationPlatform.app` in Terminal.
2. **Then send the files** (see below) — after quitting, so the log is complete, and before item 5,
   because the log lives inside the folder you are about to delete.
3. In **System Settings → Privacy & Security**, remove **AutomationPlatform** (it may be listed as
   **Automation Platform**, with a space) from **Accessibility**, **Screen Recording** (or **Screen
   & System Audio Recording**) and **Input Monitoring**: select it in each list and press the minus
   button. This needs the student's password again. Quicker, in Terminal:
   `tccutil reset All com.automationplatform.app` — if a list still shows it afterwards, remove it
   there.
4. If step 8 granted Personal Voice, tell the student; the switch that governs it is in **System
   Settings → Accessibility → Personal Voice**. Do not spend the five minutes looking for where
   macOS lists the grant.
5. In REAPER, close the new project tab and **choose not to save** when it asks — do not just press
   Return, which may choose Save. Quit Kontakt and sforzando if they are still open. Then delete the
   folder: `rm -rf ~/AutomationPlatform` in Terminal, or drag it to the Bin in Finder. If
   `~/Library/Application Support/AutomationPlatform` exists, delete that too.

---

## What the log answers on its own

Nothing to do. Lines worth knowing about, most of them new:

- **`[host] os macos … — macOS NN.N`** — the version this session ran on.
- **`[env] display 0: …`** with its scale, and **`[env] screen recording: …`** — the screen's
  size in points, its Retina factor, and what macOS reported at each launch in step 3.
- **`[macos] screen capture paths on this macOS: ScreenCaptureKit captureImageInRect present,
  CGWindowListCreateImage …, CGDisplayCreateImageForRect …`** — which ways of photographing the
  screen this macOS still has. If the last two say `absent`, the old way is gone from this
  version, and the new one is all there is.
- **`[macos] screen captures are coming from ScreenCaptureKit captureImageInRect`** — which way
  actually served. Its cost is on **`[macos] first screen capture: … came back at N.NNx and took N
  ms`**; CI Macs running macOS 15.7 and 26.6 said 61–75 ms. **`[macos] a … point capture took N
  ms …`** appears once if a later capture takes 50 ms or more — at four pixels per point, the line
  that says Retina made capturing expensive.
- **`[macos] first capture for reading text: … points came back as … px — 2.00x`** — the Retina
  measurement: the factor every word read off the screen is divided by. It is new in this build;
  the display's own scale in the `[env]` line should agree with it.
- **`'FILE': text centre …, element centre … — off by …`**, under `----- text against elements
  -----` in a probe recording: where a word was read, against where the button with exactly that
  name says it is. It appears only where a word on screen is a button's whole name — Kontakt 7 has
  them (FILE, LIBRARY, VIEW, SHOP). On Kontakt 8, sforzando and Finder the recording says `none of
  the first N distinct word(s) is the name of a button in this window` instead, and that is not a
  fault. A few points apart is normal; a gap that grows the further the word is from the window's
  corner is the Retina mistake this session exists to catch.
- **`[macos] ocr: first recognition of a … pt region took N ms`**, **`… the slowest so far`**, and
  **`[macos] ocr: gave up on a … pt region after N ms …`** — how reading text at Retina size went.
  A "gave up" next to a silent read-out or a "Menu item not found" means the text was too slow to
  read, not absent.
- **`[macos] reading text in the … part of a … region`** — a window hung off the bottom of this
  laptop's screen, and only the part on screen was read. **`not reading text in … : its top or left
  edge is off the desktop`** — nothing in that window could be read.
- **`[macos] ScreenCaptureKit refused a … capture …`** — with the reason macOS gave; usually the
  Screen Recording permission.
- **`[macos] ScreenCaptureKit did not answer …`** — it never did that on the CI Macs. Here it
  would be the most important line in the log.
- **`focus_step(…): the ring has N focusable stop(s)`** — step 4's first step into Kontakt's own
  controls. Its failures read **`nothing in this window accepts keyboard focus`** or **`none of N
  candidate(s) accepted focus`**.
- **`[kontakt] no button named 'Kontakt File Menu' could be pressed — clicking the authored
  file-menu point …`** — step 4 item 3 on Kontakt 8 was a click at a point measured on Windows, not
  the button found by name.
- **`[kontakt] Kontakt 8 panel in 'FX: …': Kontakt File Menu at …`**, or the same with **`Kontakt
  7`** and **`FILE`** — step 5's anchor. **`… publishes no button named …`** or **`… puts …'s panel
  corner at …, outside the window — not anchoring`** — why it did not come up.
- **`Kontakt: no menu item named 'Load...' could be pressed by name — reading the menu instead`**,
  and **`Kontakt: no menu row starting with 'Load...' — read: …`** — step 5's "Load instrument":
  the first, that the menu was read off the screen; the second, every word that reading gave when
  nothing matched.
- **`[gbutton] 'Increase plugin height' …`** — the result of the session's one picture search.
- **`[read] 'Pitchbend range' = "…" (region …, N ms, N word)`** — every sforzando read-out, with the
  text it read.
- **`[overlay] 'sforzando': a window appeared for the plugin while a menu was plausible … treated as
  the menu`** — sforzando's menu is a window of its own. **`… Return went through to the menu; the
  hold ends in 300 ms …`** followed by **`… the plugin's window list did not change …`** — it is
  not.
- **`[daw-hosts] … handed to '…' by asking`**, **`… after a click at content (…)`** or
  **`[daw-hosts] the keyboard was already inside '…'; nothing asked and nothing clicked`** — which
  way F6 took in steps 5 and 6b.
- **`Personal Voice authorisation: … after N ms`** — step 8's answer, including a silent grant.
- **`[env] translocated: YES`**, with **`no modules to load`** further down — macOS ran a read-only
  copy of the `.app`, and nothing else in the session can be judged. In that case the log is
  **not** beside the `.app` but in `~/Library/Application Support/AutomationPlatform/`.

## What to send

- **`automation-platform.log`**, whole, from the same folder as the `.app` — and
  **`automation-platform.log.1`** beside it if there is one: the application moves the log there
  when it starts again after the log has grown large. If there is no log beside the `.app`, send the
  one in `~/Library/Application Support/AutomationPlatform/`.
- **Every `probe-N.png`**, from `modules/probe/`.
- Your notes, with **expected** and **got** where something behaved oddly.
