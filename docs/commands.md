# Implemented commands

This inventory describes the native byte editor and initial executable analysis. The complete HIEW contract
remains in [compatibility.md](compatibility.md); missing commands are not removed
from it. The public core exports `COMMANDS` and `Command::id()` from
[`command.rs`](../crates/hiewbie-core/src/command.rs).

| Stable core ID | Arguments | Behavior |
| --- | --- | --- |
| `file.browse` | None | Request the native file browser while viewing |
| `file.history` | None | Show visited files and saved positions |
| `file.next-recent` | None | Cycle to the next visited file |
| `file.arguments` | None | Show files supplied on the command line |
| `file.previous-argument` / `file.next-argument` | None | Move backward or forward through the argument list |
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
| `analysis.find-references` | None | Start an x86 scan for the current address |
| `analysis.next-reference` | None | Continue a reference query on the current document and profile |
| `analysis.toggle-reference-alignment` | None | Switch between instruction starts and every byte |
| `navigate.follow-branch` | Visible branch number, 1–9 | Decode again and follow a mapped direct target with its mode |
| `navigate.return-branch` | None | Restore a branch's origin or cycle through visited reference hits |
| `navigate.code` | Signed `i32` row delta | Walk decoded rows forward, or retrace recent boundaries backward |
| `analysis.resynchronize` | None | Start decoding at the selected byte and clear cached movement boundaries |
| `view.set-mode` | `Hex`, `Text`, or `Code` | Select a view without changing bytes |
| `view.cycle-mode` | None | Cycle Text → Hex → Code → Text |
| `tools.calculator` | None | Open the read-only 64-bit Calculator with wrapping/checked arithmetic and explicit `:le`/`:be` reads |
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
F9 browses in view and saves in edit mode. Edit F2 assembly and F10 truncate remain
explicitly unavailable. F4 opens a chooser for Text, Hex and Code, while Enter
cycles all three views. Code editing writes bytes through the same transaction
as Hex and Text; it does not accept assembly text.

Edit F3 and Backspace use the shared source-byte restore commands. Backspace
with an incomplete first hex digit only cancels that digit. Restores participate
in undo/redo history without being aliases for undo. They use the transaction's
source, rebased after each save.

### File browser

Start without a filename or press F9 in a file view. The browser starts in
the working directory. Use arrows, Page Up, Page Down, Home, and End to
select a row. Enter on a directory lists it. Enter on a different file opens it in
Text at offset zero. Enter on `..` returns to its parent and highlights the
previous directory. Esc returns to the current document and retains the
browser's directory, selection, sort, and mask for the next F9.

F2 toggles hidden entries. F3 through F7 select name, extension,
modification time, size, or filesystem listing order. F8 reverses the
active sort. Alt+F4 reloads the current directory. F10 edits the filename mask with `*` and `?` wildcards
while keeping directories available. F1 describes the browser keys.
`--read-only` applies to every file opened by the session.

The browser bounds each directory scan at 16,384 entries and marks a
truncated listing with `+`. It passes the native path to the same file
adapter used at startup.

### File history and arguments

Ctrl+Backspace opens history from a view. Ctrl+H is the fallback for terminals
that send BS for Ctrl+Backspace. F9 inside Files opens History. F9 inside
History returns to Files. Each list replaces the preceding one. Esc returns
to the current document.

The most recently selected file starts at the top after an explicit open.
Tab cycles the existing order. If any entries are marked, Tab cycles only
those entries. F5 toggles a mark and Ctrl+F5 and Alt+F5 mark or unmark the
entire list. F2 toggles full paths. F8 removes an inactive entry from the list.
File reads still use the native file adapter and the session's read-only setting.

Launch with `hiewbie first.bin second.bin` to use argument navigation. Ctrl+F9
shows the supplied names in order. Enter opens the selected file. Esc leaves
the document open and keeps the highlighted argument as the starting point for
Ctrl+F11 and Ctrl+F12. These keys cycle backward or forward and wrap at the ends. If the current
file came from outside the argument list, the commands enter at the last or
first argument. A failed open leaves the current document and argument
position intact. Headless inspection commands take one file.

The session keeps up to 64 visited paths. Reopening a visited file restores
its mode and byte position and uses the saved Hex or Code viewport origin
when it still fits. An unvisited file opened from either list starts in
Text at zero. On-disk history, bookmarks, and other per-file analysis state
remain separate compatibility work.

### Screen bookmarks

In Text, Hex, or Code view, press `+` to store the screen in the first free
slot. Each view has eight slots. A filled view keeps its bookmarks when
you press `+` again. The title shows digits for stored slots, a diamond for the
active slot, and dashes for empty slots.

`-` recalls the active slot. Alt+1 through Alt+8 recall a numbered slot
in the current view and make it active. Alt+Minus empties the active slot and
keeps the other numbers in place. Alt+0 clears all three views' slots without
moving the cursor. Recalling an empty slot leaves the current view as it was.

