#!/bin/bash
# e2e contract test for the control socket. Requires Tiller RUNNING with
# at least one project+worktree; pass the worktree UUID as $1.
set -euo pipefail
WT="${1:?usage: e2e-control.sh <worktree-uuid>}"
TILLERCTL="$(dirname "$0")/../rust/target/debug/tillerctl"
PANEL=$("$TILLERCTL" panel create --worktree "$WT" --cmd "echo E2E-MARKER; sleep 1; exit 42")
echo "panel: $PANEL"
sleep 2
"$TILLERCTL" panel read --id "$PANEL" | grep -q "E2E-MARKER" && echo "read: OK"
set +e
"$TILLERCTL" panel wait --id "$PANEL" --timeout-ms 10000
CODE=$?
set -e
[ "$CODE" -eq 42 ] && echo "wait: OK (exit 42)" || { echo "wait: FAIL got $CODE"; exit 1; }
"$TILLERCTL" notify --session "$PANEL" --status done && echo "notify: OK"
echo "E2E PASS"
