"""Run: python -m unittest discover -s Scripts/perf -p 'test_*.py' -v"""

import argparse
from contextlib import closing
import importlib.util
from pathlib import Path
import sqlite3
import tempfile
import unittest


spec = importlib.util.spec_from_file_location("baseline", Path(__file__).with_name("windows-baseline.py"))
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    def test_backup_includes_uncheckpointed_wal_and_fixture_does_not_touch_source(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.sqlite"
            with closing(sqlite3.connect(source)) as database:
                database.execute("PRAGMA journal_mode=WAL")
                database.execute("CREATE TABLE setting(key TEXT PRIMARY KEY,value TEXT)")
                database.execute("INSERT INTO setting VALUES('sentinel','preserve')")
                database.commit()
                run = Path(directory) / "run"
                args = argparse.Namespace(run_dir=run, source_db=source)
                baseline.prepare(args)
                with closing(sqlite3.connect(run / "session.sqlite")) as working:
                    self.assertEqual(working.execute("SELECT value FROM setting").fetchone()[0], "preserve")
                    working.execute("UPDATE setting SET value='changed'")
                    working.commit()
                self.assertEqual(database.execute("SELECT value FROM setting").fetchone()[0], "preserve")
                with closing(sqlite3.connect(run / "snapshot.sqlite")) as snapshot:
                    self.assertEqual(snapshot.execute("SELECT value FROM setting").fetchone()[0], "preserve")
                with self.assertRaises(FileExistsError):
                    baseline.prepare(args)

    def test_launch_environment_strips_all_claude_names_without_mutating_parent(self):
        environment = {"CLAUDE_API_KEY": "secret", "claude_other": "secret", "PATH": "tools", "SIRIO_DB": "user.sqlite"}
        result = baseline.launch_environment(Path("fixture"), environment)
        self.assertFalse(any(key.upper().startswith("CLAUDE") for key in result))
        self.assertEqual(result["PATH"], "tools")
        self.assertEqual(result["SIRIO_SOCKET_ENABLE"], "1")
        self.assertEqual(environment["SIRIO_DB"], "user.sqlite")
        self.assertEqual(result["SIRIO_DB"], str(Path("fixture/session.sqlite")))

    def test_cpu_denominator_is_explicit_and_invalid_intervals_are_rejected(self):
        result = baseline.cpu_percentages(3, 15, 12)
        self.assertAlmostEqual(result["cpu_pct_machine"], 1.6666666667)
        self.assertEqual(result["cpu_pct_one_core"], 20)
        for cpu, elapsed, cores in ((-1, 15, 12), (1, 0, 12), (1, 15, 0)):
            with self.assertRaises(ValueError):
                baseline.cpu_percentages(cpu, elapsed, cores)

    def test_native_export_is_explicit_and_requires_regular_trace(self):
        run = Path("fixture")
        parent = {"SIRIO_PERF_NATIVE_TRACE": "inherited.jsonl", "SIRIO_PERF_TRACE": "inherited.tsv"}
        self.assertNotIn("SIRIO_PERF_NATIVE_TRACE", baseline.launch_environment(run, parent))
        self.assertNotIn("SIRIO_PERF_TRACE", baseline.launch_environment(run, parent))
        with self.assertRaises(ValueError):
            baseline.launch_environment(run, parent, native_trace="native.jsonl")
        result = baseline.launch_environment(run, parent, trace="trace.tsv", native_trace="native.jsonl")
        self.assertEqual(result["SIRIO_PERF_NATIVE_TRACE"], str(run / "native.jsonl"))
        self.assertEqual(result["SIRIO_PERF_TRACE"], str(run / "trace.tsv"))

    def test_task_snapshot_is_opt_in_and_requires_native_export(self):
        run = Path("fixture")
        parent = {"SIRIO_PERF_TASK_SNAPSHOT": "1"}
        self.assertNotIn("SIRIO_PERF_TASK_SNAPSHOT", baseline.launch_environment(run, parent))
        with self.assertRaises(ValueError):
            baseline.launch_environment(run, {}, task_snapshot=True)
        result = baseline.launch_environment(run, {}, trace="trace.tsv", native_trace="native.jsonl", task_snapshot=True)
        self.assertEqual(result["SIRIO_PERF_TASK_SNAPSHOT"], "1")
        self.assertEqual(parent, {"SIRIO_PERF_TASK_SNAPSHOT": "1"})


if __name__ == "__main__":
    unittest.main()
