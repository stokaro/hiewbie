# ADR 0002: a thin native terminal and explicit legacy-key fallbacks

Status: accepted for the native byte editor and initial executable analysis. Related requirements:
UI-001..015, KEY-001..007, TERM-001..013, TEST-001/011.

## Decision

Use Ratatui 0.29.0 with its Crossterm 0.28.1 re-export. The portable core has no
terminal dependency. The CLI preserves OS-native paths through `args_os`; file
bytes and filename/error controls are escaped before rendering. Interactive
operation requires terminal stdin and stdout. `--dump`, `--headers`, and
`--disassemble` are bounded noninteractive paths through the same logical
document and analysis commands.
Native filename bytes are forwarded to the filesystem. Filesystems such as
APFS can reject non-UTF-8 names; the CLI reports that error without interpreting
filename control bytes as terminal commands. Tests cover both accepted names
and an explicit filesystem encoding rejection instead of assuming every Unix
filesystem accepts arbitrary byte names.

The first UI is a blue/cyan, keyboard-driven byte grid. At 80 columns it shows
16 bytes per row, their fixed-width ASCII representation, a 64-bit file offset,
mode, view/edit state, readonly capability, and dirty state. Narrow layouts
reduce row width; extremely small terminals show a resize message. Text mode
shows a printable-ASCII byte grid, not decoded Unicode text. Code adds bounded
instruction rows with distinct file and interpreted address domains. All three
views keep the same cursor and transaction.

Input resolves by context: active recovery state, incremental search, dialog,
then view/edit commands. The frontend dispatches portable commands for mutations
and movement; it performs native persistence only after the core returns a save
effect. See [commands.md](../commands.md) for the actual inventory.

## Keys and transaction behavior

- View F3 begins editing. Edit F9 saves; edit Esc discards. View F9 remains the
  planned Open command and never silently becomes Save.
- Enter/F4 cycles Hex, Text, and Code. Code uses the selected executable image or
  explicit raw profile; unsupported interpretations retain raw-byte access.
- Two hex digits form one byte command. An incomplete digit is UI state only,
  cannot be saved, and is cancelled by Backspace or a context/navigation change.
- Edit Tab selects hex or printable ASCII entry. Other encodings can be edited
  by byte value; Unicode text insertion is not implied.
- Ctrl+E/G/F/S/Z/Y provide edit/goto/search/save/undo/redo alternatives. Ctrl+R
  repeats search because legacy terminals often cannot distinguish Ctrl+Enter.
- F8/Ctrl+O opens headers, with F5 entry, F6 regions, F4 fields, and S slices.
  Imports/exports remain explicitly unavailable in that context.
- Shift+F1/Ctrl+A opens architecture/endian/raw-base settings. Ctrl+F1/Ctrl+B
  cycles x86 bitness in Code. Branch numbers follow direct targets, Backspace
  returns, and `/` resynchronizes at the selected byte.
- Esc cancels a search or dialog before discarding an edit, then exits from view.
  Search is forward literal bytes or explicit UTF-8 text, with repeat and no
  wrap. Wildcards, instructions, backward/selection search remain planned.
- Bracketed paste is enabled, recognized as a paste event, and rejected without
  running pasted text as keys. Non-bracketed legacy input cannot inherently be
  distinguished from typing; untrusted pasted command sequences are not a
  supported input path.

The contextual function bar marks unavailable commands with a dash. The initial
help dialog lists commands and fallbacks. Original HIEW comparisons for all
modified/contextual variants remain outstanding; these are implementation
choices against the specification, not proof of exact reference parity.

## Terminal lifecycle

A guard tracks each acquired mode before issuing the command, including partial
initialization. It independently restores the visible cursor, bracketed-paste
mode, alternate screen, and raw mode on normal return, propagated errors, and
ordinary Rust unwinding. It does not claim recovery from abort, SIGKILL, a dead
terminal connection, or every process-level signal. Mouse capture is not enabled.

Byte-grid rendering is limited to the viewport and a 64 KiB read. Executable
inspection uses the separate budgets in ADR 0004. Active search performs
one bounded step per event iteration and remains cancellable. Normal redraws
are driven by input, resizing, or search progress, not full-file scanning.
Code shows up to eight opcode bytes followed by `..` for longer instructions;
Hex remains available for their complete bytes. Unknown, truncated, and
unsupported states precede instruction text so an 80-column viewport retains
the distinction. Parser and decoder budgets are specified in
[ADR 0004](0004-analysis-backends.md).

## Unresolved terminal work

Enhanced keyboard negotiation (Kitty/CSI-u), configurable bindings, a command
menu/key diagnostic report, Option/Alt and modified F-key coverage, SSH/tmux,
Windows ConPTY interaction, and the Terminal.app/iTerm2/Kitty application matrix
remain required. macOS Fn/media keys and OS shortcuts may intercept function
keys; users can use the documented Ctrl alternatives without changing settings.
Windows native unit/file/CLI tests do not establish ConPTY behavior.

## Evidence

[Frontend tests](../../crates/hiewbie/src/terminal/tests.rs) replay semantic key
events and compare deterministic cells, including a complete small frame.
They cover nibble atomicity, state-dependent F3/F9, undo/redo/discard, readonly,
search/goto, modifiers, paste rejection, Unicode labels, empty/tiny/large
viewports, and the post-replacement recovery state.
[CLI tests](../../crates/hiewbie/tests/cli.rs) exercise the built executable.
[Unix PTY scenarios](../../scripts/pty_smoke.py) supply actual encoded keys,
verify output bytes and termios restoration, and produce a
[terminal recording](../evidence/native-editor.cast). They are automated
terminal evidence, not manual validation of every terminal application.
The [analysis recording](../evidence/executable-analysis.cast) covers generated
PE headers, entry navigation, direct branch follow/return, dotted VA goto, and
an exact one-byte save. Generated parser, decoder, application, and
[analysis CLI tests](../../crates/hiewbie/tests/analysis_cli.rs) exercise the same
portable interpretation outside the terminal.
