#!/usr/bin/env bash
# Does an MCP Apps widget webview stay isolated on WebKitGTK?
#
# `examples/widget_isolation.rs` builds the real widget webview (`build_widget_child`) and loads
# `fixtures/mcp-apps/isolation-view.html`, a probe page that tries to reach its parent, the
# network, devices and storage, and reports what happened. This script runs it twice and judges
# the reports:
#
#   run      CSP header on,  incognito on   -- what production ships. Every escape must fail.
#   control  CSP header off, incognito off  -- the same probes with the protections removed.
#            Each protection must visibly matter here: if the control does not leak, the
#            run's "blocked" proves nothing.
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
[[ -n "$DISPLAY_ARG" && -n "$OUT_DIR" ]] || { echo "FAIL: --display and --out-dir are required" >&2; exit 2; }
case "$DISPLAY_ARG" in
    :0|:0.*|:94|:94.*|:120|:120.*|:121|:121.*) echo "FAIL: refusing display $DISPLAY_ARG" >&2; exit 2 ;;
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
import http.server, sys

log_path, port_path = sys.argv[1], sys.argv[2]

PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4"
    "890000000b49444154789c6360000200000500017a5eab3f0000000049454e44ae426082")

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        with open(log_path, "a") as log:
            log.write(self.path + "\n")
        # A valid 1x1 PNG for /img, so an `Image` that reaches us loads instead of erroring.
        body = PNG if self.path == "/img" else b""
        self.send_response(200)
        self.send_header("Content-Type", "image/png" if body else "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    do_POST = do_GET
    def log_message(self, *args):
        pass

server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(port_path, "w") as port:
    port.write(str(server.server_address[1]))
server.serve_forever()
PY

run_once() { # name csp incognito
    local name="$1" csp="$2" incognito="$3"
    local dir="$OUT_DIR/$name" home="$TMP/home-$name"
    mkdir -p "$dir" "$home/data" "$home/cache" "$home/config"
    : > "$dir/logger.log"
    : > "$dir/messages.jsonl"
    rm -f "$TMP/port-$name"
    python3 -I "$TMP/logger.py" "$dir/logger.log" "$TMP/port-$name" &
    LOGGER_PID=$!
    for _ in $(seq 1 100); do
        [[ -s "$TMP/port-$name" ]] && break
        kill -0 "$LOGGER_PID" 2>/dev/null || { echo "FAIL: the logger died" >&2; exit 1; }
        sleep 0.1
    done
    [[ -s "$TMP/port-$name" ]] || { echo "FAIL: the logger never announced a port" >&2; exit 1; }
    local port; port="$(cat "$TMP/port-$name")"
    local status=0
    env -u WAYLAND_DISPLAY -u http_proxy -u https_proxy -u all_proxy \
        -u HTTP_PROXY -u HTTPS_PROXY -u ALL_PROXY \
        DISPLAY="$DISPLAY_ARG" GDK_BACKEND=x11 NO_PROXY=127.0.0.1 no_proxy=127.0.0.1 \
        HOME="$home" XDG_DATA_HOME="$home/data" XDG_CACHE_HOME="$home/cache" \
        XDG_CONFIG_HOME="$home/config" \
        timeout 60 "$EXAMPLE" --logger "http://127.0.0.1:$port" --html "$VIEW" \
            --out "$dir/messages.jsonl" --csp "$csp" --incognito "$incognito" \
            >"$dir/example.stdout" 2>"$dir/example.stderr" || status=$?
    echo "$status" > "$dir/example.status"
    sleep 0.3
    kill "$LOGGER_PID" 2>/dev/null || true
    wait "$LOGGER_PID" 2>/dev/null || true
    LOGGER_PID=""
}

run_once run on on
run_once control off off

# --- judge -------------------------------------------------------------------
cat > "$TMP/judge.py" <<'PY'
import json, os, sys

out_dir, tmp = sys.argv[1], sys.argv[2]
failures = []

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
        "loggerPaths": paths, "homeFilesWithProbe": hits, "exampleStatus": status,
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
        check(a.get(key) == want, "run: " + key + " is " + want, str(a.get(key)))
    seen = {first_token(v) for v in a.get("violations", [])}
    for directive in ("connect-src", "img-src", "frame-src"):
        check(directive in seen, "run: a " + directive + " violation was reported", str(seen))
    for key in ("geolocation", "notification"):
        if a.get(key) == "absent":
            ok("run: " + key + " is absent in this engine")
        else:
            check(a.get(key) == "denied", "run: " + key + " is denied", str(a.get(key)))
    check(run["acceptsDrops"] == [False], "run: the webview accepts no drops",
          str(run["acceptsDrops"]))
    check(run["forgedMethodsAccepted"] == 0, "run: no forged message was accepted")
    if a.get("webkitIpc") == "present":
        check((run["forgedCount"] or 0) >= 1, "run: the forged IPC post was dropped and counted",
              str(run["forgedCount"]))
    else:
        ok("run: the iframe cannot see webkit.messageHandlers.ipc")
    check(run["loggerPaths"] == [], "run: the logger saw no path", str(run["loggerPaths"]))
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
        check(b.get("storageRead") is None, "run: B reads no storage A wrote",
              str(b.get("storageRead")))
        check(run["homeFilesWithProbe"] == [], "run: nothing reached disk under HOME",
              str(run["homeFilesWithProbe"]))
        check(cb.get("storageRead") == "A", "control: B reads the storage A wrote",
              str(cb.get("storageRead")))
        check(len(control["homeFilesWithProbe"]) >= 1, "control: storage reached disk under HOME")

results["facts"] = {
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
with open(os.path.join(out_dir, "widget-isolation.json"), "w") as f:
    json.dump(results, f, indent=2, sort_keys=True)
sys.exit(1 if failures else 0)
PY
python3 -I "$TMP/judge.py" "$OUT_DIR" "$TMP"
