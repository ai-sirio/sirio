#!/usr/bin/env bash
# Linux verification gate: checks Rust formatting/clippy/build/tests, the two
# Python supervisor suites, and a headless control-socket round trip.
#
# This gate does not cover anything visual. It has no display and cannot prove
# that the UI maps, paints, or behaves correctly on screen; CI OK is not a UI
# verification claim.
#
# Nor is CI OK an ACP claim by default. The one real-agent ACP test in the tree is
# #[ignore]d and runs only with SIRIO_ACP_REAL=1 (see the stage below). Without it
# this gate is green whether or not an agent can be connected at all.
set -Eeuo pipefail

if [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; run source ~/.cargo/env"
    exit 1
fi

# libghostty-vt-sys (a sirio_terminal dependency since #27) shells out to
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

# rust/.cargo/config.toml sets `rustc-wrapper = "sccache"`, which is right when it works:
# it serves the 408 dependency crates to every parallel worktree instead of rebuilding them.
# But sccache has to spawn a daemon and bind a socket, and the sandboxed agent panes cannot,
# so every rustc invocation fails there and the gate is unrunnable for them — which is worse
# than a slow gate, because an agent that can never reach a green gate learns to ignore it.
#
# So probe it once and fall back. An empty RUSTC_WRAPPER overrides build.rustc-wrapper and
# disables the wrapper for this run only; the config file is left alone for everyone else.
if [[ -z "${RUSTC_WRAPPER+x}" ]] && grep -qs 'rustc-wrapper' "$(dirname "${BASH_SOURCE[0]}")/../rust/.cargo/config.toml"; then
    if ! timeout 30 sccache --start-server >/dev/null 2>&1 \
       && ! timeout 15 sccache --show-stats >/dev/null 2>&1; then
        echo "note: sccache is configured but cannot serve here; building without it."
        export RUSTC_WRAPPER=""
    fi
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUN_DIR="$(mktemp -d /tmp/sirio-ci-XXXXXX)"
LOG_DIR="$RUN_DIR/logs"
SMOKE_DIR="$RUN_DIR/smoke"
mkdir -p "$LOG_DIR" "$SMOKE_DIR"

# The directory is unique and created atomically, so the socket path cannot
# collide with another agent's live Sirio instance or be raced into existence.
export SIRIO_SOCKET="$RUN_DIR/sirio.sock"
export SIRIO_DB="$RUN_DIR/sirio.sqlite"

APP_PID=""
PANE_ID=""
PANE_GROUPS=()
APP_BIN="$ROOT/rust/target/debug/sirio"
CTL_BIN="$ROOT/rust/target/debug/sirioctl"
APP_LOG="$SMOKE_DIR/app.log"
SMOKE_LOG="$SMOKE_DIR/commands.log"

terminate_tree() {
    local pid=$1
    local signal=$2
    local child
    while IFS= read -r child; do
        [[ "$child" =~ ^[0-9]+$ ]] || continue
        terminate_tree "$child" "$signal"
    done < <(pgrep -P "$pid" 2>/dev/null || true)
    kill -"$signal" "$pid" 2>/dev/null || true
}

cleanup() {
    local status=$?
    trap - EXIT INT TERM

    if [[ -n "${PANE_ID:-}" ]] && [[ -S "$SIRIO_SOCKET" ]] && [[ -x "$CTL_BIN" ]]; then
        timeout 3 "$CTL_BIN" panel close --id "$PANE_ID" >/dev/null 2>&1 || true
    fi
    for pgid in "${PANE_GROUPS[@]}"; do
        kill -TERM -- "-$pgid" 2>/dev/null || true
    done
    if [[ -n "${APP_PID:-}" ]]; then
        # Pane PTYs create their own sessions, so terminate descendants as
        # well as the app's private process group. Keep the group kill outside
        # the liveness check: a crashed app may have orphaned children.
        if kill -0 "$APP_PID" 2>/dev/null; then
            terminate_tree "$APP_PID" TERM
        fi
        kill -TERM -- "-$APP_PID" 2>/dev/null || kill -TERM "$APP_PID" 2>/dev/null || true
        for _ in $(seq 1 30); do
            kill -0 "$APP_PID" 2>/dev/null || break
            sleep 0.1
        done
        if kill -0 "$APP_PID" 2>/dev/null; then
            terminate_tree "$APP_PID" KILL
        fi
        kill -KILL -- "-$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
    for pgid in "${PANE_GROUPS[@]}"; do
        kill -KILL -- "-$pgid" 2>/dev/null || true
    done

    [[ -e "$SIRIO_SOCKET" ]] && rm -f "$SIRIO_SOCKET"
    rm -rf "$RUN_DIR"
    exit "$status"
}
trap cleanup EXIT INT TERM

rust_fingerprint() {
    git -C "$ROOT" ls-files -co --exclude-standard -- 'rust/**' |
        while IFS= read -r path; do
            case "$path" in
                */.remember|*/.remember/*|.remember|.remember/*) continue ;;
            esac
            [[ -f "$ROOT/$path" ]] || continue
            sha256sum "$ROOT/$path"
        done
}

rust_fingerprint_drift() {
    awk '
        NR == FNR {
            before[$2] = $1
            paths[$2] = 1
            next
        }
        {
            after[$2] = $1
            paths[$2] = 1
        }
        END {
            for (path in paths) {
                if (before[path] != after[path]) print path
            }
        }
    ' "$ARRIVAL_RUST_FINGERPRINT" "$FINAL_RUST_FINGERPRINT" | sort
}

ARRIVAL_RUST_FINGERPRINT="$RUN_DIR/rust-fingerprint-at-start"
if ! rust_fingerprint >"$ARRIVAL_RUST_FINGERPRINT"; then
    echo "FAILED: Rust arrival fingerprint"
    exit 1
fi

print_excerpt() {
    local log=$1
    [[ -f "$log" ]] || return 0
    sed -n '1,120p' "$log"
    if [[ "$(wc -l < "$log")" -gt 120 ]]; then
        echo "... output truncated; the failing command was: $2"
    fi
}

fail_stage() {
    local stage=$1
    local log=$2
    local command=${3:-$stage}
    echo "FAILED: $stage"
    echo "Command: $command"
    print_excerpt "$log" "$command"
    exit 1
}

run_cargo_stage() {
    local stage=$1
    shift
    local log="$LOG_DIR/$stage.log"
    local command="(cd rust && $*)"
    if ! (cd "$ROOT/rust" && "$@") >"$log" 2>&1; then
        fail_stage "$stage" "$log" "$command"
    fi
    echo "PASS: $stage"
    tail -5 "$log" || true
}

run_root_stage() {
    local stage=$1
    shift
    local log="$LOG_DIR/$stage.log"
    local command="$*"
    if ! "$@" >"$log" 2>&1; then
        fail_stage "$stage" "$log" "$command"
    fi
    echo "PASS: $stage"
    tail -5 "$log" || true
}

echo "==> Rust format and clippy"
run_cargo_stage "cargo fmt --check" cargo fmt --all -- --check

# The shell entry point and the chat surface are being edited by other owners
# in this shared worktree. Their current warnings are routed to those owners
# rather than grandfathered into this gate. Every other workspace crate is
# ours here and must fail the gate on its first new warning.
run_cargo_stage "cargo clippy (owned crates)" cargo clippy --workspace --all-targets \
    --exclude sirio --exclude sirio_ui -- -D warnings

run_cargo_stage "cargo build" cargo build -p sirio -p sirio_control
# Per-crate, not `cargo test --workspace`.
#
# Historical note: these two tests were once timing-sensitive when every crate's test
# binary ran concurrently. Their fixture synchronization is now deterministic; this
# per-crate loop remains conservative context for diagnosing future load failures.
#
# This remains a diagnostic/per-crate mode; the common gate is responsible for the
# workspace-wide no-fail-fast result.
WORKSPACE_CRATES=(
    sirio sirio_acp sirio_persistence sirio_activity sirio_agents sirio_control
    sirio_git sirio_project sirio_terminal sirio_theme sirio_ui sirio_markdown
    sirio_usage
)
for crate in "${WORKSPACE_CRATES[@]}"; do
    run_cargo_stage "cargo test -p $crate" cargo test -p "$crate"
done

# rust/vendor/gpui_linux is `exclude`d from the root workspace (see rust/Cargo.toml) because
# it carries its own `[workspace]` table -- so none of the `cargo test -p <crate>` calls above,
# nor a hypothetical `cargo test --workspace`, ever reach it. That crate is where the Wayland
# XDND slow-provider fix lives (F-CORE-FILE-03A, see rust/vendor/README.md), and until now
# nothing in either CI gate ran its regression tests. Run the whole crate, not just the
# `pending_drop` tests the fix itself added, so a future change to anything else vendored in
# there is covered too.
#
# The crate is `#![cfg(any(target_os = "linux", target_os = "freebsd"))]`-gated
# (src/gpui_linux.rs), so on any other host its whole body -- tests included -- compiles out to
# nothing. `cargo test` still exits 0 with "0 tests" there, and reporting that as PASS would
# claim coverage this gate never actually exercised. So a genuine zero-tests run is SKIP, not
# PASS, loud and named the same way the rust-std-not-installed precondition above is: a green
# gate here only means pending_drop and friends actually ran on a host that is Linux/FreeBSD.
run_gpui_linux_vendor_test_stage() {
    local stage="cargo test --manifest-path vendor/gpui_linux/Cargo.toml --locked"
    local log="$LOG_DIR/gpui_linux_vendor_test.log"
    if ! (cd "$ROOT/rust" && cargo test --manifest-path vendor/gpui_linux/Cargo.toml --locked) >"$log" 2>&1; then
        fail_stage "$stage" "$log" "(cd rust && cargo test --manifest-path vendor/gpui_linux/Cargo.toml --locked)"
    fi
    local total_passed
    total_passed=$(grep -oE '[0-9]+ passed' "$log" | awk '{s+=$1} END{print s+0}')
    if [[ "$total_passed" -eq 0 ]]; then
        echo "SKIP: $stage — crate is cfg-gated to linux/freebsd; compiled to zero tests on this host"
        return 0
    fi
    echo "PASS: $stage"
    tail -5 "$log" || true
}
run_gpui_linux_vendor_test_stage

# The project's acceptance test is "connect a real workspace agent over ACP, send
# messages, verify streaming and replies". The tree has exactly one test that does
# this — real_claude_streams_tool_permission_and_writes_nonce, which connects real
# Claude, streams, answers a tool-permission request, and writes a nonce to disk
# (the nonce is what separates "the agent replied" from "the agent did work").
#
# It is #[ignore]d, because it needs installed Claude credentials and an ACP adapter
# download, so `cargo test --workspace` above skips it. That means CI OK could be
# printed while the ACP path was completely dead — the gate would be green on the
# one path the project is actually judged by.
#
# Opt-in rather than default: an agent pane without credentials must still be able
# to reach a green gate, or it learns to ignore the gate (the same reasoning as the
# sccache fallback above). Verified passing 2026-08-13 in 8.16s.
if [ "${SIRIO_ACP_REAL:-0}" = "1" ]; then
    run_cargo_stage "real ACP acceptance (SIRIO_ACP_REAL=1)" \
        cargo test -p sirio_acp --test real_claude -- --ignored
else
    echo "SKIP: real ACP acceptance — set SIRIO_ACP_REAL=1 to exercise it"
fi

run_root_stage "test-crash-supervise.py" env PYTHONDONTWRITEBYTECODE=1 \
    python3 Scripts/Tests/test-crash-supervise.py -q
run_root_stage "test-crash-freeze-supervise.py" env PYTHONDONTWRITEBYTECODE=1 \
    python3 Scripts/Tests/test-crash-freeze-supervise.py -q
run_root_stage "test-visual-sweep.sh" env PYTHONDONTWRITEBYTECODE=1 \
    bash Scripts/Tests/test-visual-sweep.sh

run_root_stage "test-check-release-version.sh" bash Scripts/Tests/test-check-release-version.sh
run_root_stage "test-build-dmg.sh"             bash Scripts/Tests/test-build-dmg.sh
run_root_stage "test-build-app-bundle.sh"      bash Scripts/Tests/test-build-app-bundle.sh
run_root_stage "test-build-appimage.sh"         bash Scripts/Tests/test-build-appimage.sh
run_root_stage "test-release-workflow.sh"      bash Scripts/Tests/test-release-workflow.sh
run_root_stage "test-generate-changelog.sh"    bash Scripts/Tests/test-generate-changelog.sh
run_root_stage "test-install-sh.sh"            bash Scripts/Tests/test-install-sh.sh

# The one test of the update chain that exercises the *compiled* wiring: the
# manifest URL, release channel and accepted key set are `option_env!`, so the
# unit tests -- which inject a fetch and a launcher -- cannot see a build that
# never received them. It signs a manifest, serves it over loopback and drives
# the real Updater against it; the silent installer launch is asserted only on
# Windows, where it exists. Deliberately here and not in the release job: a
# release must not be gated on a local HTTP port being free.
run_root_stage "test-update-e2e.sh"            bash Scripts/Tests/test-update-e2e.sh

# The critics' own instrument. Both of these guard leaks that have already cost
# this machine real resources — 184 orphaned virtual-pointers at once, and before
# that a disk filled to within hours of full — and both failures are invisible
# from inside a critic run: the harness keeps working perfectly while the debris
# accumulates behind it, so nothing short of a gate catches a regression.
run_root_stage "test-wayland-drive-reap.sh" env PYTHONDONTWRITEBYTECODE=1 \
    bash Scripts/Tests/test-wayland-drive-reap.sh
# Skips itself with exit 0 where sway, gcc or wayland-scanner are absent, so it is
# safe on a machine that cannot host a nested compositor. Takes ~10s where it can:
# it really does start one, drive it, and kill it.
run_root_stage "test-virtual-pointer-outlives-compositor.sh" env PYTHONDONTWRITEBYTECODE=1 \
    bash Scripts/Tests/test-virtual-pointer-outlives-compositor.sh

# F-BRW: the premise the browser tab-close fix rests on — that wry's unmap sits
# in Xlib's output buffer until something flushes it. No Rust assertion can see
# this; the request is made either way and the difference is only in what the X
# server was told. Boots its own Xvfb (never the operator's display) and skips
# itself where gcc, Xvfb or the gtk3 headers are absent. ~1s where it runs.
run_root_stage "test-x11-unmap-needs-a-flush.sh" env PYTHONDONTWRITEBYTECODE=1 \
    bash Scripts/Tests/test-x11-unmap-needs-a-flush.sh

# --- Cross-platform compile gates (macOS, Windows) ---
#
# The user now requires macOS/Windows compatibility (docs/linux-rewrite/PORTABILITY.md). GPUI,
# alacritty_terminal, and wry all carry real macOS/Windows backends already; what regresses
# silently is our own glue -- an unconditional Linux-only Cargo dependency, or a Linux `cfg`
# branch added with no counterpart. `cargo check --target <triple>` does not link, so it is a
# real, cheap-enough-to-run gate against exactly that: it fails on both. These two stages are
# what makes that failure loud instead of something a critic has to rediscover by hand.
#
# They run LAST, after every stage above, on purpose: a cold `target/<triple>/` means building
# ~400 dependency crates from scratch for a target this box will never link or run, which the
# wave's own reconnaissance clocked at tens of minutes per target (~20 GB of target/ growth
# each) the first time a contributor's checkout touches them. A contributor iterating on an
# ordinary bug should hit fmt/clippy/build/test failures long before either of these starts;
# putting them first would make the common case slower for a check most edits cannot affect.
# Once `target/<triple>/` is warm the repeat cost is much smaller (single-digit minutes).
#
# Two distinct non-PASS outcomes are handled on purpose, and they are not the same thing:
#
#  SKIP -- rust-std itself is not installed for the triple (`rustup target list --installed`
#  doesn't list it). This is a one-command, always-fixable, per-contributor setup gap that
#  cannot regress from a code change, so it is treated the same way the SIRIO_ACP_REAL stage
#  above is: skip loudly, name the exact fix (`rustup target add <triple>`), and let a
#  contributor who hasn't opted in yet still reach a green gate. A maintained CI runner installs
#  the target once, permanently, so the skip path is a laptop convenience, not a hole CI lives
#  in. Silent would be wrong here; loud-and-named is not the same failure mode as silent.
#
#  BLOCKED -- rust-std is present, but the check fails while a third-party dependency's build
#  script tries to invoke a C/asm compiler for the foreign target and that invocation itself
#  fails -- observed from `psm` (pulled in transitively by gpui's `stacker`, used for
#  stack-safety in gpui's text/layout code; fails to assemble its per-target `.s` stub),
#  `stacker` itself (`windows.c` needs a real `windows.h`), and expected from `libsqlite3-sys`
#  (rusqlite's bundled sqlite3.c) once the check gets that far. Confirmed independently by two
#  prior slices (J2-windows, J3-macos): this box has no MSVC toolchain, no macOS SDK, and no
#  cross-linker (cargo-xwin/osxcross/zig all absent) either, so none of that C/asm can be built
#  for a foreign target from a plain Linux host -- the failure never reaches any of our own 13
#  workspace crates, so it is not a code regression and no `cfg` seam in our tree can fix it. On
#  real hardware (what `Scripts/ci.sh`, the Swift gate, stands in for on macOS -- it has never
#  carried Rust stages, but the day it or a native runner does, real `cc`/Xcode tools understand
#  `-arch`/`-mmacosx-version-min` and `windows.h` exists on a Windows box, so this wall does not
#  exist there) this same command would just pass, so the two gates' definitions of "green" do
#  not actually disagree -- this one just cannot prove it from here. Failing the gate on this
#  forever would make it permanently, unfixably red on every SDK-less Linux box, which teaches
#  exactly the lesson the sccache fallback above exists to prevent: a gate nobody can ever pass
#  is a gate everybody learns to ignore. So it also reports non-fatally -- but by name, with the
#  full log, and visibly distinct from PASS, never silently.
#
#  The signature is matched on the *mechanism* ("the `cc` crate's own compiler invocation
#  failed"), not on an enumerated crate list -- `stacker` failing where `psm` had failed the
#  slice before is exactly why: a name whitelist would already have missed it, and would keep
#  missing whichever sys-crate the graph reaches next. None of our own 13 workspace crates
#  shell out to a C compiler from build.rs, so this signature cannot originate from our code.
#
#  Anything else -- dependency resolution failing outright, an unconditional Linux-only crate,
#  a missing `cfg` branch anywhere in our own code -- matches neither signature above and DOES
#  fail the gate. That is the failure this pair of stages exists to catch, and it stays a hard
#  failure the same as any other stage's FAILED above.
# This box has no MSVC toolchain and no macOS SDK, so a handful of C/asm-building
# dependencies cannot compile for the cross targets no matter what our source says.
# Those failures are environmental and must not read as a code regression.
CROSS_TARGET_KNOWN_WALL='error occurred in cc-rs:'
# Packages whose *build scripts* cannot run on this box: no MSVC toolchain, no macOS SDK,
# no Windows resource compiler, no cross bindgen. Names only — cargo prints
# `pkg v1.2.3 (source)`, so an anchored `pkg`-with-closing-backtick pattern never matches.
# Getting that wrong is not academic: an earlier version of this filter required the
# backtick immediately after the bare name, so nothing ever matched, `residual` was never
# empty, and the stage could not reach BLOCKED on this box no matter what the code said.
# Always-red is safer than always-green but just as useless — a gate nobody can get green
# is a gate nobody reads.
#
# Deliberately restricted to *build-script* failures. A genuine rustc error in any of
# these packages would surface as `could not compile <pkg>` and is NOT allowlisted.
CROSS_TARGET_WALL_PKGS='psm|stacker|libsqlite3-sys|gpui|media'

# Cross-target compile check, classified into PASS / BLOCKED / FAILED.
#
# The classification used to be `grep -q <wall>` -> BLOCKED, which had a hole a critic
# demonstrated: cargo's default fail-fast aborts the build at the first failure, so
# whether an injected regression or the pre-existing wall surfaced first was a
# scheduling race. Reintroducing `gtk` as an unconditional dependency was correctly
# caught 6 times out of 7 and silently absorbed as BLOCKED on the 7th. A gate that
# misses the defect it exists to catch one time in seven is worse than no gate, because
# it is trusted.
#
# Two changes close it. `--keep-going` makes cargo compile every independent unit
# instead of stopping at the first error, so the wall can no longer pre-empt anything.
# And BLOCKED now requires that *every* error be attributable to the wall: any residual
# error — including `could not compile <one of our crates>` — fails the stage even when
# the wall is also present.
run_cross_target_stage() {
    local stage=$1
    local triple=$2
    local log="$LOG_DIR/$(echo "$stage" | tr ' /' '__').log"

    if ! rustup target list --installed 2>/dev/null | grep -qx "$triple"; then
        echo "SKIP: $stage — rust-std not installed for $triple"
        echo "  run: rustup target add $triple"
        return 0
    fi

    if (cd "$ROOT/rust" && cargo check --target "$triple" --workspace --keep-going) >"$log" 2>&1; then
        echo "PASS: $stage"
        tail -5 "$log" || true
        return 0
    fi

    # Every top-level error except the allowlisted build-script walls. `could not compile
    # <crate>` is the reliable marker for one of ours failing to build — it is what caught
    # the 30 real sirio_control errors the previous classifier was masking.
    # Second environmental shape, macOS only: Zed's vendored `media` crate generates its
    # CoreVideo/CoreMedia bindings from a build script gated on a real macOS *host*. On
    # this Linux host the script runs but writes no `bindings.rs`, so `media` fails at
    # compile time rather than at its build script. Allowlisted by name because we do not
    # author that crate — we cannot introduce an error in it — and the missing-bindings
    # line is quoted too, so the pair is unambiguous.
    local residual
    residual=$(grep -E '^error' "$log" \
        | grep -vE "$CROSS_TARGET_KNOWN_WALL" \
        | grep -vE "^error: failed to run custom build command for \`($CROSS_TARGET_WALL_PKGS) v" \
        | grep -vE "^error: could not compile \`media\` " \
        | grep -vE "^error: couldn't read .*/out/bindings\.rs" \
        || true)

    if [[ -z "$residual" ]] && grep -qE "$CROSS_TARGET_KNOWN_WALL|^error: failed to run custom build command for" "$log"; then
        echo "BLOCKED: $stage — known SDK/cross-toolchain wall, not a code regression"
        echo "  no MSVC toolchain / macOS SDK / cross-linker on this box; see docs/linux-rewrite/PORTABILITY.md"
        tail -15 "$log" || true
        return 0
    fi

    if [[ -n "$residual" ]]; then
        echo "  (errors not attributable to the known SDK wall:)"
        echo "$residual" | sed -n '1,20p'
    fi
    fail_stage "$stage" "$log" "(cd rust && cargo check --target $triple --workspace --keep-going)"
}

echo "==> Cross-platform compile checks (macOS, Windows)"
run_cross_target_stage "cargo check --target x86_64-pc-windows-msvc --workspace" x86_64-pc-windows-msvc
run_cross_target_stage "cargo check --target aarch64-apple-darwin --workspace" aarch64-apple-darwin

echo "==> Headless smoke test"
smoke_failure() {
    local reason=$1
    capture_panel_groups || true
    echo "FAILED: headless smoke test — $reason"
    if [[ -f "$SMOKE_LOG" ]]; then
        echo "--- control command output ---"
        sed -n '1,160p' "$SMOKE_LOG"
    fi
    if [[ -f "$APP_LOG" ]]; then
        echo "--- app output ---"
        sed -n '1,160p' "$APP_LOG"
    fi
    exit 1
}

if ! git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "FAILED: headless smoke test — smoke precondition: $ROOT must be inside a Git checkout"
    exit 1
fi

smoke_capture() {
    local name=$1
    shift
    local output="$SMOKE_DIR/$name.out"
    if ! "$@" >"$output" 2>&1; then
        capture_panel_groups || true
        {
            echo "[$name] FAILED"
            cat "$output"
        } >>"$SMOKE_LOG"
        smoke_failure "$name command failed"
    fi
    {
        echo "[$name]"
        cat "$output"
    } >>"$SMOKE_LOG"
    cat "$output"
}

capture_panel_groups() {
    local child
    local pgid
    while IFS= read -r child; do
        [[ "$child" =~ ^[0-9]+$ ]] || continue
        pgid=$(ps -o pgid= -p "$child" 2>/dev/null | tr -d ' ')
        [[ "$pgid" =~ ^[0-9]+$ ]] || continue
        [[ "$pgid" == "$APP_PID" ]] && continue
        case " ${PANE_GROUPS[*]} " in
            *" $pgid "*) ;;
            *) PANE_GROUPS+=("$pgid") ;;
        esac
    done < <(pgrep -P "$APP_PID" 2>/dev/null || true)
}

