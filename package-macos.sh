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
#                                      name the build for that commit (CI passes the run's
#                                      own); left out, it is this checkout's, marked
#                                      -modified when the working tree has uncommitted changes
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
while [ $# -gt 0 ]; do
  case "$1" in
    --version) version="$2"; shift 2 ;;
    --no-zip)  do_zip=0; shift ;;
    --no-build) do_build=0; shift ;;
    --universal) universal=1; do_build=0; shift ;;
    --commit) commit="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

# The build this package is, so that a report can be matched to the download it came from. The
# application reads it from build-info.txt (written below) into the header every session writes
# to its log, and into the title of its Modules window; see crates/host/src/build_info.rs.
# Asked the same way the executable asks when it is compiled (crates/app/src/main.rs), so a
# package made here from a clean checkout names the same build as the executable inside it.
if [ -z "$commit" ]; then
  commit="$(git -C "$root" describe --always --abbrev=7 --dirty=-modified --exclude='*' 2>/dev/null || true)"
  [ -n "$commit" ] || commit="unknown"
fi
# It goes into a file the application reads and into a window title, and the application
# ignores anything else, so a bad value is refused here rather than shipped unread.
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
MIN_MACOS="12.0"

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

cp "$exe" "$app/Contents/MacOS/automation-platform"
chmod +x "$app/Contents/MacOS/automation-platform"

# LSUIElement: no Dock icon, no app-switcher entry — the application lives in the menu bar,
# which is the macOS shape of the Windows tray. It can still show windows and they can still
# take focus; VoiceOver reaches the menu-bar item with VO-M, VO-M.
#
# NSHighResolutionCapable matters more than it looks: without it the system runs the app
# through a 1x scaler, which would make every captured pixel a lie on a Retina display.
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

# The documentation, if it has been built. Skipped rather than fatal: a tester without docs
# still has a working application.
if [ -d "$root/docs-site/build" ]; then
  cp -R "$root/docs-site/build" "$stage/docs"
  echo "Docs copied. NOTE: the offline rewrite (docs-offline.ps1) has no shell port yet —"
  echo "  these pages still contain absolute /osap/ links."
fi

# Signing, and why it decides whether testing is bearable.
#
# macOS records Accessibility, Screen Recording and Input Monitoring against an
# application's CODE IDENTITY. For an ad-hoc signature that identity is derived from the
# contents of the binary, so every rebuild is a different application and all three
# permissions have to be granted again — on Monterey including adding the app to Screen
# Recording by hand, because the prompt does not appear there. Measured on a tester's second
# run: every permission back to "NOT granted", and the probe reporting no focused window
# because accessibility reads were refused.
#
# A local self-signed certificate makes the identity "this bundle id, signed by this
# certificate", and neither half changes when the code does. So it is used when it exists —
# see macos-signing-identity.sh — and its absence is called out rather than passed over,
# because the cost lands on whoever is testing rather than on whoever is building.
IDENTITY="${OSAP_SIGN_IDENTITY:-OSAP Local Signing}"
if command -v codesign >/dev/null 2>&1; then
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

# Beside the .app, where the application looks for it, and NOT inside it: the CI job writes
# this file again when it reuses an earlier build's executable, and a file added to the bundle
# after signing would break the signature. Re-signing would give an ad-hoc signed app a new
# identity, and with it cost the tester every permission he has granted.
cat > "$stage/build-info.txt" <<INFO
# The commit this package was made from. Automation Platform reads it at start and names it
# in every session's header in automation-platform.log and in the title of its Modules window.
commit=$commit
INFO

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

3. Open $APP_NAME.app. It is a menu-bar application, not a window — with VoiceOver,
   press VO-M twice to reach the menu-bar extras. On a fresh copy it does not stay
   quiet: with permissions still missing it opens its own window on a Permissions
   page and says why.

4. Grant the permissions in System Settings > Privacy & Security. None of them can be
   granted by the application itself, and the Permissions page lists all four with
   what each one costs while it is missing:
     - Accessibility      — without it, nothing can be read or clicked. GRANT THIS
                            FIRST: until it is granted, this application may not
                            appear in the Screen Recording list at all.
     - Screen Recording   — without it, screen capture silently returns a picture of
                            the wallpaper instead of failing, so this one is worth
                            checking twice.
     - Input Monitoring   — without it, the overlay's own keys reach the plugin
                            instead of the overlay.
     - Automation         — only asked for when you tick "Speak through VoiceOver"
                            in Application settings; leave it alone otherwise.
   Accessibility takes effect at once: press "Re-check now" on the Permissions page.
   After granting Screen Recording, QUIT AND REOPEN the application: macOS hands that one
   only to a process that started after it was granted. Input Monitoring usually follows
   Accessibility; if Re-check still shows it missing, quit and reopen as well.

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

Speech goes through the system voice, or through VoiceOver if it is running.

$shipped module(s) included.
TXT

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
