#!/usr/bin/env python3
"""Exercise the native editor through a real Unix PTY using only the stdlib.

Run after `cargo build --package hiewbie --locked`:
    python3 scripts/pty_smoke.py --binary target/debug/hiewbie

Optionally write an asciicast v2 recording of the successful edit/save scenario:
    python3 scripts/pty_smoke.py --binary target/debug/hiewbie \
        --record docs/recordings/native-editor.cast

Fixtures are generated, all subprocesses and output buffers are bounded, and
only the owned PTY is modified. This is Unix PTY evidence, not Windows ConPTY
coverage or evidence for individual graphical terminal applications.
"""

from __future__ import annotations

import argparse
import codecs
import contextlib
import errno
import json
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import sys
import tempfile
import time

if os.name == "posix":
    import fcntl
    import pty
    import termios


WIDTH = 80
HEIGHT = 25
SESSION_SECONDS = 20.0
ACTION_SECONDS = 2.0
MAX_OUTPUT_BYTES = 512 * 1024
FAILURE_TAIL_BYTES = 4096
FIXTURE_BYTES = bytes(range(256)) * 4
FIXTURE_NAME = "данные-é.bin"

ESC = b"\x1b"
F2 = b"\x1bOQ"
F3 = b"\x1bOR"
F4 = b"\x1bOS"
F5 = b"\x1b[15~"
F7 = b"\x1b[18~"
F8 = b"\x1b[19~"
F9 = b"\x1b[20~"
SHIFT_F7 = b"\x1b[18;2~"
UP = b"\x1b[A"
DOWN = b"\x1b[B"
LEFT = b"\x1b[D"
RIGHT = b"\x1b[C"
PAGE_UP = b"\x1b[5~"
BACKSPACE = b"\x7f"
CTRL_E = b"\x05"
CTRL_G = b"\x07"
CTRL_S = b"\x13"
PASTE = b"\x1b[200~AA\x13\x1b[20~\x1bZZ\x1b[201~"

TERMINAL_MODES = (
    (b"\x1b[?1049h", b"\x1b[?1049l", "alternate screen"),
    (b"\x1b[?2004h", b"\x1b[?2004l", "bracketed paste"),
    (b"\x1b[?25l", b"\x1b[?25h", "cursor visibility"),
)


class SmokeFailure(RuntimeError):
    """A failed observable editor or terminal invariant."""


def child_session() -> None:
    """Give the supervisor a controlling terminal and its own process group.

    This pre-exec hook only calls Unix session/terminal APIs. The harness is
    single-threaded, and stdin already refers to the slave PTY at this point.
    """
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def supervise_editor(status_fd: int, release_fd: int, arguments: list[str]) -> int:
    """Own the controlling session until the parent validates terminal cleanup.

    Darwin revokes a controlling terminal when its session leader exits. The
    editor therefore inherits this supervisor's session and foreground group.
    Only the supervisor inherits these pipes; it sends the editor's exact signed
    returncode, then waits for validation acknowledgement. EOF cancels the run.
    """
    editor: subprocess.Popen[bytes] | None = None
    deadline = time.monotonic() + SESSION_SECONDS + 5.0
    reported = False
    try:
        editor = subprocess.Popen(arguments, close_fds=True)
        while time.monotonic() < deadline:
            returncode = editor.poll()
            if returncode is not None and not reported:
                # Four bytes fit atomically in the otherwise empty status pipe.
                os.write(status_fd, struct.pack("!i", returncode))
                os.close(status_fd)
                status_fd = -1
                reported = True
            ready, _, _ = select.select([release_fd], [], [], 0.025)
            if ready:
                acknowledgement = os.read(release_fd, 1)
                return 0 if reported and acknowledgement == b"V" else 1
        return 1
    finally:
        # Parent cancellation closes release_fd's writer. Reap the editor before
        # exiting the controlling session; a group-kill remains the outer bound.
        if editor is not None and editor.poll() is None:
            editor.terminate()
            try:
                editor.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                editor.kill()
                editor.wait(timeout=1.0)
        for descriptor in (status_fd, release_fd):
            if descriptor >= 0:
                os.close(descriptor)


