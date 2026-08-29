#!/usr/bin/env bash
# Does hiding a native webview actually reach the X server?
#
# F-BRW's tab-close bug survived a fix that looked complete: the browser was
# unmapped on every path that takes it off screen, and it still bled through
# after its tab was closed — reproduced twice by the `brw-unmap` critic, once
# from a clean instance, with the window still `IsViewable` five seconds and
# many forced repaints later.
#
# The reason is not in the Rust. wry unmaps and destroys its child window with
# bare Xlib calls on GDK's connection, and Xlib *buffers* those: they reach the
# server when something flushes, and the only thing that ever flushed was the
# surface's own 16 ms `gtk::main_iteration_do` pump. Tab-close is the single
# path where that pump is what we are destroying, so the request was issued
# into a buffer with nobody left to drain it.
#
# That is a claim about Xlib, not about Sirio, so this test makes Xlib answer
# it: a real X server, a real child window on GDK's real connection, and a
# second independent client asking the server what it believes. If the unmap
# turns out to be delivered eagerly, the diagnosis behind the fix is wrong and
# this test fails saying so.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SRC="$ROOT/Scripts/Tests/x11-unmap-flush-probe.c"
RUST="$ROOT/rust/crates/sirio_ui/src/browser.rs"
[[ -r "$SRC" ]] || { echo "FAIL: missing $SRC" >&2; exit 1; }

for tool in gcc pkg-config Xvfb; do
    command -v "$tool" >/dev/null || { echo "SKIP: $tool is not installed" >&2; exit 0; }
done
pkg-config --exists gtk+-3.0 gdk-x11-3.0 x11 || {
    echo "SKIP: gtk+-3.0 / gdk-x11-3.0 / x11 headers absent" >&2
    exit 0
}

TMP="$(mktemp -d "/tmp/x11unmap-$$-XXXX")"
XVFB_PID=""
cleanup() {
    [[ -n "$XVFB_PID" ]] && { kill "$XVFB_PID" 2>/dev/null || true; }
    rm -rf "$TMP" || true
    return 0
}
trap cleanup EXIT

# --- the shipped recipe has to still be the shipped recipe -------------------
# Not a rewrite of the check: if `flush_native_window_ops` stops iterating GTK
# or stops flushing GDK, what this test proves stops being about the code that
# ships. Names only — the C spells the same three functions in their C form.
for symbol in 'gtk::events_pending' 'gtk::main_iteration_do' 'display.flush()'; do
    grep -qF "$symbol" "$RUST" || {
        echo "FAIL: browser.rs no longer calls $symbol — this test proves a recipe" >&2
        echo "      the code has stopped using." >&2
        exit 1
    }
done

# --- an X server of our own, never the operator's ----------------------------
# `-displayfd` makes the server pick a free number and tell us, so there is no
# window in which this could attach to, or collide with, the real session on
# :0/:1. `env -u DISPLAY` closes the other direction: nothing here inherits a
# display we were launched from.
Xvfb -displayfd 3 -screen 0 800x600x24 -nolisten tcp 3>"$TMP/display" \
    >"$TMP/xvfb.log" 2>&1 &
XVFB_PID=$!

DISPLAY_NUM=""
for _ in $(seq 1 100); do
    DISPLAY_NUM="$(tr -d '[:space:]' < "$TMP/display" 2>/dev/null || true)"
    [[ -n "$DISPLAY_NUM" ]] && break
    kill -0 "$XVFB_PID" 2>/dev/null || {
        echo "FAIL: Xvfb died during startup" >&2
        cat "$TMP/xvfb.log" >&2
        exit 1
    }
    sleep 0.1
done
[[ -n "$DISPLAY_NUM" ]] || {
    echo "FAIL: Xvfb never announced a display" >&2
    cat "$TMP/xvfb.log" >&2
    exit 1
}

# --- build with the repo's -Werror, so the probe is part of the gate ---------
PROBE="$TMP/x11-unmap-flush-probe"
# shellcheck disable=SC2046
gcc -std=c11 -Wall -Wextra -Werror -O2 "$SRC" -o "$PROBE" \
    $(pkg-config --cflags --libs gtk+-3.0 gdk-x11-3.0 x11)

OUT="$TMP/probe.out"
env -u WAYLAND_DISPLAY GDK_BACKEND=x11 DISPLAY=":$DISPLAY_NUM" \
    "$PROBE" >"$OUT" 2>"$TMP/probe.err" || {
    echo "FAIL: probe exited $? — see below" >&2
    cat "$TMP/probe.err" >&2
    exit 1
}

read_fact() { { grep -m1 "^$1=" "$OUT" || true; } | cut -d= -f2-; }
control="$(read_fact control)"
after_unmap="$(read_fact after_unmap)"
after_flush="$(read_fact after_flush)"

# --- positive control --------------------------------------------------------
# "The server says IsUnmapped" is worth nothing from a window that was never
# mapped, or that the probe connection cannot see at all.
[[ "$control" == "IsViewable" ]] || {
    echo "FAIL: control — a freshly mapped child reads '$control', not IsViewable." >&2
    echo "      Nothing below this line means anything until that is true." >&2
    cat "$OUT" >&2
    exit 1
}

# --- the premise -------------------------------------------------------------
[[ "$after_unmap" == "IsViewable" ]] || {
    echo "FAIL: an unflushed XUnmapWindow reads '$after_unmap', not IsViewable." >&2
    echo "      Xlib is delivering eagerly here, so the buffered-request diagnosis" >&2
    echo "      behind the F-BRW tab-close fix does not hold on this platform and" >&2
    echo "      the real cause is still unfound. Do not paper over this." >&2
    cat "$OUT" >&2
    exit 1
}

# --- the cure ----------------------------------------------------------------
[[ "$after_flush" == "IsUnmapped" ]] || {
    echo "FAIL: after flush_native_window_ops's recipe the server still says" >&2
    echo "      '$after_flush'. The window the user can see is this one." >&2
    cat "$OUT" >&2
    exit 1
}

echo "x11 unmap flush OK — mapped child stayed IsViewable through an unflushed"
echo "  XUnmapWindow, and went IsUnmapped once the shipped recipe ran"
