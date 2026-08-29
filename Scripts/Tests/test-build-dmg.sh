#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DMG_SCRIPT="$SCRIPT_DIR/../build-dmg.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# `hdiutil` exists only on macOS. Stubbing it keeps this test runnable on any
# POSIX host and puts the assertion where the script's own logic actually is:
# the staging directory it hands to hdiutil. Whether hdiutil compresses
# correctly is Apple's problem, not this script's.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/hdiutil" <<'EOF'
#!/bin/bash
while [ $# -gt 0 ]; do
  case "$1" in
    -srcfolder) ls -A "$2" > "$DMG_SRCFOLDER_LISTING"; shift 2 ;;
    *) shift ;;
  esac
done
exit 0
EOF
chmod +x "$FIXTURE/bin/hdiutil"

mkdir -p "$FIXTURE/Sirio.app/Contents/MacOS"
touch "$FIXTURE/Sirio.app/Contents/MacOS/sirio"

DMG_SRCFOLDER_LISTING="$FIXTURE/listing.txt" \
PATH="$FIXTURE/bin:$PATH" \
  "$DMG_SCRIPT" "$FIXTURE/Sirio.app" "Sirio" "$FIXTURE/Sirio-0.6.0.dmg" >/dev/null

if ! grep -qx "Sirio.app" "$FIXTURE/listing.txt"; then
  echo "FAIL: the staged folder must contain Sirio.app" >&2
  cat "$FIXTURE/listing.txt" >&2
  exit 1
fi
if ! grep -qx "Applications" "$FIXTURE/listing.txt"; then
  echo "FAIL: the staged folder must contain the /Applications symlink" >&2
  cat "$FIXTURE/listing.txt" >&2
  exit 1
fi

if "$DMG_SCRIPT" "$FIXTURE/does-not-exist.app" "Sirio" "$FIXTURE/x.dmg" >/dev/null 2>&1; then
  echo "FAIL: a missing .app must exit non-zero" >&2
  exit 1
fi

echo "PASS: dmg staging layout"
