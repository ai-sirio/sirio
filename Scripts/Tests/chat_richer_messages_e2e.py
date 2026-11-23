#!/usr/bin/env python3
"""The scenarios of test-chat-richer-messages-e2e.sh (read its header first).

Everything here runs against real, isolated debug Sirio instances on the script's private Xvfb. A
`World` is one such instance with its own HOME, XDG tree, database, control socket and host root, started
under `env -i` with an allowlist, the stub `claude` and `opencode` first on its PATH. A scenario is a
function that raises `Fail` at its first failed check; the run reports it and goes on with the next one.
"""
import json
import os
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import time
import traceback
from datetime import datetime, timedelta, timezone
from pathlib import Path

E = os.environ
ROOT = Path(E["ROOT"])
RUN_DIR = Path(E["RUN_DIR"])
OUT = Path(E["OUT"])
DISP = E["DISP"]
BIN = E["BIN"]
CTL = E["CTL"]
WIN_W, WIN_H = (int(v) for v in E["WINDOW_SIZE"].split("x"))
ONLY = [name for name in E.get("ONLY", "").split(",") if name]
VULKAN = {name: E[name] for name in ("VK_DRIVER_FILES", "VK_ICD_FILENAMES")}
ENV_X = dict(os.environ, DISPLAY=DISP)
STUBS = ROOT / "Scripts" / "Tests" / "stubs" / "richer"
MARKER = f"SIRIO_RICHER_E2E_RUN={RUN_DIR}"

CHECKS = []  # (scenario, what), one per check that passed
FAILED = []  # (scenario, why)
CHECKS_LOG = None


class Fail(Exception):
    pass


class RpcError(Exception):
    pass


def log(message):
    print(message, flush=True)


# ---------------------------------------------------------------- checks


class Scenario:
    """One scenario's checks. `need` records a passed check or raises Fail with the scenario's name."""

    def __init__(self, name):
        self.name = name
        self.readouts = []  # what the scenario read, kept in scenarios/<name>.json

    def ok(self, what, detail=None):
        CHECKS.append((self.name, what))
        log(f"OK: {self.name}: {what}")
        CHECKS_LOG.write(json.dumps({"scenario": self.name, "check": what, "status": "OK", "detail": detail}) + "\n")
        CHECKS_LOG.flush()

    def need(self, condition, what, why="", detail=None):
        if condition:
            self.ok(what, detail)
        else:
            raise Fail(f"{what}" + (f": {why}" if why else ""))

    def keep(self, label, value):
        self.readouts.append({"label": label, "value": value})


# ---------------------------------------------------------------- the control socket


def rpc_in(socket_path, method, **params):
    s = socket.socket(socket.AF_UNIX)
    s.settimeout(30)
    s.connect(str(socket_path))
    s.sendall((json.dumps({"id": "e2e", "method": method, "params": params}) + "\n").encode())
    buf = b""
    while not buf.endswith(b"\n"):
        chunk = s.recv(1 << 20)
        if not chunk:
            break
        buf += chunk
    s.close()
    response = json.loads(buf)
    if not response.get("ok"):
        raise RpcError(response.get("error") or method)
    return response.get("result") or {}


# ---------------------------------------------------------------- X input and the clipboard


def x(*arguments, timeout=30):
    subprocess.run(["xdotool", *arguments], env=ENV_X, check=True, timeout=timeout)


def move(window, wx, wy):
    x("mousemove", "--window", window, str(int(wx)), str(int(wy)))


def press(window, wx, wy, button="1", hold=0.15, settle=0.1):
    """A real click, its press and release a hand's distance apart: text listens for the release in the frame
    after the press, and this window paints in software."""
    move(window, wx, wy)
    time.sleep(settle)
    x("mousedown", button)
    time.sleep(hold)
    x("mouseup", button)


GTK_CLIPBOARD = """
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if sys.argv[1] == "get":
    text = clipboard.wait_for_text()
    sys.stdout.write("" if text is None else text)
else:
    clipboard.set_text(sys.argv[2], -1)
    GLib.timeout_add_seconds(int(sys.argv[3]), Gtk.main_quit)
    Gtk.main()
"""


def clipboard_command(*arguments):
    home = RUN_DIR / "clipboard-home"
    home.mkdir(exist_ok=True)
    return ["env", "-i", "PATH=/usr/local/bin:/usr/bin:/bin", f"HOME={home}", "GDK_BACKEND=x11", f"DISPLAY={DISP}",
            "python3", "-I", "-c", GTK_CLIPBOARD, *arguments]


def clipboard_get():
    done = subprocess.run(clipboard_command("get"), capture_output=True, text=True, timeout=20)
    return done.stdout


class ClipboardOwner:
    """Another application owning the clipboard with a sentinel, until released: what a Copy must replace."""

    def __init__(self, text, seconds=120):
        self.text = text
        self.process = subprocess.Popen(clipboard_command("own", text, str(seconds)))
        for _ in range(25):
            if clipboard_get() == text:
                return
            time.sleep(0.2)
        self.release()
        raise Fail(f"the harness could not own the clipboard with {text!r}")

    def release(self):
        self.process.kill()
        self.process.wait()


def window_for(pid):
    """The largest window whose `_NET_WM_PID` is `pid`: never another process's."""
    listing = subprocess.run(["xwininfo", "-display", DISP, "-root", "-children"], capture_output=True, text=True,
                             timeout=15).stdout
    best = None
    for line in listing.splitlines():
        found = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
        if not found:
            continue
        window, width, height = found.group(1), int(found.group(2)), int(found.group(3))
        prop = subprocess.run(["xprop", "-display", DISP, "-id", window, "_NET_WM_PID"], capture_output=True,
                              text=True, timeout=15).stdout
        owner = re.search(r"=\s*(\d+)\s*$", prop)
        if owner and int(owner.group(1)) == pid and (best is None or width * height > best[0]):
            best = (width * height, window)
    return best[1] if best else None


# ---------------------------------------------------------------- one isolated Sirio


