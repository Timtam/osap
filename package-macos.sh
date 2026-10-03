#!/usr/bin/env bash
# Builds a distributable macOS application bundle — the counterpart to package.ps1.
#
#   ./package-macos.sh                 build, bundle, sign ad-hoc, zip
#   ./package-macos.sh --version 0.3.0 name the zip for that version instead of the date
#   ./package-macos.sh --no-zip        stage only, for looking at what would ship
#   ./package-macos.sh --no-build      package whatever is already in target/release
#   ./package-macos.sh --universal     join an Intel and an Apple-silicon build into one
#                                      binary (both must already exist — see below)
#   ./package-macos.sh --commit 6c95c8b
#                                      name the build in the README for that commit (CI passes
#                                      the run's own); left out, it is this checkout's, marked
#                                      -modified when the working tree has uncommitted changes
#   ./package-macos.sh --onnxruntime DIR
#                                      carry the neural text recogniser: ONNX Runtime's dylib
#                                      from DIR, Microsoft's onnxruntime-osx-universal2 archive
#                                      unpacked (tools/onnxruntime-mac.txt names it), into
#                                      Contents/Frameworks, and its model into Contents/Resources;
#                                      left out, the application reads text with Vision alone
#
# This has to run ON a Mac (it compiles). Nobody on the project owns one, so it is also run
# by .github/workflows/macos-build.yml on a GitHub runner, as the macOS half of every Build
# run (.github/workflows/build.yml) — that is currently the only way
# the macOS code gets LINKED rather than merely type-checked, and it is the thing to look at
# first when something here stops working.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version=""
do_zip=1
do_build=1
universal=0
commit=""
onnxruntime=""
while [ $# -gt 0 ]; do
  case "$1" in
    --version) version="$2"; shift 2 ;;
    --no-zip)  do_zip=0; shift ;;
    --no-build) do_build=0; shift ;;
    --universal) universal=1; do_build=0; shift ;;
    --commit) commit="$2"; shift 2 ;;
    --onnxruntime) onnxruntime="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# The neural text recogniser's runtime, checked before anything is built: Microsoft's universal
# archive unpacked, its dylib under lib/ (libonnxruntime.dylib is a link to the versioned file),
# its LICENSE and ThirdPartyNotices.txt beside. The application opens it at run time from
# Contents/Frameworks (crates/host/src/backend/paddle_ocr.rs, which says why it is not linked),
# so a package without it is an application that reads text with Vision alone, not a broken one.
ort_lib=""
if [ -n "$onnxruntime" ]; then
  [ -d "$onnxruntime" ] || { echo "--onnxruntime $onnxruntime is not a folder" >&2; exit 2; }
  for candidate in "$onnxruntime/lib/libonnxruntime.dylib" "$onnxruntime"/lib/libonnxruntime.*.dylib; do
    [ -f "$candidate" ] && { ort_lib="$candidate"; break; }
  done
  [ -n "$ort_lib" ] || { echo "no lib/libonnxruntime.dylib under $onnxruntime" >&2; exit 2; }
  for f in LICENSE ThirdPartyNotices.txt; do
    [ -f "$onnxruntime/$f" ] || { echo "no $f under $onnxruntime: its notices go out with it" >&2; exit 2; }
  done
fi

# The build this package is, so that a report can be matched to the download it came from. It
# goes into the README's `Build:` line below. The application names its build itself, in the
# header every session writes to its log and in the title of its Modules window: the commit
# compiled into the executable (crates/app/src/main.rs, crates/host/src/build_info.rs). No
# build-info.txt is written beside the .app, because this script packages the executable of
# the checkout it runs in, so the two are the same build. The CI job writes one only when it
# reuses an earlier run's executable (.github/workflows/macos-build.yml).
# Asked the same way the executable asks when it is compiled, so a package made here from a
# clean checkout names the same build in its README as the executable inside it does.
if [ -z "$commit" ]; then
  commit="$(git -C "$root" describe --always --abbrev=7 --dirty=-modified --exclude='*' 2>/dev/null || true)"
  [ -n "$commit" ] || commit="unknown"
fi
# It is what a tester quotes from the README, so a value no build could have is refused here
# rather than shipped.
case "$commit" in
  ''|*[!A-Za-z0-9._+-]*)
    echo "--commit '$commit' is not a commit: letters, digits and -._+ only" >&2; exit 2 ;;
