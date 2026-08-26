#!/bin/bash
# Single verification gate for the whole repo -- run before considering any task done.
#
# This used to xcodegen/xcodebuild the Swift/Xcode project. That project is gone; the only
# build target left in this repo is the Rust/gpui workspace under rust/, and this gate now
# builds and tests it.
#
# This is deliberately narrower than Scripts/ci-linux.sh, the fuller Rust gate that already
# existed alongside the Swift project (see docs/superpowers/plans/2026-08-13-linux-
# verification-gate.md and Scripts/Tests/test-ci-linux.sh) and additionally checks
# formatting, lints, macOS/Windows cross-target compilation, and drives a real headless
# instance of the app. Two of those checks are not clean on this tree right now, for
# reasons that have nothing to do with removing the Swift project:
#
#   - `cargo fmt --check` currently reports pre-existing drift in crates/tiller,
#     crates/tiller_agents, crates/tiller_git, crates/tiller_terminal, crates/tiller_theme,
#     crates/tiller_ui, and crates/tiller_usage.
#   - `cargo clippy` is not clean workspace-wide either -- ci-linux.sh's own
#     `--exclude tiller --exclude tiller_ui` already documents current warnings in those
#     two crates, routed to their current owners rather than gated here.
#
# Wiring either check into the one gate every task is told to pass before either is
# actually clean would make "CI OK" permanently unreachable for reasons unrelated to
# whatever change is under review -- which teaches people to ignore the gate, the same
# failure mode ci-linux.sh's own sccache-fallback and opt-in-ACP stages exist to avoid. Add
# fmt/clippy stages here once they are clean workspace-wide; until then, run
# `Scripts/ci-linux.sh` for the fuller, stricter check (it stays a separate, heavier gate on
# purpose -- see its own header for what else it covers and why some of its stages are
# allowed to SKIP or report BLOCKED rather than FAILED).
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; run source ~/.cargo/env"
    exit 1
fi

# libghostty-vt-sys (a tiller_terminal dependency since #27) shells out to
# `zig build`, and upstream pins Zig at EXACTLY 0.15.2 -- a newer Zig fails
# too, so the upgrade reflex makes it worse; 0.15.2 must be installed
# alongside and found first on PATH. Without this preflight the failure
# surfaces as an inscrutable build-script panic from a crates.io crate (#63).
ZIG_REQUIRED="0.15.2"
if ! command -v zig >/dev/null 2>&1; then
    echo "zig not found: libghostty-vt-sys needs Zig exactly ${ZIG_REQUIRED} on PATH (#63)"
    exit 1
fi
ZIG_VERSION="$(zig version 2>/dev/null || true)"
if [[ "$ZIG_VERSION" != "$ZIG_REQUIRED" ]]; then
    echo "zig ${ZIG_VERSION:-unknown} found, but libghostty-vt-sys builds only with exactly ${ZIG_REQUIRED} (newer fails too); install ${ZIG_REQUIRED} alongside and put it first on PATH (#63)"
    exit 1
fi

cd rust

echo "==> cargo build --workspace"
cargo build --workspace

# Whole-workspace, not per-crate: run every test binary even when one fails, so this
# gate reports the complete failure set under load. The two formerly timing-sensitive
# tests are now race-free at their roots; ci-linux.sh retains a short historical note.
echo "==> cargo test --workspace --no-fail-fast"
cargo test --workspace --no-fail-fast

echo "CI OK"
