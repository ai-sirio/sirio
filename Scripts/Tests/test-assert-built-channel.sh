#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/../assert-built-channel.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# A stand-in for a built sirioctl: prints whatever `sirioctl version --json`
# would, in serde_json's compact form, with the keys in whichever order the
# serializer chose -- the script must not depend on that.
stub() {
  local json="$1"
  cat > "$TMP/sirioctl" <<EOF
#!/bin/bash
[ "\$1" = "version" ] && [ "\$2" = "--json" ] || { echo "unexpected args: \$*" >&2; exit 9; }
printf '%s\n' '$json'
EOF
  chmod +x "$TMP/sirioctl"
}

# --- the shipped binary carries the channel and version it was built for -----
stub '{"appVersion":null,"channel":"nightly","cliVersion":"0.6.0-nightly.202609072133","matches":false}'
bash "$SCRIPT" "$TMP/sirioctl" nightly 0.6.0-nightly.202609072133 >/dev/null \
  || fail "a matching channel and version must pass"

# Key order must not matter, nor a running app answering with its version.
stub '{"cliVersion":"0.6.0","appVersion":"0.6.0","matches":true,"channel":"stable"}'
bash "$SCRIPT" "$TMP/sirioctl" stable 0.6.0 >/dev/null \
  || fail "key order and a present appVersion must not matter"

# --- the Zed post-mortem case: the build never saw SIRIO_RELEASE_CHANNEL -----
stub '{"appVersion":null,"channel":"dev","cliVersion":"0.6.0","matches":false}'
if ERR=$(bash "$SCRIPT" "$TMP/sirioctl" stable 0.6.0 2>&1); then
  fail "a binary compiled as dev must be refused"
fi
case "$ERR" in
  *'expected channel "stable"'*) ;;
  *) fail "a channel mismatch must be reported by name, got: $ERR" ;;
esac

# --- the version stamp did not reach the compiler --------------------------
stub '{"appVersion":null,"channel":"nightly","cliVersion":"0.6.0","matches":false}'
if ERR=$(bash "$SCRIPT" "$TMP/sirioctl" nightly 0.6.0-nightly.202609072133 2>&1); then
  fail "a binary at the wrong version must be refused"
fi
case "$ERR" in
  *'expected version "0.6.0-nightly.202609072133"'*) ;;
  *) fail "a version mismatch must be reported by name, got: $ERR" ;;
esac

# --- a prefix is not a match: 0.6.0 must not satisfy 0.6.0-nightly.1 ---------
stub '{"appVersion":null,"channel":"nightly","cliVersion":"0.6.0-nightly.1","matches":false}'
if bash "$SCRIPT" "$TMP/sirioctl" nightly 0.6.0 >/dev/null 2>&1; then
  fail "a version that merely starts with the expected one must be refused"
fi

# --- a sirioctl that cannot run is a failure, not an empty match -------------
if bash "$SCRIPT" "$TMP/does-not-exist" stable 0.6.0 >/dev/null 2>&1; then
  fail "a missing sirioctl must fail"
fi

echo "PASS: assert-built-channel refuses a mis-compiled binary"