class PtySession:
    def __init__(
        self,
        binary: Path,
        fixture: Path,
        name: str,
        *,
        read_only: bool = False,
        record: bool = False,
    ) -> None:
        self.binary = binary
        self.fixture = fixture
        self.name = name
        self.read_only = read_only
        self.record = record
        self.master: int | None = None
        self.slave: int | None = None
        self.process: subprocess.Popen[bytes] | None = None
        self.status_reader: int | None = None
        self.release_writer: int | None = None
        self.status_bytes = bytearray()
        self.editor_returncode: int | None = None
        self.original_termios: list | None = None
        self.output = bytearray()
        self.events: list[list] = []
        self.steps: list[str] = []
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.started = time.monotonic()
        self.deadline = self.started + SESSION_SECONDS

    def __enter__(self) -> PtySession:
        try:
            self.master, self.slave = pty.openpty()
            self.set_size(WIDTH, HEIGHT)
            self.original_termios = termios.tcgetattr(self.slave)
            os.set_blocking(self.master, False)
            arguments = [str(self.binary)]
            if self.read_only:
                arguments.append("--read-only")
            arguments.extend(["--", self.fixture.name])
            environment = dict(os.environ, TERM="xterm-256color")
            # The controlled terminal/recording has a stable color capability.
            environment.pop("NO_COLOR", None)
            self.status_reader, status_writer = os.pipe()
            release_reader, self.release_writer = os.pipe()
            os.set_blocking(self.status_reader, False)
            try:
                self.process = subprocess.Popen(
                    [
                        sys.executable,
                        str(Path(__file__).resolve()),
                        "--pty-supervisor",
                        str(status_writer),
                        str(release_reader),
                        *arguments,
                    ],
                    stdin=self.slave,
                    stdout=self.slave,
                    stderr=self.slave,
                    cwd=self.fixture.parent,
                    env=environment,
                    preexec_fn=child_session,
                    close_fds=True,
                    pass_fds=(status_writer, release_reader),
                )
            finally:
                os.close(status_writer)
                os.close(release_reader)
            self.wait_started()
            return self
        except BaseException:
            self.close()
            raise

    def __exit__(self, *_exc: object) -> None:
        self.close()

    def fail(self, message: str) -> None:
        tail = bytes(self.output[-FAILURE_TAIL_BYTES:]).decode("utf-8", "replace")
        recent = " -> ".join(self.steps[-8:]) or "startup"
        raise SmokeFailure(
            f"{self.name}: {message}\n"
            f"Recent actions: {recent}\n"
            f"PTY output tail (escaped, at most {FAILURE_TAIL_BYTES} bytes):\n{tail!r}"
        )

    def event(self, kind: str, text: str) -> None:
        if self.record and text:
            self.events.append([round(time.monotonic() - self.started, 6), kind, text])

    def poll_editor(self) -> int | None:
        """Read the editor status independently of its deliberately live owner."""
        if self.status_reader is not None:
            while len(self.status_bytes) < 4:
                try:
                    chunk = os.read(self.status_reader, 4 - len(self.status_bytes))
                except BlockingIOError:
                    break
                except InterruptedError:
                    continue
                if not chunk:
                    self.fail("PTY supervisor closed its pipe without an editor exit status")
                self.status_bytes.extend(chunk)
            if len(self.status_bytes) == 4:
                self.editor_returncode = struct.unpack("!i", self.status_bytes)[0]
                os.close(self.status_reader)
                self.status_reader = None
        if self.process is None or self.process.poll() is not None:
            self.fail("PTY supervisor exited before terminal restoration was validated")
        return self.editor_returncode

    def read_available(self, wait: float = 0.0) -> int:
        if self.master is None:
            return 0
        ready, _, _ = select.select([self.master], [], [], max(0.0, wait))
        if not ready:
            return 0
        read = 0
        while True:
            try:
                chunk = os.read(self.master, 65536)
            except BlockingIOError:
                break
            except InterruptedError:
                continue
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            read += len(chunk)
            if len(self.output) + len(chunk) > MAX_OUTPUT_BYTES:
                self.fail(f"terminal output exceeded {MAX_OUTPUT_BYTES} bytes")
            self.output.extend(chunk)
            self.event("o", self.decoder.decode(chunk))
        return read

    def wait_started(self) -> None:
        deadline = min(self.deadline, time.monotonic() + 5.0)
        while time.monotonic() < deadline:
            self.read_available(0.05)
            returncode = self.poll_editor()
            if returncode is not None:
                self.fail(f"editor exited during startup with {returncode}")
            if all(entered in self.output for entered, _, _ in TERMINAL_MODES):
                self.settle(require_output=False)
                current = termios.tcgetattr(self.slave)
                if current[3] & (termios.ICANON | termios.ECHO):
                    self.fail("interactive session did not disable canonical input and echo")
                return
        self.fail("startup deadline expired before terminal modes were acquired")

    def settle(self, *, require_output: bool, expect_exit: bool = False) -> None:
        """Drain responses to a batch, including delayed bare-Esc decoding.

        Ordinary actions settle after a quiet output window. An exit action
        must receive the editor's status from the supervisor, even after output
        stops. Disk bytes and terminal state supply the semantic assertions.
        """
        started = time.monotonic()
        deadline = min(self.deadline, started + ACTION_SECONDS)
        last_output = started
        received = False
        while time.monotonic() < deadline:
            if self.read_available(0.025):
                received = True
                last_output = time.monotonic()
            returncode = self.poll_editor()
            if returncode is not None:
                self.read_available()
                if expect_exit:
                    return
                self.fail(f"unexpected editor exit with {returncode}")
            now = time.monotonic()
            if (
                not expect_exit
                and now - started >= 0.15
                and now - last_output >= 0.08
                and (received or not require_output)
            ):
                return
        if expect_exit:
            self.fail("action deadline expired while waiting for the editor's exit status")
        self.fail("action deadline expired while waiting for terminal output to settle")

    def send(
        self,
        data: bytes,
        label: str,
        *,
        require_output: bool = True,
        expect_exit: bool = False,
    ) -> None:
        self.steps.append(label)
        self.event("m", label)
        if self.poll_editor() is not None:
            self.fail("cannot send input to an exited application")
        deadline = min(self.deadline, time.monotonic() + ACTION_SECONDS)
        pending = memoryview(data)
        while pending:
            if time.monotonic() >= deadline:
                self.fail("deadline expired while writing PTY input")
            try:
                written = os.write(self.master, pending)
                pending = pending[written:]
            except BlockingIOError:
                select.select([], [self.master], [], 0.025)
            except InterruptedError:
                continue
        self.settle(require_output=require_output, expect_exit=expect_exit)

    def set_size(self, width: int, height: int) -> None:
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))

    def resize(self, width: int, height: int) -> None:
        label = f"resize to {width}x{height}"
        self.steps.append(label)
        self.event("m", label)
        self.event("r", f"{width}x{height}")
        # The controlling PTY sends SIGWINCH to the child's foreground group.
        self.set_size(width, height)
        self.settle(require_output=True)

    def expect_file(self, expected: bytes, *, mtime_ns: int | None = None) -> None:
        with self.fixture.open("rb") as saved:
            actual = saved.read(len(expected) + 1)
        if actual != expected:
            differences = [
                index
                for index, (left, right) in enumerate(zip(actual, expected))
                if left != right
            ][:8]
            self.fail(
                f"file bytes differ at {differences}; "
                f"actual length {self.fixture.stat().st_size}, expected {len(expected)}"
            )
        if mtime_ns is not None and self.fixture.stat().st_mtime_ns != mtime_ns:
            self.fail("file modification time changed without a successful save")

    def verify_termios_restored(self) -> None:
        """Observe the completed canonical transition without rewriting flags.

        Darwin sets the kernel PENDIN state when raw mode returns to canonical.
        A nonblocking read processes that state before reporting an empty queue.
        See XNU 43a90889846e00bfb5cf1d255cdc0a701a1e05a4, tty.c:1358-1391,
        2073-2088, and 2117-2188. Keep every termios field in the exact comparison.
        """
        flags = fcntl.fcntl(self.slave, fcntl.F_GETFL)
        try:
            fcntl.fcntl(self.slave, fcntl.F_SETFL, flags | os.O_NONBLOCK)
            try:
                pending = os.read(self.slave, 1)
            except BlockingIOError:
                pending = b""
        finally:
            fcntl.fcntl(self.slave, fcntl.F_SETFL, flags)
        if pending:
            self.fail(f"unexpected input remained after editor exit: {pending!r}")
        if termios.tcgetattr(self.slave) != self.original_termios:
            self.fail("termios attributes were not restored exactly")

    def wait_for_supervisor(self, timeout: float, *, capture: bool) -> int:
        """Reap while consuming PTY output, including canonical-mode echo.

        Darwin's session-leader exit calls ttywait before revoking the terminal.
        Waiting without reading the master can therefore block exit itself.
        Cleanup discards fixed-size chunks so a prior output-budget failure is
        not raised again while the owned processes are being reaped.
        """
        deadline = time.monotonic() + timeout
        while True:
            returncode = self.process.poll()
            remaining = deadline - time.monotonic()
            if returncode is None and remaining <= 0:
                raise subprocess.TimeoutExpired(self.process.args, timeout)
            wait = min(0.025, max(0.0, remaining)) if returncode is None else 0.0
            if capture:
                self.read_available(wait)
            elif self.master is not None:
                ready, _, _ = select.select([self.master], [], [], wait)
                if ready:
                    try:
                        os.read(self.master, 65536)
                    except (BlockingIOError, InterruptedError):
                        pass
                    except OSError as error:
                        if error.errno != errno.EIO:
                            raise
            if returncode is not None:
                return returncode

    def finish(self) -> None:
        deadline = min(self.deadline, time.monotonic() + 5.0)
        while self.poll_editor() is None:
            if time.monotonic() >= deadline:
                self.fail("exit deadline expired")
            self.read_available(0.05)
        self.read_available()
        if self.editor_returncode != 0:
            self.fail(f"expected successful editor exit, received {self.editor_returncode}")
        # Keep the controlling session leader alive through this exact assertion.
        # Skipping ENOTTY would hide both harness errors and failed restoration.
        self.verify_termios_restored()
        for entered, restored, name in TERMINAL_MODES:
            if self.output.rfind(restored) <= self.output.rfind(entered):
                self.fail(f"missing final {name} restoration sequence")
        os.write(self.release_writer, b"V")
        os.close(self.release_writer)
        self.release_writer = None
        try:
            supervisor_code = self.wait_for_supervisor(2.0, capture=True)
        except subprocess.TimeoutExpired:
            self.fail("PTY supervisor did not exit after terminal validation")
        if supervisor_code != 0:
            self.fail(f"PTY supervisor exited with {supervisor_code} after validation")
        self.event("o", self.decoder.decode(b"", final=True))

    def close(self) -> None:
        try:
            if self.release_writer is not None:
                os.close(self.release_writer)
                self.release_writer = None
            if self.process is not None and self.process.poll() is None:
                try:
                    # EOF lets the supervisor terminate and reap its child first.
                    self.wait_for_supervisor(3.0, capture=False)
                except subprocess.TimeoutExpired:
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(self.process.pid, signal.SIGTERM)
                    try:
                        self.wait_for_supervisor(1.0, capture=False)
                    except subprocess.TimeoutExpired:
                        with contextlib.suppress(ProcessLookupError):
                            os.killpg(self.process.pid, signal.SIGKILL)
                        self.wait_for_supervisor(1.0, capture=False)
        finally:
            for descriptor in (self.master, self.slave, self.status_reader):
                if descriptor is not None:
                    os.close(descriptor)
            self.master = None
            self.slave = None
            self.status_reader = None


