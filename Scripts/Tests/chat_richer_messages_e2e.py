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


DETAIL_KEYS = ("id", "status", "errorKind", "retry", "coveredBy", "phase", "resetsAt", "live", "shownBytes", "at",
               "durationMs")


def compact(r):
    """A readout of the chat in a form the artifact keeps: the status, the queue and its hold, the composer, and one
    entry per transcript row with its kind, the start of its text and the debug columns it carries."""
    if not isinstance(r, dict) or "rows" not in r:
        return r
    rows = []
    for index, row in enumerate(r["rows"]):
        entry = {"i": index, "kind": row.get("kind")}
        text = row.get("text")
        if text is not None:
            entry["text"] = text if len(text) <= 80 else f"{text[:80]}... ({len(text.encode())} bytes)"
        entry.update({key: row[key] for key in DETAIL_KEYS if key in row})
        rows.append(entry)
    out = {"status": r.get("status"), "queue": r.get("queue"), "held": r.get("held"), "heldLine": r.get("held_line"),
           "composer": r.get("composerText"), "rows": rows, "transcriptTextChars": len(r.get("transcript_text", ""))}
    if r.get("separators"):
        out["daySeparators"] = r["separators"]
    return out


class Scenario:
    """One scenario's checks. `need` records a passed check or raises Fail with the scenario's name. A check keeps,
    in checks.jsonl and in scenarios/<name>.json, the readout it judged (`read`: the one passed as `read=`, else the
    newest read of the scenario, which is the one a check made right after a wait or a read judges), the values it
    compared (`judged`, the text `why` carries) and any detail it passes (`detail`), so the artifact can be re-checked."""

    def __init__(self, name):
        self.name = name
        self.readouts = []  # what the scenario read, kept in scenarios/<name>.json
        self.last_read = None  # the newest `World.read()` of this scenario

    def record(self, detail, read, judged):
        out = {"read": compact(read if read is not None else self.last_read)}
        if judged:
            out["judged"] = judged
        if detail is not None:
            out["detail"] = detail
        return out

    def ok(self, what, detail=None, read=None, judged=None):
        CHECKS.append((self.name, what))
        log(f"OK: {self.name}: {what}")
        recorded = self.record(detail, read, judged)
        self.readouts.append({"label": f"check: {what}", "value": recorded})
        CHECKS_LOG.write(json.dumps({"scenario": self.name, "check": what, "status": "OK", **recorded}) + "\n")
        CHECKS_LOG.flush()

    def need(self, condition, what, why="", detail=None, read=None):
        if condition:
            self.ok(what, detail, read, why)
        else:
            self.readouts.append({"label": f"FAILED: {what}", "value": self.record(detail, read, why), "why": why})
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
        self.scn.last_read = r
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




