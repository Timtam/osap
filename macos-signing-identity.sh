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

# The SYSTEM openssl, when it is there, in preference to whatever is first in PATH.
#
# macOS ships LibreSSL at /usr/bin/openssl, and LibreSSL writes a PKCS#12 container the
# Security framework can read. A Homebrew OpenSSL 3 earlier in PATH writes one with a SHA-256
# MAC, which the framework rejects as "MAC verification failed during PKCS12 import (wrong
# password?)" — a message about the password, for a fault that has nothing to do with it.
# The tester hit exactly that. Choosing the reader's own implementation avoids the argument.
OPENSSL=openssl
[ -x /usr/bin/openssl ] && OPENSSL=/usr/bin/openssl

echo "==> Creating a self-signed code-signing certificate"
echo "    openssl: $OPENSSL — $("$OPENSSL" version 2>&1 | head -1)"

# A CONFIG FILE rather than -addext, because macOS does not ship OpenSSL.
#
# /usr/bin/openssl on macOS is LibreSSL, and LibreSSL's `req` has no -addext — that flag
# arrived with OpenSSL 1.1.1. The first version of this script used it and redirected stderr
# to /dev/null, so on Monterey it printed the line above, died on the next command, and left
# the tester with one line of output and an ad-hoc signature he had been told was fixed.
# `set -e` did the killing; the redirect did the hiding.
#
# Extensions in a config file work in both, and every failure below now shows its own error.
cat > "$tmp/openssl.cnf" <<CNF
[ req ]
distinguished_name = dn
x509_extensions    = v3
prompt             = no

[ dn ]
CN = $NAME

[ v3 ]
basicConstraints       = critical,CA:false
keyUsage               = critical,digitalSignature
extendedKeyUsage       = critical,codeSigning
CNF

if ! "$OPENSSL" req -x509 -newkey rsa:2048 -nodes -days 3650 \
      -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -config "$tmp/openssl.cnf"; then
  echo ""
  echo "    Creating the certificate failed, and the error is just above this line."
  echo "    Please send it along — this step is the one that decides whether permissions"
  echo "    survive a rebuild, and it has failed silently once already."
  exit 1
fi

# A REAL PASSWORD and a SHA-1 MAC, because macOS is the reader and it is the older one.
#
# The tester's run got "SecKeychainItemImport: MAC verification failed during PKCS12 import (wrong
# password?)". The password is a red herring — that message is what the Security framework
# says when it cannot verify the container's MAC, and OpenSSL 3 writes one with SHA-256 by
# default while the framework expects SHA-1. LibreSSL, which is what /usr/bin/openssl is,
# already defaults to SHA-1; a Homebrew OpenSSL earlier in PATH does not, which is why this
# failed on his machine and not in the version I reasoned about.
#
# So the MAC and the encryption are named rather than left to a default that varies by
# implementation, and the whole thing is retried without them if an implementation rejects
# the flags — LibreSSL's pkcs12 does not take every one of them.
#
# The empty password goes too. "" is handled differently on either side of this exchange, and
# it makes the one error message you get ambiguous exactly when you can least afford it. A
# throwaway password lives in this shell for two commands and dies with the temporary
# directory.
P12PASS="osap-$$-$(date +%s)"
if ! "$OPENSSL" pkcs12 -export -out "$tmp/identity.p12" \
      -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -passout "pass:$P12PASS" \
      -macalg sha1 -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES 2>"$tmp/p12.err"; then
  echo "    (this openssl would not take the compatibility options; trying without them)"
  if ! "$OPENSSL" pkcs12 -export -out "$tmp/identity.p12" \
        -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -passout "pass:$P12PASS"; then
    echo ""
    echo "    Packing the certificate for the keychain failed. First attempt said:"
    sed 's/^/      /' "$tmp/p12.err"
    exit 1
  fi
fi

echo "==> Adding it to your login keychain"
echo "    If macOS asks to unlock the keychain or to allow access to the key, say yes."
echo "    It often asks for neither. Nothing is wrong when it does not — the earlier wording"
echo "    promised a prompt, and a tester then went looking for the fault in the wrong place."
# -T lets codesign use the private key without asking every time. macOS may still show a
# "wants to sign using a key in your keychain" dialog the first time; choose Always Allow.
security import "$tmp/identity.p12" -k "$KEYCHAIN" -P "$P12PASS" -T /usr/bin/codesign

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