def fixture(directory: Path) -> tuple[Path, int]:
    directory.mkdir()
    path = directory / FIXTURE_NAME
    path.write_bytes(FIXTURE_BYTES)
    # Compare the filesystem's actual precision, not the requested precision.
    fixed_ns = 1_700_000_000_123_456_789
    os.utime(path, ns=(fixed_ns, fixed_ns))
    return path, path.stat().st_mtime_ns


def edit_save(binary: Path, directory: Path, *, record: bool) -> list[list]:
    path, mtime_ns = fixture(directory)
    with PtySession(binary, path, "edit/save and paste rejection", record=record) as session:
        session.send(F4, "F4: choose view without switching immediately")
        session.send(UP, "select Text in the mode menu")
        session.send(b"\r", "Enter: confirm Text view")
        session.send(b"\r", "Enter: switch to Code")
        session.send(b"\r", "Enter: return to Hex")
        session.send(F5, "F5: goto dialog")
        session.send(b"10\r", "goto hexadecimal offset 10")
        session.send(F3, "F3: begin hex editing")
        session.send(b"7F", "overwrite one byte with 7F")
        session.send(b"AA", "overwrite a second byte for Backspace restoration")
        session.send(BACKSPACE, "Backspace: restore the previous original byte")
        session.send(RIGHT, "move to a third byte")
        session.send(b"BB", "overwrite a third byte for F3 restoration")
        session.send(LEFT, "return to that third byte")
        session.send(F3, "F3: restore the original current byte")
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(PASTE, "reject bracketed paste containing save and Escape keys")
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(F9, "F9: save the one-byte transaction")
        expected = bytearray(FIXTURE_BYTES)
        expected[0x10] = 0x7F
        session.expect_file(bytes(expected))

        session.send(CTRL_G, "Ctrl+G: goto dialog fallback")
        session.send(b"32t\r", "goto decimal offset 32")
        session.send(CTRL_E, "Ctrl+E: begin edit fallback")
        session.send(b"\t", "Tab: ASCII entry")
        session.send(b"Z", "overwrite one ASCII byte with Z")
        session.expect_file(bytes(expected))
        session.send(CTRL_S, "Ctrl+S: save fallback")
        expected[32] = ord("Z")
        session.expect_file(bytes(expected))
        session.send(ESC, "Escape: exit from view", expect_exit=True)
        session.finish()
        session.expect_file(bytes(expected))
        return sanitized_events(session.events, str(path.parent.resolve()))


