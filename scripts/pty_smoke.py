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
F3 = b"\x1bOR"
F4 = b"\x1bOS"
F5 = b"\x1b[15~"
F9 = b"\x1b[20~"
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
    """Give the child a controlling terminal and its own process group.

    This pre-exec hook only calls Unix session/terminal APIs. The harness is
    single-threaded, and stdin already refers to the slave PTY at this point.
    """
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


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
            self.process = subprocess.Popen(
                arguments,
                stdin=self.slave,
                stdout=self.slave,
                stderr=self.slave,
                cwd=self.fixture.parent,
                env=environment,
                preexec_fn=child_session,
                close_fds=True,
            )
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
            if self.process is not None and self.process.poll() is not None:
                self.fail(f"exited during startup with {self.process.returncode}")
            if all(entered in self.output for entered, _, _ in TERMINAL_MODES):
                self.settle(require_output=False)
                current = termios.tcgetattr(self.slave)
                if current[3] & (termios.ICANON | termios.ECHO):
                    self.fail("interactive session did not disable canonical input and echo")
                return
        self.fail("startup deadline expired before terminal modes were acquired")

    def settle(self, *, require_output: bool, allow_exit: bool = False) -> None:
        """Drain responses to a batch, including delayed bare-Esc decoding.

        Completion is a quiet output window after a minimum processing interval,
        not a match against differential ANSI frame strings. Disk bytes and
        terminal state supply the semantic assertions below.
        """
        started = time.monotonic()
        deadline = min(self.deadline, started + ACTION_SECONDS)
        last_output = started
        received = False
        while time.monotonic() < deadline:
            if self.read_available(0.025):
                received = True
                last_output = time.monotonic()
            if self.process is not None and self.process.poll() is not None:
                self.read_available()
                if allow_exit:
                    return
                self.fail(f"unexpected exit with {self.process.returncode}")
            now = time.monotonic()
            if (
                now - started >= 0.15
                and now - last_output >= 0.08
                and (received or not require_output)
            ):
                return
        self.fail("action deadline expired while waiting for terminal output to settle")

    def send(
        self,
        data: bytes,
        label: str,
        *,
        require_output: bool = True,
        allow_exit: bool = False,
    ) -> None:
        self.steps.append(label)
        self.event("m", label)
        if self.process is None or self.process.poll() is not None:
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
        self.settle(require_output=require_output, allow_exit=allow_exit)

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

    def finish(self) -> None:
        deadline = min(self.deadline, time.monotonic() + 5.0)
        while self.process is not None and self.process.poll() is None:
            if time.monotonic() >= deadline:
                self.fail("exit deadline expired")
            self.read_available(0.05)
        self.read_available()
        if self.process is None or self.process.returncode != 0:
            code = None if self.process is None else self.process.returncode
            self.fail(f"expected successful exit, received {code}")
        if termios.tcgetattr(self.slave) != self.original_termios:
            self.fail("termios attributes were not restored exactly")
        for entered, restored, name in TERMINAL_MODES:
            if self.output.rfind(restored) <= self.output.rfind(entered):
                self.fail(f"missing final {name} restoration sequence")
        self.event("o", self.decoder.decode(b"", final=True))

    def close(self) -> None:
        try:
            if self.process is not None and self.process.poll() is None:
                with contextlib.suppress(ProcessLookupError):
                    os.killpg(self.process.pid, signal.SIGTERM)
                try:
                    self.process.wait(timeout=1.0)
                except subprocess.TimeoutExpired:
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(self.process.pid, signal.SIGKILL)
                    self.process.wait(timeout=1.0)
        finally:
            for descriptor in (self.master, self.slave):
                if descriptor is not None:
                    os.close(descriptor)
            self.master = None
            self.slave = None


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
        session.send(F4, "F4: switch to Text")
        session.send(b"\r", "Enter: return to Hex")
        session.send(F5, "F5: goto dialog")
        session.send(b"10\r", "goto hexadecimal offset 10")
        session.send(F3, "F3: begin hex editing")
        session.send(b"7F", "overwrite one byte with 7F")
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
        session.send(ESC, "Escape: exit from view", allow_exit=True)
        session.finish()
        session.expect_file(bytes(expected))
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
        session.send(ESC, "Escape: exit after discard", allow_exit=True)
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
        session.send(ESC, "one Escape exits read-only view", allow_exit=True)
        session.finish()
        session.expect_file(FIXTURE_BYTES, mtime_ns=mtime_ns)


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


def write_recording(path: Path, events: list[list]) -> None:
    """Write v2 output/marker events without host paths or environment secrets.

    Format: https://docs.asciinema.org/manual/asciicast/v2/
    The header uses a fixed command; output events have their generated fixture
    directory redacted without changing the editor actions or file assertions.
    """
    header = {
        "version": 2,
        "width": WIDTH,
        "height": HEIGHT,
        "title": "hiewbie native editor: F3/F9, paste rejection, Ctrl fallbacks",
        "command": f"hiewbie -- {FIXTURE_NAME}",
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
            print("PASS: F3/F9 edit/save, Ctrl fallbacks, goto, Unicode path, paste rejection")
            discard_resize(binary, root / "discard")
            print("PASS: narrow resize, 80x25 restoration, discard preserves bytes and mtime")
            read_only(binary, root / "readonly")
            print("PASS: read-only edit/save refusal preserves bytes and mtime")
        if options.record is not None:
            write_recording(options.record, events)
            print(f"Recording: {options.record}")
        print("PASS: exact termios and alternate-screen/paste/cursor restoration in all sessions")
        return 0
    except (SmokeFailure, OSError, subprocess.SubprocessError) as error:
        print(f"PTY smoke failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
