---
title: For the macOS tester
---

# What we need from a first session on a Mac

*Send this to whoever is testing. It is written to be read aloud and followed in order, and
it deliberately asks for evidence rather than for impressions — the point is that you should
never have to describe something you cannot see.*

## What this is, and what it does not do yet

An accessibility tool: it puts a spoken overlay over music plugins that a screen reader
cannot otherwise read. It works on Windows. The macOS half was written without a Mac in the
room, so **nothing in it has ever run** before you run it.

Two things are expected to be true on the first day, and neither is a fault to report:

- **One overlay works: Sforzando standalone.** It was written from the accessibility tree
  and the coordinates your first probes recorded, and it is the thing to try first — see
  step 3. Every *other* plugin is still identified by a Windows-only detail, so nothing else
  will activate; making them identifiable is what the probe is for.
- **Speech comes out in a voice of its own**, until you tick **Speak through VoiceOver** in
  the Application settings tab — see step 4. Off to begin with, because the first line
  through that path makes macOS ask your permission, and an application should not open a
  dialog before it has been asked to do anything.

## Getting it running

Either build it yourself — one command, and then testing a fix is one command more — or ask
for a build. Building locally also sidesteps the Intel-versus-Apple-silicon question
entirely, because it builds for the Mac it is on.

```bash
git clone https://github.com/Timtam/osap.git
cd osap
./macos-signing-identity.sh     # once, ever — see below
./bootstrap-macos.sh
```

**Run `./macos-signing-identity.sh` before the first build.**

It should print FOUR lines beginning `==>` — creating, adding to the keychain, checking, done —
and finish with instructions to rebuild. **If it stops after the first one, it failed**: an
early version suppressed the error output of the step that follows, so on Monterey it printed
one line and died without saying why. It now prints the openssl version it found and the actual
error. Send those two lines if it happens again.

**Why it is worth the minute.** macOS records permissions
against an application's code identity, and without a signing certificate that identity is
derived from the binary itself — so every rebuild is a different application to macOS, and
all three permissions have to be granted again, every time. The script creates a local
self-signed certificate once and the grants then survive every later build. It takes about a
minute and asks to unlock your keychain.

The first build compiles wxWidgets from source and takes roughly 10 to 25 minutes. It is not
stuck. Everything after that is a minute or two.

Then, in order — **macOS will not tell you when a step is missing, it will simply behave as
if the application is broken**:

1. `open dist/AutomationPlatform/AutomationPlatform.app`
   Nothing visible happens: it lives in the menu bar, not in a window. With VoiceOver, VO-M
   twice reaches the menu-bar extras.
   On the first launch it asks for **Accessibility** and for **Screen Recording**. Both
   prompts may appear at once, and answering them does not finish the job — see step 4.
   There are three permissions in play but **only two to grant**: Input Monitoring comes
   along with Accessibility by itself.
2. **Accessibility**: switch it on for AutomationPlatform in the privacy settings — the log
   names the pane in your macOS version's own wording.
3. **Screen Recording**: the same pane. The application asks for this at launch, and asking
   is what normally puts it into the list.

   **On Monterey (macOS 12) the prompt often never appears and the application is never
   added.** That is a macOS behaviour, not a fault here, and waiting for a dialog that is not
   coming is the trap. Add it by hand instead: unlock the padlock at the bottom of the pane,
   press the **+** button, and choose `AutomationPlatform.app`. The log says which pane to
   open, in the wording your macOS version actually uses — Monterey and earlier call it
   *System Preferences → Security & Privacy → Privacy*, Ventura and later *System Settings →
   Privacy & Security*.
4. **After Screen Recording, quit the application and open it again.** macOS hands that
   permission only to a process that started *after* it was granted — the most common reason
   a permission appears not to have worked. Accessibility does not need this: it reaches the
   running application within a few seconds (press *Re-check now* on the Permissions page).
   Input Monitoring usually follows Accessibility; reopen only if it still reads as missing.

## The things worth doing, in order

Each one produces evidence in the log. None of them needs you to describe anything.

### 1. Did it start, and what does it think of this Mac

Nothing to do — this happens at launch. The top of the log names the macOS version, every
display and its scale factor, whether each permission was actually granted, whether
VoiceOver is running, and whether the key tap and the shortcut system came up.

That block alone answers most of what we would otherwise ask you.

### 2. Does a global shortcut work

Press **Command-Shift-F5**. It should say "reloading modules" and then how many were
reloaded.

If your Mac is set to use F1–F12 as media keys, hold **fn** as well.

