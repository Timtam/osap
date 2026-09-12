---
title: Test protocol — fourth macOS session
---

# Test protocol: the fourth macOS session

Written for the **Mac mini** you used last time — macOS 14.5, Apple silicon, REAPER and
Kontakt 7 installed — and for the GitHub artifact, since that Mac has no Xcode. If you are on
the old Air instead, see [the last section](#if-you-are-on-the-old-air).

Each step has **what to do**, **what should happen**, and **what to write down**. "It said
nothing" and "it said the wrong thing" are different faults with different causes, and the
difference is invisible from here.

**If your time is short once it is set up:** step 3 and step 4, and count on half an hour for
the two rather than twenty minutes — last time the sforzando part took ten minutes on its
own, and step 4 has sixteen controls to hear and two to press on top of getting Kontakt into
REAPER. Everything else can wait for another day; those two cannot be answered without you.

Nearly everything in this protocol exists because of what you reported last time. Where a step
checks one of those things, it says so.

## The keys

| What | Keys |
| --- | --- |
| Record the window in front (the probe) | **Command-Shift-F9** |
| Move between an overlay's controls | **Tab**, and **Shift-Tab** backwards |
| Put the keyboard back into a plugin window | **Command-Shift-F6** |
| Reload every module | **Command-Shift-F5** — keep Shift; Command-F5 alone switches VoiceOver off |

Two of these changed. Last time the last two were Control-Shift-Command-Option, and you
reported that both beeped and did nothing. Control-Option together is VoiceOver's own
modifier: on a Mac that keeps the default VoiceOver setting, VoiceOver takes those chords
before any application sees them. The probe's key was already Command-Shift and arrived every
time — four presses, four arrivals in your log — so the other two are now the same shape: an
F-key under Command-Shift, exactly as F9 was. Press them the way you pressed Command-Shift-F9
last time; nothing about that Mac's keyboard settings needs changing, because F9 already got
through them. If either of the new chords beeps or does nothing, write that down — the log
cannot hear a beep.

---

## 1. Setting up

**Do:** download the artifact **AutomationPlatform-macos-universal** from the newest **green**
run of the **macOS build** workflow on GitHub — a red or cancelled run leaves a file behind
too, and that one is missing what this session is about, so take a run with a tick. It is a
zip inside a zip: unzip twice, until you have a folder called `AutomationPlatform` holding the
`.app`, a `modules` folder and a `README.txt`. Move that whole folder somewhere you can write
to.

`README.txt` beside the `.app` has one command worth running before you open anything:
`xattr -dr com.apple.quarantine AutomationPlatform.app`. Without it macOS may run the `.app`
from a hidden read-only copy of itself, where the `modules` folder is not there — every
overlay would then be silently absent, which sounds exactly like "nothing works". The log says
`translocated: YES` when that has happened, and `no modules to load` underneath it.

**Know before you start:** every download is a **new application** as far as macOS is
concerned, so the permissions have to be granted again. That is not a bug — the artifact
carries no certificate, and we chose not to change that before this session — but it means
step 2 happens on every new build, not once.

**Write down:** nothing, unless something stopped you.

---

## 2. The permissions, in the order that works

**Do:** open `AutomationPlatform.app`.

**Should happen, in this order:** macOS raises its own **Accessibility** dialog. The
application asks for **Screen Recording** at the same moment, but whether macOS shows a dialog
for it on this first launch is not known — last time the application was not in the Screen
Recording list until Accessibility had been granted, which says the request did not take while
Accessibility was still pending. Then, because nothing has been granted yet, our module manager
**opens by itself** on the **Permissions** page and says why.

That announcement has one new sentence, because of what you found. When both are missing it
now ends with: *"Grant Accessibility first: the Screen Recording list shows this application
only afterwards."* The Screen Recording row on the page says the same thing at its end. The
announcement comes from the **system voice**, not from VoiceOver — the switch that sends speech
through VoiceOver is off in a fresh folder — so listen for that voice underneath whatever
VoiceOver is saying about the window that has just opened.

**Do:** grant Accessibility, then **quit and open the application again** before you look for
Screen Recording: the application asks macOS for Screen Recording once per launch, and the
launch that starts with Accessibility already granted is the one that put it into that list
last time. Then grant Screen Recording, and quit and open the application once more — macOS
hands a new permission only to a process that started after it was granted. If the Screen
Recording list still does not show the application after that relaunch, write it down and stop
there: the button on the page opens the pane but cannot ask again within the same launch, and
there is nothing further to try.

**Write down:**

1. **The last thing the announcement said, as near to word for word as you can.** "It stopped
   before the sentence about Accessibility" and "I could not hear the end, VoiceOver was
   reading the window" are different answers, and both are useful.
2. **Quote one row of the Permissions page back to us, word for word as VoiceOver read it.**
   Last time the answer to this question was "yes", which told us nothing; the actual words
   are what we need, because nobody here has ever heard that page.
3. When did the application appear in the Screen Recording list — straight after granting
   Accessibility, or only after the application had been opened again, or never?
4. Did a system dialog about **Screen Recording** appear at all, and on which launch — the
   first, or the one after Accessibility was granted? The log cannot tell: the application
   only learns yes or no, not whether a dialog was shown.

Only three of the four rows matter today: **Accessibility**, **Screen Recording** and **Input
Monitoring**. The fourth, **Automation**, is asked for only by ticking "Speak through
VoiceOver", which this session deliberately leaves off — so its row saying it cannot be
determined is correct, and there is nothing to do about it.

---

## 3. sforzando, and the menu — the fix for "Tab doesn't work right after"

**Do:** open **sforzando standalone**, click into its window so the overlay activates.

**Should happen:** it names "Instrument" and reads its value. **Tab** gives "Polyphony", Tab
again "Pitchbend range".

**Then:** press **Return** on Pitchbend range, choose a value the way you normally would, and
press **Return** to commit it. Then, after about half a second — count one — press **Tab**. If
that Tab said nothing, press it once more a second later, and tell us the first one was
swallowed: that gap is the measurement.

Last time you reported two things about this menu: the arrows stopped tracking after a while,
and after leaving the menu "Tab doesn't work immediately". Both were the same eight seconds —
the overlay let go of its keys for a fixed time because nothing on that Mac could see
sforzando's menu, and took them back only when the time ran out, whether the menu was still
open or not. Two things changed:

- A Return or Escape pressed while the menu is up is now noticed, and the keys come back about
  half a second after it — never sooner than a third of a second, because the overlay looks
  only every 150 ms and needs two looks. A Tab inside that half second still goes to
  sforzando, on purpose, so the menu has time to close; that is why the step says to count
  one first. So **Tab about half a second after Return should move to the next control**,
  where last time it took up to eight seconds.
- The overlay now also looks at the plugin's **windows**: a menu drawn as a window of its own
  is seen the moment it opens, and its going ends the hold at once. Whether sforzando's menu
  is such a window is not known — the log will say, and that is a measurement in itself.

If it is not such a window, the eight seconds *inside* the menu are as they were: a Return
pressed after more than eight seconds in the menu re-fires the control and reopens the menu
instead of committing — that is what "the arrows don't track after 8 seconds" was — and the
first change cannot help there, because it only shortens the wait *after* a Return that reached
the menu. So that half is fixed only if something can now see the menu, and this session is
where that gets found out.

**Then, once more, slowly:** press **Return** on Pitchbend range again, arrow to a different
value, and before pressing Return count a slow ten seconds — past the eight, on purpose. Press
**Return**, then **Tab** away and **Shift-Tab** back to Pitchbend range; it reads its value. Is it
the one you chose?

**Then, Escape:** press **Return** on Pitchbend range once more, press **Down** once so the
highlight moves off the current value, and press **Escape**. Count one, press **Tab**, then
**Shift-Tab** back to Pitchbend range and listen to what it reads — if Escape cancelled the
menu, it still reads the value from before.

**Also new:** what a read-out actually read is now in the log. If "Pitchbend range" ever says
"550." again where it should say DEF, the log will hold the text and the exact region, which
last time it did not.

**Write down:** after the first Return, did the Tab about half a second later move to the next
control — on the first press, or only on the second? After the slow ten seconds: did the value
you chose stick, or did the menu reopen? After Escape: what did Pitchbend range read when you
came back to it, and did Tab move on the first press?

**Then, with Pitchbend range set to 1:** press **Command-Shift-F9** over the sforzando window.
It says "Recording. This can take a few seconds on a large plugin", then a summary. About three
seconds after the summary, the window titled "Module error: com.tool.probe" opens and takes the
keyboard, as it did last time; VoiceOver reads it, and Escape closes it. There is nothing to
answer about it this session. It comes **once per launch of the application, on the first press
only** — the press in step 4 will not raise it again, and the log says so.

---

## 4. Kontakt 7 inside REAPER — the first Kontakt overlay on a Mac

**This is the most valuable thing in the session.** Last time the probe told us that Kontakt
inside REAPER publishes its buttons by name — FILE, LIBRARY, VIEW, SHOP — and that they sit at
exactly the offsets the Windows version already uses. This session finds out whether the
overlay built on that works.

**Do:** in REAPER, put **Kontakt 7** on a track and open its FX window. Then get the keyboard
into Kontakt's panel with **Command-Shift-F6**. It focuses the plugin window and, if the
keyboard stays on REAPER's FX list, **asks** the first of Kontakt's own controls that will take
it — Kontakt publishes its header buttons and a search field. If one of them takes the
keyboard, nothing is clicked, and about a quarter of a second later it says the window's title,
then "on" and the name of that control: `FX: Track 1 "Kontakt 7", on Search`, or whichever
control it was. That control may have no name of its own — the first thing inside Kontakt's
panel is its logo button, which publishes none — so **"on an unnamed control" is a normal
answer here, not a fault**; write it down and carry on. If none of them takes it, F6 falls back to the click it has always made —
three points inside the panel's top-left corner, which in Kontakt 7 is Kontakt's own logo
button — and then says the window's title alone. (For sforzando, which publishes nothing that
can be asked, the click is the only path.)

