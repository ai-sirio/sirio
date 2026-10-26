#!/usr/bin/env python3
"""End-to-end verification of the native Ely agent chat.

Real, isolated Sirio instances -- scratch database, control socket and
credential store, a scratch git repository, the existing Python agent fixture
standing in for the agent's binary -- driven over the control socket, with
PID-matched X11 window captures. Nothing here touches the user's own database, credentials or running
Sirio. Rerunning the script reproduces the artifact.

Scenarios (each its own instance): staged streaming with a FIFO queue,
permission, listed-option question, plan approval, cancellation, agent death
and recovery across a relaunch, authentication required, restore across a
relaunch, and the effort list (`effort`, `effort_light`: High asked of the agent
at start, the list opened and a level chosen, High asked again after a
relaunch; `effort_full`, `effort_full_light`: the whole ladder, Default and
Ultracode included, drawn, and Default chosen from it). Three more need a display and
real X input or windows, and are skipped under --state-only: `composer_keys` (the composer's
keys through xdotool: Return, Shift+Return, Up and Down, the slash popup), `reduced_motion`
(SIRIO_REDUCED_MOTION=1 holds the thinking orb still) and `unfocused_spinner` (the sidebar
row's status bloom keeps turning while another window holds the focus, and what that costs in
CPU). After them, unless --no-identity, the five agents' marks as native
views with explicitly supplied identities (the example probe, not the app:
the fixture run proves no provider identity).

The seeded chat tab records the OpenCode adapter, and an executable named
`opencode` is put first on the instance's PATH: Sirio's own restore path
resolves OpenCode's built-in `opencode acp` and launches it, so the fixture is
reached the way a real agent would be. (A tab with no recorded agent is refused
on restore -- `SIRIO_ACP_PROGRAM` serves only a new chat opened with no agent
picked -- so the fixture cannot ride that variable here.)

Control methods used are the existing ones: `surface.chat.open` selects an
already rendered chat (it does not create one -- the seeded database does),
`.send`/`.compose` with `surfaceId,text`, `.permission` with
`surfaceId,requestId,optionId`, `.stop`/`.read` with `surfaceId`; the effort
scenarios also use the debug-only `surface.chat.effort` with
`surfaceId,action` (open, close or choose) and `value` for choose. Every
parameter is a string.

Artifacts under --out-dir: transcript.log, responses.jsonl (every control
request and its reply), summary.json, and per scenario: app.log,
agent-traffic.jsonl (the fixture's stdin/stdout), exit-status and frames/*.png.

Usage: Scripts/Tests/test-ely-chat-ui-e2e.py --out-dir PATH [--state-only]
           [--display :N] [--no-build] [--no-identity] [--window-size WxH]
           [--sirio-bin PATH] [--only NAME[,NAME]] [--cpu-ceiling PCT]
           [--focus-thief-bin PATH]

--sirio-bin runs the same scenarios against another build (the baseline, to
compare); --only runs the named scenarios (function names, e.g. long_history).
--cpu-ceiling PCT fails `unfocused_spinner` when Sirio uses more than PCT of one core while
it spins unfocused (the figure is logged either way). --focus-thief-bin is the window that
takes the focus from Sirio there: the surfaces probe, `surfaces_probe --frame tooltip`, built
with the other examples unless --no-build. It is a Sirio example so that no extra tool (an xterm,
an xmessage) has to be installed for the scenario.

--state-only exercises state and omits frames. Without it a display is
required, with xwininfo, xprop, import and identify on PATH (and xdotool for
the identity lane and --window-size). To prove the runner can fail, set
ELY_E2E_CORRUPT=1: the first scenario's expected reply is corrupted and the
run must exit nonzero.
"""

from __future__ import annotations

import argparse
import itertools
import json
import os
import re
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGET = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "rust" / "target"))
BIN = TARGET / "debug" / "sirio"
SEED = TARGET / "debug" / "examples" / "ely_chat_seed"
PROBE = TARGET / "debug" / "examples" / "ely_chat_probe"
FOCUS_THIEF = TARGET / "debug" / "examples" / "surfaces_probe"
FIXTURE = ROOT / "rust" / "crates" / "sirio_ui" / "tests" / "fixtures" / "chat_fixture.py"
AGENTS = ["claude", "codex", "opencode", "pi", "omp"]

_IDS = itertools.count(1)


class Failure(Exception):
    """A check of the run failed; the message says which and why."""


def request(socket_path: Path, method: str, params: dict[str, str]) -> dict:
    """One request over the control socket: a JSON line out, a JSON line back.

    Every parameter is a string -- the protocol's own rule -- and anything
    else is refused here rather than silently stringified.
    """
    for key, value in params.items():
        if not isinstance(key, str) or not isinstance(value, str):
            raise TypeError(f"control parameters are strings: {key!r}={value!r}")
    message = {"id": f"e2e-{next(_IDS)}", "method": method, "params": params}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(20)
        connection.connect(str(socket_path))
        connection.sendall(json.dumps(message).encode() + b"\n")
        buffer = b""
        while not buffer.endswith(b"\n"):
            chunk = connection.recv(65536)
            if not chunk:
                break
            buffer += chunk
    if not buffer:
        raise Failure(f"{method}: the control socket closed without a reply")
    reply = json.loads(buffer.decode())
    if reply.get("id") != message["id"]:
        raise Failure(f"{method}: reply id {reply.get('id')!r} is not {message['id']!r}")
    return reply


def agent_proxy(mode: str, directory: str, log: str) -> int:
    """`--agent-proxy`: the fixture as an agent, with its traffic recorded.

    Sirio launches `SIRIO_ACP_PROGRAM` with no arguments, so the wrapper
    script runs this: the fixture is a child whose stdin and stdout pass
    through here, one JSON line at a time, each copied to the log.
    """
    import threading

    child = subprocess.Popen(
        [sys.executable, "-u", str(FIXTURE), mode, directory],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=open(log + ".stderr", "ab"),
    )
    lock = threading.Lock()
    record = open(log, "a", buffering=1)

    def note(direction: str, line: bytes) -> None:
        with lock:
            record.write(
                json.dumps({"dir": direction, "line": line.decode(errors="replace").rstrip("\n")})
                + "\n"
            )

    def pump_in() -> None:
        for line in sys.stdin.buffer:
            note("to-agent", line)
            try:
                child.stdin.write(line)
                child.stdin.flush()
            except (BrokenPipeError, ValueError):
                break
        try:
            child.stdin.close()
        except OSError:
            pass

    threading.Thread(target=pump_in, daemon=True).start()
    for line in child.stdout:
        note("from-agent", line)
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
    return child.wait()


