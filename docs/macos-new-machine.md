---
title: Setting up a new Mac
---

# Setting up a new Mac

For a tester starting on a machine that has never run this before, working without sight.

**The order is the whole point of this page.** Every step below can be done in any order and
still "work"; done in the wrong one, you grant four permissions and then lose three of them on
the next build, and find out only because the application has quietly stopped being able to
read anything.

Everything here is one line at a time in **Terminal**, and each says how long it takes.

---

## 0. What you need first

- **Homebrew.** If `brew --version` says nothing, install it from <https://brew.sh>.

  **On an Apple-silicon Mac it will still say nothing afterwards**, and that is not a failed
  install. Homebrew puts itself in `/opt/homebrew/bin`, which is not on the default PATH; the
  installer prints the two lines that fix it at the end of a long run, where they scroll past.
  Step 2 now finds it there by itself, so you can carry on — but to have it in every Terminal
  from now on, run this once:

  ```bash
  echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
  ```

  (On the old Intel Mac this never came up: there Homebrew lands in `/usr/local/bin`, which
  *is* on the PATH.)
- **The source.** `git clone` this repository somewhere in your home folder and `cd` into it.
  Building it yourself rather than taking a download is worth the wait: the binary is for
  *this* Mac, so the Intel-versus-Apple-silicon question disappears, and testing a later fix
  becomes `git pull && ./bootstrap-macos.sh`.

---

## 1. The signing identity — before anything is built

```bash
./macos-signing-identity.sh
```

Half a minute, once, ever.

**Why it is first and not later.** macOS remembers Accessibility, Screen Recording and Input
Monitoring against an application's *code identity*. Without this, the identity is derived
from the bytes of the binary — so **every rebuild is a different application**, and every
permission has to be granted again, by hand, from scratch. With it, the identity becomes "this
bundle id, signed by this certificate", and neither changes when the code does.

Granting the permissions first and running this afterwards is the trap: the grants you already
made belong to the old identity and are silently worthless.

It is a local certificate for testing only. It is not an Apple Developer ID and it does not
satisfy Gatekeeper.

---

## 2. Build

```bash
./bootstrap-macos.sh
```

It installs what is missing (Xcode command line tools, cmake, Rust), builds, and packages. It
is safe to run again at any time.

**The first build takes roughly 10 to 25 minutes** because it compiles wxWidgets from source.
Later builds take a minute or two. It is not stuck.

If it warns that there is no signing identity, step 1 did not take — stop and fix that before
going on, or you will do step 4 twice.

The application ends up at `dist/AutomationPlatform/AutomationPlatform.app`, with the log,
the settings and the `modules` folder beside it. **Keep that folder together** and keep it
somewhere you can write to.

A rebuild empties and rewrites that folder, and it now carries four things across on purpose:
`automation-platform.log`, its rotated `.log.1`, `settings.toml`, and any `probe-*.png` the
probe has written. Everything else there is build output. So `git pull && ./bootstrap-macos.sh`
no longer throws away the evidence of the session that prompted the fix — but if you have
installed a module through the manager rather than from the repository, that does go.

---

## 3. Open it once

Open `AutomationPlatform.app`.

**Nothing visible happens.** It is a menu-bar application with no window and no Dock icon.
With VoiceOver, **VO-M twice** reaches the menu-bar extras; its menu has **Show** (the module
manager) and **Quit**.

If macOS refuses to open it because it was downloaded rather than built, clear the quarantine
flag and try again:

```bash
xattr -dr com.apple.quarantine dist/AutomationPlatform/AutomationPlatform.app
```

An app you built yourself is not quarantined, so this is only for a zip somebody sent you.

---

## 4. The four permissions

All four live in **System Settings → Privacy & Security** on macOS 13 and later. None of them
can be granted by the application itself, and macOS never says which one is missing — it just
behaves as though the application is broken.

| Permission | Without it | Granted how |
| --- | --- | --- |
| **Accessibility** | nothing can be read or clicked; no overlay ever activates | asked for on first launch; also addable by hand |
| **Screen Recording** | captures silently return a picture of the wallpaper — never an error | asked for on first use; also addable by hand |
| **Input Monitoring** | overlay keys reach the plugin instead of the overlay | usually follows Accessibility without being asked |
| **Automation → VoiceOver** | the "Speak through VoiceOver" setting looks on and the overlay still speaks in its own voice | asked the moment you tick that setting |

