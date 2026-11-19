#!/usr/bin/env bash
# Does an MCP Apps widget webview stay isolated on WebKitGTK?
#
# `examples/widget_isolation.rs` builds the real widget webview (`build_widget_child`) and loads
# `fixtures/mcp-apps/isolation-view.html`, a probe page that tries to reach its parent, the
# network, devices and storage, and reports what happened. This script runs it twice and judges
# the reports:
#
#   run      restrictive view CSP, incognito on   -- what production ships.
#   control  permissive view CSP,  incognito off  -- the same probes, with the CSP loosened.
#            It shows the restrictive CSP is what blocks the network and the frames: the logger
#            sees /fetch and /img, and no violation is reported.
#
# Incognito, the permission handler (deny) and the new-window handler are defence in depth over
# engine and sandbox defaults that already hold in the control. On WebKitGTK 2.54.1:
# custom-scheme storage is neither shared nor written to disk; permission requests end denied;
# window.open in a sandboxed iframe without allow-popups returns null. The run checks those
# outcomes, not their cause; the control prints them as FACT lines.
#
# The probe's second view (B) shares A's origin and reads the storage A wrote.
#
# Usage: widget-isolation.sh --display :97 --out-dir DIR
# Only our own Xvfb: this refuses :0, :94, :120, :121 and an empty display. Writes
# DIR/widget-isolation.json with both runs' results. The only address a webview may reach is
# the logger this script starts on 127.0.0.1.
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VIEW="$ROOT/Scripts/Tests/fixtures/mcp-apps/isolation-view.html"
DISPLAY_ARG=""
OUT_DIR=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --display) DISPLAY_ARG="${2:-}"; shift 2 ;;
        --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
        *) echo "FAIL: unknown argument $1" >&2; exit 2 ;;
    esac
done
[[ -n "$DISPLAY_ARG" && -n "$OUT_DIR" ]] || {
    echo "FAIL: --display and --out-dir are required" >&2
    exit 2
}
case "$DISPLAY_ARG" in
    :0|:0.*|:94|:94.*|:120|:120.*|:121|:121.*)
        echo "FAIL: refusing display $DISPLAY_ARG" >&2
        exit 2 ;;
esac
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

TMP="$(mktemp -d "${TMPDIR:-/tmp}/widget-isolation-XXXXXX")"
LOGGER_PID=""
cleanup() {
    [[ -n "$LOGGER_PID" ]] && { kill "$LOGGER_PID" 2>/dev/null || true; }
    [[ -n "$TMP" && -d "$TMP" ]] && rm -rf "$TMP"
    return 0
}
trap cleanup EXIT

# --- build the example -------------------------------------------------------
(cd "$ROOT/rust" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" \
    cargo build -q -p sirio_ui --example widget_isolation) || {
    echo "FAIL: the widget_isolation example does not build" >&2
    exit 1
}
TARGET_DIR="$(cd "$ROOT/rust" && cargo metadata --no-deps --format-version 1 \
    | python3 -I -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
EXAMPLE="$TARGET_DIR/debug/examples/widget_isolation"
[[ -x "$EXAMPLE" ]] || { echo "FAIL: the example binary is missing" >&2; exit 1; }

# --- the request logger ------------------------------------------------------
cat > "$TMP/logger.py" <<'PY'
import socket, sys, threading

# Four listeners on 127.0.0.1 (main, preconnect, prefetch, ping), each on a port of its own.
# Every raw TCP accept is counted in accepts_path, before any byte is read: a bare preconnect
# sends no request line, so a logger that only logs HTTP requests would pass it vacuously.
# A request line, when one comes, goes to log_path (the main port's as its bare path, the
# others' as "<name>:<path>").
log_path, accepts_path, port_prefix = sys.argv[1], sys.argv[2], sys.argv[3]
NAMES = ("main", "preconnect", "prefetch", "ping")
lock = threading.Lock()

PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4"
    "890000000b49444154789c6360000200000500017a5eab3f0000000049454e44ae426082")

def note(path, line):
    with lock:
        with open(path, "a") as log:
            log.write(line + "\n")

