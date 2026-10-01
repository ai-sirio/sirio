#!/usr/bin/env python3
"""CPU and span cost of streaming into a long chat, for one Sirio build.

The existing perf fixture (Scripts/perf/acp-fixture.py: 500 seeded entries,
then 1500 chunks at 20 ms) is the agent; the real app runs it through the
restore path of a seeded chat (see Scripts/Tests/test-ely-chat-ui-e2e.py) with
the opt-in trace on (`SIRIO_PERF_TRACE`, content-free span names). It reports
the process's CPU seconds over the streaming window and the trace's span totals
for the window, so a branch and its baseline can be compared by running this
once per build:

    chat-stream-compare.py --sirio-bin PATH --display :N --out-dir DIR

Not a benchmark suite: one fixture, one window, numbers to read side by side.
"""

import argparse
import collections
import importlib.util
import json
import os
import shlex
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ely_e2e", ROOT / "Scripts" / "Tests" / "test-ely-chat-ui-e2e.py")
e2e = importlib.util.module_from_spec(spec)
sys.modules["ely_e2e"] = e2e
spec.loader.exec_module(e2e)


def cpu_seconds(pid: int) -> float:
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def spans(trace: Path, start_ns: int, end_ns: int) -> dict:
    epoch = None
    totals = collections.defaultdict(lambda: [0, 0])
    for line in trace.read_text().splitlines():
        if line.startswith("# sirio-perf-v1"):
            epoch = int(line.split("epoch_ns=")[1])
        if line.startswith("#") or epoch is None:
            continue
        parts = line.split("\t")
        if len(parts) != 6:
            continue
        start, _thread, kind, name, _entity, duration = parts
        if kind != "span" or not (start_ns <= epoch + int(start) < end_ns):
            continue
        totals[name][0] += 1
        totals[name][1] += int(duration)
    return {name: {"count": c, "ms": round(ns / 1e6, 1)} for name, (c, ns) in totals.items()}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sirio-bin", required=True)
    parser.add_argument("--display", required=True)
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--launcher", help='launch Sirio through this command line, e.g. "gdb -batch -x sample.py --args"')
    parser.add_argument("--window-size", help="WxH, via xdotool: a smaller window separates raster cost from layout cost")
    arguments = parser.parse_args()
    ns = argparse.Namespace(
        out_dir=arguments.out_dir, state_only=False, display=arguments.display,
        window_size=arguments.window_size, sirio_bin=arguments.sirio_bin, launcher=shlex.split(arguments.launcher) if arguments.launcher else None,
    )
    run = e2e.Run(ns)
    run.preflight()
    scenario = e2e.Scenario(run, "stream", "plain")
    # The perf fixture replaces the chat fixture behind the adapter's binary.
    scenario.agent.write_text(
        "#!/bin/sh\n"
        f'SIRIO_PERF_FIXTURE_DIR="{scenario.agent_dir}" exec "{sys.executable}" '
        f'"{ROOT / "Scripts" / "perf" / "acp-fixture.py"}"\n'
    )
    trace = scenario.dir / "trace.tsv"
    os.environ["GDB_SAMPLE_TRIGGER"] = str(scenario.dir / "sample-now")
    os.environ["SIRIO_PERF_TRACE"] = str(trace)
    chat = scenario.instance
    chat.start()
    chat.send("start")
    deadline = time.time() + 120
    while not (scenario.agent_dir / "ready-1").exists():
        if time.time() > deadline:
            raise SystemExit("the fixture never became ready")
        time.sleep(0.2)
    chat.until("500 seeded entries are in the transcript", lambda s: len(s["transcript"]) >= 500, timeout=120)
    time.sleep(3)
    pid = chat.process.pid
    if arguments.launcher:
        # The process is the launcher; the sampler reports instead of CPU.
        pid = int(next(Path("/proc").glob(f"{pid}/task/{pid}/children")).read_text().split()[0])
    start_cpu, start_wall, start_ns = cpu_seconds(pid), time.time(), time.time_ns()
    if arguments.launcher:
        (scenario.dir / "sample-now").write_text("now")
    (scenario.agent_dir / "go-1").write_text("go")
    chat.until("the stream completes", lambda s: s["status"] == "completed", timeout=180)
    end_cpu, end_wall, end_ns = cpu_seconds(pid), time.time(), time.time_ns()
    time.sleep(3)  # the trace writer flushes once a second
    result = {
        "sirio_bin": arguments.sirio_bin,
        "entries": len(chat.read()["transcript"]),
        "wall_seconds": round(end_wall - start_wall, 1),
        "cpu_seconds": round(end_cpu - start_cpu, 1),
        "cpu_percent_of_one_core": round(100 * (end_cpu - start_cpu) / (end_wall - start_wall), 1),
        "spans": spans(trace, start_ns, end_ns),
    }
    chat.quit()
    (Path(arguments.out_dir) / "stream-compare.json").write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