class Run:
    """The run's shared state: where artifacts go and how to capture."""

    def __init__(self, arguments: argparse.Namespace) -> None:
        self.out = Path(arguments.out_dir).resolve()
        self.state_only: bool = arguments.state_only
        self.display: str = arguments.display or os.environ.get("DISPLAY", "")
        self.window_size: str | None = arguments.window_size
        self.sirio_bin = Path(arguments.sirio_bin) if arguments.sirio_bin else BIN
        self.cpu_ceiling: float | None = getattr(arguments, "cpu_ceiling", None)
        self.focus_thief = Path(getattr(arguments, "focus_thief_bin", None) or FOCUS_THIEF)
        # A command that launches Sirio (`gdb -batch -x sample.py --args`, say).
        self.launcher: list[str] = list(getattr(arguments, "launcher", None) or [])
        self.out.mkdir(parents=True, exist_ok=True)
        self.work = Path(tempfile.mkdtemp(prefix="sirio-ely-e2e-"))
        self.transcript = open(self.out / "transcript.log", "w", buffering=1)
        self.responses = open(self.out / "responses.jsonl", "w", buffering=1)
        self.results: dict[str, str] = {}
        self.frames: list[str] = []
        self.instances: list["Instance"] = []

    def log(self, text: str) -> None:
        print(text, flush=True)
        self.transcript.write(text + "\n")

    def preflight(self) -> None:
        if self.state_only:
            return
        if not self.display:
            raise Failure("no display for the capture lane: pass --display :N, or --state-only")
        for tool in ("import", "identify", "magick", "xwininfo", "xprop"):
            if shutil.which(tool) is None:
                raise Failure(f"{tool} is required for captures and is not on PATH (or use --state-only)")
        probe = subprocess.run(
            ["xprop", "-display", self.display, "-root", "_NET_SUPPORTED"],
            capture_output=True,
            text=True,
            timeout=15,
        )
        if probe.returncode != 0:
            raise Failure(
                f"cannot open display {self.display!r} for the capture lane: "
                f"{probe.stderr.strip() or 'xprop failed'} (use --state-only to skip frames)"
            )

    def window_for(self, pid: int) -> str | None:
        """The largest window whose `_NET_WM_PID` is `pid` -- never another process's."""
        listing = subprocess.run(
            ["xwininfo", "-display", self.display, "-root", "-children"],
            capture_output=True,
            text=True,
            timeout=15,
        ).stdout
        best: tuple[int, str] | None = None
        for line in listing.splitlines():
            found = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
            if not found:
                continue
            window, width, height = found.group(1), int(found.group(2)), int(found.group(3))
            prop = subprocess.run(
                ["xprop", "-display", self.display, "-id", window, "_NET_WM_PID"],
                capture_output=True,
                text=True,
                timeout=15,
            ).stdout
            owner = re.search(r"=\s*(\d+)\s*$", prop)
            if owner and int(owner.group(1)) == pid and (best is None or width * height > best[0]):
                best = (width * height, window)
        return best[1] if best else None

    def resize(self, pid: int) -> None:
        """`--window-size WxH`: the capture lane's narrow or wide pane, via xdotool."""
        if self.state_only or not self.window_size:
            return
        if shutil.which("xdotool") is None:
            raise Failure("xdotool is required for --window-size")
        window = None
        for _ in range(20):
            window = self.window_for(pid)
            if window:
                break
            time.sleep(0.5)
        if not window:
            raise Failure(f"no window with _NET_WM_PID={pid} to resize")
        width, height = self.window_size.lower().split("x")
        env = dict(os.environ, DISPLAY=self.display)
        subprocess.run(["xdotool", "windowsize", window, width, height], env=env, check=True)
        time.sleep(1)

    def capture(
        self, pid: int, destination: Path, settle: float = 1.5, window: str | None = None
    ) -> None:
        """One frame of the pid's window. `settle` is the wait before it; a burst of frames a
        few hundred milliseconds apart passes 0 and the window it found once."""
        if self.state_only:
            return
        if settle:
            time.sleep(settle)
        for _ in range(20):
            window = window or self.window_for(pid)
            if window:
                break
            time.sleep(0.5)
        if not window:
            raise Failure(f"no window with _NET_WM_PID={pid} for {destination.name}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["import", "-display", self.display, "-window", window, str(destination)],
            check=True,
            timeout=30,
        )
        size = destination.stat().st_size if destination.exists() else 0
        colours = int(
            subprocess.run(
                ["identify", "-format", "%k", str(destination)],
                capture_output=True,
                text=True,
                check=True,
                timeout=30,
            ).stdout
        )
        if size == 0 or colours < 200:
            raise Failure(f"{destination.name} is blank ({colours} colours, {size} bytes)")
        self.log(f"FRAME {destination.relative_to(self.out)} ({colours} colours, window {window}, pid {pid})")
        self.frames.append(str(destination.relative_to(self.out)))


class Instance:
    """One isolated Sirio, launched against a seeded scratch database."""

    def __init__(self, run: Run, scenario: "Scenario") -> None:
        self.run = run
        self.scenario = scenario
        self.process: subprocess.Popen | None = None
        self.surface = ""
        self.launches = 0
        run.instances.append(self)

    @property
    def socket(self) -> Path:
        return self.scenario.dir / "c.sock"

    def start(self) -> None:
        scenario = self.scenario
        self.launches += 1
        env = dict(os.environ)
        env.pop("WAYLAND_DISPLAY", None)
        env.update(
            SIRIO_SOCKET=str(self.socket),
            SIRIO_DB=str(scenario.database),
            SIRIO_CREDENTIALS=str(scenario.dir / "credentials.json"),
            PATH=f"{scenario.agent.parent}{os.pathsep}{os.environ.get('PATH', '')}",
            GPUI_X11_SCALE_FACTOR="1",
        )
        env.pop("SIRIO_ACP_PROGRAM", None)
        # The scenario's own variables last: a value of None removes the variable, such as the
        # SIRIO_REDUCED_MOTION that Scripts/bezel-off-captures.sh exports for every source.
        for name, value in scenario.env.items():
            if value is None:
                env.pop(name, None)
            else:
                env[name] = value
        if self.run.state_only:
            env.pop("DISPLAY", None)
        else:
            env["DISPLAY"] = self.run.display
        if self.socket.exists():
            self.socket.unlink()
        log = open(scenario.dir / "app.log", "ab")
        log.write(f"--- launch {self.launches}\n".encode())
        self.process = subprocess.Popen(
            [*self.run.launcher, str(self.run.sirio_bin)],
            cwd=scenario.repo,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT
        )
        deadline = time.time() + 30
        while time.time() < deadline and not self.socket.exists():
            if self.process.poll() is not None:
                raise Failure(f"{scenario.name}: sirio exited ({self.process.returncode}) before opening its socket")
            time.sleep(0.2)
        if not self.socket.exists():
            raise Failure(f"{scenario.name}: sirio never opened its control socket")
        opened = self.wait("the chat surface opens", lambda: self.call("surface.chat.open"), ok=True)
        self.surface = opened["result"]["surfaceId"]
        self.run.resize(self.process.pid)

    def call(self, method: str, **params: str) -> dict:
        reply = request(self.socket, method, dict(params))
        self.run.responses.write(
            json.dumps({"scenario": self.scenario.name, "method": method, "params": params, "reply": reply}) + "\n"
        )
        return reply

    def wait(self, what: str, probe, *, ok: bool | None = None, timeout: float = 40) -> dict:
        deadline = time.time() + timeout
        last: object = None
        while time.time() < deadline:
            try:
                last = probe()
            except (OSError, Failure) as error:
                last = error
            else:
                if ok is None or (isinstance(last, dict) and last.get("ok") is ok):
                    return last
            time.sleep(0.3)
        raise Failure(f"{self.scenario.name}: timed out waiting for {what} (last: {last!r})")

    def read(self) -> dict:
        reply = self.call("surface.chat.read", surfaceId=self.surface)
        if not reply.get("ok"):
            raise Failure(f"{self.scenario.name}: surface.chat.read failed: {reply}")
        result = reply["result"]
        return {
            "status": result["status"],
            "composer": result["composerText"],
            "queued": [row["text"] for row in json.loads(result["queued"])],
            "transcript": json.loads(result["transcript"]),
            # The level the chat is on, empty when the session offers no effort.
            "effort": result.get("effort"),
        }

    def until(self, what: str, predicate, timeout: float = 40, quiet: bool = False) -> dict:
        def probe() -> dict:
            state = self.read()
            if not predicate(state):
                raise Failure("not yet")
            return state

        found = self.wait(what, probe, timeout=timeout)
        if not quiet:
            self.run.log(f"OK {self.scenario.name}: {what}")
        return found

    def send(self, text: str) -> dict:
        # A prompt sent while the agent is still connecting is held back by the
        # chat, not queued: wait for the connection to settle first.
        self.until("the chat has finished connecting", lambda s: s["status"] != "connecting", quiet=True)
        reply = self.call("surface.chat.send", surfaceId=self.surface, text=text)
        if not reply.get("ok"):
            raise Failure(f"{self.scenario.name}: send {text!r} failed: {reply}")
        if reply["result"].get("composerText") == text:
            raise Failure(f"{self.scenario.name}: send {text!r} left the prompt in the composer: {reply['result']}")
        return reply

    def answer(self, request_id: str, option: str) -> dict:
        return self.call(
            "surface.chat.permission", surfaceId=self.surface, requestId=request_id, optionId=option
        )

    def frame(self, name: str) -> None:
        if self.process is None:
            raise Failure("no process to capture")
        self.run.capture(self.process.pid, self.scenario.out / "frames" / f"{name}.png")

    def quit(self) -> int | None:
        if self.process is None:
            return None
        try:
            self.call("system.quit")
        except (OSError, Failure):
            pass
        try:
            self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        status = self.process.returncode
        (self.scenario.dir / "exit-status").write_text(f"{status}\n")
        self.process = None
        return status