If F6 does nothing, click into the panel with VOCR the way you did for sforzando, **then press
Command-Shift-F6 once more**. A click can move the keyboard without anything telling us; the
second press notices the keyboard is already inside and asks every overlay to look again,
which is the only thing that wakes one. (Command-Tab away and back does the same, if the
second press says nothing.) Only then press Tab.

**Should happen:** the overlay activates. Its only sign is one phrase, shortly after F6's own
sentence: **"Load instrument, button"** — the first of Kontakt's header controls, where the
keyboard arrives. The overlay never says its own name, so that phrase is the whole
announcement; if you hear F6's sentence and then nothing, the overlay did not come up. **Tab**
then walks the rest of the header, each stop spoken as its name, then "button", then its key in
the Windows spelling: "Save multi as", "Reset multi", "Instrument editor", "Previous
instrument", "Next instrument", "Snapshot menu", "Previous snapshot", "Next snapshot", "Side
pane", "Info pane", "Keyboard panel", "Plugin size", "Increase plugin height", "Decrease plugin
height", "Library on/off", and "Load instrument" comes round again at the end. Not every one of
those will work on a Mac — several were measured on Windows only — and the point of this step
is to learn which.

**Do, in this order, and only these:**

1. **Tab through all of them once** and write down what you heard, starting with the very first
   words before you pressed anything, and including any control that spoke a value ("Plugin
   size" should say a width and a height; "Side pane", "Info pane" and "Keyboard panel" may say
   a state or nothing).
