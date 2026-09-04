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

# Homebrew, and the reason this is not just `have brew`.
#
# On Apple silicon it installs to /opt/homebrew/bin, which is NOT on the default PATH — the
# installer prints the two lines that fix that at the end of a long run, where they scroll
# past. Somebody who cannot see the screen then opens a new Terminal, runs this, and is told
# Homebrew is not installed; installs it again; and is told the same thing. The loop has no
# exit, and it fires on the first machine this project has ever been set up on that is not
# Intel. On Intel it lands in /usr/local/bin, which IS on the default PATH, which is why
# nobody hit it before.
if ! have brew; then
  for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do
    if [ -x "$b" ]; then
      eval "$("$b" shellenv)"
      echo "    Found Homebrew at $b and put it on the PATH for this run."
      echo "    To make that permanent:  echo 'eval \"\$($b shellenv)\"' >> ~/.zprofile"
      break
    fi
  done
fi
if ! have brew; then
  step "Homebrew is not installed"
  echo "    It is needed for cmake. Install it from https://brew.sh, then run this again."
  echo "    If you HAVE just installed it and this still says otherwise, it is on the PATH"
  echo "    problem above rather than missing — run this and try again:"
  echo "        eval \"\$(/opt/homebrew/bin/brew shellenv)\""
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

# Offered before the build rather than after, because taking it changes what the build
# produces and the alternative costs the tester three permission grants per rebuild.
# Without -v: that flag means "valid", validity means a trust chain, and a self-signed
# certificate has none. Asking for valid identities is how this check used to miss the very
# identity it was looking for.
if ! security find-identity -p codesigning 2>/dev/null | grep -qF "OSAP Local Signing"; then
  step "One thing worth doing first"
  echo "    There is no local signing identity, so this build will be signed ad-hoc — and"
  echo "    macOS will therefore treat every rebuild as a different application and forget"
  echo "    its Accessibility, Screen Recording and Input Monitoring permissions each time."
  echo ""
  echo "    ./macos-signing-identity.sh  creates one, once, and stops that happening."
  echo "    Carrying on without it now; nothing breaks, it is just tedious."
fi

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