class World:
    """One isolated debug Sirio on the private display, and the chat it holds."""

    def __init__(self, scenario, name):
        self.scn = scenario
        self.name = name
        self.dir = RUN_DIR / f"{scenario.name}-{name}"  # one directory (and stub log) per world, whatever the scenario
        for sub in ("home", "data", "state", "cache", "host", "repo"):
            (self.dir / sub).mkdir(parents=True, exist_ok=True)
        self.runtime = self.dir / "runtime"
        self.runtime.mkdir(exist_ok=True, mode=0o700)
        self.socket = self.dir / "c.sock"
        if len(str(self.socket)) >= 100:
            raise Fail(f"the control socket path is too long for a unix socket: {self.socket}")
        self.repo = self.dir / "repo"
        self.stub_log = self.dir / "stub.log"
        self.gate = self.dir / "gate"  # the stub holds `<prompt> hold` turns until `gate.<n>` exists
        self.limit_file = self.dir / "limit-at"
        self.reply_file = self.dir / "reply.md"
        self.refuse_file = self.dir / "refuse-start"  # while it exists, the stub `opencode acp` fails to start
        self.proc = None
        self.surface = ""
        self.window = None
        self.launches = 0
        self._init_repo()

    def _init_repo(self):
        for command in (["git", "init", "-q", "-b", "main"], ["git", "config", "user.email", "t@example.com"],
                        ["git", "config", "user.name", "Tester"], ["git", "commit", "-q", "--allow-empty", "-m", "first"]):
            subprocess.run(command, cwd=self.repo, check=True)

    # -- the process

    def env(self):
        path = f"{STUBS}:/usr/local/bin:/usr/bin:/bin"
        pairs = {
            "PATH": path, "HOME": str(self.dir / "home"), "XDG_CONFIG_HOME": str(self.dir / "home" / ".config"),
            "XDG_DATA_HOME": str(self.dir / "data"), "XDG_STATE_HOME": str(self.dir / "state"),
            "XDG_CACHE_HOME": str(self.dir / "cache"), "XDG_RUNTIME_DIR": str(self.runtime),
            "SHELL": "/bin/bash", "TERM": "xterm-256color", "LANG": "C.UTF-8", "TZ": "UTC",
            "SIRIO_SOCKET": str(self.socket), "SIRIO_DB": str(self.dir / "session.sqlite"),
            "SIRIO_CREDENTIALS": str(self.dir / "credentials.json"), "SIRIO_HOST_HOME": str(self.dir / "host"),
            "SIRIO_STUB_LOG": str(self.stub_log), "SIRIO_FIXTURE_GATE": str(self.gate),
            "SIRIO_FIXTURE_LIMIT_FILE": str(self.limit_file), "SIRIO_FIXTURE_REPLY_FILE": str(self.reply_file),
            "SIRIO_FIXTURE_REFUSE_FILE": str(self.refuse_file),
            "SIRIO_RICHER_E2E_RUN": str(RUN_DIR),
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={self.dir}/no-bus",
            "DISPLAY": DISP, "GPUI_X11_SCALE_FACTOR": "1", **VULKAN,
        }
        return pairs

    def start(self):
        resolved = shutil.which("claude", path=self.env()["PATH"])
        if resolved != str(STUBS / "claude"):
            raise Fail(f"claude resolves to {resolved!r} under the app's PATH, not the richer stub")
        self.launches += 1
        before = len(self.records("claude"))
        if self.socket.exists():
            self.socket.unlink()
        log_file = open(self.dir / "app.log", "ab")
        log_file.write(f"--- launch {self.launches}\n".encode())
        pairs = [f"{k}={v}" for k, v in self.env().items()]
        self.proc = subprocess.Popen(["env", "-i", *pairs, BIN], cwd=self.repo, stdout=log_file,
                                     stderr=subprocess.STDOUT, start_new_session=True)
        for _ in range(150):
            if self.socket.exists():
                break
            if self.proc.poll() is not None:
                raise Fail(f"sirio exited ({self.proc.returncode}) before opening its control socket")
            time.sleep(0.2)
        if not self.socket.exists():
            raise Fail("sirio never opened its control socket")
        for _ in range(150):
            if len(self.records("claude")) > before:
                break
            time.sleep(0.2)
        else:
            raise Fail("the app's claude --version probe never reached the stub")
        # A window focused for sure, so that a chat on screen reads on screen.
        self.ctl("surface", "attention", "act", "--focus", "active")
        for _ in range(100):
            self.window = window_for(self.proc.pid)
            if self.window:
                break
            time.sleep(0.2)
        if not self.window:
            raise Fail(f"no window with _NET_WM_PID={self.proc.pid}")
        subprocess.run(["xdotool", "windowmove", self.window, "0", "0", "windowsize", self.window, str(WIN_W),
                        str(WIN_H)], env=ENV_X, check=True, timeout=10)
        time.sleep(1)

    def quit(self):
        if self.proc is None:
            return
        try:
            self.ctl("quit", check=False)
        except Exception:
            pass
        try:
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()
        self.proc = None
        self.stop_hosts()

    def stop_hosts(self):
        for state in self.dir.glob("host/host-v*.json"):
            try:
                pid = int(json.loads(state.read_text())["pid"])
                comm = Path(f"/proc/{pid}/comm").read_text()
            except (OSError, ValueError, KeyError, TypeError):
                continue
            if "sirio-host" not in comm:
                continue
            try:
                os.kill(pid, signal.SIGTERM)
                for _ in range(50):
                    time.sleep(0.1)
                    os.kill(pid, 0)
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    # -- calls

    def rpc(self, method, **params):
        return rpc_in(self.socket, method, **params)

    def ctl(self, *args, check=True):
        done = subprocess.run([CTL, *args, "--json"], env={"SIRIO_SOCKET": str(self.socket), "PATH": "/usr/bin:/bin"},
                              capture_output=True, text=True, timeout=60)
        if check and done.returncode != 0:
            raise Fail(f"sirioctl {' '.join(args)} failed: {done.stderr.strip() or done.stdout.strip()}")
        return done

    def tab_positions(self, *kinds):
        done = self.ctl("surface", "tabs", "read")
        tabs = json.loads(done.stdout)[0]
        return sorted((int(k.split(".")[1]) for k, v in tabs.items() if k.startswith("tab.") and v.split("|")[0] in kinds),
                      reverse=True)

    def open_chat(self, agent, connected=True):
        """Opens a chat of `agent` as the first and only tab of a fresh Sirio, as the session-panel E2E does."""
        self.ctl("project", "add", str(self.repo))
        for _ in range(50):
            if self.ctl("surface", "open-list", "open", "--entry", f"agent:{agent}", check=False).returncode == 0:
                break
            time.sleep(0.3)
        else:
            raise Fail(f"the Open list never offered a {agent} chat")
        old = self.tab_positions("chat", "terminal")
        self.ctl("surface", "open-list", "open", "--entry", f"agent:{agent}")
        for position in old:
            self.ctl("surface", "tabs", "close", str(position), "--confirm")
        self.surface = ""
        for _ in range(50):
            try:
                self.surface = self.rpc("surface.chat.open")["surfaceId"]
                break
            except (RpcError, OSError):
                time.sleep(0.3)
        if not self.surface:
            raise Fail("no chat tab opened")
        if connected:
            self.wait("the chat has connected", lambda r: r["status"] in ("idle", "completed"), timeout=40)

    def reopen_chat(self):
        """After a relaunch: the restored chat tab."""
        self.surface = ""
        for _ in range(100):
            try:
                self.surface = self.rpc("surface.chat.open")["surfaceId"]
                break
            except (RpcError, OSError):
                time.sleep(0.3)
        if not self.surface:
            raise Fail("no chat tab after the relaunch")
        self.wait("the restored chat has connected", lambda r: r["status"] in ("idle", "completed"), timeout=40)

    # -- the chat

    def read(self):
        result = self.rpc("surface.chat.read", surfaceId=self.surface)
        r = dict(result)
        r["rows"] = json.loads(result.get("transcript") or "[]")
        r["queue"] = [row["text"] for row in json.loads(result.get("queued") or "[]")]
        r["painted"] = json.loads(result.get("transcriptTextRows") or "[]")
        r["separators"] = json.loads(result.get("daySeparators") or "[]")
        r["composerRow"] = json.loads(result.get("composerRow") or "{}")
        r["is_held"] = result.get("held") == "true"
        r["held_line"] = result.get("heldLine", "")
        r["transcript_text"] = result.get("transcriptText", "")
        return r

    def wait(self, what, predicate, timeout=30, every=0.15):
        deadline = time.time() + timeout
        last = None
        while True:
            try:
                last = self.read()
                if predicate(last):
                    return last
            except (RpcError, OSError) as error:
                last = {"error": str(error)}
            if time.time() > deadline:
                raise Fail(f"{what}: not true within {timeout:.0f} s (last: {brief(last)})")
            time.sleep(every)

    def send(self, text, queued=False):
        if not queued:
            self.wait(f"the chat is ready for {text!r}", lambda r: r["status"] in ("idle", "completed"))
        self.rpc("surface.chat.send", surfaceId=self.surface, text=text)
        log(f"step: sent {text!r}")

    def stop_turn(self):
        self.rpc("surface.chat.stop", surfaceId=self.surface)

    def compose(self, text):
        self.rpc("surface.chat.compose", surfaceId=self.surface, text=text)

    def retry(self, entry):
        self.rpc("surface.chat.transcript", surfaceId=self.surface, action="retry", entry=str(entry))

    def retry_refusal(self, entry):
        """The sentence the socket refuses a Retry with, or None when it was accepted."""
        try:
            self.retry(entry)
        except RpcError as error:
            return str(error)
        return None

    def release_gate(self, number):
        Path(f"{self.gate}.{number}").write_text("go")

    # -- what the stubs recorded

    def records(self, kind=None):
        """The stub log as (kind, pid, payload): the shim's `claude` and `opencode` lines carry their arguments
        as text, the fixtures' `start`, `prompt` and `session_new` lines carry JSON."""
        out = []
        if not self.stub_log.exists():
            return out
        for line in self.stub_log.read_text(encoding="utf-8").splitlines():
            parts = line.split("\t", 2)
            if len(parts) != 3:
                continue
            kind_, pid, payload = parts
            if kind_ in ("claude", "opencode"):
                value = payload
            else:
                value = json.loads(payload)
            if kind is None or kind_ == kind:
                out.append((kind_, int(pid), value))
        return out

    def prompts(self):
        """Every prompt a fixture read, as (pid, text), in order."""
        return [(pid, value["text"] if isinstance(value, dict) else value) for _, pid, value in self.records("prompt")]

    def sessions(self, agent):
        """The pids of the agent processes a chat was started with, in order: the stub's `claude` calls that name a
        session (`--session-id` or `--resume`, the usage probe names none) or its `opencode acp` calls. The app
        also starts probes, and `open_chat` opens a first chat that it then closes; the newest is the live one."""
        found = []
        for kind, pid, args in self.records(agent):
            if agent == "claude" and ("--session-id" in args or "--resume" in args):
                found.append(pid)
            elif agent == "opencode" and args.strip() == "acp":
                found.append(pid)
        return found

    def capture(self, name):
        """A PID-matched window capture, kept under frames/."""
        time.sleep(0.7)
        destination = OUT / "frames" / f"{name}.png"
        subprocess.run(["import", "-display", DISP, "-window", self.window, str(destination)], check=True, timeout=30)
        colours = int(subprocess.run(["identify", "-format", "%k", str(destination)], capture_output=True, text=True,
                                     check=True).stdout)
        if colours < 200:
            raise Fail(f"the capture {name} is blank ({colours} colours)")
        log(f"FRAME: {name} ({colours} colours; pid {self.proc.pid})")

    def keep_artifacts(self):
        if self.stub_log.exists():
            shutil.copy(self.stub_log, OUT / "scenarios" / f"{self.scn.name}.{self.name}.stub.log")
        if (self.dir / "app.log").exists():
            shutil.copy(self.dir / "app.log", OUT / "logs" / f"{self.scn.name}.{self.name}.app.log")


