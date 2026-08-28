#!/bin/bash
# e2e contract test for the control socket. Requires Sirio RUNNING with
# at least one project+worktree; pass the worktree UUID as $1.
set -euo pipefail
WT="${1:?usage: e2e-control.sh <worktree-uuid>}"
SIRIOCTL="$(dirname "$0")/../rust/target/debug/sirioctl"
PANEL=$("$SIRIOCTL" panel create --worktree "$WT" --cmd "echo E2E-MARKER; sleep 1; exit 42")
echo "panel: $PANEL"
sleep 2
"$SIRIOCTL" panel read --id "$PANEL" | grep -q "E2E-MARKER" && echo "read: OK"
set +e
"$SIRIOCTL" panel wait --id "$PANEL" --timeout-ms 10000
CODE=$?
set -e
[ "$CODE" -eq 42 ] && echo "wait: OK (exit 42)" || { echo "wait: FAIL got $CODE"; exit 1; }
"$SIRIOCTL" notify --session "$PANEL" --status done && echo "notify: OK"
echo "E2E PASS"