def follow(world, until, timeout=90, every=0.04, keep=None):
    """Reads the chat over and over until `until(read)` (a read takes about 170 ms with a long transcript); every
    read is returned with the time it was taken (`t`) and how long it took (`latency`). A read the app does not answer
    within its control socket's 5 s dispatch bound is a failure that says so: the UI thread was too busy to be read.
    With `keep` (a label), the scenario's artifact keeps `summarise` of the reads taken, with their latencies, however
    the following ends: the reads that came before a failure are what tells why it failed."""
    samples = []
    deadline = time.time() + timeout
    try:
        while True:
            started = time.time()
            try:
                r = world.read()
            except RpcError as error:
                if "dispatch bound" not in str(error):
                    raise
                slowest = max((x["latency"] for x in samples), default=0.0)
                raise Fail(f"the app did not answer a read within the control socket's 5 s dispatch bound ({error}); the UI thread was "
                           f"too busy to be read, after {len(samples)} reads, the slowest of which took {slowest:.2f} s")
            r["t"] = time.time()
            r["latency"] = r["t"] - started
            samples.append(r)
            if until(r):
                return samples
            if time.time() > deadline:
                raise Fail(f"nothing happened within {timeout:.0f} s (last: {brief(r)})")
            time.sleep(every)
    finally:
        if keep:
            world.scn.keep(keep, summarise(samples))


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
    """What each read said about the newest reply, compactly (the artifact keeps this): the status, `live`,
    `shownBytes`, the received text's length, the transcript text's, and the newest marker of the reply with where it
    ends in the source and whether `transcriptText` holds it, which is what the received-text checks judged."""
    out = []
    for r in samples:
        _, row = last_reply(r)
        found = markers(row["text"]) if row else []
        out.append({"t": round(r["t"], 3), "status": r["status"],
                    "live": row.get("live") if row else None,
                    "shown": int(row["shownBytes"]) if row and "shownBytes" in row else None,
                    "len": byte_len(row["text"]) if row else None,
                    "textLen": len(r["transcript_text"]), "latency": round(r.get("latency", 0.0), 3),
                    "newestMarker": found[-1][1] if found else None,
                    "newestMarkerEnd": found[-1][0] if found else None,
                    "markerInTranscriptText": (found[-1][1] in r["transcript_text"]) if found else None,
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
        after_copy = w.read()  # the reveal must still stand where it stood at the Select All
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
    _, row_after = last_reply(after_copy)
    detail = {"attempt": number, "live": row.get("live"), "shownBytesAtSelectAll": shown, "textBytesAtSelectAll": length,
              "shownBytesAfterCopy": int(row_after["shownBytes"]), "textBytesAfterCopy": byte_len(row_after["text"]),
              "clipboardChars": len(copied), "replyCharsCopied": len(reply_copied),
              "newestMarkerCopied": top[1] if top else None, "itsSourceEnd": top[0] if top else None}
    scn.keep(f"copy attempt {number}", detail)
    log(f"copy attempt {number}: {detail}")
    return detail, copied, reply_copied, top


def live_reads(samples):
    """The (read, reply row) of the reads that found the newest reply live."""
    return [(r, last_reply(r)[1]) for r in samples if last_reply(r)[1] and last_reply(r)[1].get("live") == "true"]


def ended_read(samples):
    """The first (read, reply row) after the turn: the read that says what the end of the turn did to the reply. A read
    that said `streaming` must come before it, or the end of an earlier reply could be judged."""
    streamed = False
    for r in samples:
        if r["status"] == "streaming":
            streamed = True
        elif streamed:
            return r, last_reply(r)[1]
    raise Fail("no read of the turn's end follows a read of the turn streaming")


def latencies(samples):
    """How long the reads took, for the artifact and the log: (max, median) in seconds."""
    times = sorted(x["latency"] for x in samples if "latency" in x)
    return (times[-1], times[len(times) // 2]) if times else (0.0, 0.0)


@scenario
def scenario_stream(scn):
    w = World(scn, "stream")
    tail = World(scn, "tail")
    try:
        w.start()
        w.open_chat("claude")
        # -- a 40 KiB reply in 200 chunks, read over and over (a read takes about 170 ms) while it streams
        w.send("showcase")
        samples = follow(w, ended_after_streaming(), keep="showcase samples")
        final = samples[-1]
        _, reply = last_reply(final)
        text = reply["text"]
        scn.need(byte_len(text) >= 40 * 1024, "the reply is at least 40 KiB", f"it is {byte_len(text)} bytes", read=final)
        live = live_reads(samples)
        scn.need(live, "the reply read as live while it streamed", f"{len(live)} of {len(samples)} reads live",
                 read=live[0][0] if live else samples[0])
        odd = [r for r, _ in live if r["status"] != "streaming"]
        scn.need(not odd, "a live reply is only ever read while the turn streams",
                 f"{len(live)} live reads, statuses {sorted({r['status'] for r, _ in live})}", read=odd[0] if odd else live[-1][0])
        behind = [(r, row) for r, row in live if 0 < int(row["shownBytes"]) < byte_len(row["text"])]
        scn.need(behind, "the reveal is behind the received text (0 < shownBytes < len(text)) while the reply is live",
                 f"{len(live)} live reads, {len(behind)} with the reveal part of the way: "
                 f"{[(row['shownBytes'], byte_len(row['text'])) for _, row in live][:8]}", read=behind[0][0] if behind else live[0][0])
        # The reveal is behind the text at `shownBytes` S only if the tail, from the end of the last frozen block shown to S,
        # is within the 7 KiB budget (reveal.rs, `drawn_reply`): so at S > 12 KiB at least 5 KiB of the reply had been frozen
        # as it arrived. With nothing frozen the tail is all of S and the reveal hands over once S passes 7 KiB: the
        # `compact` reply below, which cannot freeze, was read behind at 7163 bytes at most (the budget is 7168). The bar is 12 KiB and not
        # higher because the reads are sparse here (one per 1 to 3 KiB of reveal around the bar, sparser as the transcript grows) and a
        # reply that froze well can still hand over from about 20 KiB on (the stream's `segments()` may lag by up to a
        # quarter of the reply, spec 5.1; a run read behind the text only up to 19520 of 23780, after 14272 and 16379).
        deep = [(r, int(row["shownBytes"]), byte_len(row["text"])) for r, row in behind if int(row["shownBytes"]) > 12 * 1024]
        # With no such read, say which case it is: the reveal was judged (a live read landed while the text was 14 to 24 KiB,
        # where the reveal passes 12 KiB and before the earliest handover seen), or no read landed there because the reads
        # were too slow, which judges nothing. Only the reads' times and text lengths decide it, never `shownBytes`.
        in_window = [row for _, row in live if 14 * 1024 <= byte_len(row["text"]) <= 24 * 1024]
        if deep or in_window:
            why_deep = f"the reveal never passed 12 KiB while behind: {[(row['shownBytes'], byte_len(row['text'])) for _, row in behind][-3:]}"
        else:
            gaps = [(samples[i]["latency"], len(samples[i - 1]["transcript_text"]), len(samples[i]["transcript_text"]))
                    for i in range(1, len(samples))]
            slow = max(gaps, default=(0.0, 0, 0))
            why_deep = (f"could not be judged: no live read landed while the text was 14 to 24 KiB ({len(samples)} reads in all, text "
                        f"lengths {[byte_len(row['text']) for _, row in live]}; the slowest read took {slow[0]:.2f} s while the "
                        f"transcript text went from {slow[1]} to {slow[2]} characters): the UI thread was too busy to be read")
        scn.need(deep, "the reveal was still behind the text past 12 KiB, which needs 5 KiB of the reply frozen as it arrived (the stream path)",
                 why_deep,
                 read=deep[-1][0] if deep else final,
                 detail={"readsBehindPast12KiB": len(deep), "newest": deep[-1][1:] if deep else None,
                         "deepestBehind": max((int(row["shownBytes"]) for _, row in behind), default=None)})
        shown = [int(row["shownBytes"]) for _, row in live]
        shrank = [i for i in range(1, len(shown)) if shown[i] < shown[i - 1]]
        scn.need(not shrank, "the revealed byte count only grows",
                 f"{len(shown)} live reads, first {shown[:4]}, last {shown[-4:]}"
                 + (f"; it shrank from {shown[shrank[0] - 1]} to {shown[shrank[0]]}" if shrank else ""),
                 read=live[shrank[0]][0] if shrank else live[-1][0])
        past_text = [r for r, row in live if int(row["shownBytes"]) > byte_len(row["text"])]
        scn.need(not past_text, "shownBytes never passes the received text",
                 f"{len(live)} live reads, widest gap {max(byte_len(row['text']) - int(row['shownBytes']) for _, row in live)} bytes",
                 read=past_text[0] if past_text else live[-1][0])
        # `transcriptText` is the received text: the last marker received is in it, painted or not.
        with_marker = [(r, row, markers(row["text"])) for r, row in live if markers(row["text"])]
        scn.need(with_marker, "markers arrived while the reply was live", f"{len(with_marker)} live reads with a marker",
                 read=with_marker[-1][0] if with_marker else live[-1][0])
        missing = [(r, m[-1][1], int(row["shownBytes"]), byte_len(row["text"]))
                   for r, row, m in with_marker if m[-1][1] not in r["transcript_text"]]
        scn.need(not missing, "transcriptText holds every received byte: the last marker received is in it at every read",
                 f"{len(with_marker)} reads with a marker, missing at {len(missing)}, first {[m[1:] for m in missing[:3]]}",
                 read=missing[0][0] if missing else with_marker[-1][0])
        # The reads where the last marker received lies past the byte the reveal had reached are the ones that
        # tell a received text from a revealed one: transcriptText holds that marker at every one of them.
        past = [(r, row, m[-1]) for r, row, m in with_marker if m[-1][0] > int(row["shownBytes"])]
        lacking = [(r, row, m) for r, row, m in past if m[1] not in r["transcript_text"]]
        scn.need(past and not lacking,
                 "transcriptText holds text the reveal has not reached: at every read where the newest marker lies past shownBytes, transcriptText has it",
                 f"{len(past)} such reads, missing at {[(m[1], int(row['shownBytes'])) for r, row, m in lacking[:3]]}",
                 read=lacking[0][0] if lacking else (past[-1][0] if past else live[-1][0]),
                 detail={"readsWithTheNewestMarkerPastTheReveal": len(past),
                         "newest": (past[-1][2], int(past[-1][1]["shownBytes"])) if past else None})
        # at the turn's end
        r0, row0 = ended_read(samples)
        scn.need(row0.get("live") == "false",
                 "at the turn's end live is false at once (the first read after the turn, which follows a read that streamed)",
                 f"live={row0.get('live')} shownBytes={row0.get('shownBytes')} len={byte_len(row0['text'])}", read=r0,
                 detail={"shownBytes": int(row0["shownBytes"]), "textBytes": byte_len(row0["text"])})
        w.capture("stream-ended")
        # -- the same bytes in one chunk
        w.reply_file.write_text(text, encoding="utf-8")
        w.send("replyfile")
        after = w.wait("the one-chunk reply ends", lambda r: r["status"] == "completed" and len(assistant_rows(r)) == 2)
        (_, first), (_, second) = assistant_rows(after)
        scn.need(first["text"] == second["text"], "the streamed reply's source is the one-chunk reply's",
                 f"{byte_len(first['text'])} against {byte_len(second['text'])} bytes", read=after)
        plain = plain_by_row(after)
        index_first, index_second = assistant_rows(after)[0][0], assistant_rows(after)[1][0]
        scn.need(plain[index_first] == plain[index_second] and plain[index_first] != "",
                 "the streamed reply's plain text equals the one-chunk reply's",
                 f"{len(plain[index_first])} against {len(plain[index_second])} characters", read=after)
        # -- Select All and Copy through the transcript menu while a reply streams
        chosen = None
        for number in range(1, 4):
            attempt = copy_while_streaming(scn, w, number)
            detail, top = attempt[0], attempt[3]
            valid = (detail["live"] == "true" and detail["textBytesAtSelectAll"] - detail["shownBytesAtSelectAll"] >= 2000
                     and detail["shownBytesAfterCopy"] == detail["shownBytesAtSelectAll"]
                     and top is not None and attempt[1] != f"sentinel before the copy {number}")
            w.wait("the reply ends", lambda r: r["status"] != "streaming", timeout=40)
            if valid:
                chosen = attempt
                break
            log(f"copy attempt {number} did not land while the reveal was behind; trying again")
        scn.need(chosen, "a Select All and Copy was made while the reply streamed and the reveal was behind",
                 "none of three attempts had the reveal 2000 bytes behind the received text at the Select All, standing still "
                 "through the Copy, with the live reply on the clipboard")
        detail, copied, reply_copied, top = chosen
        scn.need(detail["shownBytesAfterCopy"] == detail["shownBytesAtSelectAll"] and detail["textBytesAfterCopy"] > detail["textBytesAtSelectAll"],
                 "the reveal stood still while the reply was out of view (shownBytes unchanged from the Select All to after the Copy) and the text went on arriving",
                 f"{detail['shownBytesAtSelectAll']} then {detail['shownBytesAfterCopy']}; text {detail['textBytesAtSelectAll']} then {detail['textBytesAfterCopy']}",
                 detail=detail)
        scn.need(top[0] > detail["shownBytesAtSelectAll"],
                 "the clipboard holds received text the reveal had not reached: the live reply's newest marker copied is past shownBytes at the Select All",
                 f"the newest marker copied ({top[1]}) ends at byte {top[0]}, the reveal was at {detail['shownBytesAtSelectAll']} "
                 f"of {detail['textBytesAtSelectAll']}", detail=detail)
        scn.need(reply_copied.startswith("Markdown showcase") and len(reply_copied) > 5000,
                 "what the clipboard holds of the live reply is the reply from its first heading on, not a fragment",
                 f"{len(reply_copied)} characters copied, starting {reply_copied[:30]!r}",
                 detail={"replyCharsCopied": len(reply_copied), "starts": reply_copied[:40]})
        w.capture("stream-copied")
        w.keep_artifacts()
        w.quit()

        # ---- the tail: a Sirio of its own, so that the three long replies above are not in the transcript. A debug build
        # builds the whole of a reply the stream cannot freeze for every chunk, and the control socket must stay answered.
        # A reply with no blank line: the stream never freezes a block of it, so once the reveal's tail passes the 7 KiB
        # budget (spec 5.2) it hands over for the rest of the turn and the reply draws as it arrives: `shownBytes` is the
        # whole text while `live` is still true.
        w = tail
        w.start()
        w.open_chat("claude")
        w.send("compact")
        samples = follow(w, ended_after_streaming(), keep="compact samples")
        live_c = live_reads(samples)
        scn.need(live_c and any(int(row["shownBytes"]) < byte_len(row["text"]) for _, row in live_c),
                 "the compact reply began with a reveal that trailed the received text",
                 f"{len(live_c)} live reads, first {[(row['shownBytes'], byte_len(row['text'])) for _, row in live_c][:4]}",
                 read=live_c[0][0] if live_c else samples[0])
        # The tail passes the budget when the reveal reaches 7 KiB, and the reveal trails the text by a few KiB at most: from
        # 14 KiB of text on the reply is past the handover. The window needs reads in it; with fewer, the run says what
        # happened to them instead of judging nothing.
        slowest, median = latencies(samples)
        log(f"compact phase: {len(samples)} reads, latency max {slowest:.2f} s, median {median:.2f} s")
        over = [(r, row) for r, row in live_c if byte_len(row["text"]) >= 14 * 1024]
        scn.need(len(over) >= 5,
                 "the compact reply's window from 14 KiB of text on was read at least five times while it was live",
                 f"only {len(over)} live reads landed from 14 KiB of text on ({len(samples)} reads in all, the slowest took {slowest:.2f} s, the median "
                 f"{median:.2f} s): the UI thread was too busy to be read often enough, the handover cannot be judged",
                 read=samples[-1], detail={"reads": len(samples), "readsFrom14KiB": len(over), "latencyMax": round(slowest, 3),
                                           "latencyMedian": round(median, 3)})
        unhanded = [(r, row) for r, row in over if int(row["shownBytes"]) != byte_len(row["text"])]
        scn.need(not unhanded,
                 "past the tail budget the compact reply hands over to drawing as it arrives: at every live read from 14 KiB of text on, shownBytes is the whole text and live is true",
                 f"{len(over)} reads from 14 KiB, not whole at {[(int(row['shownBytes']), byte_len(row['text'])) for _, row in unhanded[:4]]}",
                 read=unhanded[0][0] if unhanded else over[-1][0],
                 detail={"readsFrom14KiB": len(over), "newest": (int(over[-1][1]["shownBytes"]), byte_len(over[-1][1]["text"]))})
        rc, rowc = ended_read(samples)
        scn.need(rowc.get("live") == "false",
                 "the compact reply ends with live false",
                 f"live={rowc.get('live')} shownBytes={rowc.get('shownBytes')} len={byte_len(rowc['text'])}", read=rc,
                 detail={"shownBytes": int(rowc["shownBytes"]), "textBytes": byte_len(rowc["text"])})
        compact_text = rowc["text"]
        after_c = one_chunk_plain(scn, w, compact_text, "the compact reply's final plain text equals the same bytes delivered in one chunk")
        # -- SHOWCASE eight times over: every copy cites [^1] again, so a good part of the reply is held in the tail. How
        # much of it freezes varies from run to run: its reveal has handed over after reads behind at 5824 to 13454 bytes,
        # and has stayed behind to 34764 and 40537; so only its end and its final equivalence are checked.
        w.send("showcase x8")
        samples = follow(w, ended_after_streaming(), keep="showcase x8 samples")
        r8, row8 = ended_read(samples)
        scn.need(row8.get("live") == "false",
                 "the x8 reply ends with live false",
                 f"live={row8.get('live')} shownBytes={row8.get('shownBytes')} len={byte_len(row8['text'])}", read=r8,
                 detail={"shownBytes": int(row8["shownBytes"]), "textBytes": byte_len(row8["text"])})
        one_chunk_plain(scn, w, row8["text"], "the x8 reply's final plain text equals the same bytes delivered in one chunk")
    finally:
        teardown([w, tail])


def alive(pid):
    """Whether the process runs: one that has exited and not been reaped (a zombie) does not."""
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return False
    return stat.rsplit(")", 1)[1].split()[0] != "Z"


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
        w.send("showcase slow")  # 8 s, so that a capture and a read fit between the reveal's start and the Stop
        # Early: the capture and the two reads around it take two seconds or more (about 12 KiB of this reply), and the Stop must
        # land before the reveal could hand over to drawing as the text arrives (a well-frozen reply has done so from
        # about 20 KiB on); started at 8 KiB of text, the Stop's read had landed at 17214 to 18804 bytes of reveal.
        w.wait("the reply is part way", lambda r: last_reply(r)[1] is not None and last_reply(r)[1].get("live") == "true"
               and byte_len(last_reply(r)[1]["text"]) > 1024, timeout=30, every=0.05)

        def reveal_behind(r):
            reply = last_reply(r)[1]
            return reply is not None and reply.get("live") == "true" and 0 < int(reply["shownBytes"]) < byte_len(reply["text"])

        def wait_behind(what):
            """`reveal_behind`, with a timeout that names a handover when there was one: reads that found the reply live
            with the whole text shown past the 7 KiB budget. The reply ends about 8 s in, before the 15 s run out, so those
            reads are counted as they come, not judged from the last read (which the plain timeout message shows)."""
            whole = []

            def predicate(r):
                reply = last_reply(r)[1]
                if (reply is not None and reply.get("live") == "true" and byte_len(reply["text"]) > 7 * 1024
                        and int(reply["shownBytes"]) == byte_len(reply["text"])):
                    whole.append(byte_len(reply["text"]))
                return reveal_behind(r)

            try:
                return w.wait(what, predicate, timeout=15, every=0.03)
            except Fail as failure:
                if whole:
                    raise Fail(f"{what}: the reply handed over to drawing as it arrives before the Stop was taken: {len(whole)} reads "
                               f"found it live with the whole text shown (shownBytes == len(text), from {whole[0]} bytes on), its "
                               f"tail past the 7 KiB budget, so there was no reveal behind to stop ({failure})") from None
                raise

        # The capture sits between two reads that found the reveal behind the text (by however little: the lag is a few KiB
        # and no threshold of ours is near it), so the frame shows a reveal the reads prove behind; the Stop follows the
        # second read at once, and the first read after it says what the Stop did.
        wait_behind("the reveal is behind before the capture")
        w.capture("stop-mid-reveal")
        before = wait_behind("the reveal is behind just before the Stop")
        _, behind = last_reply(before)
        w.stop_turn()
        samples = follow(w, lambda r: r["status"] != "streaming", timeout=30, every=0.03, keep="reads after the Stop")
        final = samples[-1]
        _, row = last_reply(final)
        text = row["text"]
        scn.need(row.get("live") == "false" and int(behind["shownBytes"]) < byte_len(behind["text"]) <= byte_len(text),
                 "Stop in mid-reveal ends the live reply at once: the reveal was behind the text at the read before (and at the capture), and at the first read after, live is false",
                 f"before: shownBytes={behind['shownBytes']} len={byte_len(behind['text'])}; after: live={row.get('live')} "
                 f"shownBytes={row.get('shownBytes')} len={byte_len(text)}",
                 read=final,
                 detail={"before": {"live": behind.get("live"), "shownBytes": int(behind["shownBytes"]), "textBytes": byte_len(behind["text"])},
                         "after": {"live": row.get("live"), "shownBytes": int(row["shownBytes"]), "textBytes": byte_len(text)}})
        scn.need(0 < byte_len(text) < 40 * 1024, "the reply was cut by Stop (it is shorter than the whole reply)", f"{byte_len(text)} bytes",
                 detail={"textBytes": byte_len(text)})
        scn.need(error_row(final) is None, "a Stop is not a failure: no error row",
                 f"rows after the Stop: {[row_.get('kind') for row_ in final['rows']]}", read=final)
        scn.need(not final["is_held"], "a Stop holds no queue", f"held={final['is_held']} line={final['held_line']!r}", read=final)
        w.capture("stop-ended")
        # Stop clears the prompt to resend: the agent dying now has nothing to resume or resend.
        launches = len(w.sessions("claude"))
        kill_stub(w.sessions("claude")[-1])
        died = w.wait("the lost agent is an error row", lambda r: error_row(r) is not None, timeout=30)
        index, error = error_row(died)
        scn.need(error.get("retry") == "relaunch",
                 "a client lost after Stop has no prompt to resend: Retry only relaunches (retry: relaunch)",
                 f"errorKind={error.get('errorKind')} retry={error.get('retry')}", detail={"row": error})
        w.retry(index)
        w.wait("the relaunched agent has connected", lambda r: r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        scn.need(len(w.sessions("claude")) == launches + 1 and len(w.prompts()) == 1, "Retry relaunched the agent and sent nothing",
                 f"launches {w.sessions('claude')}, prompts {w.prompts()}",
                 detail={"launches": w.sessions("claude"), "prompts": w.prompts()})
        # the same bytes in one chunk
        w.reply_file.write_text(text, encoding="utf-8")
        w.send("replyfile")
        after = w.wait("the one-chunk reply ends", lambda r: r["status"] == "completed" and len(assistant_rows(r)) == 2)
        plain = plain_by_row(after)
        (i_stopped, stopped), (i_whole, whole) = assistant_rows(after)
        scn.need(stopped["text"] == whole["text"] and plain[i_stopped] == plain[i_whole] and plain[i_whole],
                 "the stopped reply's final text equals the same bytes delivered in one chunk",
                 f"{len(plain[i_stopped])} against {len(plain[i_whole])} characters, source {byte_len(stopped['text'])} against {byte_len(whole['text'])} bytes",
                 read=after)
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
                 "the failure text is the error row's, not a reply",
                 f"error row {row['text']!r}; reply rows {[a['text'][:30] for _, a in assistant_rows(r)]}")
        (_, partial), = assistant_rows(r)
        scn.need(partial.get("live") == "false" and byte_len(partial["text"]) == byte_len(PARTIAL),
                 "the partial reply of the failed turn is ended at once (live false) and holds all that was streamed",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')} len={byte_len(partial['text'])}")
        users = rows_of(r, "user")
        at = int(users[0][1]["at"])
        scn.need(began - 5 <= at <= time.time() + 5, "the user row carries `at`, the Unix second the turn was asked", f"at={at}, now={int(time.time())}")
        w.capture("failed-error-row")
        pid = w.sessions("claude")[-1]
        w.retry(index)
        r2 = w.wait("the resent turn is answered", lambda r: r["status"] == "completed" and error_row(r) is None
                    and any(a["text"] == "recovered" for _, a in assistant_rows(r)))
        scn.need(error_row(r2) is None, "Retry removed the error row", f"rows {[row_.get('kind') for row_ in r2['rows']]}")
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
                 "error_max_turns keeps its reply text and footer: no error row, nothing held",
                 f"rows {[(row_.get('kind'), (row_.get('text') or '')[:30]) for row_ in r3['rows'][-3:]]}, held={r3['is_held']}")
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
        (_, partial), = assistant_rows(r)
        scn.need(partial.get("live") == "false" and byte_len(partial["text"]) > 0,
                 "the partial reply of the failed prompt is ended at once (live false) and kept",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')} len={byte_len(partial['text'])}")
        scn.need(alive(pid), "the agent process is still alive after the failed prompt", detail={"pid": pid})
        w.capture("acp-failed")
        w.retry(index)
        r = w.wait("the resent prompt is answered", lambda r: r["status"] == "completed" and error_row(r) is None
                   and any(a["text"] == "recovered" for _, a in assistant_rows(r)))
        scn.need(user_texts(r) == ["fail prompt", "fail prompt"], "Retry sent the same text again as a new user row", f"{user_texts(r)}")
        sessions = [value for kind, p, value in w.records("session_new") if p == pid]
        scn.need(w.prompts() == [(pid, "fail prompt"), (pid, "fail prompt")] and w.sessions("opencode")[-1] == pid and alive(pid),
                 "the agent heard the same text twice on the same process, which was never relaunched",
                 f"prompts {w.prompts()}, launches {w.sessions('opencode')}",
                 detail={"prompts": w.prompts(), "launches": w.sessions("opencode")})
        scn.need(len(sessions) == 1,
                 "that process was asked for one session only (one session/new), so the resend went to the conversation the failed prompt was in",
                 f"session/new on {pid}: {sessions}", detail={"sessionNew": sessions})
        # a failure that follows Stop is the late end of the stopped turn: no error row, no hold
        w.send("cancel fail")
        w.wait("the partial reply streams", lambda r: r["status"] == "streaming" and any(a["text"].startswith("I started") for _, a in assistant_rows(r)))
        w.stop_turn()
        r = w.wait("the stopped turn ends", lambda r: r["status"] != "streaming", timeout=30)
        time.sleep(1.5)
        r = w.read()
        cancelled = [value for kind, p, value in w.records("cancel") if p == pid]
        answered = [value for kind, p, value in w.records("cancel_answered") if p == pid]
        scn.need(len(cancelled) == 1 and len(answered) == 1,
                 "the agent received session/cancel for the stopped turn and answered it with a JSON-RPC error (the fixture's log)",
                 f"cancel {cancelled}, answered {answered}", detail={"cancel": cancelled, "cancelAnswered": answered})
        scn.need(error_row(r) is None and not r["is_held"] and r["status"] != "streaming",
                 "the error that follows Stop is the late end of the stopped turn: no error row, nothing held",
                 brief(r))
        w.send("after the stop")
        r = w.wait("the chat still works", lambda r: r["status"] == "completed" and any(a["text"] == "reply ok" for _, a in assistant_rows(r)))
        scn.ok("the chat answers the next prompt after the stopped turn", judged=f"reply rows {[a['text'] for _, a in assistant_rows(r)]}", read=r)
    finally:
        teardown(w)


@scenario
def scenario_claude_lost_resume(scn):
    w = World(scn, "lost")
    idle = World(scn, "idle")
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
        scn.need(partial.get("live") == "false" and byte_len(partial["text"]) == byte_len(PARTIAL),
                 "the partial reply of the lost client is ended at once (live false) and holds all that was streamed",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')} len={byte_len(partial['text'])}")
        scn.need(not alive(first), "the agent process is gone", f"pid {first} alive={alive(first)}")
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
        w.keep_artifacts()
        w.quit()

        # ---- the CLI dies while idle, after a finished turn: nothing in flight, so Retry only relaunches
        idle.start()
        idle.open_chat("claude")
        idle.send("ok")
        idle.wait("the turn is answered", lambda r: r["status"] == "completed" and any(a["text"] == "ok" for _, a in assistant_rows(r)))
        dead = idle.sessions("claude")[-1]
        launches = len(idle.sessions("claude"))
        kill_stub(dead)
        r = idle.wait("the lost CLI is an error row", lambda r: error_row(r) is not None, timeout=30)
        index, row = error_row(r)
        scn.need(row.get("errorKind") in ("connection", "disconnected") and row.get("retry") == "relaunch",
                 "a Claude CLI that dies while idle is an error row whose Retry only relaunches (retry: relaunch): nothing was in flight",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')}", detail={"row": row})
        idle.capture("lost-idle")
        idle.retry(index)
        idle.wait("the relaunched CLI has connected", lambda r: r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        time.sleep(1.5)  # a prompt the retry wrongly sent would be in the stub's log by now
        r = idle.read()
        scn.need(len(idle.sessions("claude")) == launches + 1 and idle.sessions("claude")[-1] != dead
                 and idle.prompts() == [(dead, "ok")]
                 and user_texts(r) == ["ok"] and r["composerText"] == ""
                 and row_with_text(r, RESTARTED_BACK) is None and row_with_text(r, RESTARTED) is None,
                 "after the idle death Retry started a new process and sent nothing: the stub heard only the first prompt, the transcript holds only that user row, the composer is empty, no notice",
                 f"launches {idle.sessions('claude')}, prompts {idle.prompts()}, user rows {user_texts(r)}, composer {r['composerText']!r}",
                 detail={"launches": idle.sessions("claude"), "prompts": idle.prompts(), "userRows": user_texts(r)})
    finally:
        teardown([w, idle])


@scenario
def scenario_acp_lost_composer(scn):
    a = World(scn, "mid")
    k = World(scn, "idlekill")
    b = World(scn, "launch")
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
        scn.need(partial.get("live") == "false" and byte_len(partial["text"]) > 0,
                 "the partial reply of the lost agent is ended at once (live false) and kept",
                 f"live={partial.get('live')} shownBytes={partial.get('shownBytes')} len={byte_len(partial['text'])}")
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

        # ---- the agent exits while idle (spec 7.2, as built): `sirio_acp` learns of a closed transport only from a pending
        # call, so the chat shows nothing until the next send, and that send is lost mid-turn with no way to resume: the
        # new message goes back to the composer. (The idle-death row, "relaunch only", is Claude's, in claude_lost_resume.)
        k.start()
        k.open_chat("opencode")
        k.send("hello")
        k.wait("the first turn is answered", lambda r: r["status"] == "completed" and any(a_["text"] == "reply ok" for _, a_ in assistant_rows(r)))
        dead = k.sessions("opencode")[-1]
        launches = len(k.sessions("opencode"))
        kill_stub(dead)
        k.send("next")
        r = k.wait("the send after the death ends in an error row", lambda r: error_row(r) is not None and r["status"] != "streaming", timeout=40)
        index, row = error_row(r)
        scn.need(row.get("errorKind") in ("connection", "disconnected") and row.get("retry") == "composer",
                 "the send after an idle ACP death lands on lost mid-turn: retry composer",
                 f"errorKind={row.get('errorKind')} retry={row.get('retry')} prompts {k.prompts()}",
                 detail={"row": row, "prompts": k.prompts()})
        k.retry(index)
        k.wait("the relaunched agent is connected with the new message back", lambda r: r["composerText"] == "next"
               and r["status"] in ("idle", "completed") and error_row(r) is None, timeout=40)
        time.sleep(1.5)  # a prompt the retry wrongly sent would be in the stub's log by now
        r = k.read()
        scn.need(row_with_text(r, RESTARTED_BACK) is not None and len(k.sessions("opencode")) == launches + 1 and k.prompts() == [(dead, "hello")]
                 and r["composerText"] == "next",
                 "Retry relaunched the agent, put the new message back in the composer with the notice, and sent nothing",
                 brief(r), detail={"launches": k.sessions("opencode"), "prompts": k.prompts(), "composer": r["composerText"]})
        k.capture("acp-idle-death")
        k.keep_artifacts()
        k.quit()

        # ---- no turn in flight: the launch fails, then works.
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
        teardown([a, k, b, c])


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
        scn.need(refusal and "no Retry" in refusal and card_phase(r) == "lifted" and time.time() >= epoch - 1,
                 "with no failed turn the card ends lifted, only once the reset has passed, and Retry is refused after it",
                 f"{refusal!r}, now {time.time():.1f}, reset {epoch}", detail={"reset": epoch, "now": round(time.time(), 1)})
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
        scn.need(card_phase(r) == "lifted" and time.time() >= epoch - 1 and c.retry_refusal(error_index) is not None,
                 "the card whose Retry was retired ends lifted, only once the reset has passed, and the row's Retry is still refused",
                 f"now {time.time():.1f}, reset {epoch}", detail={"reset": epoch, "now": round(time.time(), 1)})
    finally:
        teardown([a, b, c])


def day_name(day, today):
    """Ely's words for `day` seen from `today` (`ely_chat::day_name`), written out here as the E2E's own oracle."""
    if day == today:
        return "Today"
    if day == today - timedelta(days=1):
        return "Yesterday"
    if day.year == today.year:
        return day.strftime("%A, %B %-d")
    return day.strftime("%B %-d, %Y")


def read_on_a_day(w):
    """A read of the chat and the app's date for it. The app runs on TZ=UTC and reads its clock at each read, so the
    clock is read before and after the read and the read is taken again when midnight fell between the two."""
    while True:
        before = datetime.now(timezone.utc).date()
        r = w.read()
        after = datetime.now(timezone.utc).date()
        if before == after:
            return r, after
        time.sleep(1)


@scenario
def scenario_day_separators(scn):
    w = World(scn, "days")
    try:
        w.start()
        w.open_chat("claude")
        surface = w.surface
        w.keep_artifacts()
        w.quit()
        # A transcript with user entries on three days (and one with no date), written as the app stores it. Only the
        # moments are chosen here; what each separator must say is worked out from the clock after the relaunch.
        seeded = datetime.now(timezone.utc)
        old = datetime(seeded.year - 1, 3, 12, 12, 0, tzinfo=timezone.utc)
        yesterday = (seeded - timedelta(days=1)).replace(hour=12, minute=0, second=0, microsecond=0)
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
        r, today = read_on_a_day(w)
        scn.keep("the restored transcript", {"rows": r["rows"], "separators": r["separators"], "today": str(today)})
        scn.need(user_texts(r) == [text for text, _ in turns], "the restored transcript holds the three turns", f"{user_texts(r)}")
        users = rows_of(r, "user")
        scn.need(users[0][1].get("at") == str(turns[0][1]) and "at" not in users[1][1] and users[2][1].get("at") == str(turns[2][1]),
                 "a restored user row has `at`, the Unix second it was asked; an undated turn has no `at` key (absent, not null)",
                 f"{[row.get('at') for _, row in users]}")
        def separators_for(users, today, *moments):
            """What the separators must say, from the app's `today` of the very read they are compared with: one above
            the first dated user row of each day (a dated row on the day of the dated row before it has none), with Ely's
            words for that day."""
            out, previous = [], None
            for (entry, _), moment in zip(users, moments):
                if moment is None:
                    continue
                if moment.date() != previous:
                    out.append({"kind": "daySeparator", "entry": str(entry), "label": day_name(moment.date(), today)})
                previous = moment.date()
            return out

        want = separators_for(users[:3], today, old, None, yesterday)
        scn.need(r["separators"] == want,
                 "a separator sits above the first dated turn (a full date) and above yesterday's: undated turns get none",
                 f"{r['separators']} against {want} (today {today})", detail={"today": str(today), "want": want})
        w.send("today's question")
        w.wait("today's turn is answered", lambda r: r["status"] == "completed" and "today's question" in user_texts(r))
        r, today = read_on_a_day(w)  # one read, one clock: the labels below are worked out from this `today`
        users = rows_of(r, "user")
        at = int(users[-1][1]["at"])
        moments = [old, None, yesterday, datetime.fromtimestamp(at, timezone.utc)]  # one per user row
        want = separators_for(users, today, *moments)
        scn.need(abs(at - time.time()) < 120 and r["separators"] == want,
                 "a new turn has `at` now and a separator above it; the three labels are a full date, Yesterday and Today",
                 f"at={at}, separators {r['separators']} against {want} (today {today})", detail={"today": str(today), "want": want})
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
        scn.need(unsettled, "a thought that is still open has no durationMs key (absent, not null)",
                 f"{len(unsettled)} reads of {len(samples)} had the thought open, no durationMs key", read=unsettled[0] if unsettled else samples[0])
        settled = [r for r in samples if rows_of(r, "thought") and "durationMs" in rows_of(r, "thought")[0][1]]
        scn.need(settled, "the thought settles", f"{len(settled)} reads of {len(samples)} had the thought settled",
                 read=settled[0] if settled else samples[-1])
        first = settled[0]
        scn.need(not assistant_rows(first), "the thought settled when its block closed, before any reply text had arrived",
                 f"{len(assistant_rows(first))} reply rows at the first read with a durationMs", read=first)
        duration = int(rows_of(first, "thought")[0][1]["durationMs"])
        scn.need(300 <= duration < 1000,
                 "its durationMs is the thinking block's (under 1 s), not the 2 s wait for the text",
                 f"{duration} ms", read=first)
        last = samples[-1]
        scn.need(int(rows_of(last, "thought")[0][1]["durationMs"]) == duration and any(a["text"] == "done thinking" for _, a in assistant_rows(last)),
                 "after the reply arrived and the turn ended the thought's durationMs is the same",
                 f"{rows_of(last, 'thought')[0][1].get('durationMs')} ms against {duration} ms; reply {[a['text'] for _, a in assistant_rows(last)]}",
                 read=last)
        w.capture("thought-settled")
    finally:
        teardown(w)


if __name__ == "__main__":
    main()