This one keystroke proves that shortcut registration, the key path and speech all work. If
it says nothing, that is worth reporting on its own, with the log.

### 3. Try the one overlay that should work

Open **Sforzando standalone**, click into its window, and press **Tab**.

It should say "Instrument", and Tab again "Polyphony", and again "Pitchbend range" — each
with the value it reads off the screen. Shift-Tab goes back. Those three read-outs are the
same ones the Windows version speaks, from the same authored coordinates.

This is the first thing on macOS that has ever been more than plumbing, and it is built
entirely on what your two probes measured: the window's identity, and the discovery that
every Windows coordinate sat exactly one title bar too high until the backend learned to
subtract it.

If it says nothing at all, the overlay did not activate — send the log. If it speaks but a
value is wrong or empty, that is a coordinate a few points out, which is worth knowing
precisely: say which of the three, and the log will have the OCR to go with it.

### 4. Is it your voice, and does it get in the way — ask this early

The overlay can hand what it says to VoiceOver instead of speaking it with a voice of its
own. **That is off until you turn it on**, because turning it on is what makes macOS ask your
permission, and a dialog you did not ask for is not a good way for an application to
introduce itself.

So: open the manager from the menu-bar icon, go to the **Application settings** tab, and tick
**Speak through VoiceOver**. Three things about that moment, because they changed since the
last round and one of them cost us that round's measurement:

- **VoiceOver has to be running when you tick it.** macOS will not ask about controlling an
  application that is not there. If it was not running, the log says so and you can simply
  untick and tick again.
- **The dialog arrives a moment later, not instantly.** It is asked from a background thread
  on purpose: Apple's own documentation says this call must not be made on the main thread,
  because it sits there for as long as you take to answer — and on this platform that is the
  thread carrying the key tap, so a dialog on it would take your keyboard away while you read
  it. So: tick, wait a beat, answer **yes**.
- **If you say no, the dialog does not come back.** The log then carries a
  `voiceover automation fix` line naming the exact pane to open.

Last round this was never asked at all — the setting went on and the permission behind it was
never requested, so nothing could come through and the whole measurement below was lost. If
you hear your own voice this time, that is the fix working.

Then, with the sforzando overlay speaking (step 3), two things are worth a sentence each:

- Do the read-outs come out in **your** VoiceOver voice, at your rate — or in a second,
  different voice? A second voice means the hand-off failed and the fallback took over, and
  the log says why, in a line starting `[speech]`.
- When the overlay says something while VoiceOver is already talking, what happens — does it
  **cut VoiceOver off**, or wait its turn? Either can be right; we need to know which it
  does, and it is not settleable anywhere but on a Mac.

There is a third thing we would like to know and cannot ask you: whether what the overlay
says also reaches a **braille display**. That is the strongest single reason for going
through VoiceOver at all, and it stays unverified until somebody with a display tries it.
Not a gap in the software — a gap in what anyone has observed.

**Done, and thank you — that one is closed.** The `sdef` dump you sent carries VoiceOver's
scripting codes, which is the piece nobody could get from a machine without a Mac. It is
written up in the repository now, so it never has to be asked for again.

### 5. The module list — three things only you can hear

You already found that VoiceOver reads it now. What we cannot tell from here is whether it
reads it *correctly*, and three questions settle that. Open the manager from the menu-bar
icon (VO-M twice reaches the menu-bar extras) and go to the **Installed** tab.

1. Arrowing down the list, does **one** press give you **one** row — and does that single
   announcement carry both the module's name and whether it is ticked? Or do you have to
   interact with the row to hear the checkbox?
2. Tick exactly one module somewhere in the middle, then arrow through every row. Does each
   row report **its own** state — or do several rows claim the state of the one you just
   changed? This is the failure we most expect and least want: the list is drawn from a
   single template cell, and if the accessibility side reads the template rather than the
   row, every row lies.
3. Does **Space** actually change a module's enabled state? Judge that by what the module
   does, or by the log — **never** by what VoiceOver said. The whole point of the question is
   that the announcement might be the thing that is wrong.

### 6. Record a plugin window — still the important one

Open a music plugin (in a DAW or standalone; either is useful, both is better), put its
window in front, and press **Command-Shift-F9**.

It will say something like "Probe 1 written. 12 surfaces, 340 accessibility elements, 18
words." That writes into the log everything the platform can see about that window — how it
identifies itself, its geometry, its whole accessibility tree, what OCR reads in it — and
saves a picture of it as `probe-1.png` next to the application.

