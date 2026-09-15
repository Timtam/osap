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
  right. Nothing for you to do about it: the log records it. Text is read in steps 3 to 6.
  Pictures are searched for silently whenever Kontakt is in front; the only search you listen for
  is in step 5.
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
VoiceOver key in step 7 too. Press F6 and F5 only the way F9 worked: where F9 needed fn, F6 and F5 without it are Do Not
Disturb and Dictation; where it did not, adding fn is what switches those on. If Do Not Disturb or Dictation
switches on anyway, switch it back off and write it down.

---

## 1. Before the hour starts

**Do:** download the artifact **AutomationPlatform-macos-universal** from the newest **green**
run of the **macOS build** workflow on GitHub, a run with a tick. A red run can still carry a
file, but red can mean the check on macOS 26 failed, and that is probably what this Mac runs. Do
this on your own computer, before the hour: it saves the student's time, and GitHub asks you to
sign in before it hands over the file. Bring it to the MacBook however is quickest — AirDrop, a
USB stick, a shared folder — and keep that way at hand: step 9 takes the files back out the same
way.

**Know before you start:**

- macOS asks for an **administrator password** when you grant the permissions in step 3, and
  again when you take them away in step 9. On a borrowed Mac that is the student's, so have them
  nearby until the permissions are done — about the first quarter of an hour — and for the last
  five minutes.
- **Tell the student what gets sent.** Every probe press saves a picture of the window it is
  pressed over, including anything lying on top of it at that moment, such as a notification. It
  also records the text in that window and the titles of windows underneath it, and the log names
  the Mac's user folder. Ask them to quit mail, messages and their browser before you begin.

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

**How to quit and reopen**, which this step needs more than once: closing the manager window is
**not** quitting — it only hides it, and opening the `.app` while the old process still runs just
brings that process forward, without the new permission. Quit from the menu-bar icon: **VO-M
twice** reaches the menu-bar extras; on Automation Platform's menu choose **Quit**. If you cannot
find the icon, run `pkill -f AutomationPlatform.app` in the Terminal window. To open it again, run
`open AutomationPlatform.app` in that same window. **If System Settings offers "Quit & Reopen"**
after you switch a permission on, choose **Later**, then quit and reopen as described here: this
application turns down the quit that System Settings asks for, so "Quit & Reopen" would leave the
old process running without the permission.

**Do:** open it: `open AutomationPlatform.app` in Terminal.

**Should happen:** up to three things at once. macOS raises its own **Accessibility** dialog, very
likely a second one about **recording the screen** (the application asks for both as it starts),
and, because nothing has been granted yet, our module manager **opens by itself** on the
**Permissions** page and says why, in the **system voice** rather than VoiceOver's. The
announcement ends with: *"Grant Accessibility first: the Screen Recording list shows this
application only afterwards. On newer macOS that list is called Screen and System Audio
Recording."* On each macOS dialog choose **Open System Settings**, not Deny. System Settings then
shows the list of whichever dialog you answered last; before switching anything on, check that
VoiceOver reads it as **Accessibility**. If it does not, the Permissions page has a button **Open
the Accessibility settings**.

**Do, in this order — it is the order that worked last time:**

1. Grant **Accessibility**, then **quit and open the application again**.
2. Grant **Screen Recording**. Newer macOS versions call that list **Screen & System Audio
   Recording**; it is the same thing. If the application is listed as **Automation Platform**,
   with a space, that is it. Then quit and open it once more — macOS hands a new permission only
   to a process that started after it was granted.
3. **Input Monitoring**, only if its row on the Permissions page says it is **NOT granted**. If it
   says it cannot be determined, leave it alone; that is normal. Otherwise open the Input
   Monitoring list. If **Automation Platform** is in it, switch it on. If it is missing, press the
   **+** button, choose `AutomationPlatform.app` in the home folder's AutomationPlatform folder, and
   switch it on. Either way, quit and reopen as in item 1.

Leave **Automation** alone: it belongs to "Speak through VoiceOver", which stays off for this
session, so its row saying it cannot be determined is correct.

