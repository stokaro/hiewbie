# Implemented commands

This inventory describes the native byte editor and initial executable analysis. The complete HIEW contract
remains in [compatibility.md](compatibility.md); missing commands are not removed
from it. The public core exports `COMMANDS` and `Command::id()` from
[`command.rs`](../crates/hiewbie-core/src/command.rs).

| Stable core ID | Arguments | Behavior |
| --- | --- | --- |
| `edit.begin` | None | Enter edit mode unless readonly |
| `edit.discard` | None | Discard the active overlay/history and leave edit mode |
| `edit.write-byte` | `u8` | Overwrite at the cursor and advance, clamped at EOF |
| `edit.restore-byte` | None | Restore the source byte at the cursor, leaving the cursor in place |
| `edit.restore-previous-byte` | None | Restore the preceding source byte and move there; no-op at offset zero |
| `edit.undo` | None | Undo one pending command |
| `edit.redo` | None | Reapply one undone command |
| `block.toggle` | None | Start a mark, fix the tracked range, or clear the fixed block |
| `block.clear` | None | Cancel or remove the block mark |
| `block.select-all` | None | Fix a block over a nonempty file |
| `block.resize` | None | Set the tracked endpoint to the cursor and fix the block |
| `block.goto-boundary` | First or last | Navigate to an inclusive block endpoint |
| `block.export` | None | Request export of the completed block using a platform effect |
| `navigate.move` | Signed `i64` delta | Clamp byte movement to the existing file range |
| `navigate.goto` | `FileOffset` | Move to a raw offset, clamped to the existing range |
| `navigate.virtual` | `VirtualAddress` | Map an absolute VA to a file-backed byte; reject missing, ambiguous, or zero-fill mappings |
| `navigate.entry` | None | Follow the selected executable's entry point |
| `navigate.region` | Zero-based region index | Follow a PE section or ELF/Mach-O segment to its first backed byte |
| `navigate.symbol` | Global zero-based parsed entry index | Follow a definition's backed byte or a PE import pointer slot; reject unresolved targets |
| `navigate.symbol-record` | Global zero-based parsed entry index | Follow the exact interpreted source record, including unresolved entries |
| `analysis.select-slice` | Zero-based slice index | Select a fat Mach-O image and refresh its interpretation |
| `analysis.set-architecture` | Optional `DecodeMode` | Select x86-16/32/64, ARM, Thumb, or AArch64; `None` restores inferred architecture |
| `analysis.set-endian` | Optional `Endian` | Override or restore inferred instruction endianness |
| `analysis.set-raw-base` | `VirtualAddress` | Set the address of file byte zero and enable raw interpretation |
| `analysis.set-raw` | `bool` | Enable raw interpretation, or restore parsed mapping and clear the raw base |
| `navigate.follow-branch` | Visible branch number, 1–9 | Decode again and follow a mapped direct target with its mode |
| `navigate.return-branch` | None | Restore the previous file offset and interpretation |
| `navigate.code` | Signed `i32` row delta | Walk decoded rows forward, or retrace recent boundaries backward |
| `analysis.resynchronize` | None | Start decoding at the selected byte and clear cached movement boundaries |
| `view.set-mode` | `Hex`, `Text`, or `Code` | Select a view without changing bytes |
| `view.cycle-mode` | None | Cycle Text → Hex → Code → Text |
| `file.save` | None | Return a native-save effect; require edit mode and write capability |

The native frontend owns help and input dialogs, key translation, current input
column, an incomplete hex digit, and display of search progress. Search requests,
continuation, cancellation, and hit navigation use the shared `Editor` API. Goto
uses `Editor::goto_expression`: hexadecimal default, optional `0x`, decimal suffix
`t`, and relative `+`/`-` apply to file positions. An absolute dotted address,
such as `.401000`, uses the checked executable or explicit raw mapping. The
shared `parse_virtual_address` accepts full-width absolute addresses and rejects
relative expressions. No macro/remote protocol compatibility is implied by this list.

