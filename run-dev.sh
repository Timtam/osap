#!/usr/bin/env bash
# Runs the platform with every real module — the macOS counterpart of run-dev.ps1.
#
#   ./run-dev.sh                  build if needed, then run every module in modules/
#   ./run-dev.sh --release        the optimised build (see the note below)
#   ./run-dev.sh --build          force a rebuild first
#   ./run-dev.sh --only kontakt melodyne
#                                 just those, by directory name
#   ./run-dev.sh --examples       run the examples/ set instead
#
# One macOS-specific warning that will otherwise cost an hour: a loose binary is a
# DIFFERENT APPLICATION to macOS every time it is rebuilt. Accessibility permission is
# recorded against the executable's identity, and an unsigned binary's identity includes its
# contents — so every `cargo build` invalidates the grant and the system asks again. There is
# no way around it short of a bundle with a stable identifier, which is what
# package-macos.sh produces. If this becomes unbearable, package once and replace only the
# binary inside the bundle.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
profile="debug"
force_build=0
examples=0
only=()

while [ $# -gt 0 ]; do
  case "$1" in
    --release) profile="release"; shift ;;
    --build)   force_build=1; shift ;;
    --examples) examples=1; shift ;;
    --only) shift; while [ $# -gt 0 ] && [[ "$1" != --* ]]; do only+=("$1"); shift; done ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

exe="$root/target/$profile/automation-platform"

# Debug is the default because that is what a build-and-try loop wants. But the image
# matcher is a tight pixel loop, and unoptimised it measured TWELVE SECONDS for a single
# full-region template match on Windows — so any judgement about performance has to be made
# on --release, and a slow overlay is worth re-checking there before believing it.
if [ "$force_build" = "1" ] || [ ! -x "$exe" ]; then
  # Unlike Windows, LIBCLANG_PATH does not need setting: the Command Line Tools put libclang
  # where bindgen looks for it.
  if [ "$profile" = "release" ]; then
    (cd "$root" && cargo build --release)
  else
    (cd "$root" && cargo build)
  fi
fi

# A running copy holds nothing open on macOS the way a Windows .exe does, but two instances
# fighting over the same hotkeys is its own confusion.
pkill -f "target/$profile/automation-platform" 2>/dev/null || true

base="$root/modules"
[ "$examples" = "1" ] && base="$root/examples"

dirs=()
for dir in "$base"/*/; do
  [ -f "$dir/module.toml" ] || continue
  name="$(basename "$dir")"
  if [ ${#only[@]} -gt 0 ]; then
    match=0
    for want in "${only[@]}"; do [ "$name" = "$want" ] && match=1; done
    [ "$match" = "1" ] || continue
  fi
  dirs+=("$dir")
done

if [ ${#dirs[@]} -eq 0 ]; then
  echo "no modules found under $base" >&2
  exit 1
fi

echo "Running ${#dirs[@]} module(s) from $base"
exec "$exe" "${dirs[@]}"
