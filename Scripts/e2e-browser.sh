#!/bin/bash
# Browser surface e2e against a running Tiller.app. Pass the worktree UUID as
# the first argument; all content is served from a local temporary directory.
set -euo pipefail

WT="${1:?usage: e2e-browser.sh <worktree-uuid>}"
TILLERCTL="$(dirname "$0")/../Packages/TillerControl/.build/debug/tillerctl"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tiller-browser-e2e.XXXXXX")"
SERVER_PID=""

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

if [ ! -x "$TILLERCTL" ]; then
    echo "tillerctl not found or not executable: $TILLERCTL" >&2
    exit 1
fi
if ! "$TILLERCTL" ping >/dev/null 2>&1; then
    echo "Tiller.app is not running or its control socket is unavailable" >&2
    exit 1
fi

cat > "$TMP_DIR/fixture.html" <<'HTML'
<!doctype html>
<html>
  <head><title>Tiller Browser E2E Fixture</title></head>
  <body><p>Known browser e2e body text</p></body>
</html>
HTML

PORT="$(python3 - <<'PY'
import socket

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$TMP_DIR" \
    > "$TMP_DIR/server.log" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 50); do
    # stderr discarded: the first probes race the server's listen() and a
    # ConnectionRefusedError traceback printed next to "E2E PASS" teaches the
    # reader to ignore tracebacks.
    if python3 - "$PORT" 2>/dev/null <<'PY'
import socket
import sys

with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=0.2):
    pass
PY
    then
        ready=1
        break
    fi
    sleep 0.1
done
[ "$ready" -eq 1 ] || { echo "local fixture server did not start" >&2; exit 1; }

BASE_URL="http://127.0.0.1:$PORT/fixture.html"
OPEN_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$TILLERCTL" browser open "$BASE_URL" --id-format both)"
SURFACE="$(printf '%s\n' "$OPEN_OUTPUT" | awk -F '\t' 'NR == 1 { print $2 }')"
OPEN_TITLE="$(printf '%s\n' "$OPEN_OUTPUT" | awk -F '\t' 'NR == 1 { print $4 }')"
case "$SURFACE" in
    surface:*) ;;
    *) echo "browser open did not return a surface ref: $OPEN_OUTPUT" >&2; exit 1 ;;
esac
[ "$OPEN_TITLE" = "Tiller Browser E2E Fixture" ] || {
    echo "browser open title mismatch: $OPEN_OUTPUT" >&2
    exit 1
}

URL_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$TILLERCTL" browser get "$SURFACE" url)"
[ "$URL_OUTPUT" = "$BASE_URL" ] || {
    echo "browser get url mismatch: $URL_OUTPUT" >&2
    exit 1
}

TEXT_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$TILLERCTL" browser get "$SURFACE" text)"
printf '%s\n' "$TEXT_OUTPUT" | grep -Fq "Known browser e2e body text"

RELOAD_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$TILLERCTL" browser navigate "$SURFACE" reload)"
RELOAD_URL="$(printf '%s\n' "$RELOAD_OUTPUT" | awk -F '\t' 'NR == 1 { print $1 }')"
[ "$RELOAD_URL" = "$BASE_URL" ] || {
    echo "browser reload changed url: $RELOAD_OUTPUT" >&2
    exit 1
}

echo "E2E PASS"
