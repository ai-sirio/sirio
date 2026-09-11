#!/bin/bash
# Ask a built sirioctl what it was compiled as, and refuse a mismatch.
#
# The spec §3.5 gate test in sirio_control runs on the CI gate's build; this
# runs on the binary that ships. `sirioctl version --json` reports the
# compiled channel and version and needs no running app, so a release whose
# channel silently fell back to `dev` -- the Zed post-mortem -- fails here,
# on every platform, before anything is signed or uploaded.
#
# Substring matching on the compact JSON on purpose: no jq on the self-hosted
# macOS runner, and serde_json's key order is not something to depend on.
set -euo pipefail

usage() {
  echo "Usage: $0 <sirioctl> <channel> <version>" >&2
  exit 2
}

if [ $# -ne 3 ]; then
  usage
fi

SIRIOCTL="$1"
CHANNEL="$2"
VERSION="$3"

OUTPUT=$("$SIRIOCTL" version --json)

case "$OUTPUT" in
  *"\"channel\":\"${CHANNEL}\""*) ;;
  *)
    echo "error: $SIRIOCTL reports $OUTPUT; expected channel \"${CHANNEL}\" -- the build did not see SIRIO_RELEASE_CHANNEL" >&2
    exit 1 ;;
esac

case "$OUTPUT" in
  *"\"cliVersion\":\"${VERSION}\""*) ;;
  *)
    echo "error: $SIRIOCTL reports $OUTPUT; expected version \"${VERSION}\" -- the workspace version was not stamped before the build" >&2
    exit 1 ;;
esac

echo "$OUTPUT"