def brief(r):
    """A readout in a line, for a failure message."""
    if not isinstance(r, dict) or "rows" not in r:
        return json.dumps(r)[:300]
    rows = []
    for index, row in enumerate(r["rows"]):
        text = (row.get("text") or "").replace("\n", " ")[:40]
        extra = "".join(f" {key}={row[key]}" for key in ("errorKind", "retry", "coveredBy", "phase", "live", "shownBytes")
                        if key in row)
        rows.append(f"{index}:{row.get('kind')}{extra}:{text!r}")
    return json.dumps({"status": r.get("status"), "queue": r.get("queue"), "held": r.get("held"),
                       "heldLine": r.get("held_line"), "composer": r.get("composerText"), "rows": rows})[:900]


# ---------------------------------------------------------------- helpers on a readout


def rows_of(r, kind):
    return [(index, row) for index, row in enumerate(r["rows"]) if row.get("kind") == kind]


def error_row(r):
    """The (index, row) of the newest error row, or None."""
    found = rows_of(r, "error")
    return found[-1] if found else None


def assistant_rows(r):
    return rows_of(r, "assistant")


def user_texts(r):
    return [row["text"] for _, row in rows_of(r, "user")]


def byte_len(text):
    return len(text.encode("utf-8"))


def markers(text):
    """(byte offset of its end, marker) of every `part-NNN-end` in a reply's source."""
    out = []
    for found in re.finditer(r"part-\d{3}-end", text):
        out.append((byte_len(text[:found.end()]), found.group(0)))
    return out


def plain_by_row(r):
    """The plain text the transcript holds for each row, in row order: an assistant row's is the page's text,
    which `transcriptText` carries between the rows around it. Raises Fail when the rows do not tile it."""
    text, rows, position, out = r["transcript_text"], r["rows"], 0, []
    for index, row in enumerate(rows):
        following = next((other for other in rows[index + 1:] if other.get("kind") != "assistant"), None)
        if row.get("kind") == "assistant":
            if following is None:
                end = len(text)
            else:
                end = text.find("\n\n" + following.get("text", ""), position)
                if end < 0:
                    raise Fail(f"row {index}: the transcript text does not hold the next row {following!r} after it")
            out.append(text[position:end])
            position = end + 2
        else:
            own = row.get("text", "")
            if not text.startswith(own, position):
                raise Fail(f"row {index} ({row.get('kind')}): its text {own!r} is not where the transcript text has it")
            out.append(own)
            position += len(own) + 2
    return out


# ---------------------------------------------------------------- the scenarios

SCENARIOS = []


def scenario(function):
    SCENARIOS.append(function)
    return function


def teardown(world):
    for w in world if isinstance(world, list) else [world]:
        try:
            w.keep_artifacts()
        finally:
            w.quit()


def run_scenarios():
    names = [function.__name__.removeprefix("scenario_") for function in SCENARIOS]
    for name in ONLY:
        if name not in names:
            log(f"refused: unknown scenario {name} ({', '.join(names)})")
            sys.exit(2)
    ran = 0
    for function in SCENARIOS:
        name = function.__name__.removeprefix("scenario_")
        if ONLY and name not in ONLY:
            continue
        log(f"== scenario {name}")
        scn = Scenario(name)
        started = time.time()
        try:
            function(scn)
        except Fail as failure:
            log(f"FAIL: {name}: {failure}")
            FAILED.append((name, str(failure)))
        except Exception as error:  # a bug of the harness or an app that died: a failure all the same
            log(f"FAIL: {name}: {error!r}")
            traceback.print_exc()
            FAILED.append((name, repr(error)))
        finally:
            (OUT / "scenarios" / f"{name}.json").write_text(json.dumps(scn.readouts, indent=1))
            sweep()
        ran += 1
        log(f"== scenario {name} took {time.time() - started:.0f} s")
    return ran


def sweep():
    """Stops what this run started and left running: every process whose environment carries the run's marker."""
    marker = MARKER.encode() + b"\0"

    def matching():
        found = []
        for entry in os.listdir("/proc"):
            if not entry.isdigit() or int(entry) == os.getpid():
                continue
            try:
                environ = open(f"/proc/{entry}/environ", "rb").read()
            except OSError:
                continue
            if marker in environ:
                found.append(int(entry))
        return found

    pids = matching()
    for pid in pids:
        try:
            os.kill(pid, signal.SIGTERM)
        except OSError:
            pass
    deadline = time.time() + 3
    while pids and time.time() < deadline:
        time.sleep(0.2)
        pids = matching()
    for pid in pids:
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass


def main():
    global CHECKS_LOG
    (OUT / "scenarios").mkdir(exist_ok=True)
    (OUT / "frames").mkdir(exist_ok=True)
    (OUT / "logs").mkdir(exist_ok=True)
    CHECKS_LOG = open(OUT / "checks.jsonl", "w")
    ran = run_scenarios()
    sweep()
    if FAILED:
        for name, why in FAILED:
            log(f"FAIL: {name}: {why}")
        log(f"CHAT RICHER MESSAGES E2E FAILED ({len(FAILED)} of {ran} scenarios failed, {len(CHECKS)} checks passed)")
        sys.exit(1)
    if ONLY:
        log(f"CHAT RICHER MESSAGES E2E PARTIAL ({ran} of {len(SCENARIOS)} scenarios: {', '.join(ONLY)}; {len(CHECKS)} checks)")
    else:
        log(f"CHAT RICHER MESSAGES E2E OK ({len(CHECKS)} checks)")




def follow(world, until, timeout=90, every=0.04):
    """Reads the chat over and over until `until(read)` (a read takes about 170 ms with a long transcript); every
    read is returned with the time it was taken."""
    samples = []
    deadline = time.time() + timeout
    while True:
        r = world.read()
        r["t"] = time.time()
        samples.append(r)
        if until(r):
            return samples
        if time.time() > deadline:
            raise Fail(f"nothing happened within {timeout:.0f} s (last: {brief(r)})")
        time.sleep(every)


def ended_after_streaming():
    """A predicate for `follow`: true at the first read after a read that said `streaming`."""
    state = {"streamed": False}

    def predicate(r):
        if r["status"] == "streaming":
            state["streamed"] = True
            return False
        return state["streamed"]

    return predicate


def last_reply(r):
    found = assistant_rows(r)
    return found[-1] if found else (None, None)


def summarise(samples):
    """What each read said about the newest reply, compactly (the artifact keeps this)."""
    out = []
    for r in samples:
        _, row = last_reply(r)
        out.append({"t": round(r["t"], 3), "status": r["status"],
                    "live": row.get("live") if row else None,
                    "shown": int(row["shownBytes"]) if row and "shownBytes" in row else None,
                    "len": byte_len(row["text"]) if row else None,
                    "textLen": len(r["transcript_text"]),
                    "painted": sum(len(p["text"]) for p in r["painted"])})
    return out


def text_point(r):
    """A point on plain drawn text of the transcript's viewport: the start of the first painted heading of the showcase
    whose box is wholly inside the viewport (the menu's rows are at known offsets from a press on text that is not
    a link, and the entries of the table of contents are links)."""
    composer = r["composerRow"]
    top, bottom = composer["root"][1] + 40, composer["band"][1] - 60
    rows = [p for p in r["painted"] if p["text"] == "Markdown showcase"
            and top < float(p["y"]) and float(p["y"]) + float(p["h"]) < bottom]
    if not rows:
        raise Fail(f"no painted heading to press on (painted {[p['text'][:20] for p in r['painted']][:12]})")
    return int(float(rows[0]["x"]) + 12), int(float(rows[0]["y"]) + float(rows[0]["h"]) / 2)


