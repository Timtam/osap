#!/usr/bin/env bash
# Everything a Mac needs, in one command.
#
#   ./bootstrap-macos.sh
#
# Written for someone who is testing this and did not sign up to be its build engineer —
# and who may well not be able to see the screen. So: no decisions, no options, one line of
# output per step saying what is happening and how long it will take, and a clear sentence
# at the end saying what to do next. It installs only what is missing and it is safe to run
# again.
#
# Building locally rather than downloading a build has two advantages worth the wait. It
# produces a binary for THIS Mac, so the Intel-versus-Apple-silicon question disappears; and
# once it is set up, testing a fix is `git pull && ./bootstrap-macos.sh` rather than waiting
# for someone else to publish one.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$root"

step() { printf '\n==> %s\n' "$1"; }
have() { command -v "$1" >/dev/null 2>&1; }

step "Checking what this Mac already has"
arch="$(uname -m)"
case "$arch" in
  arm64) echo "    Apple silicon ($arch)." ;;
  x86_64) echo "    Intel ($arch)." ;;
  *) echo "    Unrecognised architecture: $arch. Carrying on anyway." ;;
esac
sw_vers -productVersion | sed 's/^/    macOS /'

# The compiler and the headers. `xcode-select -p` succeeding is the check; the install is
# a GUI prompt, so it cannot be done silently and the script waits rather than failing.
if ! xcode-select -p >/dev/null 2>&1; then
  step "Installing the Xcode command line tools"
  echo "    A system dialog will appear. Accept it, wait for it to finish, then press Return here."
  xcode-select --install >/dev/null 2>&1 || true
  read -r _
fi
echo "    Command line tools: $(xcode-select -p)"

if ! have brew; then
  step "Homebrew is not installed"
  echo "    It is needed for cmake. Install it from https://brew.sh and run this script again."
  exit 1
fi

# cmake builds wxWidgets; ninja only makes that faster and is optional.
missing=()
have cmake || missing+=(cmake)
have ninja || missing+=(ninja)
if [ ${#missing[@]} -gt 0 ]; then
  step "Installing ${missing[*]}"
  brew install "${missing[@]}"
fi

if ! have cargo; then
  step "Installing Rust"
  echo "    This is the official installer and takes a minute."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
echo "    Rust: $(cargo --version)"

step "Building"
echo "    The FIRST build compiles wxWidgets from source and takes roughly 10 to 25 minutes."
echo "    Later builds take a minute or two. It is not stuck; it is compiling."
cargo build --release

step "Packaging"
./package-macos.sh --no-build

app="$root/dist/AutomationPlatform/AutomationPlatform.app"
cat <<DONE

==> Done.

The application is at:
    $app

What to do next, in order — macOS will not tell you when a step is missing, it will just
behave as if the application is broken:

  1. open "$app"
     Nothing visible happens. It is a menu-bar application, not a window. With VoiceOver,
     VO-M twice reaches the menu-bar extras.

  2. Grant Accessibility and Screen Recording in
     System Settings > Privacy & Security, then QUIT AND REOPEN the application. macOS only
     gives a newly granted permission to a process that started after it was granted.

  3. Everything it knows about this Mac is written to
         $root/dist/AutomationPlatform/automation-platform.log
     starting with a block that lists the permissions it actually has. That file is what to
     send back.

To rebuild after a change:  git pull && ./bootstrap-macos.sh
DONE