esac
[ "${#commit}" -le 64 ] || { echo "--commit '$commit' is longer than 64 characters" >&2; exit 2; }

# The bundle identifier is the single most consequential string in this file.
#
# macOS records Accessibility and Screen Recording permission against the app's identity,
# and a signed app's identity is its bundle id plus its signature. Change either and every
# permission the user granted is silently gone — for a blind tester that is an application
# that worked yesterday and today does nothing at all, with no dialog to explain it. So this
# is fixed once and never edited casually.
BUNDLE_ID="com.automationplatform.app"
APP_NAME="AutomationPlatform"
# 12.3, not 12.0: the executable links ScreenCaptureKit (captures, and the Screen Recording
# request), which arrived in 12.3 and is a required load — on 12.0 to 12.2 the application could
# not start at all, and this way the Finder says why instead of the process dying unexplained.
MIN_MACOS="12.3"

rel="$root/target/release"
if [ "$do_build" = "1" ]; then
  # wxdragon needs libclang; on a runner it lives inside the Xcode toolchain and is not on
  # the default search path, exactly as it is not on Windows.
  if [ -z "${LIBCLANG_PATH:-}" ] && command -v xcode-select >/dev/null 2>&1; then
    candidate="$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib"
    [ -d "$candidate" ] && export LIBCLANG_PATH="$candidate"
  fi
  (cd "$root" && cargo build --release)
fi

exe="$rel/automation-platform"

# A universal binary is two builds glued together, and the gluing is the easy part. Both
# slices have to exist first, which means two passes — on one Mac:
#
#   rustup target add x86_64-apple-darwin aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
#   cargo build --release --target aarch64-apple-darwin
#   ./package-macos.sh --universal
#
# Worth doing for a release, where one download has to work on any Mac. Not worth it for a
# test build that one known person installs on one known machine, which is why the ordinary
# path builds for whatever this Mac is.
if [ "$universal" = "1" ]; then
  intel="$root/target/x86_64-apple-darwin/release/automation-platform"
  arm="$root/target/aarch64-apple-darwin/release/automation-platform"
  for slice in "$intel" "$arm"; do
    [ -x "$slice" ] || { echo "missing $slice — see the note above --universal" >&2; exit 1; }
  done
  exe="$root/target/universal-automation-platform"
  lipo -create -output "$exe" "$intel" "$arm"
  echo "Universal binary: $(lipo -archs "$exe")"
fi

[ -x "$exe" ] || { echo "no executable at $exe — build first" >&2; exit 1; }

# ONNX Runtime is opened by the application at run time (crates/host/src/backend/paddle_ocr.rs);
# an executable that LINKS it does not start on a Mac without that file. ort-sys asks pkg-config
# for it even so (`load-dynamic` downloads nothing but still asks), and a Homebrew onnxruntime on
# the building Mac answers. Refused here, before anything is packaged — on a tester's own build
# as in CI, where it would otherwise be found only after the download had gone out.
if command -v otool >/dev/null 2>&1; then
  links="$(otool -L "$exe" 2>/dev/null || true)"
  if printf '%s\n' "$links" | grep -i onnxruntime >/dev/null; then
    echo "$exe links ONNX Runtime; the application must only open it at run time" >&2
    echo "(crates/host/Cargo.toml): build where pkg-config finds no onnxruntime — Homebrew's, for one" >&2
    exit 1
  fi
fi

dist="$root/dist"
stage="$dist/$APP_NAME"
app="$stage/$APP_NAME.app"

# The log and the settings live BESIDE the .app, because the application is portable — which
# put them inside the folder this used to delete outright. A tester whose whole contribution
# is a log file was being told "git pull && ./bootstrap-macos.sh" to pick up a fix, and that
# threw away the evidence of the session that had just produced it. His settings went with it,
# so every rebuild also silently switched "Speak through VoiceOver" back off, which is exactly
# the sort of thing that gets reported as a regression in the feature itself.
#
# So they are carried across. Everything else in dist is build output and is rebuilt.
keep="$(mktemp -d)"
for f in automation-platform.log automation-platform.log.1 settings.toml settings.toml.bak; do
  [ -f "$stage/$f" ] && cp -p "$stage/$f" "$keep/$f"