def scroll(w, pixels):
    w.rpc("surface.chat.transcript", surfaceId=w.surface, action="scrollBy", pixels=str(pixels))


def copy_while_streaming(scn, w, number):
    """One attempt at Select All and Copy through the transcript menu while a reply streams.

    The reveal trails the received text by 0.35 s of arrivals at most, which a mouse and a clipboard cannot be timed
    against, so the gap is made to last: the transcript is scrolled to its top, where the live reply (the last
    row) is not drawn, and the reveal only moves when the reply is drawn. The text goes on arriving and `shownBytes`
    stays where it was. Select All (of what the document held when it was made) and Copy then go through the menu on
    the text of the first reply, and the clipboard must hold text of the live reply past that byte: a copy of the
    revealed build stops there. The text of the earlier replies is left out: their markers are the same ones. The
    attempt counts only if the gap was there at the Select All."""
    w.send("showcase slow")
    w.wait("the reply is part way", lambda r: last_reply(r)[1] is not None
           and last_reply(r)[1].get("live") == "true" and byte_len(last_reply(r)[1]["text"]) >= 6 * 1024,
           timeout=30, every=0.05)
    scroll(w, -10_000_000)
    r = w.wait("the top of the transcript is drawn", lambda r: any(p["text"] == "Markdown showcase" for p in r["painted"]),
               timeout=15, every=0.1)
    point = text_point(r)
    sentinel = f"sentinel before the copy {number}"
    owner = ClipboardOwner(sentinel)
    try:
        press(w.window, *point, button="3", hold=0.08, settle=0.05)
        time.sleep(0.3)
        press(w.window, point[0] + 40, point[1] + 46, hold=0.1, settle=0.05)  # Select All
        selected = w.read()  # the read that brackets the Select All
        time.sleep(0.3)
        press(w.window, *point, button="3", hold=0.08, settle=0.05)
        time.sleep(0.3)
        move(w.window, point[0] + 40, point[1] + 18)
        x("mousedown", "1")
        time.sleep(0.1)
        x("mouseup", "1")
        copied = clipboard_get()
        for _ in range(30):
            if copied != sentinel:
                break
            time.sleep(0.1)
            copied = clipboard_get()
        if number == 1:
            w.capture("stream-copied-scrolled-away")
    finally:
        owner.release()
        scroll(w, 10_000_000)  # the live reply is drawn again, and the reveal catches up
    index, row = last_reply(selected)
    plain = plain_by_row(selected)
    before = sum(len(plain[i]) + 2 for i in range(index))  # the transcript text ahead of the live reply
    reply_copied = copied[before:] if copied.startswith(selected["transcript_text"][:before]) else ""
    found = [(end, name) for end, name in markers(row["text"]) if name in reply_copied]
    top = max(found) if found else None
    shown, length = int(row["shownBytes"]), byte_len(row["text"])
    detail = {"attempt": number, "live": row.get("live"), "shownBytesAtSelectAll": shown, "textBytesAtSelectAll": length,
              "clipboardChars": len(copied), "replyCharsCopied": len(reply_copied),
              "newestMarkerCopied": top[1] if top else None, "itsSourceEnd": top[0] if top else None}
    scn.keep(f"copy attempt {number}", detail)
    log(f"copy attempt {number}: {detail}")
    return detail, copied, selected, plain[index], reply_copied, top


@scenario
def scenario_stream(scn):
    w = World(scn, "stream")
    try:
        w.start()
        w.open_chat("claude")
        # -- a 40 KiB reply in 200 chunks, read over and over (a read takes about 170 ms) while it streams
        w.send("showcase")
        samples = follow(w, ended_after_streaming())
        scn.keep("showcase samples", summarise(samples))
        final = samples[-1]
        _, reply = last_reply(final)
        text = reply["text"]
        scn.need(byte_len(text) >= 40 * 1024, "the reply is at least 40 KiB", f"it is {byte_len(text)} bytes")
        live = [(r, last_reply(r)[1]) for r in samples if last_reply(r)[1] and last_reply(r)[1].get("live") == "true"]
        scn.need(live, "the reply read as live while it streamed", f"{len(samples)} reads, none live")
        scn.need(all(r["status"] == "streaming" for r, _ in live), "a live reply is only ever read while the turn streams")
        behind = [(r, row) for r, row in live if 0 < int(row["shownBytes"]) < byte_len(row["text"])]
        scn.need(behind, "the reveal is behind the received text (0 < shownBytes < len(text)) while the reply is live",
                 f"{len(live)} live reads, none with the reveal part of the way: "
                 f"{[(row['shownBytes'], byte_len(row['text'])) for _, row in live][:8]}")
        shown = [int(row["shownBytes"]) for _, row in live]
        scn.need(all(a <= b for a, b in zip(shown, shown[1:])), "the revealed byte count only grows")
        scn.need(all(int(row["shownBytes"]) <= byte_len(row["text"]) for _, row in live),
                 "shownBytes never passes the received text")
        # `transcriptText` is the received text: the last marker received is in it, painted or not.
        with_marker = [(r, row, markers(row["text"])) for r, row in live if markers(row["text"])]
        scn.need(with_marker, "markers arrived while the reply was live")
        missing = [(m[-1][1], int(row["shownBytes"]), byte_len(row["text"]))
                   for r, row, m in with_marker if m[-1][1] not in r["transcript_text"]]
        scn.need(not missing, "transcriptText holds every received byte: the last marker received is in it at every read",
                 f"missing at {len(missing)} reads, first {missing[:3]}")
        unpainted = [(m[-1][1]) for r, row, m in with_marker
                     if m[-1][1] not in " ".join(p["text"] for p in r["painted"])]
        scn.need(unpainted, "the reveal really trails: the last marker received was in transcriptText and not yet painted at some read",
                 "every read had the last marker painted")
        # at the turn's end
        ended = [(r, last_reply(r)[1]) for r in samples if r["status"] != "streaming"]
        r0, row0 = ended[0]
        scn.need(row0.get("live") == "false" and int(row0["shownBytes"]) == byte_len(row0["text"]),
                 "at the turn's end live is false at once and shownBytes is the whole text",
                 f"live={row0.get('live')} shownBytes={row0.get('shownBytes')} len={byte_len(row0['text'])}")
        w.capture("stream-ended")
        # -- the same bytes in one chunk
        w.reply_file.write_text(text, encoding="utf-8")
        w.send("replyfile")
        after = w.wait("the one-chunk reply ends", lambda r: r["status"] == "completed" and len(assistant_rows(r)) == 2)
        (_, first), (_, second) = assistant_rows(after)
        scn.need(first["text"] == second["text"], "the streamed reply's source is the one-chunk reply's")
        plain = plain_by_row(after)
        index_first, index_second = assistant_rows(after)[0][0], assistant_rows(after)[1][0]
        scn.need(plain[index_first] == plain[index_second] and plain[index_first] != "",
                 "the streamed reply's plain text equals the one-chunk reply's",
                 f"{len(plain[index_first])} against {len(plain[index_second])} characters")
        # -- Select All and Copy through the transcript menu while a reply streams
        chosen = None
        for number in range(1, 4):
            attempt = copy_while_streaming(scn, w, number)
            detail, top = attempt[0], attempt[5]
            valid = (detail["live"] == "true" and detail["textBytesAtSelectAll"] - detail["shownBytesAtSelectAll"] >= 2000
                     and top is not None and attempt[1] != f"sentinel before the copy {number}")
            w.wait("the reply ends", lambda r: r["status"] != "streaming", timeout=40)
            if valid:
                chosen = attempt
                break
            log(f"copy attempt {number} did not land while the reveal was behind; trying again")
        scn.need(chosen, "a Select All and Copy was made while the reply streamed and the reveal was behind",
                 "none of three attempts had the reveal 2000 bytes behind the received text at the Select All with the live reply on the clipboard")
        detail, copied, selected, reply_plain, reply_copied, top = chosen
        scn.need(top[0] > detail["shownBytesAtSelectAll"],
                 "the clipboard holds received text the reveal had not reached: the live reply's newest marker copied is past shownBytes at the Select All",
                 f"the newest marker copied ({top[1]}) ends at byte {top[0]}, the reveal was at {detail['shownBytesAtSelectAll']} "
                 f"of {detail['textBytesAtSelectAll']}")
        scn.need(reply_copied.startswith("Markdown showcase") and len(reply_copied) > 5000,
                 "what the clipboard holds of the live reply is the reply from its first heading on, not a fragment",
                 f"{len(reply_copied)} characters copied, starting {reply_copied[:30]!r}")
        w.capture("stream-copied")
        # -- SHOWCASE eight times over: every copy cites [^1] again, so the reply cannot freeze and the tail
        # grows past the reveal's budget (7 KiB). Documented behaviour (spec 5.2): the reveal then stops for the
        # rest of the turn and the reply draws as it arrives, with no caret and no pacing.
        w.send("showcase x8")
        samples = follow(w, ended_after_streaming())
        scn.keep("showcase x8 samples", summarise(samples))
        live8 = [(r, last_reply(r)[1]) for r in samples if last_reply(r)[1] and last_reply(r)[1].get("live") == "true"]
        scn.need(live8, "the x8 reply read as live while it streamed")
        scn.need(any(int(row["shownBytes"]) < byte_len(row["text"]) for _, row in live8),
                 "the x8 reply began with a reveal that trailed the received text")
        late = [(r, row) for r, row in live8 if byte_len(row["text"]) >= 20 * 1024]
        scn.need(late and all(int(row["shownBytes"]) == byte_len(row["text"]) for _, row in late),
                 "past its tail budget the x8 reply draws as it arrives: shownBytes is the whole text at every read from 20 KiB on",
                 f"{[(row['shownBytes'], byte_len(row['text'])) for _, row in late][:6]}")
        end8 = [(r, last_reply(r)[1]) for r in samples if r["status"] != "streaming"][0]
        scn.need(end8[1].get("live") == "false" and int(end8[1]["shownBytes"]) == byte_len(end8[1]["text"]),
                 "the x8 reply ends with live false and shownBytes the whole text")
        text8 = end8[1]["text"]
        w.reply_file.write_text(text8, encoding="utf-8")
        w.send("replyfile")
        after8 = w.wait("the one-chunk x8 reply ends", lambda r: r["status"] == "completed" and len(assistant_rows(r)) >= 5)
        replies = assistant_rows(after8)
        plain8 = plain_by_row(after8)
        (i_streamed, streamed8), (i_whole, whole8) = replies[-2], replies[-1]
        scn.need(streamed8["text"] == whole8["text"] and plain8[i_streamed] == plain8[i_whole] and plain8[i_whole],
                 "the x8 reply's source and plain text equal the one-chunk reply's",
                 f"{len(plain8[i_streamed])} against {len(plain8[i_whole])} characters")
    finally:
        teardown(w)