2. Go to **"Library on/off"** and press **Return**. It clicks Kontakt's LIBRARY button, which on
   Windows opens or closes Kontakt's library browser on the left. You cannot hear the browser,
   and the control says "Library on/off, activated" whether or not anything moved — it does
   not look afterwards. What the overlay can read is the panel's size: **before** you press it,
   Tab to "Plugin size" and write down the width it says; **after** the press, go back to
   "Plugin size" and write the width down again. On Windows the browser changes the width by
   several hundred pixels; whether it does on a Mac is one of the things this step measures.
   Do not press it a second time either way — the probe picture at the end of this step shows
   the browser's state and settles it.
3. Go to **"Load instrument"** and press **Return**. It clicks Kontakt's FILE button, waits a
   quarter of a second, reads the screen below the button, and clicks the row that begins with
   "Load". Kontakt's own file dialog should open; close it with Escape. If instead you hear
   **"Menu item not found"**, say so — and do not press Escape then, the overlay has already
   pressed it. That sentence means only that nothing beginning with "Load" was read below the
   button: it cannot tell whether the click opened the menu at all, whether the menu was still
   appearing when the screen was read, or whether it was there and reads differently on a Mac.
   The log keeps what was read, and that text is what tells those apart.
4. **Tab past everything else without pressing it**, and these seven especially.
   "Instrument editor" clicks a point measured on Kontakt 8's header, which Kontakt 7 lays out
   differently. "Increase plugin height" and "Decrease plugin height" do not click at all:
   they search the plugin for a picture of Kontakt 8's resize grip and drag whatever matches
   by a fixed step, and a drag that lands somewhere else moves whatever is there. And
   "Previous instrument", "Next instrument", "Snapshot menu", "Previous snapshot" and "Next
   snapshot" are aimed at Kontakt's rack: with the preset browser open — which is how Kontakt
   starts — those coordinates point into the browser instead, and one of them sits a few
   pixels from a control that deletes a snapshot. None of the seven has been checked on a Mac.