**How you know it is finished:** on the launch where nothing is missing, the manager does **not**
open by itself, and the system voice says only *"Automation Platform is running in the menu bar.
Open its menu to manage modules."* If the manager opens by itself again instead, its sentence
names what is still missing. **If it names Screen Recording again after you have switched it on
and reopened once, do not go round a second time:** write that down and take the photograph
below. macOS's own check has been caught saying no for an application that could capture
perfectly well, and the photograph's word count tells the two apart. If that reads more than a
handful of words, capturing works; go on. If it reads almost none, write that down and go on to
step 4 anyway.

**Then, the first photograph of the screen, on purpose.** In Terminal, type `open .` and press
Return: a Finder window of our own folder comes to the front — not one of the student's. Press
**Command-Shift-F9**. It says "Recording. This can take a few seconds on a large plugin", then a
summary: "Probe N written. … surfaces, … accessibility elements, … words." **Press nothing until
that summary has been spoken** — while it records, the application cannot look at the keyboard.
A few seconds after the summary has been spoken, a window titled "Module error: com.tool.probe"
opens and takes the keyboard, as it did last time. **Wait for it before pressing anything else.**
Escape closes it, and there is nothing to answer about it; it comes only after the first
Command-Shift-F9 of each launch. Closing it can leave our application in front with no window,
so **Command-Tab to Finder** before you press anything else.

That press does three jobs at once:

- **The F-keys.** If nothing happens, press Command-Shift-**fn**-F9 (see "The keys"). If it is
  silent **both** ways, do not blame the keyboard yet: open the manager from the menu-bar icon
  (**Show module manager**; if the item says **Close the module window** instead, the manager is
  already open: choose that, open the menu again and choose **Show module manager**), go to its
  **Installed** tab, and listen to the list of installed modules. If it is empty, macOS is
  running a read-only copy after all: quit, run the `xattr` command again, and reopen. If you
  cannot find the menu-bar icon, run `ls ~/AutomationPlatform/automation-platform.log` in Terminal
  instead: if it answers "No such file or directory", it is the read-only copy — quit with
  `pkill -f AutomationPlatform.app`, run the `xattr` command again, and reopen. If F9 says *"Nothing
  is focused, so there is nothing to probe"* **while Finder is in front**, the key arrived but
  Accessibility is not in effect yet: quit and reopen. **After any of these reopens**, bring the
  Finder window forward again and press Command-Shift-F9 once more, as above, so that its summary
  and the error window come here and not over Kontakt.
- **Whether the photograph saw anything.** Its last number is the word count. A picture of a bare
  desktop reads almost no words.
- **A dialog of macOS's own**, about an application recording the screen or about bypassing a
  "window picker". The application now photographs the screen the newer way (ScreenCaptureKit),
  and nobody here has seen which wording this macOS uses, or whether it asks at all — better now
  than in the middle of Kontakt. If one appears, **allow it**; then, once the error window has come
  and gone and Finder is in front again, press Command-Shift-F9 once more. On some newer versions
  that dialog has been reported coming back later, even after Allow; if it does, allow it again
  each time, and count how often.

**Write down:**

1. When did the application first appear in the Screen Recording list — already at the first
   launch, straight after granting Accessibility, only after opening it again, or never?
2. Any system dialog about recording the screen: its wording, as near as you can, and what you
   had just done when it appeared.
3. Anything the Permissions page said that sounded wrong for this Mac, and whether it still named
   Screen Recording after you had reopened.
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
   Tab until "Kontakt file menu" comes round again; that is the only way back to our controls
   (Shift-Tab walks the same round backwards, and Command-Tab away and back puts you where you
   were). You need not let VoiceOver finish each name: press Tab about once a second, write down
   the first five names you catch and a rough count — the log records the exact number. "Button"
   on its own is an expected answer. **Do not press Return or Space while inside Kontakt's
   controls** — nobody knows what its unnamed buttons do. **If it has not come round after two
   minutes, stop pressing Tab**, write down "did not come round", skip items 3 and 4 and go to
   item 5. If the very first Tab after "Kontakt controls" already says "Kontakt file menu",
   nothing inside Kontakt took the keyboard: write that down, wait ten seconds, press **Shift-Tab**
   once (it should say "Kontakt controls" again) and press Tab once more.