def search_repeat(binary: Path, directory: Path, *, record: bool) -> list[list]:
    path, mtime_ns = fixture(directory)
    with PtySession(binary, path, "backward Shift+F7 search repeat", record=record) as session:
        session.send(F5 + b"300\r", "Goto hexadecimal offset 300")
        session.send(F7, "F7: open the two-field search dialog")
        session.send(F2, "F2: change search direction to backward")
        session.send(b"00\r", "Find a zero byte at offset 300")
        session.send(PAGE_UP, "Move the cursor below the next match")
        session.send(SHIFT_F7, "Shift+F7: resume the backward search from the last hit")
        session.send(F3, "F3: edit the repeated search result")
        session.send(b"9D", "Replace only the match at offset 200")
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(F9, "F9: save the repeated-search edit")
        expected = bytearray(FIXTURE_BYTES)
        expected[512] = 0x9D
        session.expect_file(bytes(expected))
        session.send(ESC, "Escape: exit from view", expect_exit=True)
        session.finish()
        session.expect_file(bytes(expected))
        return sanitized_events(session.events, str(path.parent.resolve()))


def block_workflow(binary: Path, directory: Path, *, record: bool) -> list[list]:
    path, mtime_ns = fixture(directory)
    with PtySession(binary, path, "reverse block, scoped search, and export", record=record) as session:
        session.send(F5 + b"5\r", "Goto file offset 5")
        session.send(b"*", "Start the block at offset 5")
        session.send(LEFT * 3, "Extend the block backward to offset 2")
        session.send(b"*", "Finalize inclusive block 2–5")
        session.send(b"[", "Go to the first block byte")
        session.send(F7 + F4, "F4 in Search selects the marked block")
        session.send(b"03\r", "Find byte 03 within the block")
        session.send(SHIFT_F7, "The next copy of 03 lies outside the block")
        session.send(F3 + b"A6", "Change the scoped match at offset 3")
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(F9, "Save exactly the scoped match")
        expected = bytearray(FIXTURE_BYTES)
        expected[3] = 0xA6
        session.expect_file(bytes(expected))
        saved_mtime = path.stat().st_mtime_ns

        session.send(F2 + b"canceled.bin", "Open PutBlk with a test destination")
        session.send(ESC, "Cancel the export before committing")
        if (directory / "canceled.bin").exists():
            session.fail("canceling the block form created a destination")

        session.send(F2 + b"extract.bin\r", "Enter a new output file")
        session.send(b"\r", "Confirm the default output offset zero")
        session.send(b"\r", "Select raw bytes and write the block")
        output = directory / "extract.bin"
        block = bytes(expected[2:6])
        if output.read_bytes() != block:
            session.fail("new output does not contain exactly the selected block")

        session.send(F2 + b"extract.bin\r", "Choose the same existing output file")
        session.send(b"*\r", "Select append at the output offset")
        session.send(b"\r", "Stage confirmation for the existing output")
        if output.read_bytes() != block:
            session.fail("the output changed before explicit confirmation")
        session.send(b"\r", "Confirm append and commit")
        if output.read_bytes() != block * 2:
            session.fail("append changed or misplaced bytes in the existing output")
        session.expect_file(bytes(expected), mtime_ns=saved_mtime)
        session.send(ESC, "Exit from view", expect_exit=True)
        session.finish()
        return sanitized_events(session.events, str(path.parent.resolve()))


