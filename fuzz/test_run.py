"""Fast runner failure-path tests; these do not start a real fuzz campaign."""

from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import run as runner


FAKE_CARGO = r'''
import json
import os
from pathlib import Path
import sys

args = sys.argv[1:]
mode = os.environ.get("HIEWBIE_TEST_FUZZ_MODE", "success")
if args == ["fuzz", "--version"]:
    print("cargo-fuzz 0.12.0")
elif args[-1:] == ["--version"]:
    print("cargo 1.92.0-nightly test")
elif "--write-corpus" in args:
    directory = Path(args[-1]) / "formats_symbols"
    directory.mkdir(parents=True)
    (directory / "known-seed").write_bytes(b"seed")
elif "metadata" in args:
    print(json.dumps({}))
elif "build" in args:
    sys.exit(9 if mode == "build-failed" else 0)
elif "run" in args:
    if mode != "missing-coverage":
        print("#2 DONE cov: 17 ft: 19 corp: 1/4b")
    if mode != "missing-executions":
        print("stat::number_of_executed_units: 2")
    sys.exit(7 if mode == "target-failed" else 0)
else:
    raise AssertionError(args)
'''


@unittest.skipUnless(os.name == "posix", "the guided runner requires process groups")
class CampaignTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "fuzz/libfuzzer").mkdir(parents=True)
        self.config = self.root / "fuzz/config.json"
        self.config.write_bytes(runner.CONFIG.read_bytes())
        (self.root / "fuzz/libfuzzer/rust-toolchain.toml").write_text(
            '[toolchain]\nchannel = "nightly-2025-09-18"\n'
        )
        (self.root / "fuzz/libfuzzer/Cargo.toml").write_text(
            '[dependencies]\nlibfuzzer-sys = "=0.4.13"\n'
        )
        (self.root / "fuzz/libfuzzer/Cargo.lock").write_text("test lock\n")
        self.cargo = self.root / "fake-cargo"
        self.cargo.write_text(f"#!{sys.executable}\n" + FAKE_CARGO)
        self.cargo.chmod(0o755)

    def campaign(self, mode):
        output = self.root / mode
        original_check_output = subprocess.check_output

        def check_output(command, **kwargs):
            if command[:2] == ["git", "rev-parse"]:
                return "1" * 40 + "\n"
            if command[:2] == ["git", "status"]:
                return b" M fuzz/run.py\n"
            if command[:2] == ["git", "ls-files"]:
                return b"fuzz/config.json\0fuzz/libfuzzer/Cargo.lock\0"
            return original_check_output(command, **kwargs)

        args = ["run.py", "--cargo", str(self.cargo), "--output", str(output),
                "--target", "formats_symbols", "--seconds", "1"]
        with patch.object(runner, "ROOT", self.root), \
             patch.object(runner, "CONFIG", self.config), \
             patch.object(sys, "argv", args), \
             patch.dict(os.environ, {"HIEWBIE_TEST_FUZZ_MODE": mode,
                                     "ASAN_OPTIONS": "detect_leaks=0"}), \
             patch.object(subprocess, "check_output", side_effect=check_output), \
             redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            status = runner.main()
        return status, json.loads((output / "manifest.json").read_text())

    def test_success_records_coverage_and_qualified_provenance(self):
        status, report = self.campaign("success")
        self.assertEqual(status, 0)
        self.assertEqual(report["status"], "passed")
        self.assertTrue(report["working_tree_dirty"])
        self.assertEqual(len(report["source_sha256"]), 64)
        self.assertEqual(report["sanitizer_environment"]["ASAN_OPTIONS"], "detect_leaks=0")
        result = report["results"][0]
        self.assertEqual((result["executions"], result["coverage_edges"]), (2, 17))
        self.assertEqual(result["initial_corpus"], result["final_corpus"])
        self.assertEqual(result["initial_corpus"]["files"], 1)

    def test_failed_or_uninstrumented_run_cannot_pass(self):
        for mode in ("target-failed", "missing-coverage", "missing-executions"):
            with self.subTest(mode=mode):
                status, report = self.campaign(mode)
                self.assertEqual(status, 1)
                self.assertEqual(report["status"], "failed")
                self.assertEqual(report["results"][0]["status"], "failed")
    def test_build_failure_retains_failed_manifest(self):
        status, report = self.campaign("build-failed")
        self.assertEqual(status, 1)
        self.assertEqual(report["status"], "failed")
        self.assertIn("instrumented fuzz build failed", report["error"])
        self.assertEqual(report["results"], [])

    def test_deadline_terminates_descendants_after_parent_exits(self):
        ready = self.root / "child-ready"
        escaped = self.root / "child-escaped"
        child = (
            "import signal, sys, time; from pathlib import Path; "
            "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            "Path(sys.argv[1]).write_text('ready'); time.sleep(0.8); "
            "Path(sys.argv[2]).write_text('escaped')"
        )
        parent = (
            "import subprocess, sys, time; "
            "subprocess.Popen([sys.executable, '-c', sys.argv[1], *sys.argv[2:]]); "
            "time.sleep(30)"
        )
        status = runner.execute(
            [sys.executable, "-c", parent, child, str(ready), str(escaped)],
            self.root / "timeout.log", 0.5, os.environ.copy()
        )
        self.assertEqual(status, 124)
        self.assertTrue(ready.exists(), "child must reach its signal handler")
        time.sleep(0.5)
        self.assertFalse(escaped.exists(), "a descendant survived the harness deadline")
        self.assertIn("deadline exceeded", (self.root / "timeout.log").read_text())


if __name__ == "__main__":
    unittest.main()
