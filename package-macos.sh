#!/usr/bin/env bash
# Builds a distributable macOS application bundle — the counterpart to package.ps1.
#
#   ./package-macos.sh                 build, bundle, sign ad-hoc, zip
#   ./package-macos.sh --version 0.3.0 name the zip for that version instead of the date
#   ./package-macos.sh --no-zip        stage only, for looking at what would ship
#   ./package-macos.sh --no-build      package whatever is already in target/release
#   ./package-macos.sh --universal     join an Intel and an Apple-silicon build into one
#                                      binary (both must already exist — see below)
#
# This has to run ON a Mac (it compiles). Nobody on the project owns one, so it is also run
# by .github/workflows/macos-build.yml on a GitHub runner — that is currently the only way
# the macOS code gets LINKED rather than merely type-checked, and it is the thing to look at
# first when something here stops working.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version=""
do_zip=1
do_build=1
universal=0
while [ $# -gt 0 ]; do
  case "$1" in
    --version) version="$2"; shift 2 ;;
    --no-zip)  do_zip=0; shift ;;
    --no-build) do_build=0; shift ;;
    --universal) universal=1; do_build=0; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

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
rm -rf "$dist"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

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

# The documentation, if it has been built. Skipped rather than fatal: a tester without docs
# still has a working application.
if [ -d "$root/docs-site/build" ]; then
  cp -R "$root/docs-site/build" "$stage/docs"
  echo "Docs copied. NOTE: the offline rewrite (docs-offline.ps1) has no shell port yet —"
  echo "  these pages still contain absolute /osap/ links."
fi

# Ad-hoc signing, deliberately, and not as a formality.
#
# An UNSIGNED binary gets no stable identity, so macOS re-asks for Accessibility on every
# rebuild and sometimes refuses to remember it at all. Ad-hoc (`-` as the identity) is not
# notarised and Gatekeeper will still object on first launch, but it gives the bundle a
# consistent code identity, which is what TCC keys on. A Developer ID and notarisation are
# the real answer and need an Apple account — see docs/macos-port.md.
if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$app" 2>/dev/null && echo "Signed ad-hoc." \
    || echo "codesign failed — permissions may not stick between launches."
fi

cat > "$stage/README.txt" <<TXT
Automation Platform — macOS test build

FIRST RUN, in order. macOS will not tell you when a step is missing; it will just
behave as if the application is broken, so please do them all.

1. Move this whole folder somewhere you can write to (Documents or the Desktop).
   The application writes its log and its settings NEXT TO the .app, and reads its
   modules from the "modules" folder beside it, so keep the folder together.

2. Remove the download quarantine flag, or macOS will refuse to open the app:
       xattr -dr com.apple.quarantine "$APP_NAME.app"

3. Open $APP_NAME.app. Nothing visible happens: it is a menu-bar application, not a
   window. With VoiceOver, press VO-M twice to reach the menu-bar extras.

4. Grant two permissions in System Settings > Privacy & Security. Neither can be
   granted by the application itself:
     - Accessibility      — without it, nothing can be read or clicked.
     - Screen Recording   — without it, screen capture silently returns a picture of
                            the wallpaper instead of failing, so this one is worth
                            checking twice.
   After granting either, QUIT AND REOPEN the application. macOS only hands the new
   permission to a process that started after it was granted.

5. To record a plugin window for us: put it in front and press
       Command-Shift-F9
   That writes everything about it to the log and a picture of it next to the .app,
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
echo "Staged $shipped module(s) into $stage  ($size)"

if [ "$do_zip" = "1" ]; then
  zip="$dist/$APP_NAME-macos-$label.zip"
  # ditto, not zip: it preserves the bundle's structure and extended attributes, and it is
  # what Apple's own notarisation flow expects. A plain `zip` can produce a bundle that
  # launches on the machine that made it and nowhere else.
  (cd "$dist" && ditto -c -k --sequesterRsrc --keepParent "$APP_NAME" "$zip")
  echo "Wrote $zip  ($(du -sh "$zip" | cut -f1))"
fi
