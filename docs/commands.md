# Implemented commands

This inventory describes the first raw-byte editor. The complete HIEW contract
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
| `view.set-mode` | `Hex` or `Text` | Select a view without changing bytes |
| `view.cycle-mode` | None | Cycle the two implemented views |
| `file.save` | None | Return a native-save effect; require edit mode and write capability |

The native frontend owns help and input dialogs, key translation, current input
column, an incomplete hex digit, and cancellable search progress. Search itself
uses the core `Search` API over the same logical document and generation. Goto
uses `parse_goto`: hexadecimal default, optional `0x`, decimal suffix `t`, and
relative `+`/`-`. Dotted virtual addresses fail explicitly until a mapping backend
exists. No macro/remote protocol compatibility is implied by this initial list.

The [README key table](../README.md#current-controls) lists physical bindings.
F9 retains different view/edit meanings; edit F2 assembly and F10 truncate
remain explicitly unavailable. F4 opens a mode chooser while Enter cycles the
implemented modes; neither provides Code mode yet. Edit F3 and Backspace use
the shared restore commands; Backspace with an incomplete first hex digit only
cancels that digit. Restores participate in undo/redo history without being
aliases for undo. They use the transaction's source, rebased after each save.

Search literals are bounded, case-sensitive, forward, and do not wrap. Shift+F7,
Ctrl+Enter and Ctrl+R share the existing repeat path. Repeating from cursor+1
and literal text matching still differ from the original documentation; see the
[reference comparison](reference/hiew-8.63.md#next-development-priorities).

After save succeeds the frontend calls `complete_save` with the verified source.
After a precommit error it retains the transaction. After a post-replacement
refresh error it keeps the error visible and requires reopening; it cannot send
further writes through stale source state.
