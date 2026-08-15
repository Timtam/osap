#!/usr/bin/env bash
# Creates a local code-signing identity, once, so that permissions survive a rebuild.
#
#   ./macos-signing-identity.sh
#
# Why this is worth five minutes: macOS records Accessibility, Screen Recording and Input
# Monitoring against an application's *code identity*. For an ad-hoc signature — which is
# what `codesign -s -` produces, and what this project uses by default — that identity is
# derived from the contents of the binary, so **every rebuild is a different application**
# and every permission has to be granted again. On Monterey that includes adding the app to
# Screen Recording by hand with the + button, because the prompt does not appear there. Doing
# that after every change is not a test cycle, it is a punishment.
#
# A self-signed certificate fixes it. The identity then becomes "the bundle id, signed by
# this certificate", and neither of those changes when the code does. Grant the three
# permissions once and they stay granted for every build afterwards.
#
# This is for local testing only. It is not a Developer ID, it does not satisfy Gatekeeper,
# and nothing signed with it should be distributed.
set -euo pipefail

NAME="OSAP Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

# Without -v. `-v` lists only VALID identities, validity requires a trust chain, and a
# self-signed certificate has none — so the identity this script creates does not appear in
# a `-v` listing at all. Checking that way is how the first version of this arrangement
# managed to create an identity, not find it, and quietly sign ad-hoc anyway.
if security find-identity -p codesigning 2>/dev/null | grep -qF "$NAME"; then
  echo "The identity \"$NAME\" already exists."
  echo "Checking that codesign can actually use it..."
  probe="$(mktemp -d)/probe"
  cp /usr/bin/true "$probe"
  if codesign --force --sign "$NAME" "$probe" 2>/dev/null; then
    echo "  It works. package-macos.sh will use it."
    exit 0
  fi
  echo "  It exists but codesign will not use it. That is usually the keychain refusing"
  echo "  access to the private key: the first use raises a dialog, and if it was dismissed"
  echo "  or answered with Deny, the answer sticks."
  echo ""
  echo "  Open Keychain Access, find \"$NAME\" under login > My Certificates, expand it,"
  echo "  double-click the private key beneath it, go to Access Control, and either choose"
  echo "  \"Allow all applications\" or add /usr/bin/codesign to the list."
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "==> Creating a self-signed code-signing certificate"
# `extendedKeyUsage=codeSigning` is the part that makes `security find-identity -p
# codesigning` list it; without it the certificate exists and codesign will not use it.
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" \
  -subj "/CN=$NAME" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" 2>/dev/null

openssl pkcs12 -export -out "$tmp/identity.p12" \
  -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -passout pass: 2>/dev/null

echo "==> Adding it to your login keychain"
echo "    macOS may ask you to unlock the keychain. That is expected."
# -T lets codesign use the private key without asking every time. macOS may still show a
# "wants to sign using a key in your keychain" dialog the first time; choose Always Allow.
security import "$tmp/identity.p12" -k "$KEYCHAIN" -P "" -T /usr/bin/codesign

# Trust it for code signing, so that tools which ask for VALID identities can see it too.
# Not strictly needed for codesign itself, which will use an untrusted certificate from the
# keychain quite happily — but every listing that filters on validity hides it otherwise,
# and that hiding is what made the first version of this fail silently. A user-domain trust
# setting, so no administrator password is needed; macOS may still ask to confirm.
if ! security add-trusted-cert -p codeSign -k "$KEYCHAIN" "$tmp/cert.pem" 2>/dev/null; then
  echo "    (could not mark it trusted — not fatal, codesign can still use it)"
fi

echo "==> Checking that codesign will actually use it"
probe="$tmp/probe"
cp /usr/bin/true "$probe"
if codesign --force --sign "$NAME" "$probe" 2>/dev/null; then
  echo "    It works."
else
  echo "    codesign could not use the new identity. If a keychain dialog appeared, answer"
  echo "    it with Always Allow and run this script again."
  exit 1
fi

cat <<DONE

==> Done.

The identity "$NAME" is in your login keychain. package-macos.sh will find it by name and
use it instead of an ad-hoc signature from now on.

Rebuild once more:

    ./bootstrap-macos.sh

The first launch after that will ask for its permissions one final time — it is a new
identity, so the old grants do not carry over. Grant all three, quit the application, open
it again, and they will stay granted through every rebuild after this one.

The first time codesign uses the key, macOS may ask whether to allow it. Choose
"Always Allow" so it does not ask again.
DONE
