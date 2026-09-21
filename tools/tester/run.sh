#!/bin/zsh
# What we ask the macOS tester to run when we need the exact text a command printed.
#
# The point of this file existing at all: the instruction we send him is then always the
# same single line — `zsh <path>/run.sh` — so from the second time it is Up-arrow and
# Return, and every fragile part (redirection order, stderr, where the file goes, what it is
# called) lives here, on our side, where getting it wrong costs an edit instead of a day.
#
# Two rules this file has to keep, both of which fail silently:
#
#   * LF line endings. A CRLF file makes zsh report "command not found" for commands that
#     plainly exist, which reads as a broken Mac. See .gitattributes.
#   * It is run as `zsh run.sh`, never `./run.sh`. That needs no executable bit — which a
#     Windows-to-macOS Dropbox sync does not carry — and asks nothing of Gatekeeper.
#
# Copy it into the shared folder next to the log, and edit the PAYLOAD block for whatever is
# being asked this time.

set -u

here=${0:a:h}
out="$here/out.txt"

{
  echo "=== run.sh $(date '+%Y-%m-%d %H:%M:%S') ==="
  echo "macOS: $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
  echo "hardware: $(sysctl -n hw.model), $(uname -m)"
  # The build the last session ran, from the log's header, so this output can be matched to
  # the download it came from without anybody reading anything out.
  if [ -f "$here/automation-platform.log" ]; then
    echo "application: $(grep '\[host\] version' "$here/automation-platform.log" | tail -1 | sed 's/^[0-9]* \[host\] //')"
  else
    echo "application: no automation-platform.log beside this script"
  fi
  echo

  # ---- PAYLOAD: replace everything between these two lines --------------------------
  #
  # Whatever goes here should be base-system only. Anything from the Xcode command line
  # tools — `sdef`, `git` on a machine that has never had them, `xcrun` — is a stub that
  # opens a modal installer offering a download of about a gigabyte. To a blind user that
  # is a window they cannot see, one Return away from something nobody wanted.

  echo "--- is VoiceOver running ---"
  pgrep -x VoiceOver >/dev/null && echo yes || echo no

  echo
  echo "--- what the application thinks of this Mac ---"
  log_file="$here/automation-platform.log"
  if [ -f "$log_file" ]; then
    grep -E '^\d+ \[(host|env|macos|speech)\]' "$log_file" | tail -40
  else
    echo "no automation-platform.log beside this script"
  fi

  # ---- end PAYLOAD -----------------------------------------------------------------

  echo
  echo "=== done ==="
} > "$out" 2>&1

# Said out loud, because the whole point is that nothing has to be read off the screen.
# `wc -c` answers with a byte count and the file name; zero is a real answer too — it means
# the commands ran and printed nothing — so it is reported rather than treated as a failure.
echo "written:"
wc -c "$out"
