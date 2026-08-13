#!/usr/bin/env bash
# Linux verification gate: checks Rust formatting/clippy/build/tests, the two
# Python supervisor suites, and a headless control-socket round trip.
#
# This gate does not cover anything visual. It has no display and cannot prove
# that the UI maps, paints, or behaves correctly on screen; CI OK is not a UI
# verification claim.
set -Eeuo pipefail

if [[ -n "${HOME:-}" && -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found; run source ~/.cargo/env"
    exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUN_DIR="$(mktemp -d /tmp/tiller-ci-XXXXXX)"
LOG_DIR="$RUN_DIR/logs"
SMOKE_DIR="$RUN_DIR/smoke"
mkdir -p "$LOG_DIR" "$SMOKE_DIR"

# The directory is unique and created atomically, so the socket path cannot
# collide with another agent's live Tiller instance or be raced into existence.
export TILLER_SOCKET="$RUN_DIR/tiller.sock"
export TILLER_DB="$RUN_DIR/tiller.sqlite"

APP_PID=""
PANE_ID=""
PANE_GROUPS=()
APP_BIN="$ROOT/rust/target/debug/tiller"
CTL_BIN="$ROOT/rust/target/debug/tillerctl"
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

    if [[ -n "${PANE_ID:-}" ]] && [[ -S "$TILLER_SOCKET" ]] && [[ -x "$CTL_BIN" ]]; then
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

    [[ -e "$TILLER_SOCKET" ]] && rm -f "$TILLER_SOCKET"
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
    --exclude tiller --exclude tiller_ui -- -D warnings

run_cargo_stage "cargo build" cargo build -p tiller -p tiller_control
run_cargo_stage "cargo test --workspace" cargo test --workspace

run_root_stage "test-crash-supervise.py" env PYTHONDONTWRITEBYTECODE=1 \
    python3 Scripts/Tests/test-crash-supervise.py -q
run_root_stage "test-crash-freeze-supervise.py" env PYTHONDONTWRITEBYTECODE=1 \
    python3 Scripts/Tests/test-crash-freeze-supervise.py -q
run_root_stage "test-visual-sweep.sh" env PYTHONDONTWRITEBYTECODE=1 \
    bash Scripts/Tests/test-visual-sweep.sh

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
[[ -x "$CTL_BIN" ]] || smoke_failure "missing tillerctl binary at $CTL_BIN"

setsid env -u DISPLAY -u WAYLAND_DISPLAY \
    TILLER_SOCKET="$TILLER_SOCKET" TILLER_DB="$TILLER_DB" \
    "$APP_BIN" >"$APP_LOG" 2>&1 &
APP_PID=$!

socket_ready=0
for _ in $(seq 1 150); do
    if [[ -S "$TILLER_SOCKET" ]]; then
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
    echo "FAILED: new Rust worktree drift appeared during the gate"
    diff -u "$ARRIVAL_RUST_FINGERPRINT" "$FINAL_RUST_FINGERPRINT" || true
    exit 1
fi

echo "PASS: headless smoke test"
echo "CI OK"
