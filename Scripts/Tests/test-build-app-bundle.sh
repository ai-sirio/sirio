#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE_SCRIPT="$SCRIPT_DIR/../build-app-bundle.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# `codesign` exists only on macOS, and even there this test must never reach a
# real keychain. The stub records its arguments so the assertions can check the
# hardened runtime is requested — the flag notarization refuses submissions
# without, and the one whose absence would only surface at the Apple round-trip.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/codesign" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" > "$CODESIGN_ARGS"
exit 0
EOF
chmod +x "$FIXTURE/bin/codesign"

printf '#!/bin/sh\nexit 0\n' > "$FIXTURE/sirio"
chmod +x "$FIXTURE/sirio"

CODESIGN_ARGS="$FIXTURE/codesign.args" \
CODESIGN_IDENTITY="Developer ID Application: Test" \
PATH="$FIXTURE/bin:$PATH" \
  "$BUNDLE_SCRIPT" "$FIXTURE/sirio" "0.6.0" "$FIXTURE/Sirio.app" >/dev/null

for path in \
  "$FIXTURE/Sirio.app/Contents/Info.plist" \
  "$FIXTURE/Sirio.app/Contents/MacOS/sirio" \
  "$FIXTURE/Sirio.app/Contents/Resources/icon.icns"
do
  if [ ! -f "$path" ]; then
    echo "FAIL: missing $path" >&2
    exit 1
  fi
done

if [ ! -x "$FIXTURE/Sirio.app/Contents/MacOS/sirio" ]; then
  echo "FAIL: the bundled binary must be executable" >&2
  exit 1
fi

PLIST="$FIXTURE/Sirio.app/Contents/Info.plist"

# The generated plist must match the identity source, not a literal retyped in
# the script: source the source itself and assert what it declares is what the
# bundle carries (#304).
. "$SCRIPT_DIR/../identity.sh"
for value in "$SIRIO_APP_IDENTIFIER" "$SIRIO_DISPLAY_NAME" 15.0 public.app-category.developer-tools; do
  if ! grep -q "$value" "$PLIST"; then
    echo "FAIL: Info.plist is missing '$value'" >&2
    cat "$PLIST" >&2
    exit 1
  fi
done

# And no packaging script may retype an identifier literal: the identity source
# is the only place an identifier is written (#304).
for script in "$BUNDLE_SCRIPT" "$SCRIPT_DIR/../build-dev.sh"; do
  if grep -Eq '(dev|ai|com)\.sirio' "$script"; then
    echo "FAIL: $script retypes an identifier literal; take it from Scripts/identity.sh" >&2
    exit 1
  fi
done

# The version belongs in both CFBundleShortVersionString and CFBundleVersion.
COUNT=$(grep -c '<string>0.6.0</string>' "$PLIST" || true)
if [ "$COUNT" != "2" ]; then
  echo "FAIL: expected the version in both version keys, found $COUNT" >&2
  cat "$PLIST" >&2
  exit 1
fi

# A nightly (#317) is versioned `0.6.0-nightly.<stamp>`. Apple documents
# CFBundleVersion as period-separated integers only, so the prerelease part
# must be stripped there; CFBundleShortVersionString, the one Finder shows,
# keeps the full version so a nightly is recognisable as one.
CODESIGN_ARGS="$FIXTURE/codesign-nightly.args" \
CODESIGN_IDENTITY="Developer ID Application: Test" \
PATH="$FIXTURE/bin:$PATH" \
  "$BUNDLE_SCRIPT" "$FIXTURE/sirio" "0.6.0-nightly.202609072133" "$FIXTURE/Nightly.app" >/dev/null
NIGHTLY_PLIST="$FIXTURE/Nightly.app/Contents/Info.plist"
if ! grep -A1 'CFBundleShortVersionString' "$NIGHTLY_PLIST" | grep -q '<string>0.6.0-nightly.202609072133</string>'; then
  echo "FAIL: CFBundleShortVersionString must carry the full nightly version" >&2
  cat "$NIGHTLY_PLIST" >&2
  exit 1
fi
if ! grep -A1 '<key>CFBundleVersion</key>' "$NIGHTLY_PLIST" | grep -q '<string>0.6.0</string>'; then
  echo "FAIL: CFBundleVersion must be the numeric part of a nightly version only" >&2
  cat "$NIGHTLY_PLIST" >&2
  exit 1
fi

if ! grep -q -- "--options runtime" "$FIXTURE/codesign.args"; then
  echo "FAIL: codesign must request the hardened runtime" >&2
  cat "$FIXTURE/codesign.args" >&2
  exit 1
fi

if "$BUNDLE_SCRIPT" "$FIXTURE/missing-binary" "0.6.0" "$FIXTURE/X.app" >/dev/null 2>&1; then
  echo "FAIL: a missing binary must exit non-zero" >&2
  exit 1
fi

# Asserted by the error message rather than just a non-zero exit, because every
# later guard in the script would also reject this invocation — the point is
# that the *version* is what gets named. An empty version otherwise substitutes
# into `<string></string>` in both version keys and yields a bundle that signs,
# notarizes and launches with no version anywhere a user or Sparkle can read one.
ERR=$("$BUNDLE_SCRIPT" "$FIXTURE/sirio" "" "$FIXTURE/Y.app" 2>&1 >/dev/null || true)
case "$ERR" in
  *"version must not be empty"*) ;;
  *)
    echo "FAIL: an empty version must be rejected by name, got: $ERR" >&2
    exit 1
    ;;
esac

echo "PASS: app bundle layout and signing flags"