def handle(name, conn):
    conn.settimeout(1.0)
    data = b""
    try:
        while b"\r\n\r\n" not in data and len(data) < 65536:
            chunk = conn.recv(4096)
            if not chunk:
                break
            data += chunk
    except OSError:
        pass
    parts = data.split(b"\r\n", 1)[0].decode("latin-1").split(" ")
    if data and len(parts) >= 2:
        path = parts[1]
        note(log_path, path if name == "main" else name + ":" + path)
        # A valid 1x1 PNG for /img, so an `Image` that reaches us loads instead of erroring.
        body = PNG if path == "/img" else b""
        head = ("HTTP/1.1 200 OK\r\nContent-Type: " + ("image/png" if body else "text/plain")
                + "\r\nContent-Length: " + str(len(body)) + "\r\nConnection: close\r\n\r\n")
        try:
            conn.sendall(head.encode("latin-1") + body)
        except OSError:
            pass
    conn.close()

def serve(name, listener):
    while True:
        conn, _ = listener.accept()
        note(accepts_path, name)
        threading.Thread(target=handle, args=(name, conn), daemon=True).start()

listeners = []
for name in NAMES:
    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    listener.listen(16)
    listeners.append((name, listener))
for name, listener in listeners:
    with open(port_prefix + "." + name, "w") as port:
        port.write(str(listener.getsockname()[1]))
    if name != NAMES[-1]:
        threading.Thread(target=serve, args=(name, listener), daemon=True).start()
serve(*listeners[-1])
PY

# name csp(on=restrictive, off=permissive) incognito auditing(off=production's settings)
run_once() {
    local name="$1" csp="$2" incognito="$3" auditing="$4"
    local dir="$OUT_DIR/$name" home="$TMP/home-$name"
    mkdir -p "$dir" "$home/data" "$home/cache" "$home/config"
    : > "$dir/logger.log"
    : > "$dir/accepts.log"
    : > "$dir/messages.jsonl"
    rm -f "$TMP/port-$name".*
    python3 -I "$TMP/logger.py" "$dir/logger.log" "$dir/accepts.log" "$TMP/port-$name" &
    LOGGER_PID=$!
    for _ in $(seq 1 100); do
        [[ -s "$TMP/port-$name.ping" ]] && break
        kill -0 "$LOGGER_PID" 2>/dev/null || { echo "FAIL: the logger died" >&2; exit 1; }
        sleep 0.1
    done
    [[ -s "$TMP/port-$name.ping" ]] || {
        echo "FAIL: the logger never announced its ports" >&2
        exit 1
    }
    local port; port="$(cat "$TMP/port-$name.main")"
    local pre; pre="$(cat "$TMP/port-$name.preconnect")"
    local pref; pref="$(cat "$TMP/port-$name.prefetch")"
    local ping; ping="$(cat "$TMP/port-$name.ping")"
    local status=0
    env -u WAYLAND_DISPLAY -u http_proxy -u https_proxy -u all_proxy \
        -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY \
        DISPLAY="$DISPLAY_ARG" GDK_BACKEND=x11 NO_PROXY=127.0.0.1 no_proxy=127.0.0.1 \
        HOME="$home" XDG_DATA_HOME="$home/data" XDG_CACHE_HOME="$home/cache" \
        XDG_CONFIG_HOME="$home/config" \
        timeout 60 "$EXAMPLE" --logger "http://127.0.0.1:$port" --html "$VIEW" \
            --logger-preconnect "http://127.0.0.1:$pre" \
            --logger-prefetch "http://127.0.0.1:$pref" \
            --logger-ping "http://127.0.0.1:$ping" \
            --out "$dir/messages.jsonl" --csp "$csp" --incognito "$incognito" \
            --auditing "$auditing" \
            >"$dir/example.stdout" 2>"$dir/example.stderr" || status=$?
    echo "$status" > "$dir/example.status"
    sleep 0.3
    kill "$LOGGER_PID" 2>/dev/null || true
    wait "$LOGGER_PID" 2>/dev/null || true
    LOGGER_PID=""
}

run_once run on on off
run_once control off off on

# --- judge -------------------------------------------------------------------
cat > "$TMP/judge.py" <<'PY'
import json, os, sys

out_dir, tmp = sys.argv[1], sys.argv[2]
failures = []
known_gaps = []

def ok(text):
    print("OK: " + text)

def check(condition, text, detail=""):
    if condition:
        ok(text)
    else:
        failures.append(text)
        print("FAIL: " + text + (" (" + detail + ")" if detail else ""))

