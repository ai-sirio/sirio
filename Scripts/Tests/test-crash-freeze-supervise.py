#!/usr/bin/env python3
import importlib.util
import pathlib
import sys
import unittest


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "crash-freeze-supervise.py"
SPEC = importlib.util.spec_from_file_location("crash_freeze_supervise", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules["crash_freeze_supervise"] = MODULE
SPEC.loader.exec_module(MODULE)


class FreezeDetectorTests(unittest.TestCase):
    def test_identical_pixels_become_a_candidate_after_threshold(self):
        detector = MODULE.FreezeDetector(threshold_seconds=2.0)

        self.assertIsNone(detector.observe("frame-a", 0.0))
        self.assertIsNone(detector.observe("frame-a", 1.9))
        candidate = detector.observe("frame-a", 2.0)

        self.assertIsNotNone(candidate)
        self.assertEqual(candidate.digest, "frame-a")
        self.assertEqual(candidate.duration_seconds, 2.0)

    def test_changed_pixels_reset_the_candidate_clock(self):
        detector = MODULE.FreezeDetector(threshold_seconds=2.0)

        detector.observe("frame-a", 0.0)
        detector.observe("frame-a", 1.9)
        self.assertIsNone(detector.observe("frame-b", 2.0))
        self.assertIsNone(detector.observe("frame-b", 3.9))
        self.assertIsNotNone(detector.observe("frame-b", 4.0))


class ProcStatTests(unittest.TestCase):
    def test_parse_proc_stat_handles_parentheses_in_command_name(self):
        fields = MODULE.parse_proc_stat(
            "42 (sirio (renderer)) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14"
        )

        self.assertEqual(fields.pid, 42)
        self.assertEqual(fields.state, "S")
        self.assertEqual(fields.utime_ticks, 11)
        self.assertEqual(fields.stime_ticks, 12)


class ControlProbeTests(unittest.TestCase):
    def test_sirioctl_subcommand_precedes_global_socket_option(self):
        command = MODULE.cli_command(
            pathlib.Path("/bin/sirioctl"), pathlib.Path("/tmp/control.sock"), "ping"
        )

        self.assertEqual(
            command,
            ["/bin/sirioctl", "ping", "--socket", "/tmp/control.sock"],
        )


if __name__ == "__main__":
    unittest.main()
