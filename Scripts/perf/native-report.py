"""Analyze explicit GPUI journal boundaries, not the nearest platform present."""

import collections
import json
from pathlib import Path
import sys


def analyze(rows, start_ns, end_ns, task_snapshot=None):
    health = [row for row in rows if row["kind"] == "health"]
    if not health or max(row["at_ns"] for row in health) < end_ns:
        raise ValueError("Journal not flushed past the measurement end")
    if any(row.get("lost", 0) for row in health) or any(row["kind"] == "lost" for row in rows):
        raise ValueError("Journal is discontinuous; latency attribution refused")
    windows = {row["window"] for row in rows if "window" in row}
    if len(windows) > 1:
        raise ValueError("InputTiming has no window ID; multiple windows are ambiguous")
    if end_ns <= start_ns:
        raise ValueError("Empty measurement interval")
    if task_snapshot is not None:
        snapshot = task_snapshot
        tasks = snapshot["tasks"]
        if (snapshot.get("schema") != "sirio-tasks-v1" or not snapshot.get("foreground_found")
                or snapshot.get("lost") != 0 or snapshot.get("retained") != len(tasks)
                or snapshot.get("total_pushed") != len(tasks)
                or snapshot["capture_start_ns"] < end_ns):
            raise ValueError("Incomplete foreground task snapshot")
        if any(task["end_ns"] < task["start_ns"] for task in tasks):
            raise ValueError("Invalid task interval")
        named = {(task["start_ns"], task["end_ns"], task["location"]) for task in tasks}
        for row in rows:
            if row["kind"] not in ("task", "small_polls"):
                continue
            if row["start_ns"] >= end_ns or row["end_ns"] <= start_ns:
                continue
            if row["kind"] == "task":
                if (row["start_ns"], row["end_ns"], row["location"]) not in named:
                    raise ValueError("Journal task missing from snapshot")
            else:
                contained = [task for task in tasks if row["start_ns"] <= task["start_ns"] and task["end_ns"] <= row["end_ns"]]
                if len(contained) != row["count"] or sum(task["end_ns"] - task["start_ns"] for task in contained) != row["total_ns"]:
                    raise ValueError("Folded task count or duration disagrees with snapshot")
        rows = [row for row in rows if row["kind"] not in ("task", "small_polls")]
        rows += [task | {"kind": "task"} for task in tasks]
    inputs = [row for row in rows if row["kind"] == "input" and row.get("input_kind") == "key_down" and start_ns <= row["start_ns"] < end_ns]
    draws = [row for row in rows if row["kind"] == "draw"]
    boundaries = [row for row in rows if row["kind"] == "presented" and row["present_end_ns"] <= end_ns]
    pairs, mid_draw, censored = [], 0, 0
    for event in inputs:
        if not event["invalidated"]:
            continue
        if any(draw["start_ns"] <= event["start_ns"] < draw["end_ns"] for draw in draws):
            mid_draw += 1
            continue
        boundary = next((row for row in boundaries if row["draw_start_ns"] >= event["end_ns"]), None)
        if boundary is None:
            censored += 1
            continue
        pairs.append({"start_ns": event["start_ns"], "draw_start_ns": boundary["draw_start_ns"], "present_end_ns": boundary["present_end_ns"], "input_to_present_ns": boundary["present_end_ns"] - event["start_ns"]})
    totals = collections.Counter()
    for row in rows:
        if row["kind"] not in ("task", "draw", "present", "action", "input", "small_polls"):
            continue
        overlap = max(0, min(end_ns, row["end_ns"]) - max(start_ns, row["start_ns"]))
        if row["kind"] == "small_polls":
            overlap *= row["total_ns"] / max(1, row["end_ns"] - row["start_ns"])
        totals[row.get("location", row["kind"])] += overlap
    return {
        "key_down_dispatches": len(inputs), "invalidating_key_down_dispatches": sum(row["invalidated"] for row in inputs),
        "input_pairs": pairs, "mid_draw_inputs": mid_draw, "right_censored_inputs": censored,
        "latency_scope": "invalidating KeyDown dispatch start to explicit newly drawn frame submission end; single window; not photon or composer-content proof",
        "top_five_inclusive_wall": [{"scope": name, "wall_pct": duration / (end_ns - start_ns) * 100} for name, duration in totals.most_common(5)],
    }


def main():
    run, filename = Path(sys.argv[1]), sys.argv[2]
    rows = [json.loads(line) for line in (run / filename).read_text().splitlines()]
    header = rows.pop(0)
    if header.get("schema") != "sirio-native-v1":
        raise ValueError("Unknown journal schema")
    snapshot_name = sys.argv[3] if len(sys.argv) > 3 else None
    snapshot = json.loads((run / snapshot_name).read_text()) if snapshot_name else None
    if snapshot is not None and (snapshot["pid"] != header["pid"] or snapshot["epoch_ns"] != header["epoch_ns"]):
        raise ValueError("Task snapshot and journal process or clock differ")
    sequence = 0
    for row in rows:
        if row["kind"] == "health":
            if row["entries"] != sequence:
                raise ValueError("Health entry count mismatch")
        else:
            if row["seq"] != sequence:
                raise ValueError("Journal sequence gap")
            sequence += 1
    samples = []
    for line in (run / "cpu.jsonl").read_text().splitlines():
        sample = json.loads(line)
        if sample["pid"] != header["pid"]:
            raise ValueError("Journal and CPU sample PIDs differ")
        sample["native"] = analyze(rows, sample["start_epoch_ns"] - header["epoch_ns"], sample["end_epoch_ns"] - header["epoch_ns"], snapshot)
        samples.append(sample)
    result = {"header": header, "samples": samples}
    if snapshot is not None:
        result["task_snapshot"] = {key: value for key, value in snapshot.items() if key != "tasks"}
    destination = "native-tasks-summary.json" if snapshot is not None else "native-summary.json"
    (run / destination).write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