**This is what unblocks everything else.** Writing the macOS half of a plugin's overlay
means knowing what that plugin calls itself in the accessibility tree, and there is no way
to find that out except from a real machine with the real plugin open.

Please do it for each plugin you would want an overlay for. Press it once per window; the
files are numbered.

**Two windows are worth more than the others this round**, because each settles a question
that has been open for weeks and cannot be settled anywhere else:

- **sforzando standalone, with PB RANGE set to 1.** You reported that this field reads as
  nothing at that value while POLY. at 1 reads fine. We already know from your last log that
  Vision reads lone digits perfectly well — it returned `1`, `2` and `0%` as words of their
  own — so the recogniser is not the suspect; the region the module reads is. Your last probe
  put the value `DEF` at content (812,97) while the module's region starts at 816, clipping
  its left edge. Probing with the value set to **1** tells us whether a `1` is read at all,
  which decides between "our box is in the wrong place" and something harder. Set the value,
  then press the key.
- **Kontakt or Komplete Kontrol, if you can install one.** Not for the plugin's own sake: it
  is a Qt application, and Qt puts its internal object names into the accessibility tree
  where macOS calls them `AXIdentifier`. Whether they survive is the single unknown standing
  between us and matchers for every Qt plugin — which is most of them. sforzando cannot
  answer it because it is not a Qt application. **No new code is needed for this**: the probe
  already prints that identifier as the third part of every class it lists, so one press over
  a Kontakt window answers it.

**The probe now also answers three things by itself**, at the end of each run, so you do not
have to describe them:

- **What it costs to ask which window is in front.** This was the biggest stall in your last
  log — fourteen of them, the worst 2605 ms, during ordinary use rather than while pressing
  anything. Since then the backend has stopped asking an application that has just failed to
  answer, and serves the window that application last described while it is not talking. The
  probe asks forty times, **one ask per tick**, and reports the whole spread; the earlier
  version asked forty times in a row and the host answered thirty-nine of those from its own
  per-tick cache, so it was measuring almost nothing. Read the TAIL rather than the median,
  and read it next to the log: an answer served from memory is fast on purpose, and the log
  line "serving the window it last described" is what tells the two apart.
- **Whether one big text read is cheaper than three small ones.** Your log suggested reading
  costs the same whatever the size, which would mean overlays should read one wide strip and
  split it afterwards. That is a real change to the runtime, so it wants a measurement.
- **The module-error window.** At the very end of a run the probe raises a real error on
  purpose, and a window appears. It is not a fault — it is there to be looked at. Three
  things about it, and they are the only part of this the log cannot answer:
  1. Does it have a **Dock icon**, and does **Command-Tab** reach it?
  2. Does **VoiceOver read the message** when it opens?
  3. Open the module manager as well, then close the manager. Does the error window **keep**
     its Dock icon? (That is the bug this window was rewritten to prevent, and nobody has
     ever seen it work.)

A quick check you can make yourself: if `probe-1.png` shows your desktop wallpaper instead
of the plugin, **Screen Recording was not granted** (or the application was not restarted
after granting it). macOS does not report that as an error — it just hands back the
wallpaper.

### 7. Does anything block

If the machine feels sluggish, or a keystroke goes missing, say when it happened. The log
records anything that took too long, and "around the time I opened the browser" is enough to
find it.

## Sending back what a command printed

Sometimes the answer is the exact text a command printed, word for word. Reading it back is
the part that goes wrong: a missing line sounds the same as no line, and neither of us can
see that it is missing. So nothing here asks you to read anything. The command writes a file
straight into the folder we share, and it turns up on our side.

Every path below is written as `~/Dropbox/osap`. If the folder we share is called something
else on your Mac, use that instead — the first command here is how we find out what it is.

**Once, so we know what that folder is called on your machine.** This one is the exception —
its answer goes on your clipboard and you paste it into a message to us:

```bash
{ cat ~/.dropbox/info.json; echo; find ~ -maxdepth 2 -iname "Dropbox*"; } 2>&1 | pbcopy
```

Then Command-V into a message. Nothing is spoken and nothing appears; that is normal.
`pbcopy` is silent whether it worked or not, which is why the paste is the proof.

**After that it is one line, and it is the same line every time:**

```bash
zsh ~/Dropbox/osap/run.sh
```

We put `run.sh` in that folder, it does whatever we needed and writes its answer next to
itself, and you send the file back. From the second time it is Up-arrow and Return. If we
have asked for something and there is no `run.sh` yet, it has not synced — wait a minute.

**When we send one command instead of a script**, add the last part of this to it:

