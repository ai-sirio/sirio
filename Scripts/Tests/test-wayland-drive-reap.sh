#!/usr/bin/env bash
# Does wayland-drive.sh's startup reap actually kill the previous invocation's
# helpers, and does it spare another lane's?
#
# Both halves matter. The first is the leak this test was written for: under
# TILLER_WL_KEEP, cleanup() returns early by design, so the only thing that ever
# removes a helper is the *next* invocation's startup reap. Before 2026-08-19
# that reap covered sirio and sway and nothing else, and a KEEP-mode critic
# driving one fixed label accumulated 20 dbus-daemons, 18 virtual-pointers and
# 13 wtypes in twenty minutes against exactly one sirio and one sway.
#
# The second half is why the reap cannot simply match on process name: several
# critics drive their own lanes in parallel on this machine, and killing by comm
# alone would take out a live one mid-drive. That failure is silent — the victim
# just starts reporting input that never arrives — so it is asserted here rather
# than trusted.
#
# The test executes the shipped text: kill_ours and the three call lines are
# extracted from wayland-drive.sh verbatim. A copy of them written out here would
# pass forever after the real ones were edited away.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/Scripts/wayland-drive.sh"
[[ -r "$SCRIPT" ]] || { echo "FAIL: no script at $SCRIPT" >&2; exit 1; }

# --- pull the shipped implementation in, rather than restating it ------------
FN="$(sed -n '/^kill_ours() {/,/^}/p' "$SCRIPT")"
[[ -n "$FN" ]] || { echo "FAIL: kill_ours() not found in $SCRIPT" >&2; exit 1; }
eval "$FN"

# `|| true` so that "no match" reaches the diagnostic below instead of tripping
# errexit inside the assignment. Without it the exact failure this test exists to
# report — the reap lines gone from the script — exits 1 in silence, which is
# indistinguishable from the test itself being broken.
CALLS="$(grep -E '^kill_ours TILLER_WL_LABEL "\$LABEL" ' "$SCRIPT" || true)"
[[ -n "$CALLS" ]] || {
    echo "FAIL: no startup reap keyed on TILLER_WL_LABEL in $SCRIPT." >&2
    echo "      Without it, every TILLER_WL_KEEP invocation leaks its helpers." >&2
    exit 1
}
for helper in dbus-daemon virtual-pointer wtype; do
    grep -Fq "\"\$LABEL\" $helper" <<<"$CALLS" || {
        echo "FAIL: $helper is not reaped at startup." >&2
        exit 1
    }
done

# --- decoys -----------------------------------------------------------------
# `sleep` copied under each helper's name: the reap keys on comm, and comm is the
# executable's basename, so a copy is indistinguishable from the real helper for
# this purpose while needing neither a compositor nor a bus.
TMP="$(mktemp -d)"
MINE="wdreap-mine-$$"      # the lane being restarted — its leftovers must die
THEIRS="wdreap-theirs-$$"  # a lane someone else is driving — must be untouched
PIDS=()
# Every `|| true` here is load-bearing under `set -e`. Most of these pids are
# already dead by the time the trap runs — killing them is the whole point of the
# test — so `kill` failing is the expected case, not an error. Without the guards
# errexit aborts the trap on the first dead pid, which both skips `rm -rf` and
# replaces a passing run's exit status with 1: the test printed OK and still
# reported failure.
cleanup_decoys() {
    local p
    for p in "${PIDS[@]:-}"; do
        [[ -n "$p" ]] && { kill "$p" 2>/dev/null || true; }
    done
    rm -rf "$TMP" || true
    return 0
}
trap cleanup_decoys EXIT

for helper in dbus-daemon virtual-pointer wtype; do
    cp /bin/sleep "$TMP/$helper"
done

spawn() { # label helper -> echoes pid
    local label="$1" helper="$2"
    # </dev/null and the two redirects are not tidiness. spawn runs inside a
    # command substitution, and `$(...)` waits for its subshell's stdout to
    # *close*, not for the subshell to exit — a decoy inheriting that pipe holds
    # it open for its full lifetime, so the assignment below would block for 300
    # seconds on a process that started instantly.
    env TILLER_WL_LABEL="$label" "$TMP/$helper" 300 </dev/null >/dev/null 2>&1 &
    echo $!
}

declare -A MINE_PID THEIRS_PID
for helper in dbus-daemon virtual-pointer wtype; do
    MINE_PID[$helper]="$(spawn "$MINE" "$helper")"
    THEIRS_PID[$helper]="$(spawn "$THEIRS" "$helper")"
    PIDS+=("${MINE_PID[$helper]}" "${THEIRS_PID[$helper]}")
done

# comm is written when the exec completes, and the reap reads it; give the six
# execs a moment rather than racing them.
for _ in $(seq 1 50); do
    ready=1
    for helper in dbus-daemon virtual-pointer wtype; do
        [[ "$(cat "/proc/${MINE_PID[$helper]}/comm" 2>/dev/null)" == "$helper" ]] || ready=0
        [[ "$(cat "/proc/${THEIRS_PID[$helper]}/comm" 2>/dev/null)" == "$helper" ]] || ready=0
    done
    [[ "$ready" == 1 ]] && break
    sleep 0.1
done
[[ "${ready:-0}" == 1 ]] || { echo "FAIL: decoys never reached their exec'd comm" >&2; exit 1; }

# Positive control. Without it a reap that matched nothing at all — a typo in the
# variable name, say — would pass every assertion below.
for helper in dbus-daemon virtual-pointer wtype; do
    kill -0 "${MINE_PID[$helper]}" 2>/dev/null || { echo "FAIL: control — $helper decoy died early" >&2; exit 1; }
done

# --- run the shipped reap ----------------------------------------------------
# In the options wayland-drive.sh actually runs under, which are NOT this file's.
# The script sets `-uo pipefail` and deliberately not `-e`, and kill_ours relies
# on that: its loop body ends in `grep -qx … && kill`, so scanning a process that
# does not match makes the whole function return non-zero. Under `-e` that would
# abort — which is a property of this test's preamble, not of the script.
#
# Do not "fix" a failure here by appending `|| true` to the shipped call sites.
# That silences the mismatch by editing the side that was already correct.
LABEL="$MINE"
( set -uo pipefail; eval "$CALLS" ) || true

# SIGTERM is asynchronous; the assertions below are about the process being gone,
# not about kill() having returned.
for _ in $(seq 1 50); do
    gone=1
    for helper in dbus-daemon virtual-pointer wtype; do
        kill -0 "${MINE_PID[$helper]}" 2>/dev/null && gone=0
    done
    [[ "$gone" == 1 ]] && break
    sleep 0.1
done

fails=0
for helper in dbus-daemon virtual-pointer wtype; do
    if kill -0 "${MINE_PID[$helper]}" 2>/dev/null; then
        echo "FAIL: stale $helper (pid ${MINE_PID[$helper]}) survived the startup reap" >&2
        fails=1
    fi
done
for helper in dbus-daemon virtual-pointer wtype; do
    if ! kill -0 "${THEIRS_PID[$helper]}" 2>/dev/null; then
        echo "FAIL: another lane's $helper (pid ${THEIRS_PID[$helper]}) was killed — the reap is not label-scoped" >&2
        fails=1
    fi
done
[[ "$fails" == 0 ]] || exit 1

echo "wayland-drive startup reap OK — stale helpers die, another lane's survive"