[[ -x "$APP_BIN" ]] || smoke_failure "missing app binary at $APP_BIN"
[[ -x "$CTL_BIN" ]] || smoke_failure "missing sirioctl binary at $CTL_BIN"

setsid env -u DISPLAY -u WAYLAND_DISPLAY \
    SIRIO_SOCKET="$SIRIO_SOCKET" SIRIO_DB="$SIRIO_DB" \
    "$APP_BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

socket_ready=0
for _ in $(seq 1 150); do
    if [[ -S "$SIRIO_SOCKET" ]]; then
        socket_ready=1
        break
    fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        wait "$APP_PID" 2>/dev/null || true
        break
    fi
    sleep 0.1
done
[[ "$socket_ready" -eq 1 ]] || smoke_failure "app did not create its private socket"
capture_panel_groups

# ControlState::from_catalog (crates/sirio/src/main.rs) builds `workspaces` only from
# the persisted project catalog -- being launched inside a git checkout is not by itself
# enough for `current-workspace` to see one, the same way `Scripts/visual-sweep.sh` already
# has to `project add` its fixture before any `workspace.current`/`workspace.select` call.
# $SIRIO_DB above is a fresh, empty, per-run database, so without this the very next call
# always reports "no current workspace" -- not a failure of anything this gate is testing.
smoke_capture project-add "$CTL_BIN" project add "$ROOT" >/dev/null