def discard_resize(binary: Path, directory: Path) -> None:
    path, mtime_ns = fixture(directory)
    with PtySession(binary, path, "discard and resize") as session:
        session.send(F3, "F3: begin editing")
        session.send(b"FE", "overwrite a byte without saving")
        session.resize(20, 4)
        session.resize(WIDTH, HEIGHT)
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(ESC, "Escape: discard and remain in view")
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(ESC, "Escape: exit after discard", expect_exit=True)
        session.finish()
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)


def read_only(binary: Path, directory: Path) -> None:
    path, mtime_ns = fixture(directory)
    with PtySession(binary, path, "read-only refusal", read_only=True) as session:
        session.send(F3, "F3: refused read-only edit")
        session.send(b"DE", "typed hex must not enter an edit", require_output=False)
        session.send(F9, "F9: view binding must not persist")
        session.send(CTRL_E, "Ctrl+E: refused read-only edit fallback")
        session.send(b"AD", "typed hex remains inert", require_output=False)
        session.send(CTRL_S, "Ctrl+S: refused read-only save", require_output=False)
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)
        session.send(ESC, "one Escape exits read-only view", expect_exit=True)
        session.finish()
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)


def pe_fixture_bytes() -> bytes:
    """Generate PE32+ metadata and known branch bytes; no imported executable."""
    data = bytearray(0x400)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", data, 0x84, 0x8664, 1)
    struct.pack_into("<H", data, 0x94, 0xF0)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<I", data, optional + 16, 0x1000)
    struct.pack_into("<Q", data, optional + 24, 0x140000000)
    struct.pack_into("<II", data, optional + 32, 0x1000, 0x200)
    struct.pack_into("<II", data, optional + 56, 0x2000, 0x200)
    section = optional + 0xF0
    data[section:section + 5] = b".text"
    struct.pack_into("<IIII", data, section + 8, 0x20, 0x1000, 0x200, 0x200)
    struct.pack_into("<I", data, section + 36, 0x60000020)
    data[0x200:0x208] = bytes.fromhex("EB 04 90 90 90 90 C3 90")
    return bytes(data)