class Scenario:
    def __init__(
        self,
        run: Run,
        name: str,
        mode: str,
        *,
        appearance: str = "dark",
        ui_size: int = 13,
        env: dict[str, str | None] | None = None,
    ) -> None:
        self.run = run
        self.name = name
        # Merged into the instance's environment; None unsets a variable.
        self.env: dict[str, str | None] = dict(env or {})
        self.dir = run.work / name
        self.dir.mkdir(parents=True)
        # Where this scenario's artifacts end up; frames are captured straight into it.
        self.out = run.out / "scenarios" / name
        self.out.mkdir(parents=True, exist_ok=True)
        self.repo = self.dir / "repo"
        self.database = self.dir / "session.sqlite"
        self.agent_dir = self.dir / "agent"
        self.agent_dir.mkdir()
        # The stand-in for the adapter's binary (`opencode acp`).
        (self.dir / "bin").mkdir()
        self.agent = self.dir / "bin" / "opencode"
        self.traffic = self.dir / "agent-traffic.jsonl"
        self.instance = Instance(run, self)
        self._seed(appearance, ui_size)
        self.agent.write_text(
            "#!/bin/sh\n"
            f'exec "{sys.executable}" "{Path(__file__).resolve()}" --agent-proxy '
            f'"{mode}" "{self.agent_dir}" "{self.traffic}"\n'
        )
        self.agent.chmod(self.agent.stat().st_mode | stat.S_IXUSR)

    def _seed(self, appearance: str, ui_size: int) -> None:
        self.repo.mkdir()
        for command in (
            ["git", "init", "-q", "-b", "main"],
            ["git", "config", "user.email", "t@example.com"],
            ["git", "config", "user.name", "Tester"],
            ["git", "commit", "-q", "--allow-empty", "-m", "first"],
        ):
            subprocess.run(command, cwd=self.repo, check=True)
        seeded = subprocess.run(
            [
                str(SEED),
                "--database", str(self.database),
                "--worktree", str(self.repo),
                "--appearance", appearance,
                "--ui-size", str(ui_size),
                "--agent", "opencode",
            ],
            capture_output=True,
            text=True,
        )
        if seeded.returncode != 0:
            raise Failure(f"{self.name}: ely_chat_seed failed: {seeded.stderr.strip()}")

    def relaunch(self) -> None:
        status = self.instance.quit()
        if status != 0:
            raise Failure(f"{self.name}: sirio quit with status {status}")
        self.instance.start()

    def finish(self) -> None:
        status = self.instance.quit()
        if status != 0:
            raise Failure(f"{self.name}: sirio exited with status {status}")
        for artifact in ("app.log", "agent-traffic.jsonl", "agent-traffic.jsonl.stderr", "exit-status"):
            if (self.dir / artifact).exists():
                shutil.copy(self.dir / artifact, self.out / artifact)
        shutil.copy(self.database, self.out / "session.sqlite")


def rows(state: dict, kind: str) -> list[dict]:
    return [row for row in state["transcript"] if row.get("kind") == kind]


def texts(state: dict, kind: str) -> list[str]:
    return [row.get("text", "") for row in rows(state, kind)]


def pending(state: dict) -> dict | None:
    found = [row for row in rows(state, "permission") if row.get("status") == "pending"]
    return found[0] if found else None


# --- the scenarios -----------------------------------------------------------


def staged_queue(run: Run) -> None:
    scenario = Scenario(run, "staged-queue", "staged")
    chat = scenario.instance
    chat.start()
    chat.send("first")
    chat.until("the first turn streams", lambda s: s["status"] == "streaming" and "first " in "".join(texts(s, "assistant")))
    chat.send("second")
    chat.send("third")
    state = chat.until("two prompts are queued, in order", lambda s: s["queued"] == ["second", "third"])
    chat.frame("streaming-queued")
    (scenario.agent_dir / "go").write_text("go")
    state = chat.until(
        "the queue drains, each prompt exactly once, in order",
        lambda s: s["status"] == "completed" and not s["queued"] and len(rows(s, "user")) == 3,
    )
    if texts(state, "user") != ["first", "second", "third"]:
        raise Failure(f"staged-queue: prompts reached the transcript as {texts(state, 'user')}")
    expected = "first streamed" + ("!" if os.environ.get("ELY_E2E_CORRUPT") else "")
    if expected not in "".join(texts(state, "assistant")):
        raise Failure(f"staged-queue: the first reply is not {expected!r}: {texts(state, 'assistant')}")
    chat.frame("queue-drained")
    # An absent surface is a failed reply, not an empty one.
    absent = chat.call("surface.chat.read", surfaceId="no-such-surface")
    if absent.get("ok") or not absent.get("error"):
        raise Failure(f"staged-queue: an absent surface did not fail: {absent}")
    run.log("OK staged-queue: an absent surface fails with: " + absent["error"])
    scenario.finish()


def permission(run: Run) -> None:
    scenario = Scenario(run, "permission", "permission")
    chat = scenario.instance
    chat.start()
    chat.send("write the nonce")
    state = chat.until("a permission request is pending", lambda s: pending(s) is not None)
    request_id = pending(state)["id"]
    chat.frame("permission-pending")
    wrong = chat.answer("999999", "allow")
    if wrong.get("ok"):
        raise Failure(f"permission: an unknown request id was accepted: {wrong}")
    chat.answer(request_id, "allow")
    chat.until(
        "the request is answered and the turn completes",
        lambda s: s["status"] == "completed"
        and any(row.get("status") == "selected" for row in rows(s, "permission")),
    )
    chat.frame("permission-answered")
    scenario.finish()


def question(run: Run) -> None:
    scenario = Scenario(run, "question-options", "question-options")
    chat = scenario.instance
    chat.start()
    chat.send("ask me")
    state = chat.until("a question is pending", lambda s: pending(s) is not None)
    chat.frame("question-pending")
    chat.answer(pending(state)["id"], "green")
    state = chat.until("the answer is echoed", lambda s: "You picked: green" in "".join(texts(s, "assistant")))
    chat.frame("question-answered")
    scenario.finish()


def pending_plan(state: dict) -> dict | None:
    found = [row for row in rows(state, "plan") if row.get("status") == "pending"]
    return found[0] if found else None


def plan(run: Run) -> None:
    scenario = Scenario(run, "plan", "plan")
    chat = scenario.instance
    chat.start()
    chat.send("plan it")
    state = chat.until("the plan awaits approval", lambda s: pending_plan(s) is not None)
    chat.frame("plan-pending")
    chat.answer(pending_plan(state)["id"], "approve")
    state = chat.until(
        "the plan advances after approval",
        lambda s: s["status"] == "completed" and "approved" in "".join(texts(s, "assistant")),
    )
    if not any("completed · Read the design" in text for text in texts(state, "plan")):
        raise Failure(f"plan: the plan did not advance: {texts(state, 'plan')}")
    chat.frame("plan-approved")
    scenario.finish()