current_workspace=$(smoke_capture current-workspace "$CTL_BIN" current-workspace)
IFS=$'\t' read -r current_project current_branch current_path current_id <<<"$current_workspace"
if [[ -z "${current_project:-}" || -z "${current_path:-}" || -z "${current_id:-}" ||
    ! -d "$current_path" ]]; then
    smoke_failure "current-workspace did not return real project/path/id state"
fi
echo "PASS: current-workspace -> $current_workspace"

PANE_ID=$(smoke_capture panel-create "$CTL_BIN" panel create --cmd cat)
[[ "$PANE_ID" == pane-* ]] || smoke_failure "panel.create returned invalid pane id: $PANE_ID"
for _ in $(seq 1 20); do
    capture_panel_groups
    if ((${#PANE_GROUPS[@]} > 0)); then
        break
    fi
    sleep 0.1
done
(( ${#PANE_GROUPS[@]} > 0 )) || smoke_failure "could not record the panel process group"

nonce="P26_CI_NONCE_$(date +%s%N)_${RANDOM}_$$"
smoke_capture panel-write "$CTL_BIN" panel write --id "$PANE_ID" --input "$nonce" >/dev/null

decoded=""
for _ in $(seq 1 50); do
    encoded=$(smoke_capture panel-read "$CTL_BIN" panel read "$PANE_ID")
    if decoded=$(printf '%s' "$encoded" | base64 --decode 2>/dev/null) &&
        [[ "$decoded" == *"$nonce"* ]]; then
        break
    fi
    sleep 0.1
done
[[ "$decoded" == *"$nonce"* ]] || smoke_failure "panel.read did not return generated nonce $nonce"
echo "PASS: panel.write → panel.read returned generated nonce"

FINAL_RUST_FINGERPRINT="$RUN_DIR/rust-fingerprint-at-end"
if ! rust_fingerprint >"$FINAL_RUST_FINGERPRINT"; then
    echo "FAILED: Rust final fingerprint"
    exit 1
fi
if ! cmp -s "$ARRIVAL_RUST_FINGERPRINT" "$FINAL_RUST_FINGERPRINT"; then
    echo "VOID: the Rust worktree changed during the gate; the measurement is invalid"
    echo "Re-run the gate when the roster is no longer writing. Changed Rust files:"
    rust_fingerprint_drift | sed 's/^/  /'
    exit 75
fi

echo "PASS: headless smoke test"
echo "CI OK"