3. Go to **"Kontakt file menu"** and press **Return**. You hear "Kontakt file menu, activated",
   and Kontakt's file menu should open: use the arrows — **does VoiceOver read the menu's items?**
   Do not choose anything. Press **Escape**, count one, and press **Tab**: moving on sounds like
   "Library on/off, button, Alt+L" on Kontakt 7, or "Kontakt controls, Tab to step through them" on
   Kontakt 8. If no menu seems to open at all, write that down; the log says whether the button
   was found by name.
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
the student's own project is not touched. If REAPER shows its evaluation reminder first, its
button only works after a few seconds. Then **Insert → Virtual instrument on new track**, and
choose Kontakt 7 or Kontakt 8 in the list. If it is listed more than once (AU, VST3, VST), take the
**VST3** one, and write the entry down exactly as VoiceOver read it. **If REAPER then asks whether
to add tracks or build routing for the instrument's outputs, choose No** — do not just press
Return: Yes adds a dozen or more tracks to the tab. If Yes was already chosen, leave it and carry
on (the track numbers in 6b will then differ, and the tab is closed unsaved in step 9). Write down
that it asked.

REAPER names the track after the plugin, and **the overlay recognises Kontakt only by that name in
the FX window's title** — so never add it to a track that already exists. The window must be the
one VoiceOver reads as `FX: Track 1 "Kontakt 8"` (or "Kontakt 7"), with REAPER's FX list on its
left. If the window that opened has no FX list, or its title does not begin with "FX:", close it
and open the track's FX chain instead; do not change REAPER's preferences. If the title does not
contain **Kontakt 8** (or Kontakt 7) anywhere, rename the track — but not from inside the FX
window, where F2 renames the plugin and the title stays as it was. Press **Escape** to close the FX
window, then in REAPER's main window press **F2** (with fn if F9 needed fn) to rename the track you
just made, type exactly **Kontakt 8** (or Kontakt 7), press Return, and open that track's FX chain
again. Write down that you had to.

With that FX window open, press **Command-Shift-F6**. It answers with one of these, and which one
is worth writing down:

- the window's title, then "on" and a control's name — "on an unnamed control" is normal: the
  keyboard was handed to one of Kontakt's own elements;
- the window's title alone: the keyboard was already inside, or nothing in Kontakt took it and the
  application clicked just inside the panel's top-left corner. On Kontakt that corner is its logo
  button: if VoiceOver then reads anything new over Kontakt (an About screen, a splash, a menu),
  press Escape once and write down what it read, before anything else in this step;
- **"could not switch to …"**, **"could not move the keyboard into …"** or **"could not tell whether
  the keyboard is in …"**: count five seconds and press Command-Shift-F6 once more; if it says the
  same again, press Command-Shift-F9 over the FX window and go on to step 6;