```bash
some-command > ~/Dropbox/osap/out.txt 2>&1
```

The `2>&1` goes at the very end, after the file name. That is the part that puts *error*
messages in the file as well as ordinary output, and the errors are usually the interesting
half. In the other order it silently keeps only half of what you wanted.

To hear that something landed:

```bash
wc -c ~/Dropbox/osap/out.txt
```

It answers with a number and the file name. Any number but 0 means there is something to
send — and **0 is also an answer**, it means the command ran and printed nothing. Send it
either way.

### When the command fails, or does not exist

Send the file anyway. That is not a wasted round trip, it *is* the answer: a command that is
missing or refuses prints its complaint as an error, the `2>&1` puts that complaint in the
file, and one line saying it was not found tells us exactly what we needed to know.

Two things worth knowing while it happens:

- **If a window appears offering to install "command line developer tools", say no** — Not
  Now, or Escape. A few commands on a Mac are only a stub that asks for a large download, and
  we do not want you to make it. The refusal is already in the file, so declining costs
  nothing.
- **If no file appears at all**, the path was mistyped and the command never ran. Run
  `mkdir -p ~/Dropbox/osap` once, then try the line again.

### Two things not to reach for

- **Not VO-Shift-C.** It copies VoiceOver's last spoken phrase and nothing before it, so what
  arrives is one fragment of what you meant to send — and it looks complete from both ends,
  which is the worst property a thing like this can have.
- **If you already ran the command and forgot the file part**, you do not have to type it
  again:

  ```bash
  !! > ~/Dropbox/osap/out.txt 2>&1
  ```

  That runs the previous line a second time with the output going to the file.

If the terminal ever goes completely quiet — you type and nothing comes back — press
**Control-Q**. Control-S sits next to it and pauses the display until Control-Q releases it.

## What to send

The whole folder next to the application:

- `automation-platform.log` — the main thing
- `probe-*.png` — the pictures from step 6

Both sit beside `AutomationPlatform.app`, in `dist/AutomationPlatform/`. If that folder was
not writable, the log went to `~/Library/Application Support/AutomationPlatform/` instead —
the log's own first lines say which it chose.

**For much more detail**, when something specific is being chased, there are two ways and
the first is easier: open the menu-bar menu, choose **Show module manager**, go to the
**Application settings** tab, and tick **Detailed (trace) logging**. It takes effect the moment you
tick it, it can be turned off the same way, and it is remembered.

The other way, for a run that has to start with it already on:

```bash
AUTOMATION_PLATFORM_TRACE=1 open dist/AutomationPlatform/AutomationPlatform.app
```

Either records every OS call that failed and every decision the key handling made. It makes
the log large, so use it for a targeted run rather than all day.

## Things that will look like faults and are not

- **A new build asks for its permissions again** — if you skipped
  `./macos-signing-identity.sh`. Without a signing certificate every rebuild is a different
  application as far as macOS is concerned. Run that script once and it stops.
- **The first build after creating the identity still asks once.** The identity is new, so
  the old grants do not carry over. It is the last time.
- **No Dock icon, no entry in the app switcher.** It is a menu-bar application by design.
- **Control-Option shortcuts belong to VoiceOver.** That is its modifier on every Mac that
  keeps the default setting, and VoiceOver handles those chords above the layer any
  application can tap — which is also why VO-arrow still works inside a password field. We
  measured it twice, differently: Control-Option-arrow never reached our own window, and a
  four-modifier chord that had worked on one tester's Mac (VoiceOver modifier set to Caps
  Lock, we believe) registered and never arrived on another, with VoiceOver's error sound
  on every press. None of our keys sits there any more. The overlays keep the keys the
  Windows version uses — `Alt+<letter>` and `Ctrl+<letter>` — and they are claimed only
  while that plugin's overlay is actually in front. If one of them turns out to be taken on
  your Mac, the log says so by name and we will move that one.
- **The overlay speaks in a second voice, not yours.** That is what it does until you tick
  **Speak through VoiceOver** in the Application settings tab. It is off to begin with
  because the first line through that path makes macOS ask your permission, and an
  application that opens a dialog you did not ask for, before you have asked it to do
  anything, is behaving badly — particularly towards someone who cannot see the dialog.
  Ticked, what the overlay says arrives in your voice, at your rate, and on your braille
  display. If VoiceOver then will not take it — most often because "Allow VoiceOver to be
  controlled with AppleScript" is off in VoiceOver Utility's General pane — it falls back to
  its own voice and writes the reason in the log rather than going quiet.