**After granting any of them, quit the application and open it again.** macOS hands a new
permission only to a process that started after it was granted. This is the single most common
reason a correctly granted permission looks like it did nothing.

The fourth one is not granted here: tick **"Speak through VoiceOver"** on the manager's
**Application settings** tab, and macOS puts the consent dialog up then. It is refused by
default and refused *silently*, which is why it is asked for at the moment you ask for the
feature. It also needs **"Allow VoiceOver to be controlled with AppleScript"** in VoiceOver
Utility's General pane.

---

## 5. Check it took, without being able to see

Everything the application knows about the machine goes into the log, beside the `.app`, in a
block at the start of **each run**.

**The file accumulates: every launch appends another block to the end.** So the top of it is
the first time you ever started the application — before any permission was granted — and
reading that would tell you the opposite of the truth. Step 4 has you quit and reopen, which
guarantees there is an older block above. Read the last one:

```bash
grep '\[env\]' dist/AutomationPlatform/automation-platform.log | tail -20
```

What a healthy new machine says:

```
[env] accessibility: granted
[env] screen recording: granted
[env] input monitoring: granted (it normally follows the Accessibility grant rather than needing one of its own)
[env] voiceover: running
[env] voiceover automation: granted
[env] signature: signed by OSAP Local Signing — permissions survive a rebuild
```

That last line is the one that says step 1 worked. If it says **ad-hoc** instead, the
permissions you just granted will be gone after the next build — run
`./macos-signing-identity.sh`, then `./bootstrap-macos.sh` again, and grant them once more.
That is annoying once and endless if left alone.

Two more lines from the same block are worth reading out on a new machine, because they are
answers we have never had:

```
[env] arch: … binary / … hw.machine / … hw.model
[env] backing scale: … on the primary display (all screens: …)
```

---

## 6. What this machine can answer that the old one could not

If it is an Apple-silicon Mac on a current macOS, four questions that were closed become
open. The first three are lines in the log; the last one needs a keypress and is the most
valuable thing in this whole document.

- **The Apple-silicon half of the build has never run.** Two lines settle it together, and
  the second is the one that matters:

  ```
  [env] arch: … binary / … hw.machine / … hw.model
  [env] rosetta: …
  ```

  `arm64 binary` with `rosetta: no` is the native slice, running natively for the first time.
  `rosetta: YES` means an Intel binary is being translated and the native half is still
  unproven. Do not try to read this off `hw.machine` alone: under Rosetta that field itself
  reports `x86_64`, so the mismatch you would look for never appears.
- **Retina has never been measured.** The old machine reported a backing scale of 1.00, where
  every coordinate agrees trivially. A `backing scale: 2.00` line is the first real test of
  whether anything is written in pixels where it should be in points.
- **Personal Voice needs macOS 14.** On an older system the switch correctly does nothing. On
  a new one it should raise a system dialog when ticked — and if you have recorded a Personal
  Voice, it should then appear among the voices a module can choose.
- **And the one press the old Mac could not make.** Kontakt and Komplete Kontrol would not run
  on macOS 12, so the question they were to answer is still open — and it decides an
  architecture, not a detail. On a current macOS they install and run; the free **Kontakt
  Player** is enough. Put its window in front and press **Command-Shift-F9**. The probe writes
  its own verdict, a line reading something like *"N of M elements publish an AXIdentifier"*,
  so you can tell the press landed without reading the rest. Send the log and the picture.

  What hangs on it: on Windows we identify the parts of a plugin by the internal names Qt
  gives them. If those names survive into what macOS publishes, the whole nested-overlay design
  — Kontakt inside a DAW, a library inside Kontakt — ports across. If they do not, it has to be
  rebuilt out of image matching.

---

## 7. When a permission will not stick

- **Quit and reopen first.** Most of the time that is the whole answer.
- **Check the identity line** in the log — the last block, not the first; see step 5. An
  ad-hoc signature loses permissions on every rebuild by design.
- **Add it by hand.** Every pane has a **+** button; point it at
  `dist/AutomationPlatform/AutomationPlatform.app`. On macOS 12 the Screen Recording prompt
  did not appear at all and this was the only way; on newer systems it should not be needed,
  and if it is, that is worth telling us.
- **Do not move the .app afterwards.** Moving or renaming it can invalidate what macOS
  recorded. If you must move it, move the whole folder.
