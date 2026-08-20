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
- **Speech comes out in the system voice, not VoiceOver's**, and there is no braille. That
  is a known, separate piece of work.

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
 macOS records permissions
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
4. **Quit the application and open it again.** macOS only hands a newly granted permission
   to a process that started *after* it was granted. This is the single most common reason
   a permission appears not to have worked.

## The things worth doing, in order

Each one produces evidence in the log. None of them needs you to describe anything.

### 1. Did it start, and what does it think of this Mac

Nothing to do — this happens at launch. The top of the log names the macOS version, every
display and its scale factor, whether each permission was actually granted, whether
VoiceOver is running, and whether the key tap and the shortcut system came up.

That block alone answers most of what we would otherwise ask you.

### 2. Does a global shortcut work

Press **Control-Shift-Command-Option-F5**. It should say "reloading modules" and then how
many were reloaded.

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
own. **That is off until you turn it on**, because the first line through it makes macOS ask
your permission, and a dialog you did not ask for is not a good way for an application to
introduce itself.

So: open the manager from the menu-bar icon, go to the **Application settings** tab, and tick
**Speak through VoiceOver**. macOS will ask once whether this application may control
VoiceOver — say yes. Then, with the sforzando overlay speaking (step 3), two things are worth
a sentence each:

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

**Parked for a later session, when it is convenient:** one command would let the overlay talk
to VoiceOver directly instead of launching a small program for every line it says.

```bash
sdef /System/Library/CoreServices/VoiceOver.app > ~/Desktop/voiceover.sdef
```

`sdef` comes with the Xcode command line tools, which are a large download, so this is not
worth doing on its own — VoiceOver does not ship its scripting definition as a file, which is
why it cannot simply be copied. Do it only if you are installing those tools anyway, or if we
come back and ask, which we will only do once the timings in the log say the change is worth
making.

### 5. Control-Option-Right in the sforzando window — one question we cannot answer here

With the sforzando window in front, press **Control-Option-Right**, then **Control-Option-Left**.

You should hear a short tone — rising for right, falling for left — and then one of the
overlay's three read-outs, the same ones Tab moves between.

The question underneath is whether an application can take Control-Option-arrow away from
VoiceOver for as long as one of its own windows is in front. If it can, then on macOS the
overlays should be steered with the keys you already use, instead of asking you to learn
Tab and Shift-Tab for these windows in particular. Nothing here can test that; only a Mac
with VoiceOver running can.

So the useful report is one line, and it is about **both** halves:

| What happened | What it means |
| --- | --- |
| Tone, then one of our read-outs, and the VoiceOver cursor stayed where it was | It works. This becomes how the overlays are navigated on macOS. |
| Tone **and** the VoiceOver cursor also moved | We can see the key but not claim it — two things would happen at once, so this idea is out. |
| No tone, the VoiceOver cursor moved | The key never reaches us at all. |
| Nothing at all | Something else has the key. Worth knowing too. |

The tones are only there for this question and go away once it is answered. They are
deliberately not speech: whatever the answer is, it has to be tellable apart from VoiceOver
talking, and speech competing with speech is the one signal that cannot be.

Outside the sforzando window the keys are VoiceOver's again, immediately — that is the other
half of the claim, and it is worth a moment to check: Command-Tab to any other application
and confirm Control-Option-Right does what it always did.

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

A quick check you can make yourself: if `probe-1.png` shows your desktop wallpaper instead
of the plugin, **Screen Recording was not granted** (or the application was not restarted
after granting it). macOS does not report that as an error — it just hands back the
wallpaper.

### 7. Does anything block

If the machine feels sluggish, or a keystroke goes missing, say when it happened. The log
records anything that took too long, and "around the time I opened the browser" is enough to
find it.

## What to send

The whole folder next to the application:

- `automation-platform.log` — the main thing
- `probe-*.png` — the pictures from step 6
- `voiceover.sdef` — only if you got to the parked command in step 4

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
- **Control-Option shortcuts do nothing** — outside the sforzando window. That is
  VoiceOver's own modifier, and apart from the probe in step 4 the overlays' Windows key
  choices have not been adapted for macOS yet.
- **The overlay speaks in a second voice, not yours.** That is what it does until you tick
  **Speak through VoiceOver** in the Application settings tab. It is off to begin with
  because the first line through that path makes macOS ask your permission, and an
  application that opens a dialog you did not ask for, before you have asked it to do
  anything, is behaving badly — particularly towards someone who cannot see the dialog.
  Ticked, what the overlay says arrives in your voice, at your rate, and on your braille
  display. If VoiceOver then will not take it — most often because "Allow VoiceOver to be
  controlled with AppleScript" is off in VoiceOver Utility's General pane — it falls back to
  its own voice and writes the reason in the log rather than going quiet.
