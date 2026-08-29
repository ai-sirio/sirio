#!/usr/bin/env python3
"""Supervise Sirio for a frozen window and preserve thread evidence.

The process is deliberately supervised from this Python process.  A freeze is
not a process exit: it is a stable window pixmap while the control socket still
answers.  Once that signature is observed, capture the process stacks and
``/proc`` state before trying the three recovery probes from P22.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import json
import os
import resource
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path


MIN_COLORS = 200
DEFAULT_SAMPLE_INTERVAL = 0.5
DEFAULT_FREEZE_THRESHOLD = 2.0
DEFAULT_STACK_DELAY = 5.0
DEFAULT_RECOVERY_TIMEOUT = 5.0


def _load_crash_supervise():
    path = Path(__file__).with_name("crash-supervise.py")
    spec = importlib.util.spec_from_file_location("crash_supervise", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CRASH_SUPERVISE = _load_crash_supervise()


@dataclass(frozen=True)
class FreezeCandidate:
    digest: str
    started_at: float
    detected_at: float
    duration_seconds: float


class FreezeDetector:
    """Turn a sequence of pixel digests into one candidate per stable period."""

    def __init__(self, threshold_seconds: float) -> None:
        self.threshold_seconds = threshold_seconds
        self._digest: str | None = None
        self._started_at: float | None = None
        self._reported = False

    def observe(self, digest: str, at: float) -> FreezeCandidate | None:
        if digest != self._digest:
            self._digest = digest
            self._started_at = at
            self._reported = False
            return None
        if self._started_at is None or self._reported:
            return None
        duration = at - self._started_at
        if duration < self.threshold_seconds:
            return None
        self._reported = True
        return FreezeCandidate(
            digest=digest,
            started_at=self._started_at,
            detected_at=at,
            duration_seconds=duration,
        )


@dataclass(frozen=True)
class ProcStat:
    pid: int
    state: str
    utime_ticks: int
    stime_ticks: int


def parse_proc_stat(line: str) -> ProcStat:
    """Parse the fields needed from /proc/<pid>/stat.

    The command name may contain spaces and parentheses, so split at the last
    ``)`` rather than using whitespace or the first closing parenthesis.
    """

    opening = line.find("(")
    closing = line.rfind(")")
    if opening <= 0 or closing <= opening:
        raise ValueError(f"malformed proc stat: {line!r}")
    pid = int(line[:opening].strip())
    fields = line[closing + 1 :].strip().split()
    if len(fields) <= 12:
        raise ValueError(f"proc stat is missing cpu fields: {line!r}")
    return ProcStat(
        pid=pid,
        state=fields[0],
        utime_ticks=int(fields[11]),
        stime_ticks=int(fields[12]),
    )


@dataclass(frozen=True)
class Liveness:
    ping_ok: bool
    workspace_ok: bool
    ping_output: str
    workspace_output: str
    ping_error: str
    workspace_error: str

    @property
    def responsive(self) -> bool:
        return self.ping_ok and self.workspace_ok


@dataclass(frozen=True)
class PresentationSample:
    elapsed: float
    colors: int
    digest: str | None


def run_command(command: list[str], timeout: float) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        return subprocess.CompletedProcess(
            command,
            124,
            stdout=error.stdout or "",
            stderr=f"command timed out after {timeout:.1f}s",
        )
    except OSError as error:
        return subprocess.CompletedProcess(command, 127, stdout="", stderr=str(error))


def cli_command(sirioctl: Path, socket: Path, subcommand: str, *arguments: str) -> list[str]:
    return [str(sirioctl), subcommand, *arguments, "--socket", str(socket)]


def pixel_digest(path: Path) -> str | None:
    """Hash pixels, not PNG metadata, so repeated captures compare honestly."""

    if not path.is_file() or path.stat().st_size == 0:
        return None
    convert = CRASH_SUPERVISE.shutil_which("convert")
    if convert:
        result = subprocess.run(
            [convert, str(path), "-depth", "8", "RGBA:-"],
            capture_output=True,
            timeout=10,
            check=False,
        )
        if result.returncode == 0 and result.stdout:
            return hashlib.sha256(result.stdout).hexdigest()
    try:
        return hashlib.sha256(path.read_bytes()).hexdigest()
    except OSError:
        return None


def default_socket_path(environment: dict[str, str]) -> Path:
    override = environment.get("SIRIO_SOCKET")
    if override:
        return Path(override)
    runtime = environment.get("XDG_RUNTIME_DIR")
    if runtime and Path(runtime).is_absolute():
        return Path(runtime) / "Sirio" / "control.sock"
    state = environment.get("XDG_STATE_HOME")
    if state and Path(state).is_absolute():
        root = Path(state)
    else:
        home = environment.get("HOME", "/tmp")
        root = Path(home) / ".local" / "state"
    return root / "Sirio" / "control.sock"


def check_liveness(sirioctl: Path, socket: Path) -> Liveness:
    ping = run_command(cli_command(sirioctl, socket, "ping"), timeout=5.0)
    workspace = run_command(
        cli_command(sirioctl, socket, "current-workspace", "--json"), timeout=5.0
    )
    workspace_ok = workspace.returncode == 0 and workspace.stdout.strip() not in {"", "[]"}
    if workspace_ok:
        try:
            decoded = json.loads(workspace.stdout)
            workspace_ok = isinstance(decoded, list) and bool(decoded)
        except json.JSONDecodeError:
            workspace_ok = False
    return Liveness(
        ping_ok=ping.returncode == 0 and ping.stdout.strip() == "pong",
        workspace_ok=workspace_ok,
        ping_output=ping.stdout.strip(),
        workspace_output=workspace.stdout.strip(),
        ping_error=ping.stderr.strip(),
        workspace_error=workspace.stderr.strip(),
    )


def write_command_result(path: Path, command: list[str], result: subprocess.CompletedProcess[str]) -> None:
    path.write_text(
        "command: "
        + " ".join(command)
        + f"\nreturncode: {result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}\n",
        encoding="utf-8",
    )


def capture_stack(pid: int, destination: Path) -> None:
    command = ["eu-stack", "-p", str(pid)]
    result = run_command(command, timeout=20.0)
    write_command_result(destination, command, result)


def copy_proc_file(source: Path, destination: Path) -> str:
    try:
        text = source.read_text(encoding="utf-8", errors="replace")
    except OSError as error:
        text = f"<unable to read {source}: {error}>\n"
    destination.write_text(text, encoding="utf-8")
    return text


def capture_proc_snapshot(pid: int, destination: Path) -> ProcStat | None:
    destination.mkdir(parents=True, exist_ok=True)
    stat_text = copy_proc_file(Path(f"/proc/{pid}/stat"), destination / "process.stat")
    copy_proc_file(Path(f"/proc/{pid}/status"), destination / "process.status")
    parsed: ProcStat | None
    try:
        parsed = parse_proc_stat(stat_text.strip())
    except (ValueError, OSError):
        parsed = None

    task_root = Path(f"/proc/{pid}/task")
    try:
        tids = sorted(path for path in task_root.iterdir() if path.name.isdigit())
    except OSError:
        tids = []
    thread_index: list[str] = []
    for task in tids:
        tid = task.name
        thread_dir = destination / "threads" / tid
        thread_dir.mkdir(parents=True, exist_ok=True)
        copy_proc_file(task / "comm", thread_dir / "comm")
        copy_proc_file(task / "wchan", thread_dir / "wchan")
        status = copy_proc_file(task / "status", thread_dir / "status")
        thread_index.append(f"{tid}: {status.splitlines()[0] if status else '<empty>'}")
    (destination / "thread-index.txt").write_text(
        "\n".join(thread_index) + ("\n" if thread_index else ""), encoding="utf-8"
    )
    return parsed


def window_geometry(window_id: str, display: str) -> tuple[int, int, int, int] | None:
    result = run_command(["xwininfo", "-display", display, "-id", window_id], timeout=3.0)
    values: dict[str, int] = {}
    for line in result.stdout.splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        if key.strip() in {
            "Absolute upper-left X",
            "Absolute upper-left Y",
            "Width",
            "Height",
        }:
            try:
                values[key.strip()] = int(value.strip())
            except ValueError:
                pass
    keys = ("Absolute upper-left X", "Absolute upper-left Y", "Width", "Height")
    if not all(key in values for key in keys):
        return None
    return tuple(values[key] for key in keys)  # type: ignore[return-value]


def damage_window(window_id: str, display: str) -> subprocess.CompletedProcess[str]:
    geometry = window_geometry(window_id, display)
    if geometry is None:
        return subprocess.CompletedProcess([], 1, stdout="", stderr="could not read window geometry")
    x, y, width, height = geometry
    bigger = run_command(
        ["xdotool", "-display", display, "windowsize", window_id, str(width + 1), str(height)],
        timeout=5.0,
    )
    if bigger.returncode != 0:
        return bigger
    restored = run_command(
        ["xdotool", "-display", display, "windowsize", window_id, str(width), str(height)],
        timeout=5.0,
    )
    if restored.returncode != 0:
        return restored
    return subprocess.CompletedProcess(
        ["xdotool", "windowsize", window_id, f"{width}x{height}"],
        0,
        stdout=f"resized {width}x{height} -> {width + 1}x{height} -> {width}x{height} at {x},{y}",
        stderr="",
    )


def current_workspace_id(workspace_output: str) -> str | None:
    try:
        rows = json.loads(workspace_output)
    except json.JSONDecodeError:
        return None
    if not isinstance(rows, list) or not rows or not isinstance(rows[0], dict):
        return None
    value = rows[0].get("id")
    return value if isinstance(value, str) and value else None


def select_current_workspace(
    sirioctl: Path, socket: Path, workspace_output: str
) -> subprocess.CompletedProcess[str]:
    workspace_id = current_workspace_id(workspace_output)
    if workspace_id is None:
        return subprocess.CompletedProcess(
            [], 1, stdout="", stderr="current workspace response had no id"
        )
    return run_command(
        cli_command(
            sirioctl,
            socket,
            "select-workspace",
            "--workspace",
            workspace_id,
        ),
        timeout=5.0,
    )


def wait_for_pixel_change(
    display: str,
    window: tuple[str, tuple[int, int, int, int], str] | None,
    frozen_digest: str,
    screenshot: Path,
    timeout: float,
    interval: float,
) -> tuple[bool, list[str]]:
    deadline = time.monotonic() + timeout
    observations: list[str] = []
    while time.monotonic() < deadline:
        current = CRASH_SUPERVISE.find_window(display, os.getpid())
        # The supervisor pid is not the app pid; keep the known window unless a caller updates it.
        candidate_window = window or current
        if candidate_window is not None:
            CRASH_SUPERVISE.capture_screen(display, screenshot, candidate_window)
            digest = pixel_digest(screenshot)
            observations.append(digest or "<no-digest>")
            if digest is not None and digest != frozen_digest:
                return True, observations
        time.sleep(min(interval, max(0.0, deadline - time.monotonic())))
    return False, observations


def recovery_probe(
    display: str,
    window: tuple[str, tuple[int, int, int, int], str] | None,
    sirioctl: Path,
    socket: Path,
    workspace_output: str,
    frozen_digest: str,
    evidence_dir: Path,
    recovery_timeout: float,
    interval: float,
) -> list[str]:
    outcomes: list[str] = []
    screenshot = evidence_dir / "recovery.png"
    if window is None:
        outcomes.append("window damage: skipped (window disappeared)")
    else:
        damaged = damage_window(window[0], display)
        changed, observations = wait_for_pixel_change(
            display, window, frozen_digest, screenshot, recovery_timeout, interval
        )
        write_command_result(evidence_dir / "recovery-window-damage.txt", [], damaged)
        outcomes.append(
            f"window damage: returncode={damaged.returncode} pixels_changed={changed} "
            f"samples={len(observations)}"
        )
        if changed:
            return outcomes

    selected = select_current_workspace(sirioctl, socket, workspace_output)
    changed, observations = wait_for_pixel_change(
        display, window, frozen_digest, screenshot, recovery_timeout, interval
    )
    write_command_result(evidence_dir / "recovery-socket-workspace.txt", [], selected)
    outcomes.append(
        f"socket select-current-workspace: returncode={selected.returncode} "
        f"pixels_changed={changed} samples={len(observations)}"
    )
    if changed:
        return outcomes

    changed, observations = wait_for_pixel_change(
        display, window, frozen_digest, screenshot, recovery_timeout, interval
    )
    outcomes.append(f"wait: pixels_changed={changed} samples={len(observations)}")
    return outcomes


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="supervise Sirio for a responsive-socket freeze")
    root = Path(__file__).resolve().parents[1]
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=root / "artifacts/freeze-runs",
    )
    parser.add_argument("--display", default=os.environ.get("DISPLAY", ":2"))
    parser.add_argument("--timeout", type=float, default=0.0)
    parser.add_argument("--poll", type=float, default=0.5)
    parser.add_argument("--sample", type=float, default=DEFAULT_SAMPLE_INTERVAL)
    parser.add_argument("--freeze-after", type=float, default=DEFAULT_FREEZE_THRESHOLD)
    parser.add_argument("--stack-delay", type=float, default=DEFAULT_STACK_DELAY)
    parser.add_argument("--recovery-timeout", type=float, default=DEFAULT_RECOVERY_TIMEOUT)
    parser.add_argument("--sirioctl", type=Path, default=root / "rust/target/debug/sirioctl")
    parser.add_argument("--socket", type=Path, default=default_socket_path(dict(os.environ)))
    parser.add_argument(
        "--continue-after-freeze",
        action="store_true",
        help="keep observing after evidence and recovery probes instead of stopping",
    )
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command and args.command[0] == "--":
        args.command = args.command[1:]
    if not args.command:
        args.command = [str(root / "rust/target/debug/sirio")]
    if min(args.poll, args.sample, args.freeze_after, args.stack_delay, args.recovery_timeout) <= 0:
        parser.error("timing options must be positive")
    return args


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def run() -> int:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    started_at = utc_now()
    started = time.monotonic()
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    label = f"{stamp}-freeze"
    output_log = args.output_dir / f"{label}.output.log"
    report = args.output_dir / f"{label}.report.txt"
    screenshot = args.output_dir / f"{label}.sample.png"
    evidence = args.output_dir / f"{label}.evidence"

    display_available, display_has_dri3, display_reason = CRASH_SUPERVISE.probe_display(args.display)
    if not display_available or not display_has_dri3:
        report.write_text(
            "\n".join(
                [
                    "crash-freeze-supervise report",
                    f"display: {args.display}",
                    f"display_probe: {display_reason}",
                    "freeze_detected: False",
                    "valid_observation_minutes: 0.000",
                ]
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"report: {report}")
        print(f"status: NOT A TRIAL — {display_reason}")
        return 2

    child_env = os.environ.copy()
    child_env["DISPLAY"] = args.display
    child_env.pop("WAYLAND_DISPLAY", None)
    child_env["GPUI_X11_SCALE_FACTOR"] = "1"
    child_env["SIRIO_SOCKET"] = str(args.socket)
    child_env.setdefault("RUST_BACKTRACE", "full")

    log_fd = os.open(output_log, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    pid = os.fork()
    if pid == 0:
        try:
            os.setsid()
            try:
                resource.setrlimit(resource.RLIMIT_CORE, (resource.RLIM_INFINITY, resource.RLIM_INFINITY))
            except OSError:
                pass
            os.dup2(log_fd, sys.stdout.fileno())
            os.dup2(log_fd, sys.stderr.fileno())
            os.close(log_fd)
            os.execvpe(args.command[0], args.command, child_env)
        except BaseException as error:
            os.write(2, f"crash-freeze-supervise: child setup/exec failed: {error!r}\n".encode())
            os._exit(127)
    os.close(log_fd)

    wait_status: int | None = None
    timed_out = False
    window = None
    detector = FreezeDetector(args.freeze_after)
    samples: list[PresentationSample] = []
    first_render_elapsed: float | None = None
    freeze_candidate: FreezeCandidate | None = None
    liveness: Liveness | None = None
    recovery: list[str] = []
    next_sample = started

    def record_sample(now: float, current_window) -> None:
        nonlocal next_sample, first_render_elapsed, freeze_candidate, liveness, recovery
        CRASH_SUPERVISE.capture_screen(args.display, screenshot, current_window)
        colors = CRASH_SUPERVISE.image_color_count(screenshot) or 0
        digest = pixel_digest(screenshot) if colors >= MIN_COLORS else None
        elapsed = now - started
        sample = PresentationSample(elapsed=elapsed, colors=colors, digest=digest)
        samples.append(sample)
        if colors >= MIN_COLORS and first_render_elapsed is None:
            first_render_elapsed = elapsed
        if digest is None or freeze_candidate is not None:
            next_sample = now + args.sample
            return
        candidate = detector.observe(digest, elapsed)
        if candidate is None:
            next_sample = now + args.sample
            return
        checked = check_liveness(args.sirioctl, args.socket)
        if not checked.responsive:
            next_sample = now + args.sample
            return
        freeze_candidate = candidate
        liveness = checked
        evidence.mkdir(parents=True, exist_ok=True)
        (evidence / "candidate.txt").write_text(
            f"digest: {candidate.digest}\n"
            f"started_after_seconds: {candidate.started_at:.3f}\n"
            f"detected_after_seconds: {candidate.detected_at:.3f}\n"
            f"duration_seconds: {candidate.duration_seconds:.3f}\n",
            encoding="utf-8",
        )
        (evidence / "liveness.txt").write_text(
            f"ping_ok: {checked.ping_ok}\n"
            f"workspace_ok: {checked.workspace_ok}\n"
            f"ping_output: {checked.ping_output}\n"
            f"workspace_output: {checked.workspace_output}\n"
            f"ping_error: {checked.ping_error}\n"
            f"workspace_error: {checked.workspace_error}\n",
            encoding="utf-8",
        )
        capture_stack(pid, evidence / "stack-immediate.txt")
        capture_proc_snapshot(pid, evidence / "proc-immediate")
        time.sleep(args.stack_delay)
        capture_stack(pid, evidence / "stack-after-delay.txt")
        capture_proc_snapshot(pid, evidence / "proc-after-delay")
        recovery = recovery_probe(
            args.display,
            current_window,
            args.sirioctl,
            args.socket,
            checked.workspace_output,
            candidate.digest,
            evidence,
            args.recovery_timeout,
            args.sample,
        )
        next_sample = now + args.sample

    print(f"supervising pid {pid}: {' '.join(args.command)}", flush=True)
    try:
        while wait_status is None:
            waited_pid, candidate_status = os.waitpid(pid, os.WNOHANG)
            if waited_pid == pid:
                wait_status = candidate_status
                break
            current_window = CRASH_SUPERVISE.find_window(args.display, pid)
            if current_window is not None:
                window = current_window
                now = time.monotonic()
                if now >= next_sample:
                    record_sample(now, current_window)
                    if freeze_candidate is not None and not args.continue_after_freeze:
                        timed_out = True
                        os.killpg(pid, signal.SIGTERM)
                        continue
            if args.timeout and time.monotonic() - started >= args.timeout:
                timed_out = True
                try:
                    os.killpg(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                while wait_status is None:
                    waited_pid, candidate_status = os.waitpid(pid, 0)
                    if waited_pid == pid:
                        wait_status = candidate_status
            else:
                time.sleep(args.poll)
    except KeyboardInterrupt:
        timed_out = True
        try:
            os.killpg(pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        _, wait_status = os.waitpid(pid, 0)

    finished_at = utc_now()
    uptime = time.monotonic() - started
    exited = os.WIFEXITED(wait_status)
    signaled = os.WIFSIGNALED(wait_status)
    exit_code = os.WEXITSTATUS(wait_status) if exited else None
    term_signal = os.WTERMSIG(wait_status) if signaled else None
    signal_name = signal.Signals(term_signal).name if term_signal is not None else None
    valid_seconds = max(0.0, uptime - first_render_elapsed) if first_render_elapsed is not None else 0.0
    final_window = CRASH_SUPERVISE.find_window(args.display, pid) or window
    lines = [
        "crash-freeze-supervise report",
        f"command: {' '.join(args.command)}",
        f"pid: {pid}",
        f"started_utc: {started_at}",
        f"finished_utc: {finished_at}",
        f"wall_clock_seconds: {uptime:.3f}",
        f"valid_observation_minutes: {valid_seconds / 60:.3f}",
        f"display: {args.display}",
        f"display_probe: {display_reason}",
        f"socket: {args.socket}",
        f"timed_out_by_supervisor: {timed_out}",
        f"wait_status_raw: {wait_status}",
        f"WIFEXITED: {exited}",
        f"exit_code: {exit_code}",
        f"WIFSIGNALED: {signaled}",
        f"WTERMSIG: {term_signal}",
        f"signal_name: {signal_name}",
        f"freeze_detected: {freeze_candidate is not None}",
        f"freeze_candidate_duration_seconds: {freeze_candidate.duration_seconds if freeze_candidate else 'None'}",
        f"freeze_digest: {freeze_candidate.digest if freeze_candidate else 'None'}",
        f"socket_ping_ok: {liveness.ping_ok if liveness else 'None'}",
        f"workspace_current_ok: {liveness.workspace_ok if liveness else 'None'}",
        f"stack_immediate: {evidence / 'stack-immediate.txt' if freeze_candidate else 'not captured'}",
        f"stack_after_delay: {evidence / 'stack-after-delay.txt' if freeze_candidate else 'not captured'}",
        f"proc_immediate: {evidence / 'proc-immediate' if freeze_candidate else 'not captured'}",
        f"proc_after_delay: {evidence / 'proc-after-delay' if freeze_candidate else 'not captured'}",
        f"recovery: {' | '.join(recovery) if recovery else 'not attempted'}",
        f"presentation_samples: {len(samples)}",
        *[
            f"presentation_sample_{index}: elapsed={sample.elapsed:.3f}s colors={sample.colors} digest={sample.digest}"
            for index, sample in enumerate(samples, start=1)
        ],
        f"output_log: {output_log}",
        f"window: {final_window[0] if final_window else '<not found>'}",
        f"window_geometry: {final_window[1] if final_window else '<not found>'}",
        "",
        "honest_remainder:",
        "A freeze is a candidate unless freeze_detected is True; no process-death claim is inferred from a frozen pixmap.",
    ]
    report.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"report: {report}")
    print(f"output: {output_log}")
    print(
        f"status: freeze_detected={freeze_candidate is not None} "
        f"WIFEXITED={exited} WIFSIGNALED={signaled} "
        f"valid_observation_minutes={valid_seconds / 60:.3f}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(run())
