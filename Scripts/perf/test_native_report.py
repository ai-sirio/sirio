import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("native_report", Path(__file__).with_name("native-report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


class NativeReportTests(unittest.TestCase):
    def test_snapshot_unfolds_small_polls_without_double_counting_named_tasks(self):
        rows = [
            {"kind": "small_polls", "start_ns": 10, "end_ns": 80, "count": 2, "total_ns": 50},
            {"kind": "task", "start_ns": 85, "end_ns": 95, "location": "alpha.rs:1:1"},
            {"kind": "health", "at_ns": 200, "lost": 0},
        ]
        snapshot = {
            "schema": "sirio-tasks-v1", "foreground_found": True,
            "total_pushed": 3, "retained": 3, "lost": 0,
            "capture_start_ns": 150, "capture_end_ns": 160,
            "tasks": [
                {"start_ns": 10, "end_ns": 30, "location": "alpha.rs:1:1"},
                {"start_ns": 50, "end_ns": 80, "location": "beta.rs:2:1"},
                {"start_ns": 85, "end_ns": 95, "location": "alpha.rs:1:1"},
            ],
        }
        result = report.analyze(rows, 0, 100, task_snapshot=snapshot)
        shares = {item["scope"]: item["wall_pct"] for item in result["top_five_inclusive_wall"]}
        self.assertEqual(shares, {"alpha.rs:1:1": 30.0, "beta.rs:2:1": 30.0})
        # GPUI deliberately discards folded-only work on a return to idle.
        # The full timing snapshot must retain those polls too.
        with_idle = snapshot | {
            "total_pushed": 4, "retained": 4,
            "tasks": [{"start_ns": 0, "end_ns": 5, "location": "idle.rs:3:1"}, *snapshot["tasks"]],
        }
        idle_result = report.analyze(rows, 0, 100, task_snapshot=with_idle)
        self.assertIn({"scope": "idle.rs:3:1", "wall_pct": 5.0}, idle_result["top_five_inclusive_wall"])
        for changes in ({"lost": 1}, {"foreground_found": False}, {"retained": 2}, {"total_pushed": 4}, {"capture_start_ns": 90}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                report.analyze(rows, 0, 100, task_snapshot=snapshot | changes)
        with self.assertRaises(ValueError):
            report.analyze(rows, 0, 100, task_snapshot=snapshot | {
                "tasks": [snapshot["tasks"][0] | {"end_ns": 31}, *snapshot["tasks"][1:]]
            })

    def test_explicit_boundary_pairs_inputs_and_retains_censored_and_mid_draw(self):
        rows = [
            {"kind": "input", "start_ns": 10, "end_ns": 15, "invalidated": True, "input_kind": "key_down"},
            {"kind": "input", "start_ns": 25, "end_ns": 26, "invalidated": True, "input_kind": "key_down"},
            {"kind": "draw", "start_ns": 20, "end_ns": 30, "window": 1},
            {"kind": "presented", "draw_start_ns": 20, "draw_end_ns": 30, "present_end_ns": 40, "window": 1},
            {"kind": "input", "start_ns": 50, "end_ns": 55, "invalidated": True, "input_kind": "key_down"},
            {"kind": "health", "at_ns": 100, "lost": 0},
        ]
        result = report.analyze(rows, 0, 90)
        self.assertEqual(result["input_pairs"], [{"start_ns": 10, "draw_start_ns": 20, "present_end_ns": 40, "input_to_present_ns": 30}])
        self.assertEqual(result["mid_draw_inputs"], 1)
        self.assertEqual(result["right_censored_inputs"], 1)

    def test_loss_unflushed_or_multiple_windows_refuse_latency(self):
        for rows in (
            [{"kind": "lost", "count": 2}, {"kind": "health", "at_ns": 100, "lost": 2}],
            [{"kind": "health", "at_ns": 50, "lost": 0}],
            [{"kind": "draw", "window": 1}, {"kind": "draw", "window": 2}, {"kind": "health", "at_ns": 100, "lost": 0}],
        ):
            with self.assertRaises(ValueError):
                report.analyze(rows, 0, 90)


if __name__ == "__main__":
    unittest.main()