done
# And the text recognition measurements (`ocr-bench`, see measure-text-recognition.command below),
# which are evidence of the same kind as the log.
for f in "$stage"/ocr-bench-*.txt; do
  [ -f "$f" ] && cp -p "$f" "$keep/"
done
# And the probe's pictures, which are the other half of what a tester sends. They are written
# INSIDE the probe's module folder rather than beside the log, because `host.screen.save`
# resolves a relative name against the calling module's own root — a capability boundary, not
# an oversight, so the fix belongs here rather than there. Kept with their paths, because the
# module folder is repopulated from the repository and would otherwise take them with it.
if [ -d "$stage/modules" ]; then
  ( cd "$stage" && find modules -name 'probe-*.png' -print0 2>/dev/null       | while IFS= read -r -d "" f; do
          mkdir -p "$keep/$(dirname "$f")" && cp -p "$f" "$keep/$f"
        done )
fi
rm -rf "$dist"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for f in automation-platform.log automation-platform.log.1 settings.toml settings.toml.bak; do
  [ -f "$keep/$f" ] && cp -p "$keep/$f" "$stage/$f" && echo "  kept $f from the previous build"
done
for f in "$keep"/ocr-bench-*.txt; do
  [ -f "$f" ] && cp -p "$f" "$stage/" && echo "  kept $(basename "$f") from the previous build"
done

cp "$exe" "$app/Contents/MacOS/automation-platform"
chmod +x "$app/Contents/MacOS/automation-platform"

# The neural text recogniser, when asked for: ONNX Runtime where the application looks for it,
# Contents/Frameworks/libonnxruntime.dylib, and the model it reads, Contents/Resources/
# ppocr-rec.onnx (on Windows both are inside the executable). `-L`, because the archive's
# libonnxruntime.dylib is a link. Its local symbols stripped (`strip -x`), which Microsoft's file
# carries for debugging and nothing here reads; whether the stripped file still loads is what CI's
# `ocr-bench --paddle-probe` answers. Signed below, before the bundle.
if [ -n "$ort_lib" ]; then
  mkdir -p "$app/Contents/Frameworks"
  dylib="$app/Contents/Frameworks/libonnxruntime.dylib"
  cp -L "$ort_lib" "$dylib"
  chmod 644 "$dylib"
  before="$(du -h "$dylib" | cut -f1)"
  strip -x "$dylib" || echo "  strip -x failed; shipping ONNX Runtime unstripped"
  echo "ONNX Runtime: $(basename "$ort_lib"), $(lipo -archs "$dylib" 2>/dev/null || echo '?'), $before, $(du -h "$dylib" | cut -f1) stripped"
  cp "$root/crates/host/models/ppocr-rec.onnx" "$app/Contents/Resources/ppocr-rec.onnx"
fi

# LSUIElement: no Dock icon, no app-switcher entry — the application lives in the menu bar,
# which is the macOS shape of the Windows tray. It can still show windows and they can still
# take focus; VoiceOver reaches the menu-bar item with VO-M, VO-M.
#
# NSHighResolutionCapable matters more than it looks: without it the system runs the app
# through a 1x scaler, which would make every captured pixel a lie on a Retina display.
#
# No NSAppSleepDisabled, on purpose. An LSUIElement application in the background is an App Nap
# candidate, and a napped main thread hands captured keys over late; the host holds a
# latency-critical activity against that while keys are captured or a controller is listened to,
# and a plain one while a first setup's Screen Recording request waits for Accessibility
# (crates/host/src/backend/macos/activity.rs). The plist key would keep App Nap away for the
# whole session, including the hours in which nothing is captured — time a laptop's battery pays
# for.
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>$APP_NAME</string>
	<key>CFBundleDisplayName</key><string>Automation Platform</string>
	<key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
	<key>CFBundleExecutable</key><string>automation-platform</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>${version:-0.1.0}</string>
	<key>CFBundleVersion</key><string>${version:-0.1.0}</string>
	<key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
	<key>LSUIElement</key><true/>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSAccessibilityUsageDescription</key>
	<string>Automation Platform reads the controls of music plugins so it can describe them aloud.</string>
	<key>NSAppleEventsUsageDescription</key>
	<string>Automation Platform speaks through VoiceOver when VoiceOver is running.</string>
	<key>NSInputMonitoringUsageDescription</key>
	<string>Automation Platform takes the keys an overlay uses, so that they do not reach the plugin underneath.</string>