def cancel(run: Run) -> None:
    scenario = Scenario(run, "cancel", "cancel")
    chat = scenario.instance
    chat.start()
    chat.send("go on")
    chat.until("a partial reply streams", lambda s: s["status"] == "streaming" and "partial" in "".join(texts(s, "assistant")))
    chat.frame("streaming-partial")
    stop = chat.call("surface.chat.stop", surfaceId=chat.surface)
    if not stop.get("ok"):
        raise Failure(f"cancel: stop failed: {stop}")
    state = chat.until("the turn stops", lambda s: s["status"] != "streaming")
    if "partial" not in "".join(texts(state, "assistant")):
        raise Failure("cancel: stopping erased the partial reply")
    chat.send("again")
    chat.until("a later prompt completes normally", lambda s: s["status"] == "completed" and "done" in "".join(texts(s, "assistant")))
    chat.frame("after-cancel")
    scenario.finish()


def death_recovery(run: Run) -> None:
    scenario = Scenario(run, "death-recovery", "death-then-ok")
    chat = scenario.instance
    chat.start()
    chat.send("hello")
    chat.until("the agent dies and the chat says so", lambda s: bool(rows(s, "error")))
    chat.frame("agent-died")
    scenario.relaunch()
    chat.send("hello again")
    chat.until("after a relaunch the same agent answers", lambda s: "alive" in "".join(texts(s, "assistant")))
    chat.frame("recovered")
    scenario.finish()


def authentication(run: Run) -> None:
    scenario = Scenario(run, "auth-required", "auth-required")
    chat = scenario.instance
    chat.start()
    state = chat.until("the chat names the authentication it needs", lambda s: any("uthentic" in t for t in texts(s, "error")))
    chat.frame("auth-required")
    scenario.finish()


def restore(run: Run) -> None:
    scenario = Scenario(run, "restore", "plain", appearance="light", ui_size=15)
    chat = scenario.instance
    chat.start()
    chat.send("remember me")
    chat.until("the turn completes", lambda s: s["status"] == "completed" and "reply" in "".join(texts(s, "assistant")))
    chat.frame("before-restart")
    scenario.relaunch()
    state = chat.until("the transcript is restored", lambda s: "remember me" in texts(s, "user"))
    if state["status"] != "completed":
        raise Failure(f"restore: a restored chat reads {state['status']!r}, not completed")
    chat.frame("restored")
    scenario.finish()


