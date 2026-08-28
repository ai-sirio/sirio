#!/usr/bin/env python3
"""Run Tiller until it exits and preserve the evidence that a bare terminal loses.

The parent uses waitpid directly so the report contains the real POSIX wait status,
not just the shell's 128 + signal convention.  The child gets its own process group,
which lets a timeout clean up descendants without disturbing the supervisor.
"""

from __future__ import annotations

import argparse
import datetime as dt
import os
import re
import resource
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path


MIN_COLORS = 200
DEFAULT_SAMPLE_INTERVAL = 5.0
WINDOW_RE = re.compile(
    r"^\s+(0x[0-9a-fA-F]+).*?(\d+)x(\d+)\+(-?\d+)\+(-?\d+)"
)


@dataclass(frozen=True)
class TrialClassification:
    verdict: str
    reason: str


def dri3_from_xdpyinfo(output: str) -> bool:
    return any(line.strip() == "DRI3" for line in output.splitlines())


def probe_display(display: str) -> tuple[bool, bool, str]:
    if not shutil_which("xdpyinfo"):
        return False, False, "xdpyinfo is not installed"
    try:
        result = subprocess.run(
            ["xdpyinfo", "-display", display],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, False, f"xdpyinfo failed for {display}: {error}"
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().splitlines()
        suffix = f": {detail[-1]}" if detail else ""
        return False, False, f"xdpyinfo failed for {display}{suffix}"
    if not dri3_from_xdpyinfo(result.stdout):
        return True, False, "display has no DRI3 extension"
    return True, True, "DRI3 extension present"


def classify_trial(
    *,
    display_available: bool,
    display_has_dri3: bool,
    window_seen: bool,
    samples: list[int],
    process_exited: bool,
    driven: bool,
    input_landed: bool | None = None,
) -> TrialClassification:
    if not display_available:
        return TrialClassification("NOT A TRIAL", "display was unavailable")
    if not display_has_dri3:
        return TrialClassification("NOT A TRIAL", "display has no DRI3 extension")
    if not window_seen:
        if process_exited:
            return TrialClassification(
                "NOT A TRIAL", "app died during startup before presenting"
            )
        return TrialClassification("NOT A TRIAL", "no PID-matched window presented")
    if process_exited and len(samples) < 2:
        return TrialClassification(
            "NOT A TRIAL", "app died during startup before two presentation samples"
        )
    rendered_start = next(
        (index for index, colors in enumerate(samples) if colors >= MIN_COLORS),
        None,
    )
    if rendered_start is None:
        return TrialClassification("NOT A TRIAL", "presentation remained flat")
    rendered_samples = samples[rendered_start:]
    if process_exited and len(rendered_samples) < 2:
        return TrialClassification(
            "NOT A TRIAL", "app died during startup before two presentation samples"
        )
    if len(rendered_samples) < 2:
        return TrialClassification(
            "NOT A TRIAL", "fewer than two presentation samples were captured"
        )
    if any(colors < MIN_COLORS for colors in rendered_samples):
        return TrialClassification(
            "NOT A TRIAL", "presentation became flat during observed run"
        )
    if driven and input_landed is not True:
        return TrialClassification("NOT A TRIAL", "input was driven but not verified")
    return TrialClassification("VALID TRIAL", "rendered throughout observed run")


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")


def find_window(display: str, pid: int) -> tuple[str, tuple[int, int, int, int], str] | None:
    """Return the largest window advertised as belonging to pid."""

    try:
        listing = subprocess.run(
            ["xwininfo", "-display", display, "-root", "-children"],
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        ).stdout
    except (OSError, subprocess.TimeoutExpired):
        return None

    candidates: list[tuple[int, str, tuple[int, int, int, int]]] = []
    for line in listing.splitlines():
        match = WINDOW_RE.match(line)
        if not match:
            continue
        window_id = match.group(1)
        width, height = int(match.group(2)), int(match.group(3))
        x, y = int(match.group(4)), int(match.group(5))
        try:
            prop = subprocess.run(
                ["xprop", "-display", display, "-id", window_id, "_NET_WM_PID"],
                capture_output=True,
                text=True,
                timeout=2,
                check=False,
            ).stdout
            advertised_pid = int(prop.rsplit("=", 1)[-1].strip())
        except (OSError, ValueError, subprocess.TimeoutExpired):
            continue
        if advertised_pid == pid:
            candidates.append((width * height, window_id, (x, y, width, height)))

    if not candidates:
        return None
    _, window_id, geometry = max(candidates)
    title = ""
    try:
        props = subprocess.run(
            ["xprop", "-display", display, "-id", window_id, "_NET_WM_NAME", "WM_NAME"],
            capture_output=True,
            text=True,
            timeout=2,
            check=False,
        ).stdout
        title = " | ".join(line.strip() for line in props.splitlines() if " = " in line)
    except (OSError, subprocess.TimeoutExpired):
        pass
    return window_id, geometry, title


def capture_screen(
    display: str,
    screenshot: Path,
    window: tuple[str, tuple[int, int, int, int], str] | None,
) -> str:
    """Capture the PID-matched app window, falling back to a PID-matched root crop."""

    screenshot.parent.mkdir(parents=True, exist_ok=True)
    screenshot.unlink(missing_ok=True)
    if not shutil_which("import"):
        return "unavailable (ImageMagick 'import' is not installed)"

    env = os.environ.copy()
    env["DISPLAY"] = display
    attempts: list[list[str]] = []
    if window is not None:
        window_id, (x, y, width, height), _ = window
        attempts.append(["import", "-window", window_id, str(screenshot)])
        attempts.append(
            [
                "import",
                "-window",
                "root",
                "-crop",
                f"{width}x{height}+{x}+{y}",
                str(screenshot),
            ]
        )
    # Never photograph the complete display without a PID-matched window.  The display is shared
    # by several agents, so a root capture in that state can make another process look like ours.
    if window is None:
        return "not captured (no PID-matched window)"

    for command in attempts:
        try:
            subprocess.run(command, env=env, capture_output=True, timeout=20, check=False)
        except (OSError, subprocess.TimeoutExpired):
            continue
        colors = image_color_count(screenshot)
        if colors is not None and colors > 1:
            return f"{screenshot} ({colors} colors)"
    colors = image_color_count(screenshot)
    if colors is not None:
        return f"{screenshot} (flat: {colors} colors; presentation may be blank)"
    return "capture failed"


def image_color_count(path: Path) -> int | None:
    if not path.is_file() or path.stat().st_size == 0 or not shutil_which("identify"):
        return None
    try:
        result = subprocess.run(
            ["identify", "-format", "%k", str(path)],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    try:
        return int(result.stdout.strip())
    except ValueError:
        return None


def shutil_which(command: str) -> str | None:
    """Small local equivalent of shutil.which, keeping imports obvious in the report script."""

    for directory in os.environ.get("PATH", os.defpath).split(os.pathsep):
        candidate = Path(directory) / command
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    return None


def tail_lines(path: Path, count: int) -> list[str]:
    try:
        with path.open("r", encoding="utf-8", errors="replace") as stream:
            lines = stream.readlines()
    except OSError as error:
        return [f"<unable to read output log: {error}>\n"]
    return lines[-count:]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="supervise a GUI process and record signal/exit status, output, uptime, and screen"
    )
    root = Path(__file__).resolve().parents[1]
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=root / "artifacts/crash-runs",
        help="directory for the report, output log, and screenshot",
    )
    parser.add_argument("--display", default=os.environ.get("DISPLAY", ":1"))
    parser.add_argument("--timeout", type=float, default=0, help="stop after this many seconds (0 means no timeout)")
    parser.add_argument("--poll", type=float, default=1.0, help="seconds between waitpid/window polls")
    parser.add_argument(
        "--sample",
        type=float,
        default=DEFAULT_SAMPLE_INTERVAL,
        help="seconds between presentation samples",
    )
    parser.add_argument("--tail", type=int, default=80, help="number of output lines copied into the report")
    parser.add_argument(
        "--driven",
        action="store_true",
        help="the app is being operated externally; requires --input-evidence",
    )
    parser.add_argument(
        "--input-evidence",
        type=Path,
        help="non-empty marker written by the driver after input is observed to land",
    )
    parser.add_argument(
        "command",
        nargs=argparse.REMAINDER,
        help="command to run; put it after -- (defaults to the debug Tiller binary)",
    )
    args = parser.parse_args()
    if args.command and args.command[0] == "--":
        args.command = args.command[1:]
    if not args.command:
        args.command = [str(root / "rust/target/debug/tiller")]
    if args.poll <= 0 or args.sample <= 0 or args.tail < 1:
        parser.error("--poll and --sample must be positive and --tail must be at least 1")
    if args.input_evidence and not args.driven:
        parser.error("--input-evidence requires --driven")
    return args


