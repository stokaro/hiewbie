"""Real-process regressions for the Unix PTY harness's ownership boundary."""

from __future__ import annotations

import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

if os.name == "posix":
    import pty
    import termios

import pty_smoke


MOCK_EDITOR = """
import json
import os
from pathlib import Path
import signal
import sys
import termios
import time
import tty

original = termios.tcgetattr(0)
Path("process.json").write_text(json.dumps({
    "pid": os.getpid(), "sid": os.getsid(0), "pgrp": os.getpgrp(),
}))
tty.setraw(0)
os.write(1, b"\\x1b[?1049h\\x1b[?2004h\\x1b[?25l")
command = os.read(0, 1)
if command in (b"c", b"w"):
    os.write(1, b"Staging block output 8/8")
    if command == b"c":
        time.sleep(0.4)
        temporary = Path("staged.bin")
        temporary.write_bytes(b"ABCDABCD")
        temporary.replace("extract.bin")
    command = os.read(0, 1)
if command == b"!":
    os.kill(os.getpid(), signal.SIGTERM)
if command == b"s":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    time.sleep(60)
if RESTORE:
    termios.tcsetattr(0, termios.TCSANOW, original)
os.write(1, b"\\x1b[?25h\\x1b[?2004l\\x1b[?1049l")
if command == b"d":
    time.sleep(0.35)
sys.exit(EXIT_CODE)
"""


@unittest.skipUnless(os.name == "posix", "Unix controlling-terminal semantics required")
class PtyLifecycleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="hiewbie-pty-test-")
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def session(self, *, exit_code: int = 0, restore: bool = True) -> pty_smoke.PtySession:
        script = self.directory / "mock-editor.py"
        script.write_text(f"EXIT_CODE = {exit_code!r}\nRESTORE = {restore!r}\n" + MOCK_EDITOR)
        # Python accepts the harness's `-- <fixture>` arguments without a shell.
        return pty_smoke.PtySession(Path(sys.executable), script, self.id())

    def process_info(self) -> dict:
        return json.loads((self.directory / "process.json").read_text())

    def output_blocked_owner(self) -> pty_smoke.PtySession:
        """Create a session owner whose output exceeds the PTY queue capacity."""
        session = pty_smoke.PtySession(Path(sys.executable), self.directory / "unused", self.id())
        session.master, session.slave = pty.openpty()
        os.set_blocking(session.master, False)
        session.process = subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import os\npending = b'x' * 131072\n"
                "while pending:\n    pending = pending[os.write(1, pending):]\n",
            ],
            stdin=session.slave,
            stdout=session.slave,
            stderr=session.slave,
            preexec_fn=pty_smoke.child_session,
            close_fds=True,
        )
        self.addCleanup(session.close)
        return session

    def test_normal_reap_captures_output_instead_of_blocking_its_owner(self) -> None:
        session = self.output_blocked_owner()
        self.assertEqual(session.wait_for_supervisor(2.0, capture=True), 0)
        self.assertEqual(session.output, b"x" * 131072)

    def test_failure_cleanup_drains_without_retriggering_the_output_limit(self) -> None:
        session = self.output_blocked_owner()
        session.output.extend(b"y" * pty_smoke.MAX_OUTPUT_BYTES)
        session.close()
        self.assertEqual(session.process.returncode, 0)
        self.assertEqual(len(session.output), pty_smoke.MAX_OUTPUT_BYTES)

    def test_owner_survives_editor_until_exact_terminal_inspection(self) -> None:
        with self.session() as session:
            child = self.process_info()
            self.assertNotEqual(child["pid"], child["sid"])
            self.assertEqual(child["sid"], session.process.pid)
            self.assertEqual(child["pgrp"], session.process.pid)
            session.send(b"q", "exit mock editor", expect_exit=True)
            self.assertEqual(session.poll_editor(), 0)
            self.assertIsNone(session.process.poll(), "terminal owner exited before validation")
            session.verify_termios_restored()
            self.assertEqual(termios.tcgetattr(session.slave), session.original_termios)
            session.finish()
            self.assertEqual(session.process.returncode, 0)

    def test_delayed_nonzero_editor_exit_is_captured_after_terminal_output(self) -> None:
        with self.session(exit_code=7) as session:
            session.send(b"d", "delayed nonzero mock exit", expect_exit=True)
            self.assertEqual(session.poll_editor(), 7)
            with self.assertRaisesRegex(pty_smoke.SmokeFailure, "editor exit, received 7"):
                session.finish()

    def test_delayed_block_commit_outlives_quiet_output(self) -> None:
        destination = self.directory / "extract.bin"
        for existing in (False, True):
            with self.subTest(existing=existing):
                if existing:
                    destination.write_bytes(b"ABCD")
                with self.session() as session:
                    session.send(b"c", "confirm delayed block commit", commit_path=destination)
                    self.assertEqual(destination.read_bytes(), b"ABCDABCD")
                    session.send(b"q", "exit after commit", expect_exit=True)
                    session.finish()

    def test_unreplaced_destination_cannot_satisfy_commit_wait(self) -> None:
        destination = self.directory / "extract.bin"
        destination.write_bytes(b"ABCD")
        with self.session() as session:
            with mock.patch.object(pty_smoke, "ACTION_SECONDS", 0.35):
                with self.assertRaisesRegex(
                    pty_smoke.SmokeFailure, "waiting for the committed file"
                ):
                    session.send(b"w", "stage without committing", commit_path=destination)
            self.assertEqual(destination.read_bytes(), b"ABCD")
            session.send(b"q", "exit after failed commit wait", expect_exit=True)
            session.finish()

    def test_signal_exit_keeps_the_original_negative_returncode(self) -> None:
        with self.session() as session:
            session.send(b"!", "signal mock editor", expect_exit=True)
            self.assertEqual(session.poll_editor(), -signal.SIGTERM)
            with self.assertRaisesRegex(
                pty_smoke.SmokeFailure, f"editor exit, received {-signal.SIGTERM}"
            ):
                session.finish()

    def test_missing_termios_restoration_still_fails(self) -> None:
        with self.session(restore=False) as session:
            session.send(b"q", "exit without restoring raw mode", expect_exit=True)
            with self.assertRaisesRegex(pty_smoke.SmokeFailure, "termios attributes"):
                session.finish()

    def test_observational_read_rejects_unexpected_input(self) -> None:
        with self.session() as session:
            session.send(b"q", "exit mock editor", expect_exit=True)
            os.write(session.master, b"unexpected\n")
            ready, _, _ = select.select([session.slave], [], [], 1.0)
            self.assertTrue(ready, "fixture input did not reach the canonical queue")
            with self.assertRaisesRegex(pty_smoke.SmokeFailure, "unexpected input remained"):
                session.verify_termios_restored()

    def test_action_timeout_terminates_and_reaps_a_stubborn_editor(self) -> None:
        session = self.session()
        started = time.monotonic()
        with self.assertRaisesRegex(pty_smoke.SmokeFailure, "action deadline expired"):
            with session:
                child = self.process_info()
                with mock.patch.object(pty_smoke, "ACTION_SECONDS", 0.25):
                    session.send(b"s", "stall while ignoring SIGTERM")
        self.assertLess(time.monotonic() - started, 5.0)
        self.assertIsNotNone(session.process.poll())
        with self.assertRaises(ProcessLookupError):
            os.kill(child["pid"], 0)


if __name__ == "__main__":
    unittest.main()
