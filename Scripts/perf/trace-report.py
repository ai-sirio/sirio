"""Summarize opt-in TSV spans over the exact CPU measurement intervals.

Usage: python Scripts/perf/trace-report.py <run-dir> <trace-name>
Profiles report inclusive span wall time / sample wall time. They are not CPU
sampling percentages and can overlap; uninstrumented functions remain unknown.
"""
from collections import Counter, defaultdict
from bisect import bisect_right
import json
from pathlib import Path
import sys


def read_trace(path):
    events = []
    epoch = None
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("# sirio-perf-v1"):
            epoch = int(line.split("epoch_ns=")[1])
        if line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 6:
            continue  # Writer may be in the middle of its last line.
        start, thread, kind, name, entity, duration = parts
        events.append({"start": epoch + int(start), "thread": int(thread), "kind": kind, "name": name, "entity": int(entity), "duration": int(duration)})
    if epoch is None:
        raise ValueError("Trace header missing")
    return events


def summarize(events, sample):
    start, end = sample["start_epoch_ns"], sample["end_epoch_ns"]
    seconds = (end - start) / 1e9
    selected = [event for event in events if start <= event["start"] < end]
    health = [event for event in events if event["kind"] == "health"]
    if not health or health[-1]["start"] < end:
        raise ValueError("Trace has not flushed past this sample yet")
    if any(event["entity"] for event in health):
        raise ValueError("Trace dropped events; counts are incomplete")
    counts = Counter(event["name"] for event in selected)
    known_views = {f'{event["name"]}:{event["entity"]}' for event in events if event["start"] < end and event["name"].endswith(".render") and not event["name"].startswith("DirectX")}
    views = Counter({name: 0 for name in known_views})
    views.update(f'{event["name"]}:{event["entity"]}' for event in selected if event["name"].endswith(".render") and not event["name"].startswith("DirectX"))
    wall = defaultdict(int)
    for event in events:
        if event["kind"] == "span":
            wall[event["name"]] += max(0, min(end, event["start"] + event["duration"]) - max(start, event["start"]))
    presents = [event for event in selected if event["name"] == "DirectXRenderer.present"]
    latencies = []
    for event in selected:
        if event["name"] == "Windows.char_input":
            completion = event["start"] + event["duration"]
            future = [paint for paint in presents if paint["start"] >= completion]
            if future:
                paint = min(future, key=lambda value: value["start"])
                latencies.append((paint["start"] + paint["duration"] - event["start"]) / 1e6)
    result = dict(sample)
    # Notification presence is only a temporal candidate. No dependency graph
    # or dirty-window cause is inferred from timestamp proximity.
    pending = defaultdict(set)
    frame_requests = defaultdict(set)
    candidates = Counter()
    root_renders = 0
    without_notification = 0
    witnessed = Counter()
    without_witness = 0
    outside_draw = 0
    draws = defaultdict(list)
    motion = defaultdict(list)
    for event in sorted(events, key=lambda value: value["start"]):
        if event["name"] == "Windows.draw_window":
            draws[event["thread"]].append(event)
        if event["kind"] == "event" and event["name"].startswith("motion."):
            motion[event["thread"]].append(event)
    draw_starts = {thread: [event["start"] for event in values] for thread, values in draws.items()}
    for event in sorted(events, key=lambda value: value["start"]):
        if event["start"] >= end:
            break
        thread = event["thread"]
        if event["kind"] == "event" and event["name"].startswith("notify."):
            pending[thread].add(f'{event["name"]}:{event["entity"]}')
        elif event["kind"] == "event" and event["name"].startswith("request_frame."):
            frame_requests[thread].add(f'{event["name"]}:{event["entity"]}')
        elif event["name"] == "SirioWorkspace.render":
            if event["start"] >= start:
                root_renders += 1
                if pending[thread]:
                    candidates[" + ".join(sorted(pending[thread]))] += 1
                else:
                    without_notification += 1
                witnesses = pending[thread] | frame_requests[thread]
                index = bisect_right(draw_starts.get(thread, []), event["start"]) - 1
                draw = draws[thread][index] if index >= 0 else None
                if draw is not None and event["start"] < draw["start"] + draw["duration"]:
                    witnesses.update(f'{tick["name"]}:{tick["entity"]}' for tick in motion[thread] if draw["start"] <= tick["start"] < draw["start"] + draw["duration"])
                else:
                    outside_draw += 1
                if witnesses:
                    witnessed[" + ".join(sorted(witnesses))] += 1
                else:
                    without_witness += 1
            pending[thread].clear()
            frame_requests[thread].clear()
    result["root_notification_windows"] = {
        "renders": root_renders,
        "without_observed_notification": without_notification,
        "candidate_sets": dict(sorted(candidates.items())),
        "interpretation": "temporal candidates, not proven causal attribution",
    }
    result["root_render_witnesses"] = {
        "renders": root_renders, "without_witness": without_witness,
        "outside_platform_draw": outside_draw,
        "witness_sets": dict(sorted(witnessed.items())),
        "interpretation": "notification and frame-request candidates before root plus Bezel clock witnesses in its platform draw; not exclusive causal attribution",
    }
    result.update(present_fps=len(presents) / seconds, rates={name: count / seconds for name, count in sorted(counts.items())}, view_rates={name: count / seconds for name, count in sorted(views.items())}, top_spans_inclusive_wall_pct=sorted(((name, nanos / (end - start) * 100) for name, nanos in wall.items() if nanos), key=lambda pair: -pair[1])[:5], key_to_present_ms=latencies, trace_dropped=0)
    return result


def main():
    run = Path(sys.argv[1])
    events = read_trace(run / sys.argv[2])
    samples = [json.loads(line) for line in (run / "cpu.jsonl").read_text().splitlines()]
    results = [summarize(events, sample) for sample in samples if "start_epoch_ns" in sample]
    (run / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
    for result in results:
        print(json.dumps(result))


if __name__ == "__main__":
    main()
