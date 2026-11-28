# Observed HIEW 8.63 UX

This reference describes visible behavior observed on 2026-11-28. It is an
independently written implementation specification. It contains no original
binary, screen capture, artwork, source code, or copied manual passage.

## Evidence envelope

- Runtime: Windows 11 22H2, x86-64, in an isolated Windows Server Core LTSC
  2022 container with an 80 by 25 interactive console.
- Raw fixture: generated 1,024-byte sequence containing four repetitions of
  byte values `00` through `FF`.
- Structured fixture: the supplied PE32 console executable, opened read-only.
- The program identified itself in the title row as Hiew 8.63.
- The supplied profile retained `KbdFlush=On`. Batched synthetic input lost
  later queued keys, so observation used one key event followed by a settled
  screen capture.

These observations establish the named screens and transitions. They do not
cover every modifier, file format, terminal size, encoding, or error path.

## Visual grammar

The interface fills an 80 by 25 console. It has one title/status row, 22 content
rows, and one function-key row. Dense information replaces decorative chrome.

The default palette uses a blue content background with bright cyan text. The
title row is light gray. The scrollbar occupies the first cell of each content
row and uses green arrows, solid blocks, and shaded blocks. The selected cursor
cell uses a light background with dark blue text. Edited bytes and their text
counterparts use yellow. A completed block uses yellow on magenta. Dialogs use
light gray or cyan surfaces with single- or double-line box drawing. Their active
field uses a contrasting cyan background and a visible hardware cursor.

Dialogs overlay the document. Uncovered cells remain visible, preserving spatial
context. Closing a dialog redraws the underlying viewport and restores the main
function-key bar.

## Persistent frame

The title row is a compact status sentence. At 80 columns it presents, from left
to right, the file name, a downward marker, file access/status code, eight-character
operation state, optional decoding profile, optional recognized format, current
address, a vertical separator, and `Hiew 8.63 (c)SEN`.

Observed examples included `FRO --------` in initial viewing, `FWO EDITMODE`
during editable Hex mode, and `FUO --------` after an update. A raw Code view
showed `a16`. A recognized PE added `PE` and dotted virtual addresses. Treat the
three-letter access codes as observed tokens whose full semantics still require
dedicated tests.

The bottom row consists of ten fixed-width slots. Each slot starts with its
function-key number and a label of up to six characters. Empty or unavailable
commands retain their slots. This stable geometry makes contextual changes easy
to scan. Modifier keys may temporarily replace the entire bar with a second
command layer.

The left scrollbar shows top and bottom arrows, a proportional solid thumb, and
shaded track. It stays present in Text, Hex, and Code. Cursor movement updates
the current address in the title row and the highlighted cell without opening a
separate status panel.

## Primary views

Enter cycles Code to Text to Hex while preserving the logical position. F4 opens
a centered `Select mode` dialog with three rows, Text, Hex, and Decode. The current
mode is selected. Up and Down move the inverse-video selection, Enter accepts,
and Escape cancels. Its function bar maps F1 to Text, F2 to Hex, F3 to Decode,
and F4 to Next.

Hex shows 16 bytes per row. Each row contains an eight-digit address, two-digit
uppercase hex bytes grouped visually in fours with hyphen separators, and a
16-character CP437-style text column. The selected byte and corresponding text
cell share the selection treatment. Raw files show file offsets. A recognized
PE shows dotted virtual addresses and the format in the title row.

Text uses the full row width for decoded characters. It preserves the title,
scrollbar, and function bar. Its observed bar was Help, Unwrap, blank F3, Mode,
Goto, LnFeed, Search, Table, Files, Quit. Binary control bytes appeared through
the console glyph set. Empty tail space remained blue rather than introducing a
separate end-of-file panel.

Code uses one instruction per row. A row contains an address, contiguous encoded
bytes, mnemonic, and operands. Operand classes have distinct colors. The observed
raw fixture defaulted to 16-bit x86 and displayed `a16` in the title. The Code
bar changed F6 from Hex `DatRef` to `Refer` while retaining the stable slots.

## Main commands and transient input

The observed Hex bar was Help, PutBlk, Edit, Mode, Goto, DatRef, Search, Header,
Files, Quit. The Code bar substituted Refer at F6. Commands that need a value
replace the relevant screen area or open an overlay while blanking unrelated
function-key slots.

Goto uses an inline field on the first content row. The field begins near the
left edge, uses yellow placeholder cells, shows the hardware cursor, and blanks
the function bar. Escape restores the prior viewport. This interaction is more
compact than a centered form.

Search uses a wide centered box with ASCII and Hex fields. Its title line exposes
direction, scope, case, and even-alignment toggles. The active field uses a cyan
input surface. Its contextual bar exposes Help, Direct, Area, GoLast, Even, and
Asm in their corresponding function-key slots. Escape cancels and restores the
viewport.

During a forward scan of a generated, 512 MiB zero-filled sparse file for
`FF`, HIEW displayed a short processing panel with a double outline, an
offset and a shaded progress strip in the title row, and blank function
slots. Escape opened an alert. Dismissing it restored the previous byte
position. See [the sanitized observation](../../../../docs/evidence/hiew-8.63-windows-dialogs.json)
for dialog cell bounds and caret measurements.

F1 opens a large contextual help panel over the viewport. The panel title names
the current mode. It lists shared keys followed by contextual function keys,
uses color to distinguish key names and descriptions, blanks the function-key
bar, and closes with Escape. Help is a state-specific command index rather than
a separate application shell.