def identities(run: Run) -> None:
    """The five agents' marks, as native views with explicit identities."""
    if run.state_only:
        run.log("SKIP identities: --state-only omits frames")
        return
    if shutil.which("xdotool") is None:
        raise Failure("xdotool is required for the identity lane (or pass --no-identity)")
    if not PROBE.exists():
        raise Failure(f"{PROBE} is missing: cargo build -p sirio_ui --example ely_chat_probe")
    for agent in AGENTS:
        for appearance in ("dark", "light"):
            env = dict(os.environ)
            env.pop("WAYLAND_DISPLAY", None)
            env.update(
                DISPLAY=run.display,
                ELY_PROBE_CHAT="1",
                ELY_PROBE_AGENT=agent,
                ELY_PROBE_APPEARANCE=appearance,
                ELY_PROBE_FIXTURE_MODE="plain",
            )
            log = open(run.work / f"identity-{agent}-{appearance}.log", "wb")
            process = subprocess.Popen([str(PROBE)], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                window = None
                for _ in range(40):
                    window = run.window_for(process.pid)
                    if window:
                        break
                    time.sleep(0.5)
                if not window:
                    raise Failure(f"identity {agent}/{appearance}: no window for pid {process.pid}")
                time.sleep(2)
                geometry = subprocess.run(
                    ["xwininfo", "-display", run.display, "-id", window], capture_output=True, text=True
                ).stdout
                height = int(re.search(r"Height:\s*(\d+)", geometry).group(1))
                click = ["xdotool", "mousemove", "--window", window, "300", str(height - 70), "click", "1"]
                subprocess.run(click, env=env, check=True)
                subprocess.run(["xdotool", "type", "--window", window, "hello"], env=env, check=True)
                subprocess.run(["xdotool", "key", "--window", window, "Return"], env=env, check=True)
                run.capture(process.pid, run.out / "identity" / f"{agent}-{appearance}.png")
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
            shutil.copy(run.work / f"identity-{agent}-{appearance}.log", run.out / "identity" / f"{agent}-{appearance}.log")


def activity(run: Run) -> None:
    scenario = Scenario(run, "activity", "ely-activity")
    chat = scenario.instance
    chat.start()
    chat.send("inspect rich tools")
    state = chat.until("the turn completes", lambda s: s["status"] == "completed" and rows(s, "tool"))
    statuses = {row["id"]: row["status"] for row in rows(state, "tool")}
    wanted = {"shell": "Completed", "read-failed": "Failed", "read-running": "InProgress"}
    for tool, status in wanted.items():
        if statuses.get(tool) != status:
            raise Failure(f"activity: tool {tool!r} reads {statuses.get(tool)!r}, not {status!r}: {statuses}")
    if not texts(state, "thought"):
        raise Failure("activity: the reasoning never reached the transcript")
    chat.frame("tools-and-reasoning")
    scenario.finish()


def wide_content(run: Run) -> None:
    """Content wider than the column: a diff row, a code fence and a table.

    A tool row is collapsed until clicked, and no control verb expands one, so
    the diff is drawn only after a real click on the row's header; the state
    read's `expanded` says which click did it.
    """
    scenario = Scenario(run, "wide-content", "ely-wide")
    chat = scenario.instance
    chat.start()
    chat.send("show wide content")
    state = chat.until(
        "the turn completes with the edit and the answer",
        lambda s: s["status"] == "completed" and rows(s, "tool") and texts(s, "assistant"),
    )
    answer = "".join(texts(state, "assistant"))
    if "```json" not in answer or "column-8" not in answer:
        raise Failure(f"wide-content: the answer lacks the fence or the table: {answer[:200]!r}")
    chat.frame("wide-content")
    if run.state_only:
        scenario.finish()
        return
    if shutil.which("xdotool") is None:
        raise Failure("xdotool is required to open the edit's diff (or use --state-only)")
    # Where the row lands depends on the answer's height, so walk a column
    # down the transcript's upper half and stop at the click the state read
    # reports as having opened it. A click elsewhere selects text or nothing.
    window = run.window_for(chat.process.pid)
    geometry = subprocess.run(
        ["xwininfo", "-display", run.display, "-id", window], capture_output=True, text=True
    ).stdout
    width = int(re.search(r"Width:\s*(\d+)", geometry).group(1))
    height = int(re.search(r"Height:\s*(\d+)", geometry).group(1))
    env = dict(os.environ, DISPLAY=run.display)
    expanded = lambda s: any(row.get("id") == "wide-edit" and row.get("expanded") == "true" for row in rows(s, "tool"))
    for y in range(int(height * 0.08), int(height * 0.5), 10):
        subprocess.run(
            ["xdotool", "mousemove", "--window", window, str(int(width * 0.45)), str(y), "click", "1"],
            env=env,
            check=True,
        )
        time.sleep(0.3)
        if expanded(chat.read()):
            run.log(f"wide-content: the edit row opened at y {y}")
            break
    else:
        raise Failure("wide-content: no click on the transcript opened the edit's row")
    time.sleep(1)
    chat.frame("wide-content-expanded")
    # The transcript keeps its tail in view, so the opened diff is above it.
    scroll_pane(run, chat.process.pid, 6, "4")
    chat.frame("wide-content-diff")
    scenario.finish()


def subagent(run: Run) -> None:
    scenario = Scenario(run, "subagent", "subagent")
    chat = scenario.instance
    chat.start()
    chat.send("delegate")
    # The nested read lives inside the delegating task's own card, so the
    # transcript carries the one task row.
    state = chat.until(
        "the delegated turn completes",
        lambda s: s["status"] == "completed"
        and any(row.get("id") == "task-1" and row.get("status") == "Completed" for row in rows(s, "tool")),
    )
    chat.frame("subagent")
    scenario.finish()


def scroll_pane(run: Run, pid: int, clicks: int, button: str) -> None:
    """The wheel over the chat pane's transcript, through xdotool."""
    window = run.window_for(pid)
    geometry = subprocess.run(
        ["xwininfo", "-display", run.display, "-id", window], capture_output=True, text=True
    ).stdout
    width = int(re.search(r"Width:\s*(\d+)", geometry).group(1))
    height = int(re.search(r"Height:\s*(\d+)", geometry).group(1))
    env = dict(os.environ, DISPLAY=run.display)
    point = [str(int(width * 0.33)), str(int(height * 0.35))]
    subprocess.run(["xdotool", "mousemove", "--window", window, *point], env=env, check=True)
    subprocess.run(
        ["xdotool", "click", "--repeat", str(clicks), "--delay", "25", button], env=env, check=True
    )
    time.sleep(1)


def pane_difference(run: Run, before: Path, after: Path) -> int:
    """Differing pixels between two frames of the transcript area (the rail and scrollbar left out)."""
    width, height = (int(v) for v in subprocess.run(
        ["identify", "-format", "%w %h", str(before)], capture_output=True, text=True, check=True
    ).stdout.split())
    crop = f"{int(width * 0.25)}x{int(height * 0.40)}+{int(width * 0.195)}+{int(height * 0.12)}"
    cuts = []
    for index, frame in enumerate((before, after)):
        cut = run.work / f"pane-{index}.png"
        subprocess.run(["magick", str(frame), "-crop", crop, "+repage", str(cut)], check=True)
        cuts.append(cut)
    compared = subprocess.run(
        ["compare", "-metric", "AE", "-fuzz", "2%", *map(str, cuts), "null:"], capture_output=True, text=True
    )
    return int(float(compared.stderr.strip().split()[0]))


def long_history(run: Run) -> None:
    scenario = Scenario(run, "long-history", "plain")
    chat = scenario.instance
    chat.start()
    turns = 60
    for number in range(1, turns + 1):
        chat.send(f"question {number}")
        chat.until(
            f"turn {number} completes",
            lambda s, number=number: s["status"] == "completed" and len(rows(s, "user")) == number,
            timeout=30,
            quiet=True,
        )
    state = chat.read()
    if texts(state, "user") != [f"question {n}" for n in range(1, turns + 1)]:
        raise Failure("long-history: the prompts were not kept in order")
    chat.frame("tail")
    if run.state_only:
        scenario.finish()
        return
    if shutil.which("xdotool") is None:
        raise Failure("xdotool is required for the scroll check (or use --state-only)")
    pid = chat.process.pid
    scroll_pane(run, pid, 30, "4")
    chat.frame("scrolled-up")
    # A wheel that moved nothing would make the anchoring check below vacuous.
    scrolled = pane_difference(
        run, scenario.out / "frames" / "tail.png", scenario.out / "frames" / "scrolled-up.png"
    )
    run.log(f"long-history: scrolling back changed {scrolled} pixels of the transcript")
    if scrolled < 500:
        raise Failure(f"long-history: the wheel did not scroll the transcript ({scrolled} pixels changed)")
    chat.send("one more question")
    chat.until("the new turn completes", lambda s: s["status"] == "completed" and len(rows(s, "user")) == turns + 1)
    chat.frame("scrolled-up-after-new-turn")
    moved = pane_difference(
        run,
        scenario.out / "frames" / "scrolled-up.png",
        scenario.out / "frames" / "scrolled-up-after-new-turn.png",
    )
    run.log(f"long-history: {moved} pixels of the transcript changed while a turn arrived below")
    if moved > 4000:
        raise Failure(f"long-history: the scrolled-back view jumped when a turn arrived ({moved} pixels changed)")
    scroll_pane(run, pid, 80, "5")
    chat.frame("back-at-the-tail")
    scenario.finish()


def effort_act(chat: Instance, action: str, value: str | None = None) -> None:
    """`surface.chat.effort`: what the effort chip's list does, by the same path as a click."""
    params = {"surfaceId": chat.surface, "action": action}
    if value is not None:
        params["value"] = value
    reply = chat.call("surface.chat.effort", **params)
    if not reply.get("ok"):
        raise Failure(f"{chat.scenario.name}: surface.chat.effort {action} {value or ''} failed: {reply}")


def effort_requests(scenario: Scenario) -> list[str]:
    """The levels the app asked the agent for, in order.

    Read from the harness's record of what reached the fixture: the client-to-agent
    `session/set_config_option` requests whose `configId` is `effort`.
    """
    asked: list[str] = []
    if not scenario.traffic.exists():
        return asked
    for raw in scenario.traffic.read_text().splitlines():
        try:
            record = json.loads(raw)
            message = json.loads(record["line"])
        except (ValueError, KeyError, TypeError):
            continue
        if record.get("dir") != "to-agent" or message.get("method") != "session/set_config_option":
            continue
        params = message.get("params") or {}
        if params.get("configId") == "effort":
            asked.append(params.get("value"))
    return asked


def effort_asked(chat: Instance, what: str, enough) -> list[str]:
    """The effort requests once `enough` accepts them: a chip moves before its request is written."""

    def probe() -> list[str]:
        asked = effort_requests(chat.scenario)
        if not enough(asked):
            raise Failure(f"asked so far: {asked}")
        return asked

    return chat.wait(what, probe, timeout=15)


def effort_steps(run: Run, name: str, appearance: str) -> None:
    """A chat asks for High, shows the effort list, takes a pick, and asks for High again once restarted."""
    scenario = Scenario(run, name, "composer", appearance=appearance)
    chat = scenario.instance
    chat.start()
    chat.send("hi")
    chat.until("the turn completes on High", lambda s: s["status"] == "completed" and s["effort"] == "high")
    asked = effort_asked(chat, "the agent is asked for effort high", lambda asked: "high" in asked)
    run.log(f"OK {name}: the agent was asked for effort high (asked: {asked})")
    effort_act(chat, "open")
    chat.frame("effort-list")
    effort_act(chat, "choose", "low")
    chat.until("the chat is on Low", lambda s: s["effort"] == "low")
    asked = effort_asked(chat, "choosing Low reaches the agent", lambda asked: asked[-1:] == ["low"])
    run.log(f"OK {name}: choosing Low reached the agent (asked: {asked})")
    chat.frame("effort-low")
    scenario.relaunch()
    chat.until("a restored chat is back on High", lambda s: s["effort"] == "high")
    restarted = effort_asked(
        chat,
        "the restarted agent is asked for High again",
        lambda now: len(now) > len(asked) and now[-1] == "high",
    )
    run.log(f"OK {name}: the restarted agent was asked for High again (asked: {restarted})")
    scenario.finish()


def effort_full_steps(run: Run, name: str, appearance: str) -> None:
    """The whole ladder an agent can offer (Default, five levels, Ultracode) drawn, and Default chosen from it."""
    scenario = Scenario(run, name, "effort-full", appearance=appearance)
    chat = scenario.instance
    chat.start()
    chat.send("hi")
    chat.until("the turn completes on High", lambda s: s["status"] == "completed" and s["effort"] == "high")
    asked = effort_asked(chat, "the agent is asked for effort high", lambda asked: "high" in asked)
    run.log(f"OK {name}: the agent was asked for effort high (asked: {asked})")
    effort_act(chat, "open")
    chat.frame("effort-full-list")
    effort_act(chat, "choose", "default")
    chat.until("the chat is on Default", lambda s: s["effort"] == "default")
    asked = effort_asked(chat, "choosing Default reaches the agent", lambda asked: asked[-1:] == ["default"])
    run.log(f"OK {name}: choosing Default reached the agent (asked: {asked})")
    # Open again: the list marks the level in force, here the row below the separator.
    effort_act(chat, "open")
    chat.frame("effort-full-default")
    scenario.finish()


def effort(run: Run) -> None:
    effort_steps(run, "effort", "dark")


def effort_light(run: Run) -> None:
    effort_steps(run, "effort-light", "light")


def effort_full(run: Run) -> None:
    effort_full_steps(run, "effort-full", "dark")


def effort_full_light(run: Run) -> None:
    effort_full_steps(run, "effort-full-light", "light")


# --- real X input, and the scenarios that need it ----------------------------------


# Fractions of a frame (x, y, width, height). The transcript column from the top of the list down
# to just above the composer card, with the thinking orb at its foot. The thinking row carries no
# elapsed-seconds label in this build (none appeared in 15 s of a turn), so nothing in the region
# needs masking; a build that draws one must mask it here, or `reduced_motion` breaks every time a
# second ticks over.
TRANSCRIPT_REGION = (0.228, 0.12, 0.52, 0.635)
# The sidebar row's status bloom, the ring glyph that stands where the row's status word was, in
# pixels of the default 1470x833 window (the sidebar is a fixed-width column, so it does not move
# with the window's size): the 14 px glyph centred at (265, 199), with a margin. `unfocused_spinner`
# judges this and not the transcript, see there.
SIDEBAR_BLOOM = "32x30+250+184"
# The slash popup's box: the three command rows hung above the composer card, which starts below.
POPUP_REGION = (0.231, 0.606, 0.204, 0.148)
# Where a click lands in the composer card, in window pixels from the bottom edge and as a
# fraction of the width: the card's text area, in the chat column.
COMPOSER_CLICK = (0.487, 150)


def crop_difference(run: Run, before: Path, after: Path, crop: str, tolerance: int = 0) -> int:
    """Pixels inside `crop` (an ImageMagick `WxH+X+Y`) whose largest channel differs by more than
    `tolerance` levels between two frames. Counted from a difference image, not from `compare`:
    its AE comes back fractional and counts a faint fill (the popup's, 7 levels off the page's)
    as a few text pixels. `Scripts/capture-diff.py` has the same rule."""
    cuts = []
    for index, frame in enumerate((before, after)):
        cut = run.work / f"region-{index}.png"
        subprocess.run(["magick", str(frame), "-crop", crop, "+repage", str(cut)], check=True)
        cuts.append(cut)
    # A level is 100/255 of a percent; half a level above the tolerance rounds to "more than".
    threshold = (tolerance + 0.5) / 255 * 100
    counted = subprocess.run(
        [
            "magick", *map(str, cuts), "-compose", "Difference", "-composite", "-channel", "RGB",
            "-separate", "-evaluate-sequence", "Max", "-threshold", f"{threshold}%",
            "-format", "%[fx:round(mean*w*h)]", "info:",
        ],
        capture_output=True, text=True, check=True,
    )
    return int(counted.stdout.strip())


def region_difference(run: Run, before: Path, after: Path, region: tuple[float, float, float, float], tolerance: int = 0) -> int:
    """`crop_difference` over `region`, as fractions of the frame (x, y, width, height)."""
    width, height = (int(v) for v in subprocess.run(
        ["identify", "-format", "%w %h", str(before)], capture_output=True, text=True, check=True
    ).stdout.split())
    x, y, w, h = region
    crop = f"{int(width * w)}x{int(height * h)}+{int(width * x)}+{int(height * y)}"
    return crop_difference(run, before, after, crop, tolerance)


def needs_display(run: Run, name: str) -> bool:
    """Whether the scenario has to be skipped: it needs a display and xdotool."""
    if run.state_only:
        run.log(f"SKIP {name}: needs a display")
        return True
    if shutil.which("xdotool") is None:
        raise Failure(f"xdotool is required for {name} (or use --state-only)")
    return False


def x_run(run: Run, *arguments: str) -> str:
    return subprocess.run(
        ["xdotool", *arguments],
        env=dict(os.environ, DISPLAY=run.display),
        capture_output=True,
        text=True,
        check=True,
        timeout=30,
    ).stdout.strip()


def x_focus(run: Run, window: str) -> None:
    """Give the window the X input focus, as a window manager would on a click on it."""
    x_run(run, "windowfocus", "--sync", window)


def x_focused(run: Run) -> int:
    """The window holding the X input focus: xdotool prints it in decimal, xwininfo in hex."""
    return int(x_run(run, "getwindowfocus"))


def x_type(run: Run, text: str) -> None:
    x_run(run, "type", "--delay", "40", "--clearmodifiers", text)


def x_key(run: Run, *keys: str) -> None:
    x_run(run, "key", "--clearmodifiers", *keys)


def sirio_window(run: Run, chat: Instance) -> str:
    window = run.window_for(chat.process.pid)
    if not window:
        raise Failure(f"{chat.scenario.name}: no window for pid {chat.process.pid}")
    return window


def click_composer(run: Run, window: str) -> None:
    geometry = subprocess.run(
        ["xwininfo", "-display", run.display, "-id", window], capture_output=True, text=True
    ).stdout
    width = int(re.search(r"Width:\s*(\d+)", geometry).group(1))
    height = int(re.search(r"Height:\s*(\d+)", geometry).group(1))
    x_run(
        run, "mousemove", "--window", window, str(int(width * COMPOSER_CLICK[0])),
        str(height - COMPOSER_CLICK[1]), "click", "1",
    )


def burst(run: Run, chat: Instance, prefix: str, count: int = 3, gap: float = 0.25) -> list[Path]:
    """`count` frames of the chat's window about `gap` seconds apart (the capture itself takes a
    little too): the window is found once and nothing waits before a frame."""
    window = sirio_window(run, chat)
    frames = []
    for number in range(1, count + 1):
        if number > 1:
            time.sleep(gap)
        destination = chat.scenario.out / "frames" / f"{prefix}-{number}.png"
        run.capture(chat.process.pid, destination, settle=0, window=window)
        frames.append(destination)
    return frames


def composer_keys(run: Run) -> str | None:
    """The composer's keys, typed through X as a hand would: the gpui tests drive the keymap, this
    drives the whole path from the server's key event to the draft.

    Its fixture is `composer`, not `plain`: only the modes that advertise a session's configuration
    (`COMPOSER_LIKE` in chat_fixture.py) also advertise slash commands, and `plain` has none, so the
    slash popup the last two steps need could never open there. `composer` answers a prompt the same
    way (`reply `).
    """
    if needs_display(run, "composer_keys"):
        return "skipped"
    scenario = Scenario(run, "composer-keys", "composer")
    chat = scenario.instance
    chat.start()
    chat.until("the chat has finished connecting", lambda s: s["status"] != "connecting", quiet=True)
    window = sirio_window(run, chat)
    # No window manager here: nothing hands the keys to Sirio but this.
    x_focus(run, window)
    if x_focused(run) != int(window, 16):
        raise Failure(f"composer_keys: Sirio's window {window} did not take the X focus")
    click_composer(run, window)

    def clear_draft() -> None:
        x_key(run, "ctrl+a")
        x_key(run, "BackSpace")
        chat.until("the draft is empty", lambda s: s["composer"] == "", timeout=10, quiet=True)

    # 1. Return sends.
    x_type(run, "first")
    chat.until("the composer holds what was typed", lambda s: s["composer"] == "first", timeout=10)
    x_key(run, "Return")
    chat.until(
        "Return sent it: the user rows end with 'first' and the composer is empty",
        lambda s: texts(s, "user")[-1:] == ["first"] and s["composer"] == "",
        timeout=15,
    )
    chat.until("the turn completes", lambda s: s["status"] == "completed")

    # 2. Shift+Return is a newline, not a send; with no popup open Up and Down move the caret.
    x_type(run, "a")
    x_key(run, "shift+Return")
    x_type(run, "b")
    state = chat.until("Shift+Return made a newline in the draft", lambda s: s["composer"] == "a\nb", timeout=10)
    if texts(state, "user") != ["first"] or state["status"] != "completed":
        raise Failure(f"composer_keys: Shift+Return sent something: {texts(state, 'user')} / {state['status']}")
    x_key(run, "Up")
    x_type(run, "x")
    x_key(run, "Down")
    x_type(run, "y")
    state = chat.until("Up and Down moved the caret, so the draft reads 'ax\\nby'", lambda s: s["composer"] == "ax\nby", timeout=10)
    if texts(state, "user") != ["first"]:
        raise Failure(f"composer_keys: moving the caret sent something: {texts(state, 'user')}")

    # 3. The slash popup: Down takes the second row, Tab accepts it. The frame before the `/` and
    # the one with the popup open differ in the popup's box (a popup that never drew would make
    # step 4's comparison empty).
    clear_draft()
    chat.frame("before-slash")
    x_type(run, "/")
    chat.until("the draft is '/'", lambda s: s["composer"] == "/", timeout=10)
    chat.frame("slash-popup")
    frames = scenario.out / "frames"
    opened = region_difference(run, frames / "before-slash.png", frames / "slash-popup.png", POPUP_REGION)
    run.log(f"composer_keys: the popup's box changed by {opened} pixels when '/' was typed")
    if opened < 5000:
        raise Failure(f"composer_keys: the slash popup did not draw ({opened} pixels of its box changed)")
    x_key(run, "Down")
    x_key(run, "Tab")
    chat.until("Down and Tab took the second row", lambda s: s["composer"] == "/create-plan ", timeout=10)

    # 4. Escape closes the popup without sending anything: its box is back to what it was before
    # the `/`.
    clear_draft()
    x_type(run, "/")
    chat.until("the draft is '/' again", lambda s: s["composer"] == "/", timeout=10, quiet=True)
    # The popup has to be open again before Escape closes it: otherwise the comparison below would
    # pass on a popup that never reopened.
    chat.frame("slash-reopened")
    reopened = region_difference(run, frames / "before-slash.png", frames / "slash-reopened.png", POPUP_REGION)
    run.log(f"composer_keys: the popup's box changed by {reopened} pixels when '/' was typed again")
    if reopened < 5000:
        raise Failure(f"composer_keys: the slash popup did not reopen ({reopened} pixels of its box changed)")
    x_key(run, "Escape")
    state = chat.until("Escape left the draft alone", lambda s: s["composer"] == "/", timeout=10)
    chat.frame("slash-dismissed")
    gone = region_difference(run, frames / "before-slash.png", frames / "slash-dismissed.png", POPUP_REGION)
    run.log(f"composer_keys: after Escape the popup's box differs from before the '/' by {gone} pixels")
    if gone != 0:
        raise Failure(f"composer_keys: Escape left the slash popup drawn ({gone} pixels of its box differ)")
    state = chat.read()
    if texts(state, "user") != ["first"] or state["status"] != "completed":
        raise Failure(f"composer_keys: Escape sent or stopped something: {texts(state, 'user')} / {state['status']}")
    scenario.finish()
    return None


def open_turn(run: Run, scenario: Scenario) -> Instance:
    """A chat whose turn stays open: the `cancel` fixture streams `partial ` and then waits."""
    chat = scenario.instance
    chat.start()
    chat.send("go on")
    chat.until("a partial reply streams", lambda s: s["status"] == "streaming" and "partial" in "".join(texts(s, "assistant")))
    return chat


def reduced_motion(run: Run) -> str | None:
    """With SIRIO_REDUCED_MOTION=1 an open turn draws nothing that moves: three frames a quarter of
    a second apart are pixel-identical in the transcript region, thinking orb and all."""
    if needs_display(run, "reduced_motion"):
        return "skipped"
    scenario = Scenario(run, "reduced-motion", "cancel", env={"SIRIO_REDUCED_MOTION": "1"})
    chat = open_turn(run, scenario)
    time.sleep(1.5)
    frames = burst(run, chat, "still")
    state = chat.read()
    if state["status"] != "streaming":
        raise Failure(f"reduced_motion: the turn did not stay open ({state['status']})")
    differences = [
        region_difference(run, frames[a], frames[b], TRANSCRIPT_REGION)
        for a, b in ((0, 1), (1, 2), (0, 2))
    ]
    run.log(f"reduced_motion: transcript pixels differing between frames 1-2, 2-3, 1-3: {differences}")
    if any(differences):
        raise Failure(f"reduced_motion: the transcript moved with reduced motion on ({differences} pixels)")
    chat.call("surface.chat.stop", surfaceId=chat.surface)
    chat.until("the turn stops", lambda s: s["status"] != "streaming", quiet=True)
    scenario.finish()
    return None


def cpu_ticks(pid: int) -> int:
    """utime + stime of the process, in clock ticks (`proc(5)`: fields 14 and 15)."""
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return int(fields[11]) + int(fields[12])


def unfocused_spinner(run: Run) -> str | None:
    """A running turn's status bloom in the sidebar keeps turning while another window holds the
    X input focus: `sirio` turns bezel's pause-when-inactive off. A second window (the surfaces
    probe) takes the focus; frames and CPU are sampled while it holds it.

    The frames are judged on the sidebar row's bloom and NOT on the transcript. The transcript's
    thinking row carries Ely's `ShimmerText`, a gpui animation that asks for its own frames and
    never reads the pause, and every repaint of the chat view samples the orb's live phase, so
    the transcript moves whether or not the pause is on (it did, with the pause turned on: the
    check could not fail). The row's bloom is drawn through bezel's clock alone, a `Painter`
    lease taken by the row's own cached view: gpui replays a cached view until it is notified,
    so nothing but that lease repaints it. Leases are refused while no window of the app is
    active and the pause is on (`bezel-motion`, `lease`), so the region freezes exactly then.
    The tab strip's working mark is not such a region: it keeps moving with the pause on.

    SIRIO_REDUCED_MOTION is removed from the environment: the capture script exports it for every
    source, and it would hold the bloom still.
    """
    if needs_display(run, "unfocused_spinner"):
        return "skipped"
    thief_bin = run.focus_thief
    if not thief_bin.exists():
        raise Failure(f"{thief_bin} is missing: cargo build -p sirio_ui --example surfaces_probe (or --focus-thief-bin)")
    scenario = Scenario(run, "unfocused-spinner", "cancel", env={"SIRIO_REDUCED_MOTION": None})
    chat = scenario.instance
    chat.start()
    chat.until("the chat has finished connecting", lambda s: s["status"] != "connecting", quiet=True)
    window = sirio_window(run, chat)
    x_focus(run, window)
    before_thief = x_focused(run)
    run.log(f"unfocused_spinner: before the second window, getwindowfocus is {before_thief} (Sirio's window {window} is {int(window, 16)})")
    if before_thief != int(window, 16):
        raise Failure("unfocused_spinner: Sirio's window did not take the X focus")
    # A click in the composer and a prompt typed there: the composer keeps the focus, so its caret
    # blink is part of what the CPU figure samples.
    click_composer(run, window)
    x_type(run, "go on")
    chat.until("the composer holds the prompt", lambda s: s["composer"] == "go on", timeout=10, quiet=True)
    x_key(run, "Return")
    chat.until("a partial reply streams", lambda s: s["status"] == "streaming" and "partial" in "".join(texts(s, "assistant")))

    # The second window: an absolute path and no PATH, so its pid is the probe's own.
    thief_env = {name: value for name, value in os.environ.items() if name not in ("PATH", "WAYLAND_DISPLAY")}
    thief_env["DISPLAY"] = run.display
    thief_log = open(scenario.dir / "focus-thief.log", "wb")
    thief = subprocess.Popen(
        [str(thief_bin.resolve()), "--frame", "tooltip"], env=thief_env, stdout=thief_log, stderr=subprocess.STDOUT
    )
    try:
        thief_window = None
        for _ in range(60):
            if thief.poll() is not None:
                raise Failure(f"unfocused_spinner: the second window exited ({thief.returncode}); see focus-thief.log")
            thief_window = run.window_for(thief.pid)
            if thief_window:
                break
            time.sleep(0.5)
        if not thief_window:
            raise Failure(f"unfocused_spinner: no window for the second process {thief.pid}")
        time.sleep(1)
        x_focus(run, thief_window)
        focused = x_focused(run)
        run.log(f"unfocused_spinner: after focusing the second window, getwindowfocus is {focused} (second window {thief_window} is {int(thief_window, 16)})")
        if focused != int(thief_window, 16):
            raise Failure("unfocused_spinner: the second window did not take the X focus from Sirio")
        # The second window is mapped over Sirio's (an X capture of a covered window returns the
        # covering pixels), so Sirio goes back on top. Raising is not focusing: it must not have
        # handed the focus back.
        x_run(run, "windowraise", window)
        time.sleep(0.5)
        focused_after_raise = x_focused(run)
        run.log(f"unfocused_spinner: after raising Sirio's window, getwindowfocus is {focused_after_raise}")
        if focused_after_raise != int(thief_window, 16):
            raise Failure("unfocused_spinner: raising Sirio's window handed the focus back to it")
        time.sleep(1.5)

        frames = burst(run, chat, "spin")
        differences = [
            crop_difference(run, frames[a], frames[b], SIDEBAR_BLOOM)
            for a, b in ((0, 1), (1, 2), (0, 2))
        ]
        run.log(f"unfocused_spinner: sidebar bloom pixels differing between frames 1-2, 2-3, 1-3: {differences}")
        if not any(differences):
            # Three identical crops have two causes the message has to tell apart: the bloom froze
            # because the pause is on, or the crop holds no bloom because the row moved (the
            # rectangle is pixels of the default window). Give Sirio the focus back, with the
            # second window still mapped, and look at the same crop again: only on this path, so a
            # green run takes no extra frames.
            x_focus(run, window)
            refocused = x_focused(run)
            run.log(f"unfocused_spinner: control, getwindowfocus is {refocused} after focusing Sirio again")
            if refocused != int(window, 16):
                raise Failure("unfocused_spinner: the control could not give Sirio the X focus back")
            time.sleep(1)
            control = burst(run, chat, "focused-control")
            moved = [crop_difference(run, control[a], control[b], SIDEBAR_BLOOM) for a, b in ((0, 1), (1, 2), (0, 2))]
            run.log(f"unfocused_spinner: control, sidebar bloom pixels differing with Sirio focused: {moved}")
            if any(moved):
                raise Failure(
                    "unfocused_spinner: the sidebar's status bloom froze while Sirio was unfocused and moved "
                    "again once it was focused: bezel's pause-when-inactive is on (three identical frames)"
                )
            raise Failure(
                f"unfocused_spinner: SIDEBAR_BLOOM {SIDEBAR_BLOOM} holds nothing that moves even with "
                "Sirio focused; the row moved, update the rectangle"
            )
        if x_focused(run) != int(thief_window, 16):
            raise Failure("unfocused_spinner: the second window lost the focus during the frames")

        seconds = 10
        try:
            first_ticks, started = cpu_ticks(chat.process.pid), time.monotonic()
            time.sleep(seconds)
            used = cpu_ticks(chat.process.pid) - first_ticks
        except FileNotFoundError:
            raise Failure("unfocused_spinner: Sirio exited during the CPU sample") from None
        if x_focused(run) != int(thief_window, 16):
            raise Failure("unfocused_spinner: the second window lost the focus during the CPU sample")
        percent = 100.0 * used / os.sysconf("SC_CLK_TCK") / (time.monotonic() - started)
        load = " ".join(f"{value:.2f}" for value in os.getloadavg())
        run.log(f"unfocused-spinner CPU {percent:.1f}% of one core over {seconds} s (load average {load})")
        if run.cpu_ceiling is not None and percent > run.cpu_ceiling:
            raise Failure(f"unfocused_spinner: {percent:.1f}% of one core is above the ceiling {run.cpu_ceiling}%")
    finally:
        thief.terminate()
        try:
            thief.wait(timeout=10)
        except subprocess.TimeoutExpired:
            thief.kill()
            thief.wait()
        thief_log.close()
    chat.call("surface.chat.stop", surfaceId=chat.surface)
    chat.until("the turn stops", lambda s: s["status"] != "streaming", quiet=True)
    scenario.finish()
    return None


SCENARIOS = [
    staged_queue,
    permission,
    question,
    plan,
    cancel,
    death_recovery,
    authentication,
    restore,
    activity,
    wide_content,
    subagent,
    long_history,
    effort,
    effort_light,
    effort_full,
    effort_full_light,
    composer_keys,
    reduced_motion,
    unfocused_spinner,
]


def stop_hosts(run: Run) -> None:
    """End the hosts the scenarios' instances started beside their databases.

    A quit only detaches, so each host outlives its instance. Only a pid its own
    state file names, and only while that pid is still a sirio-host, is signalled:
    TERM, then KILL after 5 s.
    """
    for state in run.work.glob("*/host/host-v*.json"):
        try:
            pid = int(json.loads(state.read_text())["pid"])
            comm = Path(f"/proc/{pid}/comm").read_text()
        except (OSError, ValueError, KeyError, TypeError):
            continue
        if "sirio-host" not in comm:
            continue
        try:
            os.kill(pid, 15)
            for _ in range(50):
                time.sleep(0.1)
                os.kill(pid, 0)
            os.kill(pid, 9)
        except ProcessLookupError:
            pass


def build(run: Run) -> None:
    run.log("building sirio, ely_chat_seed, the probe and the focus-stealing window")
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio", "--bin", "sirio"], cwd=ROOT / "rust", check=True
    )
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio_persistence", "--example", "ely_chat_seed"],
        cwd=ROOT / "rust",
        check=True,
    )
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio_ui", "--example", "ely_chat_probe", "--example", "surfaces_probe"],
        cwd=ROOT / "rust",
        check=True,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--state-only", action="store_true")
    parser.add_argument("--display")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--no-identity", action="store_true")
    parser.add_argument("--window-size")
    parser.add_argument("--sirio-bin")
    parser.add_argument("--only")
    parser.add_argument("--cpu-ceiling", type=float)
    parser.add_argument("--focus-thief-bin")
    arguments = parser.parse_args()
    run = Run(arguments)
    try:
        run.preflight()
        if not arguments.no_build:
            build(run)
        for needed in (run.sirio_bin, SEED):
            if not needed.exists():
                raise Failure(f"{needed} is missing: build it first, or drop --no-build")
        chosen = set(arguments.only.split(",")) if arguments.only else None
        if chosen and not chosen <= {scenario.__name__ for scenario in SCENARIOS}:
            raise Failure(f"--only names no scenario: {sorted(chosen - {s.__name__ for s in SCENARIOS})}")
        for scenario in SCENARIOS:
            name = scenario.__name__
            if chosen and name not in chosen:
                continue
            run.log(f"== scenario {name}")
            # A scenario that cannot run here (no display) returns "skipped".
            run.results[name] = scenario(run) or "passed"
        if not arguments.no_identity and not arguments.only:
            run.log("== identities")
            identities(run)
            run.results["identities"] = "skipped" if run.state_only else "passed"
    except (Failure, Exception) as failure:
        run.log(f"FAIL: {type(failure).__name__}: {failure}" if not isinstance(failure, Failure) else f"FAIL: {failure}")
        run.results["failure"] = str(failure)
        (run.out / "summary.json").write_text(json.dumps({"results": run.results, "frames": run.frames}, indent=2))
        return 1
    finally:
        for instance in run.instances:
            instance.quit()
        stop_hosts(run)
    (run.out / "summary.json").write_text(json.dumps({"results": run.results, "frames": run.frames}, indent=2))
    run.log("artifacts:")
    for name in ("transcript.log", "responses.jsonl", "summary.json", "scenarios"):
        run.log(f"  {run.out / name}")
    for frame in run.frames:
        run.log(f"  {run.out / frame}")
    run.log("ELY CHAT UI E2E OK")
    return 0


if __name__ == "__main__":
    if len(sys.argv) >= 5 and sys.argv[1] == "--agent-proxy":
        sys.exit(agent_proxy(sys.argv[2], sys.argv[3], sys.argv[4]))
    sys.exit(main())
