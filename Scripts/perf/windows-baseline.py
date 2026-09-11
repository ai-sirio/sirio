"""Local Windows baseline helper. Never modifies the source session database.

Examples (from the repository root):
  python Scripts/perf/windows-baseline.py prepare --source-db <sqlite> --run-dir <dir>
  python Scripts/perf/windows-baseline.py launch --run-dir <dir>
  python Scripts/perf/windows-baseline.py cpu --run-dir <dir> --scenario idle-focused
  python Scripts/perf/windows-baseline.py stop --run-dir <dir>

CPU is process CPU seconds / elapsed seconds / logical processors * 100.
This is NOT a frame, latency, or function-profile measurement.
"""

import argparse
from contextlib import closing
import ctypes
import csv
import datetime
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import re
import subprocess
import time


ROOT = Path(__file__).resolve().parents[2]


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def prepare(args):
    run = args.run_dir.resolve()
    run.mkdir(parents=True, exist_ok=False)
    source = args.source_db.resolve()
    with closing(sqlite3.connect(source.as_uri() + "?mode=ro", uri=True)) as original:
        with closing(sqlite3.connect(run / "snapshot.sqlite")) as snapshot:
            original.backup(snapshot)
    with closing(sqlite3.connect(run / "snapshot.sqlite")) as snapshot:
        with closing(sqlite3.connect(run / "session.sqlite")) as session:
            snapshot.backup(session)
    save(run / "environment.json", {
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "logical_processors": os.cpu_count(),
        "source_database": str(source),
        "snapshot_sha256": hashlib.sha256((run / "snapshot.sqlite").read_bytes()).hexdigest(),
    })
    print(run)


def launch_environment(run, parent, trace=None, native_trace=None, task_snapshot=False):
    if native_trace and not trace:
        raise ValueError("Native journal export requires --trace too")
    if task_snapshot and not native_trace:
        raise ValueError("Task snapshot requires --native-trace")
    environment = {key: value for key, value in parent.items() if not key.upper().startswith("CLAUDE") and key.upper() not in ("SIRIO_PERF_TRACE", "SIRIO_PERF_NATIVE_TRACE", "SIRIO_PERF_TASK_SNAPSHOT")}
    environment.update(SIRIO_DB=str(run / "session.sqlite"), SIRIO_SOCKET=str(run / "control.sock"), SIRIO_SOCKET_ENABLE="1")
    if trace:
        environment["SIRIO_PERF_TRACE"] = str(run / trace)
    if native_trace:
        environment["SIRIO_PERF_NATIVE_TRACE"] = str(run / native_trace)
    if task_snapshot:
        environment["SIRIO_PERF_TASK_SNAPSHOT"] = "1"
    return environment


def launch(args):
    run = args.run_dir.resolve()
    executable = ROOT / "rust/target/debug/sirio.exe"
    environment = launch_environment(run, os.environ, args.trace, args.native_trace, args.task_snapshot)
    if args.acp:
        environment["SIRIO_ACP_PROGRAM"] = str(args.acp.resolve())
        environment["SIRIO_PERF_FIXTURE_DIR"] = str(run)
    started = time.perf_counter_ns()
    with (run / "stdout.log").open("ab") as stdout, (run / "stderr.log").open("ab") as stderr:
        process = subprocess.Popen([str(executable)], cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
    save(run / "process.json", {"pid": process.pid, "launch_perf_counter_ns": started, "executable": str(executable), "exe_sha256": hashlib.sha256(executable.read_bytes()).hexdigest()})
    print(json.dumps({"pid": process.pid, "socket": str(run / "control.sock")}))


def fixture(args):
    run = args.run_dir.resolve()
    # Operate on the private working copy only; keep the backup untouched.
    with closing(sqlite3.connect(run / "session.sqlite")) as session:
        for table in ("chat_turn", "tab_state", "session_ref", "tab", "sidebar_state", "sidebar_expanded_project", "worktree", "project", "account_identity", "agent_account"):
            session.execute(f"DELETE FROM {table}")
        for key, value in (("session.resumeAgentSessions", "false"), ("usage.claudeVisible", "false"), ("usage.codexVisible", "false"), ("usage.ollamaVisible", "false"), ("usage.opencodeVisible", "false")):
            session.execute("INSERT OR REPLACE INTO setting(key,value) VALUES(?,?)", (key, value))
        session.commit()
    print("Private fixture reset; snapshot unchanged")


def process_times(pid):
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    kernel.OpenProcess.restype = ctypes.c_void_p
    kernel.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(ctypes.c_uint64)] * 4
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        created, exited, system, user = (ctypes.c_uint64() for _ in range(4))
        if not kernel.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(system), ctypes.byref(user)):
            raise ctypes.WinError(ctypes.get_last_error())
        if exited.value:
            raise RuntimeError("Measured process exited")
        return created.value, (system.value + user.value) / 10_000_000
    finally:
        kernel.CloseHandle(handle)