**If anything says "something else is covering …":** that is new, and it is the application
refusing to click because another window is drawn over the spot it was about to press. Write
down which control said it and what else was on screen; the log names the window and which
application it belongs to. It has never run on a Mac before this session.

**If F6 says "could not move the keyboard into …":** that sentence covers two different
things, and only the log tells them apart. Either one of Kontakt's controls did take the
keyboard when asked but a quarter of a second later the keyboard could not be confirmed inside
the panel — nothing was clicked in that case — or none of Kontakt's controls took it, and F6
fell back to the corner click, which in Kontakt is its logo. So if you hear it, first notice
whether anything in Kontakt opened or changed and write that down; then click into the panel
with VOCR as above.

**Not covered, so do not spend time on them:** Kontakt **standalone** (macOS calls its window a
dialog, and nothing matches it yet), **Kontakt 8** (its file menu is the KONTAKT wordmark
rather than a FILE button, and no Kontakt 8 has been measured on a Mac yet — the overlay stays
inert there and says so in the log at start-up), and **Komplete Kontrol** (it publishes
nothing at all to the accessibility layer on a Mac, as last session's probe showed).

**Then:** press **Command-Shift-F9** over the FX window, so we have this session's picture
next to last session's.

**Write down:** everything from 1–3, and whether the overlay activated at all. Whether the
panel was found does not depend on that: the log says so by itself while the FX window is in
front — a line beginning `[kontakt] Kontakt 7 panel in 'FX: …'`, or one beginning
`[kontakt] 'FX: …' publishes no button named FILE`.

---

## 5. Back into a plugin window, second attempt

Last time this could not happen: the key never arrived, and REAPER had refused to let the
application watch its focus, so the overlay could not have noticed the keyboard moving anyway.
Both are changed.

**Do:** first close the Kontakt track's FX window from step 4 — F6 takes the first FX window
it finds, and with two open it may take Kontakt's. Then put **sforzando** on a **new track of
its own** (Insert → Virtual instrument on new track; REAPER names the track after it, and the
overlay finds sforzando by that name in the window's title — added to the Kontakt track it
would not be found). With sforzando's FX window open, click into REAPER's **FX list** and press
**Command-Shift-F6**.

**Should happen:** it says the window's title once the keyboard is inside — `FX: Track 2
"sforzando"`, or whatever number the track has; if it names the Kontakt track instead, that
window was still open: close it and press again — and, about a third of a second later,
sometimes more, the first control of the sforzando overlay. For sforzando this is the click
path: it publishes nothing that could be asked for the keyboard, so F6 clicks just inside the
panel, as it did last time.

Last time you reported that the control cut the title off, "when speaking through VoiceOver".
On our side the control's announcement no longer interrupts anything — and **that is the half
this step can test**, because "Speak through VoiceOver" stays off for the whole session.
Through the system voice the second line waits for the first, so if it is cut off here the
fault is ours and we can fix it. The VoiceOver half stays open for another session: there each
line is handed over the moment it arrives, and whether a new one cuts off what VoiceOver is
already saying is VoiceOver's decision, not ours.

**Write down:** what it said, in order; whether the title was heard to its end or cut off; and
which voice was speaking — VoiceOver's or the system voice.

---

## 6. Reload, and the tray

**Do:** press **Command-Shift-F5** — and keep the Shift held. **Command-F5 on its own is
macOS's own switch for turning VoiceOver off.** If you hear "VoiceOver off", press Command-F5
again to bring it back and repeat the step; nothing is broken and nothing is lost.

**Should happen:** after a moment, a spoken count. On the mini it should be **"12 modules
reloaded"** — that is how many the log shows loading on each launch there; a module that is
Windows-only by its manifest is not counted. If a module does not rebuild, the sentence names
it instead: "11 reloaded, 1 failed: Kontakt", or "Reload failed: …" if none did. Silence is a
different fault from either.

**Then:** the menu-bar icon's menu has **one** item for the window now, not two, and it says
what pressing it will do next: "Show module manager" while the window is hidden, "Close the
module window" while it is on screen — on screen even if REAPER is in front of it. Last time
you found "Close the module window" offered while nothing was open.

**Do:** open the menu-bar icon's menu and listen to the item's name. Choose it. Then open the
menu again: the item should have the other name now. If an earlier step left the manager open behind
REAPER, the first name is "Close the module window" — that is right, and choosing it puts the
window away; the second opening should then say "Show module manager".

**Write down:** the reload sentence, word for word — or that there was none; the item's name at
the first opening and at the second; and whether "Show module manager" and "Close the module
window" were ever offered together.

---

## 7. Personal Voice — last, and for a reason

Last time: "the permissions dialog doesn't show, but the checkbox gets checked. Nothing happens
otherwise." The log then claimed you had refused a dialog you never saw. That sentence is gone.

**Do:** open the manager (menu-bar icon → **Show module manager**), go to the **Application
settings** tab — the fourth of five — and tick **"Offer my Personal Voice to modules…"**.

**Should happen, one of three things:**

- **macOS shows its own permission dialog.** Then nothing of ours appears; the system's dialog
  is the feedback.
- **A dialog of ours opens, and the application itself says nothing.** VoiceOver reads the
  dialog's text, which has the focus, as it read the probe's error window. This is what happens
  when macOS had already answered before we asked, and your last log says that Mac answers that
  way — so this is the likeliest of the three. The text says what macOS answered and, if the
  answer was "denied", where the switch for it is (System Settings → Accessibility → Personal
  Voice, "Allow applications to use your Personal Voice"). It does not say *why* your Mac
  answers that way: whether a Mac with no Personal Voice recorded answers the same as one where
  applications are not allowed to use it, nothing we have establishes, so the sentence names
  the pane and stops.
- **No dialog at all, and one sentence spoken in a system voice — not VoiceOver's.** This is
  the case where macOS was asked for the first time and answered without showing a dialog.

Last time you got none of these; that is the fault this step checks.

**Write down:** which of the three happened, and — if it was ours, dialog or spoken — the
sentence, as near to word for word as you can.

---

**Why last:** the request and every spoken line go through one worker thread, in order, and
the request waits up to two minutes for macOS to answer. If a dialog does appear and takes a
moment, everything the application would have said meanwhile waits behind it. At the end of
the session that costs nothing; before the Kontakt step it would have cost the session.

---

## Two questions, not steps

1. **What is the VoiceOver modifier set to** on each Mac you have used — in VoiceOver Utility,
   under General: Control-Option, Caps Lock, or both? This is the likeliest reason the old F6
   chord worked on your Air and beeped on the mini, and nothing in the log records it.
2. On the mini, does `~/Library/Application Support/AutomationPlatform/` contain an
   `automation-platform.log`? The log you sent starts with Accessibility already granted, and
   the very first launch — the one where it was not — must have written somewhere else.

## What the log answers on its own

Nothing to do. Lines to know about:

- **`[read] 'Pitchbend range' = "DEF" (region 569,38 38x22, N ms, 1 word)`** — every read-out
  read, with the text. New.
- **`[overlay] 'sforzando': Return went through to the menu; the hold ends in 300 ms …`** or
  **`… a window appeared for the plugin while a menu was plausible … treated as the menu`** —
  which of the two appears says whether sforzando's menu is a window of its own.
- **`observing focus in sforzando (pid N), on retry N`** — an application that was busy is now
  asked again on a clock instead of only when you switch applications. Last time sforzando's
  restarted process and REAPER were never asked again.
- **`[kontakt] Kontakt 7 panel in 'FX: Track 1 "Kontakt 7"': FILE at … puts the corner …`** —
  step 4's anchor.
- **`[daw-hosts] … handed to '…' by asking, it is now: …`** or **`… nothing in its panel took
  focus by asking; after a click at content (…)`** — which path F6 took, with a
  `focus_within` line just before it saying how many of the plugin's controls accepted the
  question.
- **`Kontakt: no menu row starting with 'Load...' — read: …`** — what the file menu read as,
  if step 4's item 3 said "Menu item not found".
- **`the point x,y is under window N of <application> …`** — a click was refused because
  another window covered the spot. New on the Mac; last time every click went wherever it went.
- **`translocated: YES`**, with **`no modules to load`** under it — macOS was running a
  read-only copy of the `.app` and nothing could load. If those are in the log, everything
  else in the session is explained by them.
- **`focus_within(…): nothing inside … took keyboard focus — N candidate(s), R refused the
  write …`** — step 4's F6 when nothing in Kontakt's panel would take the keyboard, and which
  of the two reasons it was.

## What to send

- **`automation-platform.log`**, whole, from the same folder as the `.app`.
- **Every `probe-N.png`**, from `modules/probe/`.
- Your notes, with **expected** and **got** where something behaved oddly.

## If you are on the old Air {#if-you-are-on-the-old-air}

Skip **step 4** (Kontakt does not run on macOS 12) and **step 7** (Personal Voice needs
macOS 14 — the switch says so there). Steps 2, 3, 5 and 6 apply unchanged, and step 3 is the
one carrying the fix for what you reported.
