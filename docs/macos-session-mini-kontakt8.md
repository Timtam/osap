---
title: Test protocol — the Mac mini, with Kontakt 7 and 8
---

# Mac mini, with Kontakt 7 and Kontakt 8

Same Mac mini as last time (macOS 14.5), now with Kontakt 8 as well as Kontakt 7. The steps are in
order of importance: if time runs short, steps 3 and 4 matter most, then 5, then 6. Within steps 4
and 5, do Kontakt 8 first, then Kontakt 7. **Always do step 9.**

For every step, write down what you heard, word for word where you can. Silence and wrong words
are both useful answers.

## Keys

- Record the window in front (the probe): **Command-Shift-F9**
- Move between our controls: **Tab** / **Shift-Tab**
- Put the keyboard into a plugin window: **Command-Shift-F6**
- Reload modules: **Command-Shift-F5** — keep Shift held; Command-F5 alone turns VoiceOver off (press
  it again to bring VoiceOver back)

**After every Command-Shift-F9, press nothing until the summary ("Probe N written … N words") has
been spoken.** Over Kontakt that can take up to half a minute.

If F9 does nothing, try it with **fn** held; then press F6 and F5 the same way.

## 1. Before you start

- Download **AutomationPlatform-macos-universal** from the newest green run of the **macOS
  build** workflow.
- The student is needed for the admin password in steps 3 and 9.
- Tell the student that every probe saves a picture of the window and the text in it, and ask them
  to quit mail, messages and their browser.

## 2. Set up

1. If last time's `AutomationPlatform` folder is still there: quit that app if it runs
   (`pkill -f AutomationPlatform.app` in Terminal) and move the old folder to the Bin.
2. Unzip the download twice. Move the `AutomationPlatform` folder into your **home folder**
   (Command-Shift-H in Finder) — not Desktop, Documents or Downloads.
3. In Terminal, one line after the other:

   ```bash
   cd ~/AutomationPlatform && xattr -dr com.apple.quarantine AutomationPlatform.app
   ```

   ```bash
   tccutil reset All com.automationplatform.app
   ```

   **Read the tccutil line back (VO-L) before pressing Return: without the name at the end it
   removes the permissions of every app on this Mac.** It should answer "Successfully reset…". If
   it says anything else, remove AutomationPlatform by hand from Accessibility, Screen Recording and
   Input Monitoring (System Settings → Privacy & Security, minus button).
4. Keep Terminal open.

**Write down:** what tccutil answered.

## 3. Permissions

**To quit:** menu-bar icon (VO-M twice) → **Quit**, or `pkill -f AutomationPlatform.app`.
**To open:** `open AutomationPlatform.app` in Terminal. Closing the manager window does not quit.
If System Settings offers "Quit & Reopen", choose **Later** and quit and open it yourself.

1. Open the app. macOS asks for Accessibility, maybe Screen Recording too: choose **Open System
   Settings**. Our manager opens on its Permissions page and says what is missing.
2. Grant **Accessibility**, then quit and open.
3. Grant **Screen Recording**, then quit and open. Entries appear switched off: switch them on,
   never remove them.
4. **Input Monitoring** only if the Permissions page says NOT granted: switch it on (or add it with
   +), then quit and open.
5. Finished when the app starts without opening the manager and says "Automation Platform is
   running in the menu bar…". If it still names Screen Recording after one reopen, carry on anyway.

**Then:** in Terminal type `open .` (a Finder window of our folder comes up) and press
**Command-Shift-F9**. Wait for the summary. A few seconds later a window "Module error:
com.tool.probe" opens: close it with Escape, then Command-Tab to Finder. It comes once per launch.

If F9 stays silent: open the manager (menu-bar icon → Show module manager), tab **Installed**. An
empty list → run the xattr line again, quit, open. If F9 says "Nothing is focused" while Finder is
in front → quit and open. After any reopen, do the Finder F9 again.

**Write down:** when the app appeared in the Screen Recording list; the word count of the
summary; whether F9 needed fn.

## 4. Kontakt on its own — the most important step

Do this with **Kontakt 8** first, then quit it and do it again with **Kontakt 7**.