## Hex editing transaction

F3 enters editing from Hex. The title changes immediately to `EDITMODE`, and a
hardware cursor appears on the first hex digit of the selected byte. The bar
becomes Help, blank F2, Undo, Byte, Word, Dword, Crypt, Xor, Update, Trunc.
The Windows console reported a 12% cursor in Hex and ASCII editing, both
search fields, and Goto. The prior console size and visibility returned on exit.

Hex input is nibble-oriented. The first digit replaces the high nibble, leaves
the cursor on the low nibble, and immediately redraws the byte and text column in
yellow. The second digit completes the byte, advances to the next byte's high
nibble, and updates the title address. Left and Right navigate at nibble/byte
granularity while the title reports the logical byte address.

F3 in edit mode restores the source value of the byte under the cursor. In the
observed completed-byte scenario, it removed the yellow changed state and then
advanced to the next byte. Escape discarded pending changes and returned to view
without a confirmation box. F9 persisted pending bytes and returned directly to
view. The saved byte lost the pending-change color, the cursor position was
retained, and the title access token changed from `FWO` to `FUO`.

F10 in edit mode is labeled Trunc. Its destructive confirmation and boundary
behavior remain unobserved. Text editing and Code assembler entry were not part
of this capture.

## Block selection and export

`*` starts a block at the current byte. Movement extends the block and paints
selected hex and text cells yellow on magenta. The bottom-right corner reports
the current span in hexadecimal and decimal, observed as `00000002/2` for two
bytes. Pressing `*` again finalizes the range while leaving it highlighted.

During block interaction, a transient alternate bar exposed DelBlk, Insert,
PrtBlk, CpyBlk, MovBlk, NextSr, OffTbl, and GoCurs. The ordinary bar returned
after the key was released.

F2 with a completed block opens a centered `Write block` form. It reports the
inclusive start and end, length in hexadecimal and decimal, file field, output
offset with `*` as append, and translation table. Only F9 Files was shown in the
contextual bar. Escape canceled without writing.

## File selection

Starting without a file opens a centered file browser. Its title is the current
directory. Rows contain name, size or parent-directory marker, DOS attributes,
date, and time. The selected row uses bright inverse video. Directories and files
share the same table geometry. A centered asterisk in the bottom border marks the
list footer.

The file-browser bar was Help, Hidden, byName, byExt, byTime, Size, Unsort,
Revers, FilHis, Filter. Arrow keys move the selection. Escape exits the program
from this startup browser.

## PE header workflow

For a recognized PE32 file, F8 opens a large overlay that summarizes COFF and
optional-header values in two aligned columns. The background viewport remains
visible around it. The bar becomes Help, Flags, Edit, GoHdr, Entry, ObjTbl,
Import, OldHdr, Export, Dir.

F6 from the PE summary opens a nested section table. Columns are number, name,
virtual size, RVA, physical size, file offset, and flags. The selected section
uses inverse video. Its bar becomes Help, Flags, Edit, Add, RVA, Offset, Clear,
Delete, Update. Escape returns one level at a time, first to the PE summary and
then to the primary view.

This observation confirms the overlay hierarchy and contextual command labels.
It does not establish behavior for imports, exports, directories, malformed PE,
or other executable formats.

## Calculator and modifier feedback

Alt+= opens a wide centered Calculator overlay. It has one active expression
field and live signed, unsigned, 64-bit binary, and split hexadecimal results.
The binary row groups bits with dot and vertical separators. The Hex row uses a
backtick between 32-bit halves and reserves a quoted character representation.
The function bar is blank except for Help.

While the Alt chord was being delivered, the bar briefly showed an alternate
layer including Global, FilBlk, CryBlk, ReLoad, String, Direct, Table, and Leave.
This confirms that modifier feedback is visible before a complete chord resolves.

## State and implementation rules

Model each screen as the product of orthogonal state, including view mode,
document access, pending edit transaction, block state, format context, address
representation, active overlay, and modifier layer. Do not encode the observed
screens as unrelated page types.

Keep overlays on a stack. Escape pops the active operation first. The PE section
table returns to the PE summary, which returns to the document. Search, Goto,
Calculator, Write block, and Help return directly to the underlying document.

Keep cursor, address, and document shared across views. Use color as redundant
state feedback, paired with title text, cursor position, range counters, or
labels. Preserve all ten bottom slots even when commands are unavailable.

At minimum, terminal snapshots should cover raw Hex, Text, raw Code, mode chooser,
Goto input, Search, clean Hex edit, pending nibble, completed pending byte,
discard, save, active/finalized block, Write block, startup file browser,
recognized PE summary, PE section table, Calculator, contextual Help, and every
Escape return path.

## Open observations

The executable acceptance criteria and evidence required to close these gaps are
tracked in [acceptance-matrix.md](acceptance-matrix.md).

- Exact semantics of `FRO`, `FWO`, and `FUO` beyond the observed transitions.
- Wide and narrow terminal reflow, resize while a dialog is active, and Unicode
  console behavior outside CP437-style glyphs.
- Shift/Ctrl/Alt function-key layers beyond the briefly observed Alt layer.
- Goto validation, search execution and continuation, error and empty-result UI.
- Text edit availability, Code assembly, references, names/comments, macros,
  timestamps, truncation confirmation, and read-only failure paths.
- Import/export/directory browsers, non-PE formats, malformed structures, and
  very large files.
