#!/bin/bash
# Browser surface e2e against a running Sirio instance. Pass the worktree UUID as
# the first argument; all content is served from a local temporary directory.
set -euo pipefail

WT="${1:?usage: e2e-browser.sh <worktree-uuid>}"
SIRIOCTL="$(dirname "$0")/../rust/target/debug/sirioctl"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/sirio-browser-e2e.XXXXXX")"
SERVER_PID=""

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT INT TERM

if [ ! -x "$SIRIOCTL" ]; then
    echo "sirioctl not found or not executable: $SIRIOCTL" >&2
    exit 1
fi
if ! "$SIRIOCTL" ping >/dev/null 2>&1; then
    echo "Sirio is not running or its control socket is unavailable" >&2
    exit 1
fi

cat > "$TMP_DIR/fixture.html" <<'HTML'
<!doctype html>
<html>
  <head><title>Sirio Browser E2E Fixture</title></head>
  <body>
    <p>Known browser e2e body text</p>
    <form onsubmit="event.preventDefault(); document.querySelector('#result').textContent = document.querySelector('#name').value">
      <label>Name <input id="name" aria-label="Name"></label>
      <button id="submit" type="submit">Submit</button>
    </form>
    <p id="result">Waiting</p>
  </body>
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
OPEN_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser open "$BASE_URL" --id-format both)"
SURFACE="$(printf '%s\n' "$OPEN_OUTPUT" | awk -F '\t' 'NR == 1 { print $2 }')"
OPEN_TITLE="$(printf '%s\n' "$OPEN_OUTPUT" | awk -F '\t' 'NR == 1 { print $4 }')"
case "$SURFACE" in
    surface:*) ;;
    *) echo "browser open did not return a surface ref: $OPEN_OUTPUT" >&2; exit 1 ;;
esac
[ "$OPEN_TITLE" = "Sirio Browser E2E Fixture" ] || {
    echo "browser open title mismatch: $OPEN_OUTPUT" >&2
    exit 1
}

URL_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser get "$SURFACE" url)"
[ "$URL_OUTPUT" = "$BASE_URL" ] || {
    echo "browser get url mismatch: $URL_OUTPUT" >&2
    exit 1
}

TEXT_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser get "$SURFACE" text)"
printf '%s\n' "$TEXT_OUTPUT" | grep -Fq "Known browser e2e body text"

RELOAD_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser navigate "$SURFACE" reload)"
RELOAD_URL="$(printf '%s\n' "$RELOAD_OUTPUT" | awk -F '\t' 'NR == 1 { print $1 }')"
[ "$RELOAD_URL" = "$BASE_URL" ] || {
    echo "browser reload changed url: $RELOAD_OUTPUT" >&2
    exit 1
}

SNAPSHOT_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser snapshot "$SURFACE" --json)"
read -r GENERATION INPUT_REF BUTTON_REF <<EOF
$(SNAPSHOT_OUTPUT="$SNAPSHOT_OUTPUT" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["SNAPSHOT_OUTPUT"])[0]
nodes = json.loads(payload["nodes"])
input_ref = next(node["ref"] for node in nodes if node["name"] == "Name")
button_ref = next(node["ref"] for node in nodes if node["name"] == "Submit")
print(payload["generation"], input_ref, button_ref)
PY
)
EOF

TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser act "$SURFACE" fill \
    --ref "$INPUT_REF" --value "Ada" --generation "$GENERATION" >/dev/null
TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser act "$SURFACE" click \
    --ref "$BUTTON_REF" --generation "$GENERATION" >/dev/null
TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser wait "$SURFACE" \
    --text "Ada" --timeout-ms 3000 >/dev/null
LOOP_TEXT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser get "$SURFACE" text)"
printf '%s\n' "$LOOP_TEXT" | grep -Fq "Ada"

STALE_OUTPUT="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser snapshot "$SURFACE" --json)"
read -r STALE_GENERATION STALE_REF <<EOF
$(STALE_OUTPUT="$STALE_OUTPUT" python3 - <<'PY'
import json
import os

payload = json.loads(os.environ["STALE_OUTPUT"])[0]
nodes = json.loads(payload["nodes"])
print(payload["generation"], nodes[0]["ref"])
PY
)
EOF
TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser navigate "$SURFACE" reload >/dev/null
set +e
STALE_ERROR="$(TILLER_WORKTREE_ID="$WT" "$SIRIOCTL" browser act "$SURFACE" click \
    --ref "$STALE_REF" --generation "$STALE_GENERATION" 2>&1)"
STALE_STATUS=$?
set -e
[ "$STALE_STATUS" -ne 0 ] || { echo "stale ref unexpectedly succeeded" >&2; exit 1; }
printf '%s\n' "$STALE_ERROR" | grep -Fq "stale_ref"

echo "E2E PASS"