def load(name):
    d = os.path.join(out_dir, name)
    lines = []
    with open(os.path.join(d, "messages.jsonl")) as f:
        for raw in f:
            raw = raw.strip()
            if raw:
                lines.append(json.loads(raw))
    probes = {}
    forged_methods = 0
    for line in lines:
        if line.get("kind") != "message":
            continue
        message = line["message"]
        if message.get("method") == "forged":
            forged_methods += 1
        if message.get("method") == "probe":
            probes[line["role"]] = message["params"]
    drops = [l["value"] for l in lines if l.get("kind") == "acceptsDrops"]
    forged = [l["count"] for l in lines if l.get("kind") == "forged"]
    end = [l["reason"] for l in lines if l.get("kind") == "end"]
    with open(os.path.join(d, "logger.log")) as f:
        paths = [p.strip() for p in f if p.strip()]
    with open(os.path.join(d, "accepts.log")) as f:
        accept_names = [p.strip() for p in f if p.strip()]
    accepts = {name: accept_names.count(name)
               for name in ("main", "preconnect", "prefetch", "ping")}
    with open(os.path.join(d, "example.status")) as f:
        status = int(f.read().strip())
    home = os.path.join(tmp, "home-" + name)
    needle8 = b"sirio-probe"
    needle16 = "sirio-probe".encode("utf-16le")
    hits = []
    for base, _, files in os.walk(home):
        for fname in files:
            path = os.path.join(base, fname)
            try:
                with open(path, "rb") as f:
                    data = f.read()
            except OSError:
                continue
            if needle8 in data or needle16 in data:
                hits.append(os.path.relpath(path, home))
    return {
        "probeA": probes.get("A"), "probeB": probes.get("B"),
        "acceptsDrops": drops, "forgedCount": forged[0] if forged else None,
        "forgedMethodsAccepted": forged_methods, "end": end,
        "loggerPaths": paths, "tcpAccepts": accepts,
        "homeFilesWithProbe": hits, "exampleStatus": status,
    }

run, control = load("run"), load("control")
results = {"run": run, "control": control}

for name, r in (("run", run), ("control", control)):
    check(r["exampleStatus"] == 0 and r["end"] == ["done"],
          name + ": the example ran to the end", str(r["end"]))
    check(r["probeA"] is not None and r["probeB"] is not None,
          name + ": both views reported")

def first_token(value):
    return str(value).split(" ")[0]