The [README key table](../README.md#current-controls) lists physical bindings.
F9 retains different view/edit meanings; edit F2 assembly and F10 truncate remain
explicitly unavailable. F4 opens a chooser for Text, Hex and Code, while Enter
cycles all three views. Code editing writes bytes through the same transaction
as Hex and Text; it does not accept assembly text.

Edit F3 and Backspace use the shared source-byte restore commands. Backspace
with an incomplete first hex digit only cancels that digit. Restores participate
in undo/redo history without being aliases for undo. They use the transaction's
source, rebased after each save.

### Terminal focus

F4 opens the mode chooser. Up and Down move the highlight and Enter selects.
F1, F2, and F3 select Text, Hex, and Decode directly. F4 while the chooser is
open cycles the view and closes the chooser. F1 opens a contextual index for
the current view, edit session, or Search prompt. Up and Down move the highlight.
Page Down selects the last row of the next page. Page Up selects the first row
of the previous page. Home and End select the first and last entries. Enter and
Esc dismiss Help and restore the previous caret or prompt state. The highlighted
entry is informational. Commands marked `(planned)` describe the original binding.

A paired byte highlight marks the current byte while viewing. An underline
marks the next Hex digit, ASCII byte, active search field, or Goto entry.
On Windows, hiewbie requests the observed HIEW height of 12% and restores
the console buffer's prior cursor size when the buffer supports native
sizing. A buffer that keeps its old size and other terminals receive
DECSCUSR underline and user-default reset requests. Terminal emulators
control their presentation. Caret position and visibility are controlled
independently.

Hex fits 16 bytes in each full-width row, with groups of four and a CP437
display column. The selected byte is marked in both columns during viewing.
The edit caret sits on the active digit or character and pending changes appear
in bright yellow. The character column changes presentation only. Search and
ASCII overwrite retain their existing input byte semantics.

PE Hex viewing uses mapped virtual addresses where available. A PE32+ title
always shows the full address. Its rows show the low eight hex digits below
89 columns. Hex editing shows file offsets, and unavailable mappings fall
back to raw offsets. The title uses hiewbie's identity and the Hex function
bar dims unsupported commands.

### Text view

Up and Down move the first visible row. Page Up and Page Down move one
viewport. F2 toggles wrapping. With wrapping off, Left and Right scroll
horizontally, and Home resets the column. These keys leave the byte offset
unchanged. F6 selects CR/LF, CR, LF, or NULL (a zero byte). Function keys
F1–F4 select a delimiter directly in that dialog. F6 advances to the next
delimiter, arrows move the selection, Enter applies it, and Esc cancels.
Ctrl+F6 toggles eight-column tab expansion. Text navigation retains byte
offsets for Goto, Search, and mode changes. F8 translation tables are pending.

### Search

F7/Ctrl+F opens the search dialog. The field labeled ASCII accepts UTF-8 and
folds ASCII A–Z and a–z.
Other UTF-8 bytes and Hex input compare exactly.
Tab or Up/Down converts the query to the other representation when possible.
F2 switches between forward and backward scans. F5 in the dialog jumps to the
most recent hit in the current document generation. F1 describes the active
bindings.

A new query scans from the cursor inclusive. Shift+F7, Ctrl+Enter, and Ctrl+R
resume one byte beyond the last hit in the original scan direction, allowing
overlapping matches. Cursor movement preserves that point. Ctrl+Home/End and
a successful view Goto reset the resume point to the destination. Cancellation
preserves the previous completed query and resume point. Searches stop at the
boundary without wrapping. Search shows an offset and a processing panel while
pending. A miss opens an alert and leaves the query exhausted until reset.
Enter or Esc dismisses the alert. Esc during a scan cancels it and opens an
acknowledgment. Dismissing it leaves the original cursor and selection intact.
Following an edit, repeat keeps the request and starts from the current cursor.

F4 switches between the file and a completed block. A match must fit entirely
inside the selected range. Repeat uses the range captured at confirmation even
if the visible mark later changes. An even-offset constraint exists in the
engine. Unicode alignment controls, legacy case mappings, wildcards, and
instruction search remain tracked in the
[reference comparison](reference/hiew-8.63.md#search-fields-direction-and-continuation).

### Blocks

In Hex and Code view, `*` starts a mark at the cursor. Movement tracks the
other endpoint in either direction. The counter shows the live length in
hexadecimal and decimal. A second `*` fixes the inclusive range. A third
clears it. Esc cancels an active mark. `[` and `]` jump to the first and last
byte. Alt+`*` moves the endpoint opposite the original anchor to the cursor
and fixes the range. Ctrl+`*` or Ctrl+K marks the whole nonempty file.
Switching to Text or entering edit mode fixes a live mark.

F2 opens raw block export. Enter accepts the path, output offset, and raw
format in sequence. Tab and Shift+Tab move between fields. A blank offset is
zero, a number is an absolute file position, and `*` appends. An existing
destination requires explicit confirmation. Existing bytes outside the
destination span are preserved. An output offset past EOF creates a zero-filled
gap. Esc during staging cancels the export before commit. Export is available
for a read-only source and requires a distinct regular output file.

## Analysis contexts

F8/Ctrl+O opens executable metadata. F5 follows the entry point; F6 lists PE
sections or ELF/Mach-O segments; F4 lists numeric header fields; S lists fat
Mach-O slices. Enter follows the selected backed range or selects a slice.
F7 lists parsed imports, F9 exports, and N all parsed entries. These pages load
symbols lazily. Enter dispatches `navigate.symbol`; R dispatches
`navigate.symbol-record`. Filtered rows retain their global entry indices, which
are tied to the current document generation and selected image. Indices are not
persistent identifiers across edits or slice changes. A PE import destination is
the IAT pointer slot, not executable code in another module. Unsupported loader
tables appear in coverage notes; an empty parsed list does not establish the
absence of runtime imports/exports. `--symbols`, `--imports` and `--exports`
provide bounded headless equivalents and print every note.

ARM/Thumb symbol metadata supplies an advisory decoder hint below an explicit
user profile. Successful absolute/entry/region/record navigation, byte-generation
changes and slice selection clear it; failure leaves the context unchanged.
Branch return restores such a hint only when the logical generation is unchanged.
This keeps mixed ARM/Thumb navigation separate from persistent user overrides.

Shift+F1/Ctrl+A opens the interpretation profile. Keys 0–6 choose automatic
architecture, x86-16, x86-32, x86-64, ARM, Thumb, or AArch64; Enter applies it and
opens Code. L/B/E select little/big/inferred endian, R toggles raw interpretation,
and V enters a raw base. Automatic architecture does not erase independent
endian/raw overrides. Ctrl+F1/Ctrl+B cycles x86 bitness in Code. The current
decoder adapters return `Unsupported` for big-endian instruction bytes; parsing
a big-endian container does not imply instruction support.

In Code view outside edit mode, 1–9 follows direct branches visible in the last requested Code window;
Backspace returns. Windows contain at most 256 rows. Movement does not silently
shrink the remembered visible window. Branch and movement histories are bounded
to 256 entries. Backward movement retraces known boundaries; without history it
falls back to the preceding byte or ISA minimum alignment, which need not be a
true instruction boundary. `/` explicitly resynchronizes at the current byte.
Indirect/far targets and complete reference indexing remain future work.
In Code edit mode, digits enter byte values, Backspace cancels a pending digit
or restores the preceding source byte and moves there, and vertical navigation
follows the byte grid. The active context distinguishes restoration from the
Code-view branch return command.

Parsed metadata is tied to the logical document generation. Edits, undo, discard,
save, and slice changes cause fresh interpretation before use. A malformed
pending header leaves raw byte editing available and cannot retain stale mappings.
Symbols use the same logical generation and selected slice, with separate lazy
read/entry/text budgets. Their failures do not invalidate otherwise usable image
mappings. Following a metadata record performs ordinary byte navigation; it
never rebuilds symbols, relocations or checksums. See the
[symbol decision](adr/0005-symbol-navigation.md) for supported tables and gaps.

After save succeeds the frontend calls `complete_save` with the verified source.
After a precommit error it retains the transaction. After a post-replacement
refresh error it keeps the error visible and requires reopening; it cannot send
further writes through stale source state.
