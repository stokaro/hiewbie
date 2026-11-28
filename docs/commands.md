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
| `navigate.move` | Signed `i64` delta | Clamp byte movement to the existing file range |
| `navigate.goto` | `FileOffset` | Move to a raw offset, clamped to the existing range |
| `navigate.virtual` | `VirtualAddress` | Map an absolute VA to a file-backed byte; reject missing, ambiguous, or zero-fill mappings |
| `navigate.entry` | None | Follow the selected executable's entry point |
| `navigate.region` | Zero-based region index | Follow a PE section or ELF/Mach-O segment to its first backed byte |
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
| `view.cycle-mode` | None | Cycle Hex → Text → Code → Hex |
| `file.save` | None | Return a native-save effect; require edit mode and write capability |

The native frontend owns help and input dialogs, key translation, current input
column, an incomplete hex digit, and cancellable search progress. Search itself
uses the core `Search` API over the same logical document and generation. Goto
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

Search literals are bounded, case-sensitive, forward, and do not wrap. Shift+F7,
Ctrl+Enter and Ctrl+R share the existing repeat path. Repeating from cursor+1
and literal text matching still differ from the original documentation; see the
[reference comparison](reference/hiew-8.63.md#next-development-priorities).

## Analysis contexts

F8/Ctrl+O opens executable metadata. F5 follows the entry point; F6 lists PE
sections or ELF/Mach-O segments; F4 lists numeric header fields; S lists fat
Mach-O slices. Enter follows the selected backed range or selects a slice.
Header F7/F9 imports/exports remain explicit unavailable commands.

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

After save succeeds the frontend calls `complete_save` with the verified source.
After a precommit error it retains the transaction. After a post-replacement
refresh error it keeps the error visible and requires reopening; it cannot send
further writes through stale source state.