def analyze_edit_save(binary: Path, directory: Path, *, record: bool) -> list[list]:
    directory.mkdir()
    path = directory / "analysis.exe"
    original = pe_fixture_bytes()
    path.write_bytes(original)
    fixed_ns = 1_700_000_000_123_456_789
    os.utime(path, ns=(fixed_ns, fixed_ns))
    mtime_ns = path.stat().st_mtime_ns
    with PtySession(binary, path, "PE code, branches, and virtual goto", record=record) as session:
        session.send(F8, "F8: inspect PE32+ headers")
        session.send(F5, "Header F5: goto entry at file 200 / VA 140001000")
        session.send(F4, "F4: choose mode at the entry")
        session.send(DOWN, "select Code in the mode menu")
        session.send(b"\r", "Enter: confirm Code with inferred x86-64")
        session.send(b"1", "Branch 1: follow jmp to file 206")
        session.send(b"\x7f", "Backspace: return to file 200")
        session.send(F3, "F3: verify return location with a byte transaction")
        session.send(b"EB", "Same opcode at file 200 must leave every byte unchanged")
        session.send(F9, "F9: complete clean byte transaction")
        session.expect_file(original, mtime_ns=mtime_ns)
        session.send(F5, "F5: open virtual-address goto")
        session.send(b".140001000\r", "Map VA 140001000 to file 200")
        session.send(b"1", "Branch 1: follow again to file 206")
        session.send(F3, "F3: begin Code-mode byte editing")
        session.send(b"CC", "Temporarily replace the branch destination byte")
        session.send(BACKSPACE, "Code edit Backspace restores the byte, not branch history")
        session.send(F9, "F9: complete restored clean Code transaction")
        session.expect_file(original, mtime_ns=mtime_ns)
        session.send(F3, "F3: begin the final Code-mode byte transaction")
        session.send(b"CC", "Replace exactly the branch destination byte")
        session.expect_file(original, mtime_ns=mtime_ns)
        session.send(F9, "F9: save byte edit from Code mode")
        expected = bytearray(original)
        expected[0x206] = 0xCC
        session.expect_file(bytes(expected))
        session.send(ESC, "Escape: exit Code view", expect_exit=True)
        session.finish()
        session.expect_file(bytes(expected))
        return sanitized_events(session.events, str(path.parent.resolve()))