def alive(pid):
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def kill_stub(pid):
    """SIGKILL of an agent process this run started (its environment carries the run's marker)."""
    environ = Path(f"/proc/{pid}/environ").read_bytes()
    if MARKER.encode() + b"\0" not in environ:
        raise Fail(f"process {pid} is not one this run started")
    os.kill(pid, signal.SIGKILL)


@scenario
def scenario_stop(scn):
    w = World(scn, "stop")
    try:
        w.start()
        w.open_chat("claude")
        w.send("showcase")
        w.wait("the reveal is part way through a long reply",
               lambda r: last_reply(r)[1] is not None and last_reply(r)[1].get("live") == "true"
               and 0 < int(last_reply(r)[1]["shownBytes"]) < byte_len(last_reply(r)[1]["text"]) - 1000
               and byte_len(last_reply(r)[1]["text"]) > 10 * 1024, timeout=30, every=0.03)
        mid = w.read()
        _, row = last_reply(mid)
        scn.keep("read at the Stop", {"shownBytes": row["shownBytes"], "textBytes": byte_len(row["text"])})
        w.capture("stop-mid-reveal")
        w.stop_turn()
        samples = follow(w, lambda r: r["status"] != "streaming", timeout=30, every=0.03)
        scn.keep("reads after the Stop", summarise(samples))
        final = samples[-1]
        _, row = last_reply(final)
        text = row["text"]
        scn.need(row.get("live") == "false" and int(row["shownBytes"]) == byte_len(text),
                 "after Stop the rest shows at once: live is false and shownBytes is the whole received text",
                 f"live={row.get('live')} shownBytes={row.get('shownBytes')} len={byte_len(text)}")
        scn.need(0 < byte_len(text) < 40 * 1024, "the reply was cut by Stop (it is shorter than the whole reply)", f"{byte_len(text)} bytes")
        scn.need(error_row(final) is None, "a Stop is not a failure: no error row")
        scn.need(not final["is_held"], "a Stop holds no queue")
        w.capture("stop-ended")
        # Stop clears the prompt to resend: the agent dying now has nothing to resume or resend.
        launches = len(w.sessions("claude"))
        kill_stub(w.sessions("claude")[-1])
        died = w.wait("the lost agent is an error row", lambda r: error_row(r) is not None, timeout=30)
        index, error = error_row(died)
        scn.need(error.get("retry") == "relaunch",
                 "a client lost after Stop has no prompt to resend: Retry only relaunches (retry: relaunch)",
                 f"errorKind={error.get('errorKind')} retry={error.get('retry')}")
        w.retry(index)
        w.wait("the relaunched agent has connected", lambda r: r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        scn.need(len(w.sessions("claude")) == launches + 1 and len(w.prompts()) == 1, "Retry relaunched the agent and sent nothing",
                 f"launches {w.sessions('claude')}, prompts {w.prompts()}")
        # the same bytes in one chunk
        w.reply_file.write_text(text, encoding="utf-8")
        w.send("replyfile")
        after = w.wait("the one-chunk reply ends", lambda r: r["status"] == "completed" and len(assistant_rows(r)) == 2)
        plain = plain_by_row(after)
        (i_stopped, stopped), (i_whole, whole) = assistant_rows(after)
        scn.need(stopped["text"] == whole["text"] and plain[i_stopped] == plain[i_whole] and plain[i_whole],
                 "the stopped reply's final text equals the same bytes delivered in one chunk",
                 f"{len(plain[i_stopped])} against {len(plain[i_whole])} characters")
    finally:
        teardown(w)


PARTIAL = ("I started on it. Here is **what I found** so far, with `code` and a list:\n\n"
           "- the first point, written out\n- the second point\n\nThe next step is")
HELD_LINE = "Waiting: press Retry or send a message."
HELD_LINE_NO_RETRY = "Waiting: send a message."
QUEUE_ROW = 33.0  # the height of a queue entry's row at this window size, measured on a capture
QUEUE_HELD_LINE = 30.0  # and of the held line under the entries


def queue_remove_point(r, index, count, held):
    """Where the remove button of queue entry `index` is drawn. The queue is a card as wide as the reading column
    (760 px, centred in the chat, 30 px in from each side), 8 px above the composer's top edge (`composerRow.edge`),
    with its held line, if any, under the entries; the button is 23 px in from the card's right edge. The press is
    checked by what it does: the entry goes from the queue, and Send now (its neighbour) would send it."""
    composer = r["composerRow"]
    root, edge = composer["root"], composer["edge"]
    right = root[0] + root[2] - 30
    bottom = edge[1] - 8
    first = bottom - (QUEUE_HELD_LINE if held else 0) - count * QUEUE_ROW
    return int(right - 23), int(first + index * QUEUE_ROW + QUEUE_ROW / 2)


def one_chunk_plain(scn, w, source, what):
    """The plain text the transcript holds for a reply whose source is `source` delivered in one chunk, and the
    plain text of the reply the scenario already has with that source: equal, or the scenario fails."""
    w.reply_file.write_text(source, encoding="utf-8")
    w.send("replyfile")
    after = w.wait("the one-chunk reply ends", lambda r: r["status"] == "completed" and rows_of(r, "assistant")
                   and last_reply(r)[1]["text"] == source and len([1 for _, row in assistant_rows(r) if row["text"] == source]) >= 2)
    plain = plain_by_row(after)
    same = [index for index, row in assistant_rows(after) if row["text"] == source]
    scn.need(len(same) >= 2 and plain[same[0]] == plain[same[-1]] and plain[same[-1]],
             what, f"{[len(plain[i]) for i in same]} characters")
    return after


@scenario
def scenario_claude_turn_failed(scn):
    w = World(scn, "failed")
    q = World(scn, "queue")
    try:
        # ---- an is_error result is an error row whose Retry resends
        w.start()
        w.open_chat("claude")
        began = time.time()
        w.send("fail")
        r = w.wait("the failed turn ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(row.get("errorKind") == "turnFailed" and row.get("retry") == "resend",
                 "a failed Claude turn is an error row with errorKind turnFailed and retry: resend",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}")
        scn.need(row["text"] == "API Error: 529 overloaded" and not any("529" in a["text"] for _, a in assistant_rows(r)),
                 "the failure text is the error row's, not a reply")
        (_, partial), = assistant_rows(r)
        scn.need(partial.get("live") == "false" and int(partial["shownBytes"]) == byte_len(partial["text"]) == byte_len(PARTIAL),
                 "the partial reply of the failed turn is ended at once: live false, shownBytes the whole text",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')}")
        users = rows_of(r, "user")
        at = int(users[0][1]["at"])
        scn.need(began - 5 <= at <= time.time() + 5, "the user row carries `at`, the Unix second the turn was asked", f"at={at}, now={int(time.time())}")
        w.capture("failed-error-row")
        pid = w.sessions("claude")[-1]
        w.retry(index)
        r2 = w.wait("the resent turn is answered", lambda r: r["status"] == "completed" and error_row(r) is None
                    and any(a["text"] == "recovered" for _, a in assistant_rows(r)))
        scn.need(error_row(r2) is None, "Retry removed the error row")
        scn.need(user_texts(r2) == ["fail", "fail"], "Retry pushed the same text as a new user row", f"{user_texts(r2)}")
        new_at = int(rows_of(r2, "user")[1][1]["at"])
        scn.need(new_at >= at and new_at <= time.time() + 5, "the resent user row has its own `at`", f"{at} then {new_at}")
        scn.need(w.prompts() == [(pid, "fail"), (pid, "fail")] and w.sessions("claude")[-1] == pid and alive(pid),
                 "the stub heard the same text twice on the same process (same pid), still running",
                 f"prompts {w.prompts()}, launches {w.sessions('claude')}")
        # the partial reply equals the same bytes in one chunk
        one_chunk_plain(scn, w, PARTIAL, "the failed turn's partial reply equals the same bytes delivered in one chunk")
        # error_max_turns keeps today's reply: no error row
        w.send("fail max turns")
        r3 = w.wait("the turn-limit turn ends", lambda r: r["status"] == "completed"
                    and any("Reached the turn limit" in a["text"] for _, a in assistant_rows(r)))
        scn.need(error_row(r3) is None and not r3["is_held"],
                 "error_max_turns keeps its reply text and footer: no error row, nothing held")
        w.capture("failed-max-turns")

        # ---- the queue variant
        w.keep_artifacts()
        w.quit()
        q.start()
        q.open_chat("claude")
        q.send("fail hold")
        q.wait("the failing turn streams its partial reply", lambda r: r["status"] == "streaming" and last_reply(r)[1] is not None)
        q.send("q1", queued=True)
        q.send("q2", queued=True)
        q.wait("two messages are queued behind the failing turn", lambda r: r["queue"] == ["q1", "q2"])
        pid = q.sessions("claude")[-1]
        q.release_gate(1)
        r = q.wait("the failed turn ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(r["is_held"] and r["held_line"] == HELD_LINE and r["queue"] == ["q1", "q2"],
                 "after the failure the queue is held and says so (held, heldLine, both items still queued)",
                 f"held={r['is_held']} line={r['held_line']!r} queue={r['queue']}")
        time.sleep(1.5)
        r = q.read()
        scn.need(q.prompts() == [(pid, "fail hold")] and r["queue"] == ["q1", "q2"],
                 "nothing more was sent after the failure: the stub heard only the failed prompt",
                 f"prompts {q.prompts()}")
        q.capture("queue-held")
        x_at = queue_remove_point(r, 0, 2, True)
        press(q.window, *x_at, hold=0.15)
        r = q.wait("the removed item is gone", lambda r: r["queue"] == ["q2"], timeout=10)
        scn.need(r["is_held"] and r["held_line"] == HELD_LINE and q.prompts() == [(pid, "fail hold")],
                 "a held item removed with its X is gone from the queue and the rest stays held, nothing sent",
                 f"queue={r['queue']} held={r['is_held']} prompts {q.prompts()}")
        q.retry(index)
        r = q.read()
        scn.need(not r["is_held"] and r["held_line"] == "" and r["queue"] == ["q2"],
                 "Retry lifts the hold at the send: held is false, the line is gone, the other item is still queued",
                 f"held={r['is_held']} line={r['held_line']!r} queue={r['queue']}")
        q.wait("the resent turn is running", lambda r: r["status"] == "streaming", timeout=10)
        time.sleep(1.0)
        scn.need(q.prompts() == [(pid, "fail hold"), (pid, "fail hold")],
                 "the remaining item waits for the retried turn's end: the stub heard only the resent prompt so far",
                 f"prompts {q.prompts()}")
        q.release_gate(2)
        r = q.wait("the retried turn ended and the remaining item was sent and answered",
                   lambda r: r["status"] == "completed" and not r["queue"] and "q2" in user_texts(r)
                   and sum(1 for _, a in assistant_rows(r) if a["text"] == "ok") >= 1, timeout=40)
        scn.need(q.prompts() == [(pid, "fail hold"), (pid, "fail hold"), (pid, "q2")] and not r["is_held"],
                 "the remaining item was sent at the retried turn's end, and q1 never was; held is false",
                 f"prompts {q.prompts()}")
        q.capture("queue-drained")
    finally:
        teardown([w, q])


RESTARTED_BACK = "The agent restarted without this conversation. Your message is back in the composer."
RESTARTED = "The agent restarted without this conversation."


def row_with_text(r, text):
    return next(((i, row) for i, row in enumerate(r["rows"]) if row.get("text") == text), None)


@scenario
def scenario_acp_prompt_failed(scn):
    w = World(scn, "acp")
    try:
        w.start()
        w.open_chat("opencode")
        pid = w.sessions("opencode")[-1]
        w.send("fail prompt")
        r = w.wait("the failed prompt ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(row.get("errorKind") == "turnFailed" and row.get("retry") == "resend",
                 "a failed ACP prompt is an error row with errorKind turnFailed and retry: resend",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}")
        scn.need(alive(pid), "the agent process is still alive after the failed prompt")
        w.capture("acp-failed")
        w.retry(index)
        r = w.wait("the resent prompt is answered", lambda r: r["status"] == "completed" and error_row(r) is None
                   and any(a["text"] == "recovered" for _, a in assistant_rows(r)))
        scn.need(user_texts(r) == ["fail prompt", "fail prompt"], "Retry sent the same text again as a new user row", f"{user_texts(r)}")
        prompts = [value for kind, p, value in w.records("prompt")]
        sessions = [value for kind, p, value in w.records("session_new") if p == pid]
        scn.need(w.prompts() == [(pid, "fail prompt"), (pid, "fail prompt")] and w.sessions("opencode")[-1] == pid and alive(pid),
                 "the agent heard the same text twice on the same process, which was never relaunched",
                 f"prompts {w.prompts()}, launches {w.sessions('opencode')}")
        scn.need(len(sessions) == 1 and prompts[0]["sessionId"] and prompts[0]["sessionId"] == prompts[1]["sessionId"],
                 "both prompts went to the same session (one session/new, one session id)", f"{prompts}, session/new {sessions}")
        # a failure that follows Stop is the late end of the stopped turn: no error row, no hold
        w.send("cancel fail")
        w.wait("the partial reply streams", lambda r: r["status"] == "streaming" and any(a["text"].startswith("I started") for _, a in assistant_rows(r)))
        w.stop_turn()
        r = w.wait("the stopped turn ends", lambda r: r["status"] != "streaming", timeout=30)
        time.sleep(1.5)
        r = w.read()
        scn.need(error_row(r) is None and not r["is_held"] and r["status"] != "streaming",
                 "the error that follows Stop is the late end of the stopped turn: no error row, nothing held",
                 brief(r))
        w.send("after the stop")
        w.wait("the chat still works", lambda r: r["status"] == "completed" and any(a["text"] == "reply ok" for _, a in assistant_rows(r)))
        scn.ok("the chat answers the next prompt after the stopped turn")
    finally:
        teardown(w)


@scenario
def scenario_claude_lost_resume(scn):
    w = World(scn, "lost")
    try:
        w.start()
        w.open_chat("claude")
        first = w.sessions("claude")[-1]
        w.send("die")
        r = w.wait("the lost client ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(row.get("errorKind") in ("connection", "disconnected") and row.get("retry") == "resume",
                 "a Claude client lost mid-turn is an error row whose Retry resumes (retry: resume)",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}")
        (_, partial), = assistant_rows(r)
        scn.need(partial.get("live") == "false" and int(partial["shownBytes"]) == byte_len(partial["text"]) == byte_len(PARTIAL),
                 "the partial reply of the lost client is ended at once: live false, shownBytes the whole text",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')}")
        scn.need(not alive(first), "the agent process is gone")
        w.capture("lost-error-row")
        w.retry(index)
        r = w.wait("the resumed client answers the resent prompt", lambda r: r["status"] == "completed" and error_row(r) is None
                   and any(a["text"] == "recovered" for _, a in assistant_rows(r)), timeout=40)
        launches = w.sessions("claude")
        second = launches[-1]
        args = {pid: a for kind, pid, a in w.records("claude")}
        scn.need(second != first and "--resume fixture-session-1" in args[second] and "--resume" not in args[first],
                 "Retry relaunched with --resume of the agent's session id (the stub recorded it); the first launch had none",
                 f"first {args[first][-60:]!r}, second {args[second][-80:]!r}")
        scn.need(w.prompts() == [(first, "die"), (second, "die")],
                 "the prompt was resent to the relaunched agent", f"{w.prompts()}")
        scn.need(user_texts(r) == ["die", "die"], "the resent prompt is a new user row", f"{user_texts(r)}")
        one_chunk_plain(scn, w, PARTIAL, "the partial reply of the lost client equals the same bytes delivered in one chunk")
    finally:
        teardown(w)


@scenario
def scenario_acp_lost_composer(scn):
    a = World(scn, "mid")
    b = World(scn, "idle")
    c = World(scn, "draft")
    try:
        # ---- the agent exits in the middle of a turn
        a.start()
        a.open_chat("opencode")
        a.send("die")
        r = a.wait("the lost agent ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(row.get("errorKind") in ("connection", "disconnected") and row.get("retry") == "composer",
                 "an ACP agent lost mid-turn is an error row whose Retry puts the message back (retry: composer)",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}")
        (_, partial), = assistant_rows(r)
        scn.need(partial.get("live") == "false" and int(partial["shownBytes"]) == byte_len(partial["text"]),
                 "the partial reply of the lost agent is ended at once: live false, shownBytes the whole text")
        launches, prompts = len(a.sessions("opencode")), a.prompts()
        a.capture("acp-lost")
        a.retry(index)
        r = a.wait("the relaunched agent is connected with the message back", lambda r: r["composerText"] == "die"
                   and r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        scn.need(row_with_text(r, RESTARTED_BACK) is not None,
                 "the notice says the agent restarted without the conversation and the message is back",
                 brief(r))
        scn.need(len(a.sessions("opencode")) == launches + 1 and a.prompts() == prompts,
                 "Retry relaunched the agent and sent nothing", f"launches {a.sessions('opencode')}, prompts {a.prompts()}")
        a.capture("acp-composer-back")
        a.keep_artifacts()
        a.quit()

        # ---- no turn in flight: the launch fails, then works. (An ACP agent that exits while idle is not noticed
        # until the next send, so it cannot show a row of its own; Claude's idle death is the `stop` scenario's.)
        b.refuse_file.write_text("refuse")
        b.start()
        b.open_chat("opencode", connected=False)
        r = b.wait("the failed launch is an error row", lambda r: error_row(r) is not None, timeout=40)
        index, row = error_row(r)
        scn.need(row.get("retry") == "relaunch",
                 "a failed launch has no prompt to put back: Retry only relaunches (retry: relaunch)",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}")
        b.refuse_file.unlink()
        launches = len(b.sessions("opencode"))
        b.retry(index)
        r = b.wait("the relaunched agent is connected", lambda r: r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        scn.need(r["composerText"] == "" and row_with_text(r, RESTARTED_BACK) is None and row_with_text(r, RESTARTED) is None
                 and len(b.sessions("opencode")) == launches + 1 and b.prompts() == [],
                 "Retry relaunched: no message back, no notice, nothing sent",
                 brief(r))
        b.keep_artifacts()
        b.quit()

        # ---- the composer holds a draft
        c.start()
        c.open_chat("opencode")
        c.send("die")
        r = c.wait("the lost agent ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming")
        index, row = error_row(r)
        scn.need(row.get("retry") == "composer", "the row says retry: composer", f"retry={row.get('retry')}")
        c.compose("my own draft")
        c.wait("the draft is in the composer", lambda r: r["composerText"] == "my own draft")
        prompts = c.prompts()
        c.retry(index)
        r = c.wait("the relaunched agent is connected", lambda r: r["status"] in ("idle", "completed") and error_row(r) is None
                   and row_with_text(r, RESTARTED) is not None, timeout=40)
        scn.need(r["composerText"] == "my own draft" and row_with_text(r, RESTARTED_BACK) is None and c.prompts() == prompts,
                 "a draft already in the composer stays, the notice does not say the message is back, nothing is sent",
                 brief(r))
        c.capture("acp-draft-kept")
    finally:
        teardown([a, b, c])


def card_row(r):
    """The (index, row) of the limit card, or None."""
    found = rows_of(r, "limitCard")
    return found[-1] if found else None


def card_phase(r):
    found = card_row(r)
    return found[1].get("phase") if found else None


def set_limit(w, seconds):
    """Writes the reset the stub's next rate-limit event will carry: `seconds` from now, in Unix seconds."""
    epoch = int(time.time()) + seconds
    w.limit_file.write_text(str(epoch))
    return epoch


@scenario
def scenario_limit(scn):
    a = World(scn, "failed")
    b = World(scn, "lifted")
    c = World(scn, "retired")
    try:
        # ---- a rejected limit and a failed turn, with a queue held behind it
        a.start()
        a.open_chat("claude")
        pid = a.sessions("claude")[-1]
        a.send("limit rejected hold")
        a.wait("the turn streams", lambda r: r["status"] == "streaming")
        a.send("lq", queued=True)
        a.wait("one message is queued", lambda r: r["queue"] == ["lq"])
        epoch = set_limit(a, 14)
        a.release_gate(1)
        r = a.wait("the limit card appears", lambda r: card_row(r) is not None and error_row(r) is not None and r["status"] != "streaming")
        card_index, card = card_row(r)
        error_index, error = error_row(r)
        scn.need(card.get("phase") == "counting" and card.get("resetsAt") == str(epoch),
                 "a rejected rate limit with a reset ahead is a limitCard row counting, with resetsAt in Unix seconds",
                 f"phase={card.get('phase')} resetsAt={card.get('resetsAt')} (the reset is {epoch})")
        scn.need("usage limit" in card["text"].lower() and "resets" in card["text"],
                 "the card's text is the limit sentence (window and reset time)", f"{card['text']!r}")
        scn.need(error.get("errorKind") == "turnFailed" and error.get("coveredBy") == "limitCard" and error.get("retry") == "none",
                 "the failed turn's error row is covered by the card while it counts and has no Retry (coveredBy limitCard, retry none)",
                 f"errorKind={error.get('errorKind')} coveredBy={error.get('coveredBy')} retry={error.get('retry')}")
        on_card, on_row = a.retry_refusal(card_index), a.retry_refusal(error_index)
        scn.need(on_card and on_row and "not lifted" in on_card and "not lifted" in on_row,
                 "the socket refuses Retry on the card and on the covered row while the limit counts",
                 f"card: {on_card!r}, row: {on_row!r}")
        r = a.read()
        scn.need(r["is_held"] and r["held_line"] == HELD_LINE_NO_RETRY and r["queue"] == ["lq"] and a.prompts() == [(pid, "limit rejected hold")],
                 "a held queue says \"Waiting: send a message.\" while the card counts (no Retry to press), and nothing was sent",
                 f"held={r['is_held']} line={r['held_line']!r} prompts {a.prompts()}")
        a.capture("limit-counting")
        r = a.wait("the card offers Try again once the reset has passed", lambda r: card_phase(r) == "retry", timeout=25, every=0.3)
        card_index, card = card_row(r)
        error_index, error = error_row(r)
        scn.need(time.time() >= epoch - 1 and error.get("coveredBy") == "limitCard" and error.get("retry") == "resend"
                 and r["held_line"] == HELD_LINE,
                 "after the reset the card reads retry, its row's Retry is back (retry resend) and the held line names Retry again",
                 f"coveredBy={error.get('coveredBy')} retry={error.get('retry')} line={r['held_line']!r}")
        a.capture("limit-try-again")
        a.retry(card_index)
        r = a.read()
        scn.need(card_phase(r) == "lifted" and error_row(r) is None and not r["is_held"] and r["queue"] == ["lq"]
                 and user_texts(r) == ["limit rejected hold", "limit rejected hold"],
                 "Try again resent the prompt (the error row is gone, the hold lifted at the send) and the card now reads lifted",
                 brief(r))
        card_index, _ = card_row(r)
        refusal = a.retry_refusal(card_index)
        scn.need(refusal and "no Retry" in refusal, "Retry is refused on a lifted card", f"{refusal!r}")
        a.release_gate(2)
        r = a.wait("the resent turn is answered and the queued message after it", lambda r: r["status"] == "completed"
                   and "lq" in user_texts(r) and not r["queue"] and sum(1 for _, row in assistant_rows(r) if row["text"] == "ok") >= 1, timeout=40)
        scn.need(a.prompts() == [(pid, "limit rejected hold"), (pid, "limit rejected hold"), (pid, "lq")]
                 and any(row["text"] == "recovered" for _, row in assistant_rows(r)),
                 "the resent prompt reached the same process and was answered, then the queued message was sent",
                 f"prompts {a.prompts()}")
        a.capture("limit-lifted")
        a.keep_artifacts()
        a.quit()

        # ---- a rejection in a turn that did not fail: no failed row, the card ends lifted
        b.start()
        b.open_chat("claude")
        epoch = set_limit(b, 12)
        b.send("limit event")
        r = b.wait("the limit card appears", lambda r: card_row(r) is not None)
        card_index, card = card_row(r)
        scn.need(card.get("phase") == "counting" and card.get("resetsAt") == str(epoch) and error_row(r) is None,
                 "a rejected limit in a turn that did not fail is a limitCard row counting, with no error row",
                 f"phase={card.get('phase')} resetsAt={card.get('resetsAt')}")
        refusal = b.retry_refusal(card_index)
        scn.need(refusal and "not lifted" in refusal, "the socket refuses Retry on the counting card", f"{refusal!r}")
        r = b.wait("the card reads lifted once the reset has passed", lambda r: card_phase(r) == "lifted", timeout=25, every=0.3)
        refusal = b.retry_refusal(card_row(r)[0])
        scn.need(refusal and "no Retry" in refusal and card_phase(r) == "lifted",
                 "with no failed turn the card ends lifted, and Retry is refused after it",
                 f"{refusal!r}")
        b.capture("limit-lifted-no-failure")
        b.keep_artifacts()
        b.quit()

        # ---- a prompt sent while the card counts retires the row's Retry
        c.start()
        c.open_chat("claude")
        epoch = set_limit(c, 14)
        c.send("limit rejected")
        r = c.wait("the limit card covers the failed row", lambda r: card_row(r) is not None and error_row(r) is not None
                   and error_row(r)[1].get("coveredBy") == "limitCard" and r["status"] != "streaming")
        c.send("ok")
        r = c.wait("the next turn is answered", lambda r: r["status"] == "completed" and any(row["text"] == "ok" for _, row in assistant_rows(r)))
        error_index, error = error_row(r)
        scn.need("coveredBy" not in error and error.get("retry") == "none",
                 "sending a prompt retired the failed row's Retry: the card no longer covers it and it keeps OK only",
                 f"coveredBy={error.get('coveredBy')} retry={error.get('retry')}")
        c.capture("limit-retired")
        r = c.wait("the card reads lifted, not retry, once the reset has passed", lambda r: card_phase(r) == "lifted", timeout=25, every=0.3)
        scn.need(card_phase(r) == "lifted" and c.retry_refusal(error_index) is not None,
                 "the card whose Retry was retired ends lifted, and the row's Retry is still refused")
    finally:
        teardown([a, b, c])


@scenario
def scenario_day_separators(scn):
    w = World(scn, "days")
    try:
        w.start()
        w.open_chat("claude")
        surface = w.surface
        w.keep_artifacts()
        w.quit()
        # A transcript with user entries on three days (and one with no date), written as the app stores it.
        now = datetime.now(timezone.utc)
        old = datetime(now.year - 1, 3, 12, 12, 0, tzinfo=timezone.utc)
        yesterday = (now - timedelta(days=1)).replace(hour=12, minute=0, second=0, microsecond=0)
        turns = [("a question from last year", int(old.timestamp())), ("a question with no date", None),
                 ("yesterday's question", int(yesterday.timestamp()))]
        db = sqlite3.connect(w.dir / "session.sqlite")
        try:
            tab = db.execute("SELECT id FROM tab WHERE id = ?", (surface,)).fetchone()
            if tab is None:
                raise Fail(f"the chat tab {surface} is not in the database: {db.execute('SELECT id, kind FROM tab').fetchall()}")
            for ordinal, (text, at) in enumerate(turns):
                user = {"text": text} if at is None else {"text": text, "at": at}
                payload = json.dumps({"entries": [{"UserMessage": user}, {"AssistantMessage": {"text": f"the answer to {text}"}},
                                                  {"TurnFooter": {"text": "12:00"}}]})
                db.execute("INSERT INTO chat_turn (tab_id, ordinal, payload, updated_at) VALUES (?, ?, ?, ?)",
                           (surface, ordinal, payload.encode(), int(time.time() * 1000)))
            db.commit()
        finally:
            db.close()
        w.start()
        w.reopen_chat()
        r = w.read()
        scn.keep("the restored transcript", {"rows": r["rows"], "separators": r["separators"]})
        scn.need(user_texts(r) == [text for text, _ in turns], "the restored transcript holds the three turns", f"{user_texts(r)}")
        users = rows_of(r, "user")
        scn.need(users[0][1].get("at") == str(turns[0][1]) and "at" not in users[1][1] and users[2][1].get("at") == str(turns[2][1]),
                 "a restored user row has `at`, the Unix second it was asked; an undated turn has no `at` key (absent, not null)",
                 f"{[row.get('at') for _, row in users]}")
        want = [{"kind": "daySeparator", "entry": str(users[0][0]), "label": old.strftime("%B %-d, %Y")},
                {"kind": "daySeparator", "entry": str(users[2][0]), "label": "Yesterday"}]
        scn.need(r["separators"] == want,
                 "a separator sits above the first dated turn (a full date) and above yesterday's: undated turns get none",
                 f"{r['separators']} against {want}")
        w.send("today's question")
        r = w.wait("today's turn is answered", lambda r: r["status"] == "completed" and "today's question" in user_texts(r))
        users = rows_of(r, "user")
        at = int(users[-1][1]["at"])
        want.append({"kind": "daySeparator", "entry": str(users[-1][0]), "label": "Today"})
        scn.need(abs(at - time.time()) < 120 and r["separators"] == want,
                 "a new turn has `at` now and a separator labelled Today above it; the three labels are a full date, Yesterday and Today",
                 f"at={at}, separators {r['separators']}")
        w.capture("day-separators")
    finally:
        teardown(w)


@scenario
def scenario_thought_end(scn):
    w = World(scn, "thought")
    try:
        w.start()
        w.open_chat("claude")
        w.send("think slow")
        samples = follow(w, ended_after_streaming(), every=0.05)
        scn.keep("thought samples", [{"t": round(r["t"], 3), "thoughts": [t.get("durationMs") for _, t in rows_of(r, "thought")],
                                      "assistant": len(assistant_rows(r))} for r in samples])
        unsettled = [r for r in samples if rows_of(r, "thought") and "durationMs" not in rows_of(r, "thought")[0][1]]
        scn.need(unsettled, "a thought that is still open has no durationMs key (absent, not null)", "no read saw the open thought")
        settled = [r for r in samples if rows_of(r, "thought") and "durationMs" in rows_of(r, "thought")[0][1]]
        scn.need(settled, "the thought settles")
        first = settled[0]
        scn.need(not assistant_rows(first), "the thought settled when its block closed, before any reply text had arrived",
                 f"{len(assistant_rows(first))} reply rows at that read")
        duration = int(rows_of(first, "thought")[0][1]["durationMs"])
        scn.need(300 <= duration < 1000,
                 "its durationMs is the thinking block's (under 1 s), not the 2 s wait for the text",
                 f"{duration} ms")
        last = samples[-1]
        scn.need(int(rows_of(last, "thought")[0][1]["durationMs"]) == duration and any(a["text"] == "done thinking" for _, a in assistant_rows(last)),
                 "after the reply arrived and the turn ended the thought's durationMs is the same",
                 f"{rows_of(last, 'thought')[0][1].get('durationMs')} ms")
        w.capture("thought-settled")
    finally:
        teardown(w)


if __name__ == "__main__":
    main()