def run() -> int:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    started_at = utc_now()
    started = time.monotonic()
    stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    label = f"{stamp}-supervised"
    output_log = args.output_dir / f"{label}.output.log"
    report = args.output_dir / f"{label}.report.txt"
    screenshot = args.output_dir / f"{label}.png"
    sample_screenshot = args.output_dir / f"{label}.sample.png"

    display_available, display_has_dri3, display_reason = probe_display(args.display)
    if not display_available or not display_has_dri3:
        classification = classify_trial(
            display_available=display_available,
            display_has_dri3=display_has_dri3,
            window_seen=False,
            samples=[],
            process_exited=False,
            driven=args.driven,
            input_landed=False,
        )
        lines = [
            "crash-supervise report",
            f"command: {' '.join(args.command)}",
            "pid: <not started>",
            f"started_utc: {started_at}",
            f"finished_utc: {utc_now()}",
            f"uptime_seconds: 0.000",
            f"display: {args.display}",
            f"display_probe: {display_reason}",
            f"trial_verdict: {classification.verdict}",
            f"trial_reason: {classification.reason}",
            "valid_trial_minutes: 0.000",
            "presentation_samples: 0",
            f"driven: {args.driven}",
            f"input_evidence: {args.input_evidence or '<none>'}",
            "wait_status_raw: None",
            "WIFEXITED: False",
            "exit_code: None",
            "WIFSIGNALED: False",
            "WTERMSIG: None",
            "signal_name: None",
            "core_dumped: False",
            "screenshot: not captured",
            "window: <not found>",
            "window_geometry: <not found>",
            "window_title: <not found>",
        ]
        report.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"report: {report}")
        print(f"status: {classification.verdict} — {classification.reason}")
        return 2

    child_env = os.environ.copy()
    child_env["DISPLAY"] = args.display
    child_env.pop("WAYLAND_DISPLAY", None)
    child_env["GPUI_X11_SCALE_FACTOR"] = "1"
    child_env.setdefault("RUST_BACKTRACE", "full")

    log_fd = os.open(output_log, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o644)
    pid = os.fork()
    if pid == 0:
        try:
            os.setsid()
            try:
                resource.setrlimit(resource.RLIMIT_CORE, (resource.RLIM_INFINITY, resource.RLIM_INFINITY))
            except OSError:
                # A restrictive hard limit should not turn the evidence collector into a
                # startup failure.  The report still records WCOREDUMP when the kernel allows it.
                pass
            os.dup2(log_fd, sys.stdout.fileno())
            os.dup2(log_fd, sys.stderr.fileno())
            os.close(log_fd)
            os.execvpe(args.command[0], args.command, child_env)
        except BaseException as error:
            message = f"crash-supervise: child setup/exec failed: {error!r}\n"
            os.write(sys.stderr.fileno(), message.encode())
            os._exit(127)
    os.close(log_fd)

    window: tuple[str, tuple[int, int, int, int], str] | None = None
    wait_status: int | None = None
    timed_out = False
    screenshot_result: str | None = None
    sample_records: list[tuple[float, int]] = []
    next_sample = started

    def record_sample(at: float, sample_window: tuple[str, tuple[int, int, int, int], str] | None) -> None:
        nonlocal next_sample
        if sample_window is None:
            return
        capture_screen(args.display, sample_screenshot, sample_window)
        colors = image_color_count(sample_screenshot) or 0
        sample_records.append((at - started, colors))
        next_sample = at + args.sample

    print(f"supervising pid {pid}: {' '.join(args.command)}", flush=True)
    try:
        while wait_status is None:
            waited_pid, candidate_status = os.waitpid(pid, os.WNOHANG)
            if waited_pid == pid:
                wait_status = candidate_status
                break
            current_window = find_window(args.display, pid)
            if current_window is not None:
                window = current_window
                now = time.monotonic()
                if now >= next_sample:
                    record_sample(now, current_window)
            if args.timeout and time.monotonic() - started >= args.timeout:
                timed_out = True
                screenshot_result = capture_screen(args.display, screenshot, window)
                now = time.monotonic()
                if window is not None and (not sample_records or now - started > sample_records[-1][0]):
                    record_sample(now, window)
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
    final_window = find_window(args.display, pid) or window
    if screenshot_result is None and final_window is not None and wait_status is not None and not os.WIFEXITED(wait_status):
        screenshot_result = capture_screen(args.display, screenshot, final_window)

    signaled = os.WIFSIGNALED(wait_status)
    exited = os.WIFEXITED(wait_status)
    exit_code = os.WEXITSTATUS(wait_status) if exited else None
    term_signal = os.WTERMSIG(wait_status) if signaled else None
    signal_name = signal.Signals(term_signal).name if term_signal is not None else None
    core_dumped = bool(os.WCOREDUMP(wait_status)) if signaled and hasattr(os, "WCOREDUMP") else False
    input_landed = bool(args.input_evidence and args.input_evidence.is_file() and args.input_evidence.stat().st_size > 0)
    classification = classify_trial(
        display_available=True,
        display_has_dri3=True,
        window_seen=window is not None,
        samples=[colors for _, colors in sample_records],
        process_exited=(exited or signaled) and not timed_out,
        driven=args.driven,
        input_landed=input_landed,
    )
    first_render_elapsed = next(
        (elapsed for elapsed, colors in sample_records if colors >= MIN_COLORS),
        None,
    )
    valid_trial_seconds = (
        max(0.0, uptime - first_render_elapsed)
        if classification.verdict == "VALID TRIAL" and first_render_elapsed is not None
        else 0.0
    )

    lines = [
        "crash-supervise report",
        f"command: {' '.join(args.command)}",
        f"pid: {pid}",
        f"started_utc: {started_at}",
        f"finished_utc: {finished_at}",
        f"uptime_seconds: {uptime:.3f}",
        f"display: {args.display}",
        f"display_probe: {display_reason}",
        f"timed_out_by_supervisor: {timed_out}",
        f"wait_status_raw: {wait_status}",
        f"WIFEXITED: {exited}",
        f"exit_code: {exit_code}",
        f"WIFSIGNALED: {signaled}",
        f"WTERMSIG: {term_signal}",
        f"signal_name: {signal_name}",
        f"core_dumped: {core_dumped}",
        f"trial_verdict: {classification.verdict}",
        f"trial_reason: {classification.reason}",
        f"rendering_started_after_seconds: {first_render_elapsed if first_render_elapsed is not None else 'None'}",
        f"valid_trial_minutes: {valid_trial_seconds / 60:.3f}",
        f"driven: {args.driven}",
        f"input_evidence: {args.input_evidence or '<none>'}",
        f"input_landed: {input_landed}",
        f"presentation_samples: {len(sample_records)}",
        *[
            f"presentation_sample_{index}: elapsed={elapsed:.3f}s colors={colors}"
            for index, (elapsed, colors) in enumerate(sample_records, start=1)
        ],
        f"output_log: {output_log}",
        f"screenshot: {screenshot_result or 'not captured'}",
        f"sample_screenshot: {sample_screenshot if sample_records else 'not captured'}",
        f"window: {final_window[0] if final_window else '<not found>'}",
        f"window_geometry: {final_window[1] if final_window else '<not found>'}",
        f"window_title: {final_window[2] if final_window else '<not found>'}",
        "",
        f"last_{args.tail}_output_lines:",
        *tail_lines(output_log, args.tail),
    ]
    report.write_text("\n".join(lines), encoding="utf-8")
    print(f"report: {report}")
    print(f"output: {output_log}")
    print(f"screen: {screenshot_result}")
    print(
        f"status: WIFEXITED={exited} exit_code={exit_code} "
        f"WIFSIGNALED={signaled} WTERMSIG={term_signal} uptime={uptime:.3f}s "
        f"trial={classification.verdict} ({classification.reason})"
    )
    return 0 if exited and exit_code == 0 else 1


if __name__ == "__main__":
    raise SystemExit(run())