The restored screen includes the Hex top row and the selected file byte,
Text's horizontal scroll when wrapping is off, or the active Code decoder
mode. Bookmarks are held for the current open document. Stored bookmarks
across separate openings, annotations, and general navigation return remain
separate repository work.

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

### Calculator

Alt+= or Ctrl+L opens the Calculator in view or edit mode. Enter evaluates
the expression. Insert toggles between overwrite and insert, Page Up clears
the input, and Esc closes the dialog. Reopening restores the expression and
the last result. F1 shows operators, number bases, and the cursor-value tags.
The [expression contract](calculator.md) specifies precedence, associativity,
limits, errors and the distinction between observed HIEW behavior and additions.

The shared `Editor::calculate` uses hexadecimal by default. Use `0x` or
`h` for explicit hexadecimal, or the `i`, `o`, and `t` suffixes for
binary, octal, and decimal. Quoted ASCII packs up to eight bytes starting
with the high byte. Addition, subtraction, and multiplication wrap at 64
bits. Division and remainder are signed, zero divisors produce an error,
and right shifts fill with zero.

Ctrl+E evaluates the current expression with `Editor::calculate_checked`.
It reports signed 64-bit overflow in addition, subtraction, negation,
multiplication, division, and remainder. Both forms accept full 64-bit bit
patterns. Checked shifts require a count from 0 through 63 and still shift
the bit pattern with zero fill. The same expression can be evaluated again
with Enter using HIEW arithmetic.
The title marks the last checked result or error with `[checked i64]`; Enter
restores the original Calculator title. Clearing the field retains the last
result and its mode label.

`@b`, `@w`, `@d`, and `@q` read signed little-endian values of 1, 2,
4, or 8 bytes. The uppercase versions read the same widths as unsigned
values. The additive suffixes `:le` and `:be` explicitly select little or big
endian for that read; they are case-insensitive. For example, `@W:be` reads an
unsigned big-endian word and `@w:be` sign-extends the same two bytes. Different
orders may appear in one expression, and bare tags remain little endian.
Unknown widths or orders are errors. `@o` uses the local address from the current
executable or raw mapping. `@O` uses the file offset. Reads see the pending byte
transaction and report short reads. Offset tags reject byte-order suffixes. A
single pending hex digit stays in the editor and is applied only after the user
completes the byte.

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
opens Code. Automatic uses image metadata and selects x86-32 for ordinary
unrecognized raw input. L/B/E select little/big/inferred endian, R toggles raw
interpretation, and V enters a raw base. A forced raw profile or invalid image
can require an explicit decoder. Ctrl+F1/Ctrl+B cycles x86 bitness in Code. The current
decoder adapters return `Unsupported` for big-endian instruction bytes; parsing
a big-endian container does not imply instruction support.

The Decode viewport shows addresses, contiguous opcode bytes, mnemonic and
operands on rows between the title and function keys. The x86 view groups runs
of single-byte NOPs and INT3 instructions into bounded rows. Up and Down select an instruction while
keeping it visible, and Page Up and Page Down move by displayed rows.
Recognized legacy-only instructions in long mode display `#UD(64)`.
Incomplete and unrecognized encodings preserve the current byte for
inspection. See the [x86 Decode capability table](x86-decode-coverage.md)
for tested encodings and HIEW 8.63 observations.

In Code view outside edit mode, 1–9 and A–Z follow direct destinations labeled
in the displayed window. 0 or Backspace returns after a branch or cycles through
reference hits, starting with the latest hit. Windows contain at most 256 rows.
Code navigation and movement histories are bounded
to 256 entries. Backward movement retraces known boundaries; without history it
falls back to the preceding byte or ISA minimum alignment, which need not be a
true instruction boundary. `/` explicitly resynchronizes at the current byte.
Indirect/far targets and full control-flow analysis remain future work.
In Code edit mode, digits enter byte values, Backspace cancels a pending digit
or restores the preceding source byte and moves there, and vertical navigation
follows the byte grid. The active context distinguishes restoration from the
Code-view branch return command.

#### x86 reference search

F6 or Ctrl+D finds the first reference to the address at the cursor. Starting
in Hex switches to Decode when a hit is found. In Decode, Ctrl+F6 or Ctrl+T
continues forwards from the preceding hit. Esc cancels an in-progress scan and
keeps the current byte selected. A miss displays `Target not found` and leaves
the cursor on its previous byte.

Alt+F9 toggles the default instruction-aligned scan and a scan that tries every
byte as an instruction start. The scan starts at file byte zero for raw input
or at the selected image's first file-backed span. It finds x86 near branch
operands in raw files. In mapped images it also matches RIP/EIP-relative and
fixed-address memory operands and full-width address-like immediates. It skips
unmapped and ambiguous bytes. Embedded data can decode as instructions,
especially in the byte-by-byte scan. The search uses the current decoder
profile and does not resolve dynamic addresses or follow control flow.

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
