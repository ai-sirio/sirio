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
relaunch. After them, unless --no-identity, the five agents' marks as native
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
`surfaceId,requestId,optionId`, `.stop`/`.read` with `surfaceId`. Every
parameter is a string.

Artifacts under --out-dir: transcript.log, responses.jsonl (every control
request and its reply), summary.json, and per scenario: app.log,
agent-traffic.jsonl (the fixture's stdin/stdout), exit-status and frames/*.png.

Usage: Scripts/Tests/test-ely-chat-ui-e2e.py --out-dir PATH [--state-only]
           [--display :N] [--no-build] [--no-identity] [--window-size WxH]
           [--sirio-bin PATH] [--only NAME[,NAME]]

--sirio-bin runs the same scenarios against another build (the baseline, to
compare); --only runs the named scenarios (function names, e.g. long_history).

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
        for tool in ("import", "identify", "xwininfo", "xprop"):
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

    def capture(self, pid: int, destination: Path) -> None:
        if self.state_only:
            return
        time.sleep(1.5)
        window = None
        for _ in range(20):
            window = self.window_for(pid)
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
    def __init__(self, run: Run, name: str, mode: str, *, appearance: str = "dark", ui_size: int = 13) -> None:
        self.run = run
        self.name = name
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
    subagent,
    long_history,
]


def build(run: Run) -> None:
    run.log("building sirio, ely_chat_seed and the probe")
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio", "--bin", "sirio"], cwd=ROOT / "rust", check=True
    )
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio_persistence", "--example", "ely_chat_seed"],
        cwd=ROOT / "rust",
        check=True,
    )
    subprocess.run(
        ["cargo", "build", "--quiet", "-p", "sirio_ui", "--example", "ely_chat_probe"],
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
            scenario(run)
            run.results[name] = "passed"
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