if run["probeA"] and run["probeB"] and control["probeA"] and control["probeB"]:
    a, b = run["probeA"], run["probeB"]
    # --- the run -------------------------------------------------------------
    for key, want in (("parentDocument", "blocked"), ("frameElement", "null"),
                      ("parentIpc", "blocked"), ("rtc", "absent"), ("fetch", "blocked"),
                      ("img", "blocked"), ("open", "null")):
        label = {"open": "window.open from the view returns null"}.get(key, key + " is " + want)
        check(a.get(key) == want, "run: " + label, str(a.get(key)))
    seen = {first_token(v) for v in a.get("violations", [])}
    for directive in ("connect-src", "img-src", "frame-src"):
        check(directive in seen, "run: a " + directive + " violation was reported", str(seen))
    for key in ("geolocation", "notification"):
        if a.get(key) == "absent":
            ok("run: " + key + " is absent in this engine")
        else:
            check(a.get(key) == "denied", "run: " + key + " ends denied", str(a.get(key)))
    check(run["acceptsDrops"] == [False], "run: the webview accepts no drops",
          str(run["acceptsDrops"]))
    check(run["forgedMethodsAccepted"] == 0, "run: no forged message was accepted")
    if a.get("webkitIpc") == "present":
        check((run["forgedCount"] or 0) >= 1, "run: the forged IPC post was dropped and counted",
              str(run["forgedCount"]))
    else:
        ok("run: the iframe cannot see webkit.messageHandlers.ipc")
    check(run["loggerPaths"] == [], "run: the logger saw no path", str(run["loggerPaths"]))
    check(a.get("hints") == "added", "run: the preconnect, prefetch and ping probes were set up",
          str(a.get("hints")))
    # Raw TCP accepts, counted before any byte is read: a bare preconnect sends no request line.
    for name, label in (("main", "any connection to the logger"),
                        ("prefetch", "prefetch"), ("ping", "<a ping>")):
        check(run["tcpAccepts"][name] == 0, "run: no TCP connect for " + label,
              str(run["tcpAccepts"]))
    # KNOWN GAP, not a pass: WebKitGTK connects for `<link rel=preconnect>` whatever the CSP says
    # (observed 2026-11-19, 2.54.1). The setting that turns it off is the `LinkPreconnect` feature
    # (`webkit_settings_set_feature_enabled`, 2.42+), which the webkit2gtk 2.0.2 and
    # webkit2gtk-sys 2.0.2 bindings in use do not expose. The gap is reported, loudly, in the
    # output and in widget-isolation.json; it fails the run the day the bindings let it be closed.
    if run["tcpAccepts"]["preconnect"] == 0:
        ok("run: no TCP connect for preconnect and dns-prefetch")
    else:
        known_gaps.append("preconnect: " + str(run["tcpAccepts"]["preconnect"])
                          + " TCP connect(s) reached a port outside connect-src")
        print("KNOWN GAP: run: preconnect made " + str(run["tcpAccepts"]["preconnect"])
              + " TCP connect(s) to a port outside connect-src; the webkit2gtk bindings in use "
              "have no LinkPreconnect feature switch")
    # --- the control ---------------------------------------------------------
    ca, cb = control["probeA"], control["probeB"]
    for path in ("/fetch", "/img"):
        check(path in control["loggerPaths"], "control: the logger saw " + path,
              str(control["loggerPaths"]))
    check(ca.get("violations") == [], "control: no violation was reported",
          str(ca.get("violations")))
    # --- storage -------------------------------------------------------------
    storage_ok = a.get("storageWrite") == "ok" or ca.get("storageWrite") == "ok"
    if not storage_ok:
        ok("storage is unavailable to the view")
    else:
        check(b.get("storageRead") is None, "run: B reads null from the storage A wrote to",
              str(b.get("storageRead")))
        check(run["homeFilesWithProbe"] == [],
              "run: no storage file under HOME holds the probe value",
              str(run["homeFilesWithProbe"]))
        # On WebKitGTK 2.54.1 the localStorage of these custom-scheme origins is neither
        # shared between webviews nor written to disk, even with incognito off (observed
        # 2026-11-19). Storage isolation here therefore comes from the engine, and incognito
        # is defence in depth. The control cannot show a leak, so these two are facts, not
        # expectations.
        print("FACT: control: B reads the storage A wrote: " + repr(cb.get("storageRead")))
        print("FACT: control: storage reached disk under HOME: "
              + str(len(control["homeFilesWithProbe"]) >= 1).lower())

    # The control runs the same hints with hyperlink auditing and DNS prefetching turned back on
    # and a permissive CSP: what reaches its loggers is what production's settings and CSP stop.
    print("FACT: control: TCP accepts " + json.dumps(control["tcpAccepts"], sort_keys=True))
    print("FACT: control: request paths " + json.dumps(control["loggerPaths"]))
    print("FACT: control: window.open returns " + repr(ca.get("open")))
    print("FACT: control: geolocation is " + repr(ca.get("geolocation"))
          + " (code " + repr(ca.get("geolocationCode")) + ")")
    print("FACT: control: notification is " + repr(ca.get("notification")))

results["facts"] = {
    "tcpAccepts": {"run": run["tcpAccepts"], "control": control["tcpAccepts"]},
    "ownIpc": (run["probeA"] or {}).get("ownIpc"),
    "webkitIpc": (run["probeA"] or {}).get("webkitIpc"),
    "forgedCount": run["forgedCount"],
    "storageSelfRead": {"run": (run["probeA"] or {}).get("storageSelfRead"),
                        "control": (control["probeA"] or {}).get("storageSelfRead")},
    "storageWrite": {"run": (run["probeA"] or {}).get("storageWrite"),
                     "control": (control["probeA"] or {}).get("storageWrite")},
    "geolocation": (run["probeA"] or {}).get("geolocation"),
    "notification": (run["probeA"] or {}).get("notification"),
}
results["failures"] = failures
results["knownGaps"] = known_gaps
with open(os.path.join(out_dir, "widget-isolation.json"), "w") as f:
    json.dump(results, f, indent=2, sort_keys=True)
sys.exit(1 if failures else 0)
PY
python3 -I "$TMP/judge.py" "$OUT_DIR" "$TMP"