1. Open Kontakt. Write down the window title VoiceOver reads, word for word. Close any What's New,
   update or activation notice (choose Run Demo; never sign in to anyone's account).
2. Command-Tab to Finder and straight back, every 15–30 seconds, until you hear **"Kontakt file
   menu, button"**. Stop after 4 minutes — or at once if the title is anything but "Kontakt 8" or
   "Kontakt 7" — then press F9 over Kontakt and go on.
3. **Tab** through our controls:
   - Kontakt 8: **"Kontakt controls, Tab to step through them"** — that is all.
   - Kontakt 7: "Library on/off", "View menu", "Shop", then **"Kontakt controls, Tab to step
     through them"**.
4. On "Kontakt controls", **Tab** again → VoiceOver reads Kontakt's own controls. Keep tabbing
   about once a second until "Kontakt file menu" comes back (at most 2 minutes). Note roughly how
   many and a few names. **No Return or Space in there.** If the first Tab goes straight back to
   "Kontakt file menu": wait 10 seconds, Shift-Tab, Tab again.
5. On "Kontakt file menu" press **Return**: "activated", and Kontakt's file menu should open. Do
   the arrows read its items? Escape, count one, Tab → it should say the next stop from 3.
6. **Kontakt 7 only:** on "Library on/off" press **Return** once. Then do 7, and afterwards press
   "Library on/off" once more to put Kontakt back.
7. **Command-Shift-F9** over Kontakt, wait for the summary.

**Write down, for each version:** the title, what you heard in 2 to 5, whether the Tab after Escape
moved on, the summary.

## 5. Kontakt in REAPER

Do this with **Kontakt 8** first, then with **Kontakt 7** — each on its own new track, and **close
the Kontakt 8 FX window before starting Kontakt 7**.

1. Quit the standalone Kontakt. In REAPER: **File → New project tab** (once, for both versions),
   then **Insert → Virtual instrument on new track** → Kontakt 8 or Kontakt 7 (take VST3 if it is
   listed more than once; note the entry). If REAPER asks to build routing or add tracks: **No**.
2. The FX window should read like `FX: Track 1 "Kontakt 8"` (or "Kontakt 7"), with the FX list on
   its left. No FX list → close it and open the track's FX chain. No version name in the title →
   Escape, press F2 in REAPER's main window, rename the track "Kontakt 8" or "Kontakt 7", open its
   FX chain again.
3. **Command-Shift-F6**. Note its sentence. If it says "could not …", wait 5 seconds and press it
   again. If VoiceOver reads something new over Kontakt, press Escape once.
4. Expected: **"Load instrument, button"** (it may already have come before F6; then Tab should say
   "Save multi as"). If nothing: F9 over the FX window, then F6 once more. Still nothing → go on.
5. Tab through once and note the stops. Roughly:
   - Kontakt 8: Load instrument, Save multi as, Reset multi, then either "Switch to classic view",
     Side pane, Info pane, Keyboard panel, Plugin size — or "Switch to play view", then
     instrument, multi and snapshot controls, the panes, Plugin size, Increase and Decrease plugin
     height.
   - Kontakt 7: Load instrument, Save multi as, Reset multi, Instrument editor, Previous and Next
     instrument, Snapshot menu, Previous and Next snapshot, Side pane, Info pane, Keyboard panel,
     Plugin size, Increase and Decrease plugin height, Library on/off.

   After Increase/Decrease plugin height note what follows: nothing, "not found", or "at maximum
   height, unavailable".
6. "Load instrument" → **Return**, then **press nothing for 5 seconds**. Kontakt's file dialog
   should open (close it with Escape). "Menu item not found" → do not press Escape. Nothing at all
   after 5 seconds → Escape, note "silent".
7. **Press no other control.** F9 over the FX window, then close the FX window.

**Write down, for each version:** the plugin entry, F6's sentence, 4 to 6.

## 6. sforzando

**On its own:**

1. Open sforzando standalone (if silent after 10 seconds: Command-Tab to Finder and back).
   Expected: "Instrument, button" and its value; Tab: "Polyphony"; Tab: "Pitchbend range".
2. Note Pitchbend range's value. You set it to 1 last time: if it reads 1, ask the student what it
   was before (the default shows as DEF). If it reads "no text", use **Polyphony** instead below.
3. Return on Pitchbend range, choose another value, Return. Count a full second, Tab → it should
   say "Instrument" at the first press.
4. Shift-Tab to Pitchbend range. Return, Down, Escape. Count a second, Tab, Shift-Tab → it should
   still read the value from 3.
5. F9 over sforzando. Set the value back (the student's, or what you noted). Quit sforzando.

**In REAPER** (same new project tab, all Kontakt FX windows closed): Insert → Virtual instrument on
new track → sforzando. The FX window should read like `FX: Track 3 "sforzando"` with the FX list.
Put VoiceOver on the FX list and press VO-Space once (not twice). **Command-Shift-F6** → the title,
then "Instrument, button". Is the title heard to its end? Then F9 over the FX window.

**Write down:** the values read, whether Tab moved at the first press, what Escape left, and what
F6 said in REAPER.

## 7. Reload

**Command-Shift-F5** → expected "12 modules reloaded". Note the sentence.

## 8. Personal Voice — only with 8 minutes left

Open the manager (menu-bar icon → Show module manager; if the item says "Close the module window",
choose it, then Show module manager). Tab **Application settings** → tick **"Offer my Personal
Voice to modules…"** — the 7th checkbox, right after "Speak through VoiceOver…" (do not tick that
one).

Possible answers: a dialog of ours (VoiceOver reads it); macOS's own dialog (if you refuse it, a
sentence "…without showing a dialog" follows — that is known, just note that you refused);
sentences with no dialog; nothing within 30 seconds. Note which, and the first words.

## 9. Clean up — always, in the last 5 minutes

1. Quit the app.
2. Copy the `AutomationPlatform` folder **to yourself only** — AirDrop to your own device or a USB
   stick; not the student's mail or a shared channel. If
   `open ~/Library/Application\ Support/AutomationPlatform` opens a folder, copy that too. Check
   that the copy arrived.
3. In Terminal, read back before Return:

   ```bash
   tccutil reset All com.automationplatform.app
   ```

4. REAPER, **only if you opened a project tab today**: VO-F2 — the title must not name a saved
   project — then File → Close project → **don't save**. Quit Kontakt and sforzando.
5. Finder: Command-Shift-H, select `AutomationPlatform`, Command-Delete. (Or in Terminal exactly
   `cd ~ && rm -rf AutomationPlatform`.) If there was an Application Support folder in 2:
   `cd ~/Library/"Application Support" && rm -rf AutomationPlatform`.
6. If Personal Voice was granted, tell the student (System Settings → Accessibility → Personal
   Voice).

## What to send

`automation-platform.log` (and `automation-platform.log.1` if there is one), the Application Support
log if there was one, every `probe-N.png` from `modules/probe/`, and your notes.