def symbol_fixture_bytes() -> bytes:
    """Add ordinary imports and exports to the independently generated PE."""
    data = bytearray(pe_fixture_bytes())
    data.extend(bytes(0x800 - len(data)))
    struct.pack_into("<I", data, 0x190, 0x600)
    struct.pack_into("<I", data, 0x198, 0x600)
    struct.pack_into("<I", data, 0x104, 16)
    struct.pack_into("<IIII", data, 0x108, 0x1200, 0x100, 0x1100, 40)
    struct.pack_into("<I", data, 0x300, 0x1140)
    struct.pack_into("<II", data, 0x30C, 0x1180, 0x1160)
    struct.pack_into("<Q", data, 0x340, 0x11A0)
    struct.pack_into("<Q", data, 0x360, 0x11A0)
    data[0x380:0x389] = b"demo.dll\0"
    data[0x3A2:0x3AB] = b"external\0"
    struct.pack_into("<IIIIIII", data, 0x40C, 0x1280, 1, 2, 1, 0x1240, 0x1250, 0x1258)
    struct.pack_into("<II", data, 0x440, 0x1000, 0x1290)
    struct.pack_into("<I", data, 0x450, 0x1260)
    data[0x460:0x46D] = b"entry_symbol\0"
    data[0x480:0x487] = b"sample\0"
    data[0x490:0x49E] = b"other.forward\0"
    return bytes(data)


def symbols_edit_save(binary: Path, directory: Path, *, record: bool) -> list[list]:
    directory.mkdir()
    path = directory / "symbols.exe"
    original = symbol_fixture_bytes()
    path.write_bytes(original)
    with PtySession(binary, path, "symbol targets and metadata records", record=record) as session:
        session.send(F8, "F8: open headers")
        session.send(F7, "Header F7: load ordinary imports")
        session.send(b"\r", "Enter: follow the import pointer slot at file 360")
        session.send(F3, "F3: edit the pointer slot bytes")
        session.send(b"A1", "Change only the first IAT slot byte")
        session.expect_file(original)
        session.send(F9, "F9: save the pointer slot change")
        expected = bytearray(original)
        expected[0x360] = 0xA1
        session.expect_file(bytes(expected))
        session.send(F8 + F9, "Headers then F9: refresh exports after save")
        session.send(b"\r", "Enter: follow named export at file 200")
        session.send(F3 + b"CC", "Patch the export's code byte")
        session.send(F9, "F9: save the exact export target byte")
        expected[0x200] = 0xCC
        session.expect_file(bytes(expected))
        session.send(F8 + F9 + DOWN, "Select forwarded export, which has no local target")
        session.send(b"\r", "Enter refuses the unresolved destination and keeps the dialog")
        session.send(b"r", "R: follow its export address-table record at file 444")
        session.send(F3 + b"91", "Change only the forwarded export record's first byte")
        session.send(F9, "F9: save ordinary metadata bytes without rebuilding")
        expected[0x444] = 0x91
        session.expect_file(bytes(expected))
        session.send(F8 + b"n", "N: inspect every parsed symbol after record edit")
        session.send(ESC, "Escape: close the symbol dialog")
        session.send(ESC, "Escape: exit the editor", expect_exit=True)
        session.finish()
        session.expect_file(bytes(expected))
        return sanitized_events(session.events, str(path.parent.resolve()))


def sanitized_events(events: list[list], directory: str) -> list[list]:
    """Redact the generated directory even when a read splits the path.

    Keep only a possible partial match between output events. Markers remain
    in order, and terminal input, commands, file bytes, and escape codes are
    unaffected. The recording substitutes ``<fixture>`` for this temporary path.
    """
    sanitized: list[list] = []
    pending = ""
    for timestamp, kind, text in events:
        if kind != "o":
            sanitized.append([timestamp, kind, text])
            continue
        pending = (pending + text).replace(directory, "<fixture>")
        keep = 0
        for length in range(1, min(len(pending), len(directory) - 1) + 1):
            if pending.endswith(directory[:length]):
                keep = length
        ready = pending[:-keep] if keep else pending
        pending = pending[-keep:] if keep else ""
        if ready:
            sanitized.append([timestamp, "o", ready])
    if pending:
        sanitized.append([events[-1][0], "o", pending])
    return sanitized


