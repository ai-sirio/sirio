"""The harness of the chat end-to-end tests: test-chat-richer-messages-e2e.sh (chat_richer_messages_e2e.py),
test-chat-message-actions-e2e.sh (chat_message_actions_e2e.py) and test-chat-shell-e2e.sh (chat_shell_e2e.py) all import
it, and each registers its own scenarios.

Everything here runs against real, isolated debug Sirio instances on the script's private Xvfb. A
`World` is one such instance with its own HOME, XDG tree, database, control socket and host root, started
under `env -i` with an allowlist, the stub `claude` and `opencode` first on its PATH
(Scripts/Tests/stubs/richer). A scenario is a function that raises `Fail` at its first failed check; the run
reports it and goes on with the next one. The environment (ROOT, RUN_DIR, OUT, DISP, BIN, CTL, WINDOW_SIZE,
ONLY and the Vulkan files) is set by the .sh that runs the script.
"""
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
import traceback
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
               "durationMs", "actions", "actionsShown", "quote")
SHELL_KEYS = ("command", "exit", "sent", "lines", "expanded")  # a shell block's columns (`!cmd`), besides `status`


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
        if row.get("kind") == "shell":
            entry.update({key: row[key] for key in SHELL_KEYS if key in row})
            output = row.get("output", "")  # thousands of lines at times: its two ends
            entry["output"] = output if len(output) <= 80 else f"{output[:40]}...{output[-30:]} ({len(output)} chars)"
        entry.update({key: value for key, value in row.items() if key.startswith("actionBounds.")})
        rows.append(entry)
    out = {"status": r.get("status"), "queue": r.get("queue"), "held": r.get("held"), "heldLine": r.get("held_line"),
           "composer": r.get("composerText"), "rows": rows, "transcriptTextChars": len(r.get("transcript_text", ""))}
    # What the message actions add to the readout: what Continue does and where, the quote held for the next prompt.
    out.update({key: r[key] for key in ("continue", "continueRow", "composerQuote", "composerQuoteBounds") if key in r})
    if r.get("separators"):
        out["daySeparators"] = r["separators"]
    composer_row = r.get("composerRow") or {}
    if "shellMode" in composer_row or "bandRule" in composer_row:  # the chat shell E2E's command mode, when the build reports it
        out["composerCommandMode"] = {key: composer_row[key] for key in ("shellMode", "bandRule", "band") if key in composer_row}
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

    def __init__(self, scenario, name, extra_env=None):
        self.scn = scenario
        self.name = name
        self.extra_env = dict(extra_env or {})  # added to the app's environment, e.g. SIRIO_PERF_TRACE
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
            "DISPLAY": DISP, "GPUI_X11_SCALE_FACTOR": "1", **VULKAN, **self.extra_env,
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

    def toggle_shell(self, entry):
        """The debug press of a shell block's Show all / Show less (`toggleShell`, as `toggleTurn` is for a turn's fold)."""
        self.rpc("surface.chat.transcript", surfaceId=self.surface, action="toggleShell", entry=str(entry))

    def escape(self):
        """A real Escape in the composer: a click on its field first, so that the key lands there (the pointer stays over
        it, which is where the X server sends keys), then the key through the X server's input, not a message to the window.
        Returns the time just before the key, which is where a "within N s of the Escape" window starts."""
        field = self.read()["composerRow"]["field"]
        press(self.window, field[0] + field[2] / 2, field[1] + field[3] / 2, hold=0.1, settle=0.1)
        time.sleep(0.3)
        pressed = time.time()
        x("key", "Escape")
        time.sleep(0.3)
        return pressed

    def click_composer(self):
        """A real click on the composer's field, so that the keys that follow land there (and the caret is where the click
        puts it: at the end of a short draft)."""
        field = self.read()["composerRow"]["field"]
        press(self.window, field[0] + field[2] / 2, field[1] + field[3] / 2, hold=0.1, settle=0.1)
        time.sleep(0.3)

    def type_keys(self, text):
        """Types `text` with the X server's keys, one every 40 ms, where the focus is (see `click_composer`)."""
        x("type", "--delay", "40", text)
        time.sleep(0.3)

    def type_text(self, text):
        """Types `text` into the composer as a user does: a click on its field, then the keys."""
        self.click_composer()
        self.type_keys(text)

    def key(self, name):
        """One real key of the X server (`Escape`, `Home`, `BackSpace`), with no click and no wait after it. Returns the time
        just before it."""
        pressed = time.time()
        x("key", name)
        return pressed

    def press_enter(self):
        """A real Enter, which goes to the composer that `type_text` or `escape` last clicked."""
        x("key", "Return")
        time.sleep(0.3)

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
        text = (row.get("text") or row.get("command") or "").replace("\n", " ")[:40]
        keys = ("errorKind", "retry", "coveredBy", "phase", "live", "shownBytes")
        if row.get("kind") == "shell":
            keys += ("status", "exit", "sent")
        extra = "".join(f" {key}={row[key]}" for key in keys if key in row)
        rows.append(f"{index}:{row.get('kind')}{extra}:{text!r}")
    return json.dumps({"status": r.get("status"), "queue": r.get("queue"), "held": r.get("held"),
                       "heldLine": r.get("held_line"), "composer": r.get("composerText"), "rows": rows})[:900]


# ---------------------------------------------------------------- helpers on a readout


def rows_of(r, kind):
    return [(index, row) for index, row in enumerate(r["rows"]) if row.get("kind") == kind]


def shell_rows(r):
    """The (index, row) of every shell block (`!cmd` in the chat), oldest first."""
    return rows_of(r, "shell")


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


def main(label):
    """Runs the registered scenarios and prints `<label> E2E OK (<n> checks)`, or FAILED or PARTIAL."""
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
        log(f"{label} E2E FAILED ({len(FAILED)} of {ran} scenarios failed, {len(CHECKS)} checks passed)")
        sys.exit(1)
    if ONLY:
        log(f"{label} E2E PARTIAL ({ran} of {len(SCENARIOS)} scenarios: {', '.join(ONLY)}; {len(CHECKS)} checks)")
    else:
        log(f"{label} E2E OK ({len(CHECKS)} checks)")


def last_reply(r):
    found = assistant_rows(r)
    return found[-1] if found else (None, None)



def scroll(w, pixels):
    w.rpc("surface.chat.transcript", surfaceId=w.surface, action="scrollBy", pixels=str(pixels))



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


def row_with_text(r, text):
    return next(((i, row) for i, row in enumerate(r["rows"]) if row.get("text") == text), None)
