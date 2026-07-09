#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DMG_SCRIPT="$SCRIPT_DIR/../build-dmg.sh"

FIXTURE=$(mktemp -d)
MOUNT_DIR=$(mktemp -d)
cleanup() {
  hdiutil detach "$MOUNT_DIR" -quiet 2>/dev/null || true
  rm -rf "$FIXTURE" "$MOUNT_DIR"
}
trap cleanup EXIT

mkdir -p "$FIXTURE/Fake.app/Contents/MacOS"
echo "fake binary" > "$FIXTURE/Fake.app/Contents/MacOS/Fake"

OUTPUT_DMG="$FIXTURE/Fake.dmg"
"$BUILD_DMG_SCRIPT" "$FIXTURE/Fake.app" "FakeVolume" "$OUTPUT_DMG"

if [ ! -f "$OUTPUT_DMG" ]; then
  echo "FAIL: dmg not created at $OUTPUT_DMG" >&2
  exit 1
fi

hdiutil attach "$OUTPUT_DMG" -mountpoint "$MOUNT_DIR" -nobrowse -quiet

if [ ! -d "$MOUNT_DIR/Fake.app" ]; then
  echo "FAIL: Fake.app not found inside mounted dmg" >&2
  exit 1
fi
if [ ! -L "$MOUNT_DIR/Applications" ]; then
  echo "FAIL: Applications symlink not found inside mounted dmg" >&2
  exit 1
fi

echo "PASS: dmg contains app and Applications symlink"
