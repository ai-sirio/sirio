#!/usr/bin/env python3
import importlib.util
import pathlib
import sys
import unittest


SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "crash-supervise.py"
SPEC = importlib.util.spec_from_file_location("crash_supervise", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules["crash_supervise"] = MODULE
SPEC.loader.exec_module(MODULE)


class TrialClassificationTests(unittest.TestCase):
    def test_two_rendering_samples_make_a_timed_out_run_valid(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=True,
            samples=[11_400, 11_100],
            process_exited=False,
            driven=False,
        )

        self.assertEqual(result.verdict, "VALID TRIAL")
        self.assertEqual(result.reason, "rendered throughout observed run")

    def test_flat_sample_after_rendering_rejects_the_run(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=True,
            samples=[11_400, 1],
            process_exited=False,
            driven=False,
        )

        self.assertEqual(result.verdict, "NOT A TRIAL")
        self.assertEqual(result.reason, "presentation became flat during observed run")

    def test_flat_startup_sample_before_two_rendering_samples_is_ignored(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=True,
            samples=[1, 6_288, 9_260],
            process_exited=False,
            driven=False,
        )

        self.assertEqual(result.verdict, "VALID TRIAL")
        self.assertEqual(result.reason, "rendered throughout observed run")

    def test_missing_dri3_is_not_a_trial(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=False,
            window_seen=False,
            samples=[],
            process_exited=False,
            driven=False,
        )

        self.assertEqual(result.verdict, "NOT A TRIAL")
        self.assertEqual(result.reason, "display has no DRI3 extension")

    def test_exit_without_a_window_is_startup_failure(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=False,
            samples=[],
            process_exited=True,
            driven=False,
        )

        self.assertEqual(result.verdict, "NOT A TRIAL")
        self.assertEqual(result.reason, "app died during startup before presenting")

    def test_exit_after_first_frame_is_still_startup_failure(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=True,
            samples=[11_400],
            process_exited=True,
            driven=False,
        )

        self.assertEqual(result.verdict, "NOT A TRIAL")
        self.assertEqual(
            result.reason, "app died during startup before two presentation samples"
        )

    def test_driven_run_requires_input_evidence(self):
        result = MODULE.classify_trial(
            display_available=True,
            display_has_dri3=True,
            window_seen=True,
            samples=[11_400, 11_100],
            process_exited=False,
            driven=True,
            input_landed=False,
        )

        self.assertEqual(result.verdict, "NOT A TRIAL")
        self.assertEqual(result.reason, "input was driven but not verified")

    def test_dri3_extension_parser_requires_the_extension_name(self):
        self.assertTrue(MODULE.dri3_from_xdpyinfo("number of extensions: 2\n    DRI3\n    RANDR\n"))
        self.assertFalse(MODULE.dri3_from_xdpyinfo("number of extensions: 1\n    RANDR\n"))


if __name__ == "__main__":
    unittest.main()
