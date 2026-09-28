#!/bin/bash
set -euo pipefail

# End-to-end test of text selection (rust/crates/sirio_ui/src/text_selection.rs
# and selectable_markdown.rs): a real pointer drag across text a surface draws,
# a real Ctrl+C, and the real clipboard, on the real views -- Settings, the
# change request tab (title, meta line, Markdown description), the file view's
# Markdown preview, the chat (a tool call's output, an error card), the diff in
# both view modes -- and, in the `sirio` crate, through the real workspace root.
#
# Why beside the unit tests: text is selectable only if a surface draws it as
# `selectable_text`, and nothing but a drag on the drawn frame notices the day a
# surface goes back to a plain string. The host half is the same: the key
# binding, the focus sink and the copy action on each render branch are three
# separate wires, and losing any one of them is silent.
#
# What it also holds: Ctrl+C is never taken from anything that is not a
# selection (a focused terminal's SIGINT, the file editor's own Copy), the
# selection follows a pointer past a short line and onto a clipped title's
# ellipsis, and a link in rendered Markdown is still followed by a click but
# not by the release that ends a drag.
#
# The artifact: --out-dir DIR (default artifacts/text-selection-e2e-<stamp>-<pid>)
# keeps transcript.log, the per-test verdicts. Rerunning the script reproduces it.
#
# Usage: Scripts/Tests/test-text-selection-e2e.sh [--out-dir DIR]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CARGO_DIR="$REPO_ROOT/rust"

OUT_DIR=""
while [ $# -gt 0 ]; do
    case "$1" in
        --out-dir) OUT_DIR="${2:?--out-dir needs a directory}"; shift 2 ;;
        -h|--help) sed -n '3,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$OUT_DIR" ] || OUT_DIR="$REPO_ROOT/artifacts/text-selection-e2e-$(date +%Y%m%d%H%M%S)-$$"
mkdir -p "$OUT_DIR"
TRANSCRIPT="$OUT_DIR/transcript.log"
: > "$TRANSCRIPT"

command -v cargo-nextest >/dev/null 2>&1 || {
    echo "cargo-nextest is required: cargo install cargo-nextest --locked" >&2
    exit 2
}

# Every test that drives a drag and a copy. A name filter, not a list of
# names, so a new surface's test joins by being named for what it proves.
UI_FILTER='test(/^text_selection::/) | test(/^selectable_markdown::/) | test(/selected_and_copied/) | test(/can_be_copied/) | test(/selected_and_copied_in_unified_and_split_view/) | test(/^chat::tool_calls::tests::the_selectable_output_well/)'
HOST_FILTER='test(/text_selected_in_/)'

run() {
    local package="$1" filter="$2"
    echo "== cargo nextest run -p $package -E '$filter'" | tee -a "$TRANSCRIPT"
    # A red test must reach the tally below, not abort here with no verdict.
    (cd "$CARGO_DIR" && cargo nextest run -p "$package" -E "$filter" --no-fail-fast 2>&1) | tee -a "$TRANSCRIPT" || true
}

run sirio_ui "$UI_FILTER"
run sirio "$HOST_FILTER"

# A filter that selects nothing prints a green "0 tests run"; count what ran.
# `grep -c` exits 1 when it counts nothing, which `set -e` would take for a failure.
ran=$(grep -cE "^\s+PASS " "$TRANSCRIPT" || true)
# nextest prints a failing test again in its closing list; count each once.
failed=$( (grep -E "^\s+FAIL " "$TRANSCRIPT" || true) | sed -E 's/\[[^]]*\]//; s/\([0-9]+\/[0-9]+\)//' | sort -u | wc -l | tr -d ' ')
if [ "$failed" != "0" ]; then
    echo "TEXT SELECTION E2E FAILED: $failed failing (see $TRANSCRIPT)" >&2
    exit 1
fi
if [ "$ran" -lt 30 ]; then
    echo "TEXT SELECTION E2E FAILED: only $ran tests matched -- a filter drifted (see $TRANSCRIPT)" >&2
    exit 1
fi
echo "TEXT SELECTION E2E OK ($ran tests; transcript: $TRANSCRIPT)"