- **"could not find a plugin window"**: no window titled `FX: …` is open (a plugin window without
  REAPER's FX list does not count), or REAPER did not answer for a moment. Open the track's FX
  chain, wait five seconds, and press again.

**Should happen:** the overlay activates, and its only sign is one phrase: **"Load instrument,
button"**. The overlay never says its own name, and it says that phrase only once, when it comes
up — which can be before F6, as the FX window opens, or after an earlier F6 press. So if you hear
F6's sentence and then nothing, think back: if the phrase came earlier, press **Tab** once and
listen for **"Save multi as"** in our voice. If you hear it, the overlay is up. If the phrase never
came, or that Tab brings VoiceOver instead, it did not come up.

On **Kontakt 8** that phrase is the whole question of this step: whether a Kontakt 8 inside a DAW
publishes anything a Mac can find. On Windows it publishes nothing at all. If nothing comes up,
write down exactly what F6 said, press **Command-Shift-F9** over the FX window, and move on to
step 6.

**Do, in this order, and only these:**

1. **Tab through all the controls once** and write down what you heard, starting with the very
   first words before you pressed anything. Most stops say their name, "button" and a key. Some
   put a state between the name and "button", such as "closed, press to open" or "collapsed, press
   to expand". "Plugin size" says its width and height, and "at maximum height" after them when
   the window already reaches near the bottom of the screen. Roughly what to expect:
   - **Kontakt 7:** "Load instrument", "Save multi as", "Reset multi", "Instrument editor",
     "Previous instrument", "Next instrument", "Snapshot menu", "Previous snapshot", "Next
     snapshot", "Side pane", "Info pane", "Keyboard panel", "Plugin size", "Increase plugin
     height", "Decrease plugin height", "Library on/off".
   - **Kontakt 8:** "Load instrument", "Save multi as", "Reset multi", then one of two lists.
     Either "Switch to classic view", "Side pane", "Info pane", "Keyboard panel", "Plugin size".
     Or "Switch to play view", "Instrument editor", "Previous instrument", "Next instrument",
     "Previous multi", "Next multi", "Snapshot menu", "Previous snapshot", "Next snapshot", "Side
     pane", "Info pane", "Keyboard panel", "Plugin size", "Increase plugin height", "Decrease plugin
     height". If Kontakt's instrument editor happens to be open, the seven instrument, multi and
     snapshot stops are missing, and "Instrument buses", "Insert effects", "Send effects", "Main
     effects", "Modulation" come at the end instead.

   **"Increase plugin height" and "Decrease plugin height" are the picture search you listen
   for** — on Kontakt 7 always, on Kontakt 8 only after "Switch to play view" — and so are the five
   editor sections, if you hear them. After each of these names listen for nothing, for **"not
   found"**, or, after "Increase plugin height" only, for **"at maximum height, unavailable"**
   (likely on a laptop screen: the window already reaches near the bottom, so no search was made).
   After an editor section, **"collapsed, press to expand"** or **"expanded, press to
   collapse"** means it was found. Write down which you heard. None of these answers is your
   mistake.
2. Go to **"Load instrument"** and press **Return**. It opens Kontakt's file menu, **reads the menu
   off the screen**, finds the row that begins with "Load", and presses it. Kontakt's own file
   dialog should open; close it with Escape. If you hear **"Menu item not found"** instead, do not
   press Escape — the overlay already has. **After Return, press nothing at all for a full five
   seconds**: reading the menu has never been timed on this screen, and a key pressed during it can
   make the overlay click where the menu was. Only if by then you have heard neither a dialog nor
   that sentence, press Escape once and write down "Load instrument: silent". If a dialog or "Menu
   item not found" still comes after that Escape, write down what you heard and in which order;
   press Escape once more only if VoiceOver says a dialog is open.
3. **Tab past everything else without pressing it.** Several of those controls click at points
   measured on Windows, and some of Kontakt's rows put those points close to controls that
   overwrite or delete things.

**Then:** press **Command-Shift-F9** over the FX window and press nothing until its summary. Then
**close that FX window** before step 6 — F6 takes the first FX window it finds, and with two open
it may take Kontakt's.

**Write down:** which Kontakt and the plugin entry you chose, F6's sentence, everything from 1 and
2, and whether the overlay activated at all.

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
the value Pitchbend range reads now** — it is the student's, and you will put it back. If it reads
"no text", or a value you could not pick again in its menu, write down what it read and leave
Pitchbend range alone: do everything below on **Polyphony** instead. Press Return on Polyphony, and
the Tab a second later should say "Pitchbend range", not "Instrument". For Escape, Shift-Tab once
to Polyphony, and after the Tab, Shift-Tab back to Polyphony. At the end, put Polyphony back to the
value it read.

**Then:** press **Return** on Pitchbend range, choose a different value the way you normally would,
and press **Return** to commit it. Then count a full second — "one-and" — and press **Tab**. The
keys come back within about two-thirds of a second of the Return, so that Tab should move on at
the first press, and say **"Instrument"**: the ring wraps round from Pitchbend range. If it said
nothing, press it once more and tell us the first one was swallowed — the log may say why, but
only you can say that it happened.

Last time, the overlay let go of its keys for a fixed eight seconds whenever a menu might be open,
because nothing on that Mac could see sforzando's menu. Now a Return or Escape pressed in the menu
is noticed, and a menu that is a window of its own is seen the moment it opens.

**Then, Escape:** press **Shift-Tab** once; it should say "Pitchbend range". Press **Return** on it,
press **Down** once, and press **Escape**. Count a full second, press **Tab** (it should say
"Instrument"), then **Shift-Tab** back to Pitchbend range and listen: if Escape cancelled the menu,
it still reads the value you chose a moment ago, not the one below it. If it reads anything else,
"no text", or nothing, press Escape once more and write down what it read.

**Then:** press **Command-Shift-F9** over the sforzando window, and press nothing until its summary.

**Finally, put Pitchbend range back** to the value you noted, the same way as before, and quit
sforzando.

**Write down:** the three values read on the first Tab round; whether the Tab a second after
Return moved on at the first press; what Pitchbend range read after Escape.

### 6b. Back into a plugin window with F6

**Do:** in REAPER, in the new project tab from step 5 (if there is none, choose **File → New project
tab** first), **Insert → Virtual instrument on new track** and choose **sforzando** (the VST3 entry,
if there are several; write down which). Never add it to a track that already exists: REAPER names
the track after it, and the overlay finds sforzando by that name in the window's title. Its FX
window should be the one VoiceOver reads as `FX: Track 2 "sforzando"` (Track 1 if you skipped
step 5), with REAPER's FX list on its left; if the window that opened has no FX list, close it and
open the track's FX chain instead. Move VoiceOver to REAPER's **FX list** on the left of that window
and press **VO-Space once** — once, not twice: a double-click opens sforzando in a window of its
own. Then press **Command-Shift-F6**.

**Should happen:** it says the window's title once the keyboard is inside — `FX: Track 2
"sforzando"`, or whatever number the track has; if it names the Kontakt track instead, that window
was still open: close it and press again — and, about a third of a second later, sometimes more,
the sforzando overlay's first control, the same words step 6a began with: **"Instrument, button"**
and the instrument's value. Through the system voice the second line waits for the first, so **the
title should be heard to its end**.

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

**If you could not reach the menu-bar icon in step 3, skip this step:** on a Mac nothing else
opens the manager. Write down that you skipped it and go to step 9.

Last time: "the permissions dialog doesn't show, but the checkbox gets checked. Nothing happens
otherwise." The log then claimed you had refused a dialog you never saw. That sentence is gone.

**Do:** open the menu-bar icon's menu (VO-M twice). If its item says **"Show module manager"**,
choose it. If it says **"Close the module window"**, the manager is already open somewhere: choose
that, open the menu again, and choose "Show module manager". Go to the **Application settings**
tab — the fourth of five — and tick **"Offer my Personal Voice to modules — asks macOS for
permission…"**. It is the seventh of eight checkboxes. Each checkbox is followed by a paragraph
explaining it, so after "Speak through VoiceOver…" you first hear that switch's paragraph, and the
next checkbox is this one. **Do not tick "Speak through VoiceOver…"**, or anything else on this
tab.

**Should happen, one of these:**

- **macOS shows its own permission dialog.** If you allow it, nothing of ours follows; the
  system's dialog is the feedback. If you refuse it, a few sentences follow in the system voice,
  beginning "macOS answered 'denied' without showing a dialog". The words "without showing a
  dialog" are wrong here, and that is known; it is not the old fault. Write down that you saw
  macOS's dialog and refused it: the log gives the same lines as in the third case below, so only
  your note tells them apart.
- **A dialog of ours opens, and the application itself says nothing.** VoiceOver reads the
  dialog's text, which has the focus. This is what happens when macOS had already answered before
  we asked. The text says what macOS answered and, if the answer was "denied", where the switch
  for it is (System Settings → Accessibility → Personal Voice).
- **No dialog at all, and a few sentences in a system voice — not VoiceOver's**, beginning either
  "macOS answered 'denied' without showing a dialog" or "macOS reports that this Mac does not
  support Personal Voice". This is the case where macOS was asked for the first time and answered
  without showing a dialog — but only if no dialog of macOS's came before those sentences.
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
2. **Copy the files off this Mac** (see "What to send") the way the artifact came in — AirDrop, the
   USB stick or the shared folder, not the student's mail. The simplest way is to copy the whole
   `AutomationPlatform` folder from the home folder: it holds the log and every probe picture. Then
   run `open ~/Library/Application\ Support/AutomationPlatform` in Terminal; if a Finder window
   opens, copy that folder too. **Check on your own device that the copy arrived** before item 6
   deletes it.
3. In **System Settings → Privacy & Security**, remove **AutomationPlatform** (it may be listed as
   **Automation Platform**, with a space) from **Accessibility**, **Screen Recording** (or **Screen
   & System Audio Recording**) and **Input Monitoring**: select it in each list and press the minus
   button. This needs the student's password again. Quicker, in Terminal — and **before** item 6,
   because the command only finds the application while it still exists — run exactly this line,
   the name at the end included:

   ```bash
   tccutil reset All com.automationplatform.app
   ```

   **Never press Return on a shorter line:** `tccutil reset All` without the name at the end takes
   every permission away from every application on the student's Mac, without asking. Read the
   line back (VO-L) before pressing Return. If Terminal answers with an error, or a list still
   shows the application afterwards, remove it in that list as above.
4. If step 8 granted Personal Voice, tell the student; the switch that governs it is in **System
   Settings → Accessibility → Personal Voice**. Do not spend the five minutes looking for where
   macOS lists the grant.
5. In REAPER, check that the project in front is the one from step 5: **VO-F2** reads REAPER's
   window title, and that project was never saved, so the title should not name a saved project.
   If it does name one, that is the student's: leave REAPER as it is and tell the student there is
   an extra unsaved tab with the Kontakt and sforzando tracks, which they can close without saving.
   Otherwise choose **File → Close project**, and when it asks whether to save, choose **not to
   save** — do not just press Return, which may choose Save. Quit Kontakt and sforzando if they are
   still open.
6. **Delete the folder.** In Finder press Command-Shift-H, type `Automation` so that the
   `AutomationPlatform` folder is selected, make sure VoiceOver says that name, and press
   **Command-Delete**: it goes to the Bin, from which the student can get it back — tell them.
   If Finder does not cooperate, type exactly this in Terminal instead:

   ```bash
   cd ~ && rm -rf AutomationPlatform
   ```

   Never type `rm -rf` directly before a `~`: one stray space there deletes the whole home folder.
   If step 2's check found the Application Support folder, remove it too, exactly like this:

   ```bash
   cd ~/Library/"Application Support" && rm -rf AutomationPlatform
   ```

   Optionally, `defaults delete com.automationplatform.app` removes the little preferences file macOS
   keeps for the menu-bar icon; it is fine if it says the domain does not exist.

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
  ms`**. CI Macs running macOS 15.7 and 26.6, without Retina, have said anywhere from 61 to 149 ms,
  a different number each run, so one slow first capture alone does not mean Retina made it slow.
  **`[macos] a … point capture took N ms …`** appears once if a later capture takes 50 ms or more.
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
  A "gave up" next to a sforzando read-out that said "no text" (its `[read]` line shows `= ""`)
  means something was drawn in that field, two
  readings found no text in it, and the slower readings were skipped so the keys would not stall;
  the value may still have been there, and the probe picture from step 6a shows the field.
- **`[macos] reading text in the … part of a … region at …; the rest hangs off the right or bottom
  edge`** — once per launch: a region being read, often a window on this laptop's shorter screen,
  was partly off screen, and only the part on screen was read; later clipped reads leave no second
  line. **`not reading text in … at …: its top or left edge is off the desktop`** or **`…: it is not
  on the desktop`** — that one read returned nothing.
- **`[macos] ScreenCaptureKit refused a … capture …`** — with the reason macOS gave; usually the
  Screen Recording permission.
- **`[macos] ScreenCaptureKit did not answer …`** — it never did that on the CI Macs. Here it
  would be the most important line in the log.
- **`[macos] the system disabled the event tap (…) — re-enabled (#N)`** or **`[macos] the event tap
  had been switched off and the watchdog found it …`** — for a moment the overlay was not catching
  keys, and they went straight to the application in front. A Tab, Return or arrow key that did
  nothing shortly before one of these lines is explained by it. Command-Shift-F5, F6 and F9 are not
  affected.
- **`focus_step(…): the ring has N focusable stop(s)`** — step 4's first step into Kontakt's own
  controls. Its failures read **`nothing in this window accepts keyboard focus`** or **`none of N
  candidate(s) accepted focus`**.
- **`[kontakt] no button named '…' could be pressed — clicking the authored file-menu point …`** —
  the file menu was opened by a click, not by pressing the button by name; the name in quotes is
  `Kontakt File Menu` on Kontakt 8 and `FILE` on Kontakt 7. In step 4 item 3 the click went to a
  point measured on Windows. **In step 5 this line is expected on every "Load instrument" press**:
  nothing inside REAPER's FX window is named after Kontakt, so the menu is opened by a click at the
  anchored spot, which is the button's own centre, and then read off the screen without saying so.
- **`[kontakt] Kontakt 8 panel in 'FX: …': Kontakt File Menu at …`**, or the same with **`Kontakt
  7`** and **`FILE`** — step 5's anchor. **`… publishes no button named …`** — why it did not come
  up. **`… puts …'s panel corner at …, outside the window — not anchoring`** — the button's
  position and the window's frame disagree.
- **`Kontakt: no menu row starting with 'Load...' — read: …`** — step 5's "Load instrument" read
  the menu off the screen and found no row starting with "Load"; after "read:" comes every word it
  read. When a row matches, no line is written. **`Kontakt: no menu item named 'Load...' could be
  pressed by name — reading the menu instead`** should not appear inside REAPER.
- **`[gbutton] 'Increase plugin height' …`** — the result of the picture search you listened for in
  step 5, one line per arrival.
- **`[read] 'Pitchbend range' = "…" (region …, N ms, N word)`** — every sforzando read-out, with the
  text it read.
- **`[overlay] 'sforzando': a window appeared for the plugin while a menu was plausible … treated as
  the menu`** — sforzando's menu is a window of its own. **`… Return went through to the menu; the
  hold ends in 300 ms …`** followed by **`… the plugin's window list did not change …`** — it is
  not.
- **`[daw-hosts] keyboard was on the host's chrome after focusing '…'; handed to '…' by asking, it
  is now: …`** or **`… after a click at content (…) it is: …`** — which way F6 took in steps 5 and
  6b; the last word says whether it worked (`inside` is success). **`[daw-hosts] the keyboard was
  already inside '…'; nothing asked and nothing clicked`** — it was already there. **`[daw-hosts]
  the focus chain after focusing '…' was empty or belonged elsewhere; not clicking`** — F6's "could
  not tell whether the keyboard is in …" before anything was tried.
- **`[speech] Personal Voice was switched on — asking macOS…`**, then **`Personal Voice
  authorisation: granted after N ms`** or **`… not granted after N ms`** — step 8's answer,
  including a silent grant. An `asking` line with no `authorisation` line after it means the
  application was quit before macOS answered. **`Personal Voice: …`** with no `asking` line before
  it — macOS had answered before we asked, so nothing was asked.
- **`[env] translocated: YES`**, with **`no modules to load`** further down — macOS ran a read-only
  copy of the `.app`, and nothing else in that launch can be judged. That launch's log is **not**
  beside the `.app` but in `~/Library/Application Support/AutomationPlatform/`; later launches
  still write beside the `.app`.

## What to send

- **`automation-platform.log`**, whole, from the same folder as the `.app` — and
  **`automation-platform.log.1`** beside it if there is one: the application moves the log there
  when it starts again after the log has grown large.
- If **`~/Library/Application Support/AutomationPlatform/`** exists, the `automation-platform.log`
  in it as well, **even when there is one beside the `.app`**: a launch macOS ran from its
  read-only copy wrote its log there.
- **Every `probe-N.png`**, from `modules/probe/`.
- Your notes, with **expected** and **got** where something behaved oddly.
