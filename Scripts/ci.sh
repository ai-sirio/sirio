#!/bin/bash
# The single verification gate for this repo: compile every target in the Rust
# workspace, then run its tests. Run it before considering any task done.
#
# Deliberately narrower than Scripts/ci-linux.sh, the fuller gate, which also
# checks formatting and lints, cross-compiles for macOS and Windows, and drives
# a real headless instance of the app. Neither `cargo fmt --check` nor `cargo
# clippy` is clean workspace-wide today, and wiring a permanently-red check
# into the one gate every task must pass teaches people to ignore the gate --
# the same failure mode ci-linux.sh's sccache fallback exists to avoid. Add
# them here once they are clean.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi

require() {
    command -v "$1" >/dev/null 2>&1 || { echo "$2" >&2; exit 1; }
}
require cargo         "cargo not found; run source ~/.cargo/env"
require zig           "zig not found: libghostty-vt-sys needs Zig exactly 0.15.2 on PATH (#63)"
require cargo-nextest "cargo-nextest not found; install it with: cargo install cargo-nextest --locked"

# libghostty-vt-sys (a sirio_terminal dependency since #27) shells out to
# `zig build`, and upstream pins Zig at EXACTLY 0.15.2 -- a newer Zig fails
# too, so the upgrade reflex makes it worse; 0.15.2 must be installed
# alongside and found first on PATH. Without this preflight the failure
# surfaces as an inscrutable build-script panic from a crates.io crate (#63).
ZIG_FOUND="$(zig version 2>/dev/null || true)"
if [[ "$ZIG_FOUND" != "0.15.2" ]]; then
    echo "zig ${ZIG_FOUND:-unknown} found, but libghostty-vt-sys builds only with exactly 0.15.2 (newer fails too); install 0.15.2 alongside and put it first on PATH (#63)" >&2
    exit 1
fi

cd rust

# SIRIO_CI_RELEASE_GATE=1 is set by .github/workflows/build-release.yml and by
# nothing else; every local run leaves it unset. It selects the `ci` nextest
# profile -- retries, and the skip list for the two tests that probe the agent
# CLIs installed on the machine -- which lives in .config/nextest.toml, next to
# the reasoning, rather than as argv here.
PROFILE=default
if [[ "${SIRIO_CI_RELEASE_GATE:-}" == "1" ]]; then
    PROFILE=ci
    # .cargo/config.toml keeps incremental compilation on because sccache
    # cannot cache incremental crates and the edit-test loop needs it more
    # than the cache. The CI machine has no edit-test loop, so there the trade
    # goes the other way and our own thirteen crates become cacheable too.
    export CARGO_INCREMENTAL=0
fi

# --all-targets, so the examples are compiled too. nextest builds only what it
# can run, and Scripts/Tests/test-update-e2e.sh's probe lives in
# crates/sirio_apply/examples: without this it would stop being checked here
# and break in the release job instead.
echo "==> cargo build --workspace --all-targets"
cargo build --workspace --all-targets

# nextest rather than `cargo test`: cargo runs the workspace's 45 test binaries
# one after another, so the two slow ones dominate the wall clock while the
# other 43 wait at idle cores. nextest schedules all of them together and gives
# each test its own process -- see .config/nextest.toml for why that process
# boundary matters here beyond speed.
echo "==> cargo nextest run --workspace (profile: $PROFILE)"
cargo nextest run --workspace --no-fail-fast --profile "$PROFILE"

echo "CI OK"