</dict>
</plist>
PLIST

# Beside the .app, not inside it — the same layout as the Windows package, and the same
# rule the application itself uses to find them (crates/host/src/portable.rs). Inside the
# bundle they would be read-only for an app in /Applications, and installing a module would
# also break the signature.
modules_out="$stage/modules"
mkdir -p "$modules_out"
shipped=0
for dir in "$root"/modules/*/; do
  [ -f "$dir/module.toml" ] || continue
  name="$(basename "$dir")"
  cp -R "$dir" "$modules_out/$name"
  rm -rf "$modules_out/$name/calibration"   # 123 screenshots of evidence; not runtime data
  shipped=$((shipped + 1))
done
[ "$shipped" -gt 0 ] || { echo "no modules found under $root/modules" >&2; exit 1; }

# The window probe, which is not an ordinary module and ships here on purpose.
#
# This build exists to answer questions about a machine nobody here can touch, and the probe
# is the instrument that answers them: one keystroke records everything about the window in
# front — its identity, its geometry, its accessibility tree, a picture of it, and what OCR
# reads in it. Without it, a tester who cannot see the screen has nothing to send but an
# impression. It is left out of the Windows package, where that problem does not exist.
if [ -d "$root/tools/probe" ]; then
  cp -R "$root/tools/probe" "$modules_out/probe"
  shipped=$((shipped + 1))
fi

# The pictures set aside at the top go back now, after the module folders they live in have
# been repopulated. Restored rather than merged: nothing here writes a probe-*.png, so a name
# that exists in both places would be the same file.
if [ -d "$keep/modules" ]; then
  ( cd "$keep" && find modules -name 'probe-*.png' -print0 2>/dev/null       | while IFS= read -r -d "" f; do
          mkdir -p "$stage/$(dirname "$f")" && cp -p "$f" "$stage/$f"             && echo "  kept $f from the previous build"
        done )
fi
rm -rf "$keep"

# The documentation, made to work from the folder rather than from a web server, by the same
# conversion package.ps1 runs — see docs-offline.ps1 for why that is a conversion and not a
# copy. It is a PowerShell script, run here by PowerShell 7 (`pwsh`), which GitHub's macOS
# runners have and a Mac of your own gets from Microsoft's installer package. Skipped rather
# than fatal when the site has never been built or there is no pwsh: a tester without docs
# still has a working application, and failing the whole package over them would be the wrong
# trade. A copy of the site as it is would not do instead: its links point at a server.
if [ ! -d "$root/docs-site/build" ]; then
  echo "No built docs at docs-site/build — packaging without them."
  echo "  Build them once with:  (cd docs-site && npm ci && npm run build)"
elif ! command -v pwsh >/dev/null 2>&1; then
  echo "No PowerShell 7 (pwsh) to make the built docs work from a folder — packaging without them."
  echo "  Microsoft's installer package for macOS: https://aka.ms/powershell-release?tag=stable"
else
  pwsh -NoProfile -NonInteractive -File "$root/docs-offline.ps1" -Out "$stage/docs"
fi

# Signing, and why it decides whether testing is bearable.
#
# macOS records Accessibility, Screen Recording and Input Monitoring against an
# application's CODE IDENTITY. For an ad-hoc signature that identity is derived from the
# contents of the binary, so every rebuild is a different application and all three
# permissions have to be granted again — including adding the app to Screen Recording by hand
# wherever the application's own requests do not put it into that list (docs/macos-
# permissions.md). Measured on a tester's second run: every permission back to "NOT granted",
# and the probe reporting no focused window because accessibility reads were refused.
#
# A local self-signed certificate makes the identity "this bundle id, signed by this
# certificate", and neither half changes when the code does. So it is used when it exists —
# see macos-signing-identity.sh — and its absence is called out rather than passed over,
# because the cost lands on whoever is testing rather than on whoever is building.
IDENTITY="${OSAP_SIGN_IDENTITY:-OSAP Local Signing}"
if command -v codesign >/dev/null 2>&1; then
  # ONNX Runtime first, by the same identity or ad hoc, as nested code is signed before what
  # holds it: the stripped copy carries no valid signature, and code without one does not load on
  # Apple silicon at all. `--deep` below signs it again with the bundle; this is what makes it so
  # whichever way that goes.
  if [ -f "$app/Contents/Frameworks/libonnxruntime.dylib" ]; then
    codesign --force --sign "$IDENTITY" "$app/Contents/Frameworks/libonnxruntime.dylib" 2>/dev/null \
      || codesign --force --sign - "$app/Contents/Frameworks/libonnxruntime.dylib" 2>/dev/null \
      || echo "  ONNX Runtime could not be signed; the neural recogniser will not load on Apple silicon"
  fi
  # Tried, not looked up.
  #
  # This used to gate on `security find-identity -v -p codesigning`, and that is what made
  # the whole arrangement useless: `-v` means VALID identities, validity means a trust
  # chain, and a self-signed certificate has none unless it has been explicitly trusted. So
  # the identity existed, the grep missed it, every build fell through to ad-hoc, and the
  # tester went on re-granting permissions after every rebuild exactly as before — while
  # being told the problem was fixed. Attempting the signature answers the only question
  # that matters, and it answers it about this machine rather than about a listing.
  if codesign --force --deep --sign "$IDENTITY" "$app" 2>/dev/null; then
    echo "Signed with \"$IDENTITY\" — granted permissions will survive a rebuild."
  else
    echo "Could not sign with \"$IDENTITY\"; falling back to an ad-hoc signature."
    echo "  That means macOS will treat the next build as a different application and ask"
    echo "  for Accessibility and Screen Recording again. ./macos-signing-identity.sh"
    echo "  creates the identity; if you have already run it, run it again — it will say"
    echo "  what is wrong with the one that is there."
    codesign --force --deep --sign - "$app" 2>/dev/null       || echo "  ad-hoc signing failed too; this bundle is UNSIGNED."
  fi
  # Said out loud either way, because the tester's log reports the same fact from the other
  # side and the two should agree.
  codesign -dv --verbose=2 "$app" 2>&1 | grep -E "^(Authority|Signature)=" | sed 's/^/  /' || true
fi

# The licences, beside the .app as on Windows: the application's own, and — when the package
# carries the neural recogniser — ONNX Runtime's (MIT) with Microsoft's notices for what it is
# built from, and the model's (Apache-2.0) with where it comes from. Both licences ask for
# exactly this: their text goes out with the program.
licences="$stage/licences"
mkdir -p "$licences"
cp "$root/LICENSE" "$licences/automation-platform-GPL-3.0.txt"
recogniser_note="The neural text recogniser is not in this package: text is read by Apple Vision alone."
if [ -n "$ort_lib" ]; then
  cp "$onnxruntime/LICENSE" "$licences/onnxruntime-MIT.txt"
  cp "$onnxruntime/ThirdPartyNotices.txt" "$licences/onnxruntime-ThirdPartyNotices.txt"
  cp "$root/crates/host/models/LICENSE-Apache-2.0.txt" "$licences/paddleocr-model-Apache-2.0.txt"
  cp "$root/crates/host/models/NOTICE.txt" "$licences/paddleocr-model-NOTICE.txt"
  recogniser_note="The neural text recogniser runs on ONNX Runtime by Microsoft, used under the MIT
licence (onnxruntime-MIT.txt), which is built from the projects whose notices are in
onnxruntime-ThirdPartyNotices.txt; it is AutomationPlatform.app/Contents/Frameworks/
libonnxruntime.dylib, from https://github.com/microsoft/onnxruntime. Its model is
PaddleOCR's, used under the Apache License 2.0 (paddleocr-model-Apache-2.0.txt), from
https://github.com/PaddlePaddle/PaddleOCR; paddleocr-model-NOTICE.txt says which."
fi
cat > "$licences/README.txt" <<TXT
Licences
========

Automation Platform is free software under the GNU General Public License, version 3 or
later. The full text is in automation-platform-GPL-3.0.txt. The source is at
https://github.com/Timtam/osap

$recogniser_note

The VPS Avenger preset database (modules/vps-avenger-presets/data) comes from the
avenger_control project, whose developer gave it to this project without restriction; it is
under the GPL like the rest. See modules/vps-avenger-presets/NOTICE.
TXT

# The real captures `ocr-bench` reads beside its own pictures, as the repository carries them
# (crates/host/bench-data/ocr/real, with their manifest and NOTICE): value fields and control
# words of plug-ins, cut on Windows by tools/ocr-fixtures/crop.py. measure-text-recognition.command
# hands them to it.
pictures="$stage/ocr-pictures"
mkdir -p "$pictures"
cp "$root"/crates/host/bench-data/ocr/real/*.png "$root/crates/host/bench-data/ocr/real/manifest.toml" \
  "$root/crates/host/bench-data/ocr/real/NOTICE" "$pictures/"

# The README's line about the documentation, only when there is documentation: a package made
# without it would otherwise point a tester at a file that does not exist.
docs_note=""
if [ -f "$stage/docs/index.html" ]; then
  docs_note="The documentation is in docs/index.html — open it in a browser. It works from
this folder; no internet connection and no server are needed.

"
fi

# The build comes first, because it is what a report has to name. The "Build:" line is also
# rewritten by the CI job when it reuses an executable (.github/workflows/macos-build.yml), so
# its shape is not to be changed on its own.
cat > "$stage/README.txt" <<TXT
Automation Platform — macOS test build

Build: $commit
The application names the same build at the start of every session in its log,
automation-platform.log, and in the title of its Modules window. Please mention it
when you report something.

FIRST RUN, in order. macOS will not tell you when a step is missing; it will just
behave as if the application is broken, so please do them all.

1. Move this whole folder into your home folder (in Finder, Command-Shift-H opens it).
   Not the Desktop, Documents or Downloads: macOS guards those three with a permission
   dialog of its own. The application writes its log and its settings NEXT TO the .app,
   and reads its modules from the "modules" folder beside it, so keep the folder together.

2. Remove the download quarantine flag, or macOS will refuse to open the app:
       xattr -dr com.apple.quarantine "$APP_NAME.app"
   If you have already opened it without this step (with Open on the right-click
   menu, or Open Anyway in System Settings), macOS runs a read-only copy of it from a
   hidden folder, a new one at every launch. The application then asks macOS where
   the original is and uses this folder all the same, and its log's "translocated"
   line says whether that worked. Do this step anyway, then quit the application and
   open it again.

3. Open $APP_NAME.app. It is a menu-bar application, not a window — with VoiceOver,
   press VO-M twice to reach the menu-bar extras. On a fresh copy it does not stay
   quiet: with permissions still missing it opens its own window on a Permissions
   page and says why.

4. Grant the permissions in System Settings > Privacy & Security. None of them can be
   granted by the application itself, and the Permissions page lists all four with
   what each one costs while it is missing:
     - Accessibility      — without it, nothing can be read or clicked. GRANT THIS
                            FIRST: the application asks for Screen Recording as soon
                            as it is granted, and asking is what normally puts it
                            into the Screen Recording list.
     - Screen Recording   — without it, screen capture silently returns a picture of
                            the wallpaper instead of failing, so this one is worth
                            checking twice. If the application is not in that list,
                            its button on the Permissions page asks again; if it is
                            still not there, press + under the list, choose
                            $APP_NAME.app and switch it on (on Monterey, unlock the
                            padlock first).
     - Input Monitoring   — only if the overlay's own keys reach the plugin instead of
                            the overlay: whether it is needed next to Accessibility is
                            not known yet. Its button on the Permissions page asks.
     - Automation         — only asked for when you tick "Speak through VoiceOver"
                            in Application settings; leave it alone otherwise.
   Accessibility takes effect at once: press "Re-check now" on the Permissions page.
   After granting Screen Recording, QUIT AND REOPEN the application: macOS hands that one
   only to a process that started after it was granted. If Re-check still shows Input
   Monitoring missing after granting it, quit and reopen as well.

5. To record a plugin window for us: put it in front and press
       Command-Shift-F9
   That writes everything about it to the log and a picture of it into modules/probe/,
   which together are what we need to make the overlays work on macOS. Press it over
   a plugin you would want an overlay for.

6. Everything the application knows about your machine is written to
       automation-platform.log
   beside the .app, starting with a block that lists the permissions it actually has.
   That file is what to send when something does not work. For much more detail:
       AUTOMATION_PLATFORM_TRACE=1 open $APP_NAME.app

7. Only when we ask you to measure text recognition: connect the power adapter,
   quit the application (its menu-bar menu, Quit), then in Terminal type
       zsh ~/$APP_NAME/measure-text-recognition.command
   (with this folder's path instead if it is not in your home folder). It reads
   pictures it carries and those in the ocr-pictures folder, not your screen, and
   needs no permission. It prints one line when it starts and one when it is done,
   so VoiceOver has little to read while it measures; everything else goes into
   the file. A full run takes up to half an hour on an Intel Mac and less on Apple
   silicon, 168 seconds of which it waits on purpose. It ends with the Glass sound
   when it finished and the Basso sound when it stopped early, and then shows the
   file in Finder, selected: send that ocr-bench-N.txt, either way.

Speech goes through the system voice, or through VoiceOver if it is running.

Licences are in the licences folder, including where to get the source.

$docs_note$shipped module(s) included.
TXT

# What the README's step 7 runs: `automation-platform ocr-bench`, which measures what Apple
# Vision's text recognition costs on this Mac on pictures the executable carries (no screen
# capture, no permission, no speech; docs/building-on-macos.md). Run as `zsh <file>`, like
# tools/tester/run.sh, so that it needs no executable bit and asks nothing of Gatekeeper; named
# .command, so that a double-click in Finder opens it in Terminal as well, where Gatekeeper may
# ask first.
#
# Made for a tester who listens with VoiceOver: `--quiet`, so that VoiceOver reads two lines
# rather than a hundred while the processor is meant to be measuring; a sound at the end, Glass
# when the run finished and Basso when it did not, so that the two are told apart without
# reading; and the file shown in Finder, selected, which VoiceOver announces and which can be
# attached from there.
cat > "$stage/measure-text-recognition.command" <<'CMD'
#!/bin/zsh
# Measures what text recognition costs on this Mac, for the Automation Platform developers.
#   zsh <this folder>/measure-text-recognition.command
# Add --quick for a shorter run. It reads pictures it carries, and the real captures in
# ocr-pictures beside this file, not the screen.
here=${0:a:h}
exe="$here/AutomationPlatform.app/Contents/MacOS/automation-platform"
if [ ! -x "$exe" ]; then
  echo "No AutomationPlatform.app beside this file: keep it in the folder it came in."
  afplay /System/Library/Sounds/Basso.aiff >/dev/null 2>&1
  exit 1
fi
# Its reads would compete with the measurement for the processor. By the executable's path:
# pgrep -x compares the process name, which macOS cuts to 16 characters, and
# "automation-platform" has 19, so it would never find it.
if pgrep -f 'AutomationPlatform\.app/Contents/MacOS/automation-platform' >/dev/null 2>&1; then
  echo "Automation Platform is running. Quit it first (its menu-bar menu, Quit), then run this again."
  afplay /System/Library/Sounds/Basso.aiff >/dev/null 2>&1
  exit 1
fi
# The real captures beside this file, unless the command line names other pictures already.
if [[ -d "$here/ocr-pictures" && ${argv[(I)--pictures]} -eq 0 ]]; then
  set -- --pictures "$here/ocr-pictures" "$@"
fi
echo "Measuring text recognition: up to half an hour. It ends with a sound and shows the file in Finder."
result=$("$exe" ocr-bench --quiet "$@")
run_status=$?
print -r -- "$result"
file=$(print -r -- "$result" | sed -n 's/^OCR BENCH: writing to //p' | head -n 1)
if [ $run_status -eq 0 ]; then
  afplay /System/Library/Sounds/Glass.aiff >/dev/null 2>&1
else
  echo "It stopped before the end (exit status $run_status). Please send the file all the same."
  afplay /System/Library/Sounds/Basso.aiff >/dev/null 2>&1
fi
if [ -n "$file" ] && [ -f "$file" ]; then
  open -R "$file"
fi
exit $run_status
CMD
chmod +x "$stage/measure-text-recognition.command"

label="${version:-$(date +%Y-%m-%d)}"
size="$(du -sh "$stage" | cut -f1)"
echo "Staged $shipped module(s) into $stage  ($size), build $commit"

if [ "$do_zip" = "1" ]; then
  zip="$dist/$APP_NAME-macos-$label.zip"
  # ditto, not zip: it preserves the bundle's structure and extended attributes, and it is
  # what Apple's own notarisation flow expects. A plain `zip` can produce a bundle that
  # launches on the machine that made it and nowhere else.
  (cd "$dist" && ditto -c -k --sequesterRsrc --keepParent "$APP_NAME" "$zip")
  echo "Wrote $zip  ($(du -sh "$zip" | cut -f1))"
fi
