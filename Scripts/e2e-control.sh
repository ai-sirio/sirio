#!/bin/bash
# e2e contract test for the control socket. Requires Sirio RUNNING with
# at least one project+worktree; pass the worktree UUID as $1.
set -euo pipefail
WT="${1:?usage: e2e-control.sh <worktree-uuid>}"
SIRIOCTL="$(dirname "$0")/../rust/target/debug/sirioctl"
PANEL=$("$SIRIOCTL" panel create --worktree "$WT" --cmd "echo E2E-MARKER; sleep 1; exit 42")
echo "panel: $PANEL"
sleep 2
# `panel read` prints the pane's output base64-encoded.
"$SIRIOCTL" panel read --id "$PANEL" | { base64 -d 2>/dev/null || base64 -D; } | grep -q "E2E-MARKER" && echo "read: OK"
set +e
# `panel wait` exits 0 and prints the pane's exit code.
CODE="$("$SIRIOCTL" panel wait --id "$PANEL" --timeout-ms 10000)"
set -e
[ "$CODE" = 42 ] && echo "wait: OK (exit 42)" || { echo "wait: FAIL got $CODE"; exit 1; }
"$SIRIOCTL" notify --session "$PANEL" --status done && echo "notify: OK"
echo "E2E PASS"
