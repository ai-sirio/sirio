#!/bin/bash
set -euo pipefail

usage() {
  echo "Usage: $0 <app-path> <volume-name> <output-dmg-path>" >&2
  exit 2
}

if [ $# -lt 3 ]; then
  usage
fi

APP_PATH="$1"
VOLUME_NAME="$2"
OUTPUT_DMG="$3"

if [ ! -d "$APP_PATH" ]; then
  echo "error: app not found at $APP_PATH" >&2
  exit 1
fi

STAGING=$(mktemp -d)
trap 'rm -rf "$STAGING"' EXIT

cp -R "$APP_PATH" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

rm -f "$OUTPUT_DMG"

# Left to itself, `-srcfolder` sizes the intermediate image from its own
# estimate of the folder, and on the hosted macOS runners that estimate comes
# up short: the v0.27.0 release and the 2026-09-27 nightly both died here with
# "No space left on device" after signing and notarisation had passed, while
# the nightly between them went through (actions/runner-images#4288). So the
# image is sized from the measured folder plus a quarter and 64 MB of room for
# the filesystem's own structures, and made HFS+, the filesystem that thread
# found `-srcfolder` sizes correctly. UDZO compresses the free space away, so
# the headroom costs nothing in the download.
STAGED_KB=$(du -sk "$STAGING" | cut -f1)
IMAGE_KB=$((STAGED_KB + STAGED_KB / 4 + 65536))

if ! hdiutil create -volname "$VOLUME_NAME" -srcfolder "$STAGING" -fs HFS+ \
  -size "${IMAGE_KB}k" -ov -format UDZO "$OUTPUT_DMG" >/dev/null; then
  # Which "no space" it was -- inside the image or on the runner's disk --
  # is the one thing the bare hdiutil error does not say.
  echo "error: hdiutil create failed; staged ${STAGED_KB} KB into a ${IMAGE_KB} KB image" >&2
  df -h "$(dirname "$OUTPUT_DMG")" "$STAGING" >&2 || true
  exit 1
fi

echo "$OUTPUT_DMG"
