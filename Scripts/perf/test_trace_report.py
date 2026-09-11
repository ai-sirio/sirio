import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("trace_report", Path(__file__).with_name("trace-report.py"))
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def event(start, name, kind="span", entity=1, duration=0):
    return dict(start=start, thread=1, kind=kind, name=name, entity=entity, duration=duration)


class TraceTests(unittest.TestCase):
    def test_follow_correction_is_a_candidate_for_next_root_not_current_draw(self):
        events = [
            event(90, "Windows.draw_window", duration=80),
            event(100, "SirioWorkspace.render"),
            event(140, "request_frame.Chat.thought_follow", "event", entity=7),
            event(290, "Windows.draw_window", duration=80),
            event(300, "SirioWorkspace.render"),
            event(490, "Windows.draw_window", duration=80),
            event(500, "SirioWorkspace.render"),
            event(1100, "trace.dropped", "health", entity=0),
        ]
        result = report.summarize(events, dict(start_epoch_ns=0, end_epoch_ns=1000))
        self.assertEqual(result["root_notification_windows"]["without_observed_notification"], 3)
        self.assertEqual(result["root_render_witnesses"]["without_witness"], 2)
        self.assertEqual(result["root_render_witnesses"]["witness_sets"], {
            "request_frame.Chat.thought_follow:7": 1,
        })

    def test_motion_witness_belongs_to_its_draw_not_the_next_root(self):
        events = [
            event(90, "Windows.draw_window", duration=80),
            event(100, "SirioWorkspace.render"),
            event(140, "motion.Chat.woken", "event", entity=7),
            event(200, "notify.Chat.acp_message", "event", entity=7),
            event(290, "Windows.draw_window", duration=80),
            event(300, "SirioWorkspace.render"),
            event(490, "Windows.draw_window", duration=80),
            event(500, "SirioWorkspace.render"),
            event(1100, "trace.dropped", "health", entity=0),
        ]
        result = report.summarize(events, dict(start_epoch_ns=0, end_epoch_ns=1000))
        self.assertEqual(result["root_render_witnesses"], {
            "renders": 3, "without_witness": 1, "outside_platform_draw": 0,
            "witness_sets": {"motion.Chat.woken:7": 1, "notify.Chat.acp_message:7": 1},
            "interpretation": "notification and frame-request candidates before root plus Bezel clock witnesses in its platform draw; not exclusive causal attribution",
        })

    def test_notification_candidates_are_not_reused_or_claimed_as_causes(self):
        events = [
            event(100, "SirioWorkspace.render"),
            event(200, "notify.Terminal.output", "event", entity=7),
            event(300, "SirioWorkspace.render"),
            event(400, "SirioWorkspace.render"),
            event(500, "notify.RightPanel.refresh_complete", "event", entity=9),
            event(600, "notify.Terminal.output", "event", entity=7),
            event(700, "SirioWorkspace.render"),
            event(1100, "trace.dropped", "health", entity=0),
        ]
        result = report.summarize(events, dict(start_epoch_ns=250, end_epoch_ns=1000))
        self.assertEqual(result["root_notification_windows"], {
            "renders": 3,
            "without_observed_notification": 1,
            "candidate_sets": {
                "notify.Terminal.output:7": 1,
                "notify.RightPanel.refresh_complete:9 + notify.Terminal.output:7": 1,
            },
            "interpretation": "temporal candidates, not proven causal attribution",
        })

    def test_presentation_is_separate_from_invalidation_and_zero_render_is_retained(self):
        events = [event(0, "TerminalView.render"), event(1100, "Windows.vsync", "event"), event(1200, "Windows.vsync", "event"), event(1300, "DirectXRenderer.present"), event(2100, "trace.dropped", "health", entity=0)]
        result = report.summarize(events, dict(start_epoch_ns=1000, end_epoch_ns=2000))
        self.assertEqual(result["present_fps"], 1_000_000)
        self.assertEqual(result["view_rates"]["TerminalView.render:1"], 0)

    def test_profile_clips_spans_crossing_the_sample_boundary(self):
        events = [event(500, "work", duration=1000), event(2100, "trace.dropped", "health", entity=0)]
        result = report.summarize(events, dict(start_epoch_ns=1000, end_epoch_ns=2000))
        self.assertEqual(result["top_spans_inclusive_wall_pct"], [("work", 50)])

    def test_unflushed_or_dropped_trace_is_rejected(self):
        sample = dict(start_epoch_ns=1000, end_epoch_ns=2000)
        for events in ([event(1900, "trace.dropped", "health", entity=0)], [event(2100, "trace.dropped", "health", entity=1)]):
            with self.assertRaises(ValueError):
                report.summarize(events, sample)


if __name__ == "__main__":
    unittest.main()