def write_recording(
    path: Path,
    events: list[list],
    *,
    title: str = "hiewbie native editor: F3/F9, paste rejection, Ctrl fallbacks",
    filename: str = FIXTURE_NAME,
) -> None:
    """Write v2 output/marker events without host paths or environment secrets.

    Format: https://docs.asciinema.org/manual/asciicast/v2/
    The header uses a fixed command; output events have their generated fixture
    directory redacted without changing the editor actions or file assertions.
    """
    header = {
        "version": 2,
        "width": WIDTH,
        "height": HEIGHT,
        "title": title,
        "command": f"hiewbie -- {filename}",
        "env": {"TERM": "xterm-256color"},
        "idle_time_limit": 1.0,
    }
    if events:
        header["duration"] = events[-1][0]
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="utf-8", dir=path.parent, prefix=".pty-recording-", delete=False
    ) as output:
        temporary = Path(output.name)
        try:
            for value in [header, *events]:
                output.write(json.dumps(value, ensure_ascii=False, separators=(",", ":")))
                output.write("\n")
            output.flush()
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="built native hiewbie executable")
    parser.add_argument("--record", type=Path, help="write a successful asciicast v2 recording")
    parser.add_argument("--record-analysis", type=Path, help="record PE headers, Code, branch/return, virtual goto, and byte save")
    parser.add_argument("--record-symbols", type=Path, help="record import/export target and metadata navigation with exact byte saves")
    parser.add_argument("--record-search", type=Path, help="record the two-field search, backward repeat, and exact byte save")
    parser.add_argument("--record-block", type=Path, help="record reverse selection, scoped search, and raw block export")
    options = parser.parse_args()
    if os.name != "posix":
        parser.error("Unix PTYs are required; Windows ConPTY needs a separate harness")
    binary = options.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        parser.error(f"binary is missing or not executable: {binary}")
    try:
        # These Unix targets provide /tmp. Keeping the generated path short also
        # keeps the Unicode basename visible in the initial 80-column frame.
        with tempfile.TemporaryDirectory(prefix="hiewbie-pty-", dir="/tmp") as temporary:
            root = Path(temporary)
            events = edit_save(binary, root / "save", record=options.record is not None)
            print("PASS: F4 mode menu, F3/Backspace restore, F9 save, Ctrl fallbacks, paste rejection")
            search_events = search_repeat(binary, root / "search", record=options.record_search is not None)
            print("PASS: backward Shift+F7 resumes from the last match after cursor movement")
            block_events = block_workflow(binary, root / "block", record=options.record_block is not None)
            print("PASS: reverse block, range-limited search, cancel, exact export and append")
            discard_resize(binary, root / "discard")
            print("PASS: narrow resize, 80x25 restoration, discard preserves bytes and mtime")
            read_only(binary, root / "readonly")
            print("PASS: read-only edit/save refusal preserves bytes and mtime")
            analysis_events = analyze_edit_save(binary, root / "analysis", record=options.record_analysis is not None)
            print("PASS: PE headers/entry, Code, branch/return, virtual goto, exact byte save")
            symbol_events = symbols_edit_save(binary, root / "symbols", record=options.record_symbols is not None)
            print("PASS: import slot, export target, unresolved forwarder, metadata record, exact byte saves")
        if options.record is not None:
            write_recording(options.record, events)
            print(f"Recording: {options.record}")
        if options.record_analysis is not None:
            write_recording(
                options.record_analysis,
                analysis_events,
                title="hiewbie executable analysis: PE headers, branch/return, byte patch",
                filename="analysis.exe",
            )
            print(f"Analysis recording: {options.record_analysis}")
        if options.record_symbols is not None:
            write_recording(
                options.record_symbols,
                symbol_events,
                title="hiewbie symbols: imports, exports, metadata records, exact byte saves",
                filename="symbols.exe",
            )
            print(f"Symbol recording: {options.record_symbols}")
        if options.record_search is not None:
            write_recording(
                options.record_search,
                search_events,
                title="hiewbie search: backward repeat after cursor movement",
                filename=FIXTURE_NAME,
            )
            print(f"Search recording: {options.record_search}")
        if options.record_block is not None:
            write_recording(
                options.record_block,
                block_events,
                title="hiewbie blocks: reverse selection, scoped search, export and append",
                filename=FIXTURE_NAME,
            )
            print(f"Block recording: {options.record_block}")
        print("PASS: exact termios and alternate-screen/paste/cursor restoration in all sessions")
        return 0
    except (SmokeFailure, OSError, subprocess.SubprocessError) as error:
        print(f"PTY smoke failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    if len(sys.argv) >= 5 and sys.argv[1] == "--pty-supervisor":
        raise SystemExit(supervise_editor(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4:]))
    raise SystemExit(main())