def cpu_percentages(cpu_seconds, elapsed_seconds, logical_processors):
    if cpu_seconds < 0 or elapsed_seconds <= 0 or logical_processors <= 0:
        raise ValueError("CPU delta must be nonnegative; elapsed time and processor count must be positive")
    one_core = cpu_seconds / elapsed_seconds * 100
    return {"cpu_pct_machine": one_core / logical_processors, "cpu_pct_one_core": one_core}


def cpu(args):
    run = args.run_dir.resolve()
    process = json.loads((run / "process.json").read_text())
    pid = process["pid"]
    created, before = process_times(pid)
    start_epoch_ns = time.time_ns()
    started = time.perf_counter()
    focus_samples = []
    user32 = ctypes.WinDLL("user32")
    user32.GetForegroundWindow.restype = ctypes.c_void_p
    user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    while time.perf_counter() - started < args.seconds:
        foreground_pid = ctypes.c_uint32()
        user32.GetWindowThreadProcessId(user32.GetForegroundWindow(), ctypes.byref(foreground_pid))
        focus_samples.append(foreground_pid.value == pid)
        time.sleep(min(0.1, max(0, args.seconds - (time.perf_counter() - started))))
    after_created, after = process_times(pid)
    elapsed = time.perf_counter() - started
    end_epoch_ns = time.time_ns()
    if after_created != created:
        raise RuntimeError("Process identity changed during measurement")
    result = {"scenario": args.scenario, "pid": pid, "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "elapsed_s": elapsed, "cpu_s": after - before, "logical_processors": os.cpu_count(), **cpu_percentages(after - before, elapsed, os.cpu_count())}
    result.update(start_epoch_ns=start_epoch_ns, end_epoch_ns=end_epoch_ns, foreground_samples=sum(focus_samples), focus_samples=len(focus_samples))
    with (run / "cpu.jsonl").open("a", encoding="utf-8") as output:
        output.write(json.dumps(result) + "\n")
    print(json.dumps(result))


def stop(args):
    run = args.run_dir.resolve()
    # The private socket belongs only to the process launched by this harness.
    subprocess.run([str(ROOT / "rust/target/debug/sirioctl.exe"), "--socket", str(run / "control.sock"), "quit"], check=True)


def pipe_name(path, sid):
    namespace = "\\\\.\\pipe\\Sirio\\"
    value = 0xcbf29ce484222325
    for byte in str(path).encode("utf-8"):
        value = ((value ^ byte) * 0x100000001b3) & 0xffffffffffffffff
    sanitized = re.sub(r"[^a-zA-Z0-9._-]", "_", str(path))
    budget = 256 - len(namespace) - len(sid) - 2 - 16
    return f"{namespace}{sid}-{sanitized[:budget]}-{value:016x}"


def control(args):
    rows = list(csv.reader(subprocess.check_output(["whoami", "/user", "/fo", "csv", "/nh"], text=True).splitlines()))
    sid = rows[0][1]
    name = pipe_name(args.run_dir.resolve() / "control.sock", sid)
    params = dict(item.split("=", 1) for item in args.param)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateFileW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
    kernel.CreateFileW.restype = ctypes.c_void_p
    kernel.WriteFile.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32, ctypes.POINTER(ctypes.c_uint32), ctypes.c_void_p]
    kernel.ReadFile.argtypes = kernel.WriteFile.argtypes
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    # Same identification-only SQOS as sirio_control::open_client.
    handle = kernel.CreateFileW(name, 0xc0000000, 0, None, 3, 0x00110000, None)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        payload = (json.dumps({"id": "perf", "method": args.method, "params": params}) + "\n").encode()
        written = ctypes.c_uint32()
        if not kernel.WriteFile(handle, payload, len(payload), ctypes.byref(written), None):
            raise ctypes.WinError(ctypes.get_last_error())
        response = bytearray()
        buffer = ctypes.create_string_buffer(1)
        while not response.endswith(b"\n"):
            if not kernel.ReadFile(handle, buffer, 1, ctypes.byref(written), None):
                raise ctypes.WinError(ctypes.get_last_error())
            if not written.value:
                raise RuntimeError("Control pipe closed before response")
            response.extend(buffer.raw[:written.value])
    finally:
        kernel.CloseHandle(handle)
    print(response.decode().strip())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "fixture", "launch", "cpu", "stop", "control"):
        command = commands.add_parser(name)
        command.add_argument("--run-dir", type=Path, required=True)
        if name == "prepare":
            command.add_argument("--source-db", type=Path, required=True)
        if name == "launch":
            command.add_argument("--trace")
            command.add_argument("--native-trace", help="Requires a perf-native build and --trace")
            command.add_argument("--task-snapshot", action="store_true", help="Allow one native.tasks.request marker to export native.tasks.json after sampling")
            command.add_argument("--acp", type=Path)
        if name == "cpu":
            command.add_argument("--scenario", required=True)
            command.add_argument("--seconds", type=float, default=15)
        if name == "control":
            command.add_argument("--method", required=True)
            command.add_argument("--param", action="append", default=[])
    args = parser.parse_args()
    globals()[args.command](args)


if __name__ == "__main__":
    main()
