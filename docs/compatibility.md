# HIEW compatibility and acceptance specification

Status: **Planned**. This document specifies intended behavior.
No feature, platform, dependency, or test below is claimed as implemented or validated.
Project name, repository name, and executable name are **hiewbie**.

## 1. Purpose and evidence

Build a native Rust terminal application that preserves HIEW's editing workflow.
Compatibility includes commands, navigation, interpretation, and resulting bytes.
Visual resemblance alone is insufficient.
macOS is a first-class platform, including Apple Silicon and Intel.

Evidence categories used below:

- **Official**: the original author's HIEW feature page or HEM documentation.
- **Community**: the Hiew Documentation Project linked by the official website.
- **Proposed**: a hiewbie requirement or improvement, not a claim about HIEW.
- **Verify**: behavior requiring a recorded comparison with a lawful reference copy.

The researched official page identifies console HIEW VIII as 8.89F/8.90C.
Hiewix is a separate WinAPI presentation; it does not define hiewbie's UI technology.
Record the exact reference version, terminal, fixture, and observation date for comparisons.
The compatibility ledger must identify evidence and intentional differences per requirement.
Do not infer original behavior from another clone's implementation.

### 1.1 Official baseline

The official page advertises text, hex, and disassembler views; x86-64 assembly
and disassembly including AVX; ARMv6 disassembly; executable-format navigation;
branch navigation; instruction-pattern search; block operations; multi-file
search and replacement; macros; Unicode; a 64-bit calculator and transform
interpreter; physical/logical devices; and HEM extensions.
Its format list includes NE, LE, LX, PE/PE32+, little-endian ELF/ELF64 and
Mach-O, TE/TE64, and NetWare modules. Sources: [S1], [S3].
ARM64, broader endianness, and additional instruction sets are hiewbie additions.

## 2. Compatibility contract

- **UI-001**: Provide Text, Hex, and Code modes with a shared document and cursor.
- **UI-002**: Use a HIEW-style keymap and compact function-key bar by default.
- **UI-003**: Keep current address, bitness, architecture, mode, and dirty state visible.
- **UI-004**: Preserve useful position and navigation state across mode switches.
- **UI-005**: Resolve commands by state: viewing, editing, selection, or active dialog.
- **UI-006**: Show the effective bindings for the current state and modifiers.
- **UI-007**: Support keyboard-only operation; mouse operation is supplementary.
- **UI-008**: Retain the familiar dense layout at 80x25; use larger terminals effectively.
- **UI-009**: Offer original-style colors plus accessible alternatives without workflow changes.
- **UI-010**: Keep every command reachable when the terminal cannot transmit its usual chord.
- **UI-011**: Record visible deviations explicitly; never silently redefine a familiar command.
- **UI-012**: Provide contextual help, a command index, and key-event diagnostics.
- **UI-013**: Opening a file must not wait for whole-file disassembly or reference indexing.
- **UI-014**: Work offline; ordinary inspection and editing must not require a service.
- **UI-015**: Restore terminal state after normal exit, errors, and recoverable failures.

### 2.1 Initial binding inventory

This is a condensed community-documented baseline, not a complete keymap. Source: [S2].
Bindings remain **Planned**. Validate their contextual details before declaring parity.

| Context | Keys | Action |
|---|---|---|
| View | Enter / F4 | Cycle / choose mode |
| View | F3 | Begin editing |
| Edit | F9 / Esc | Save / discard |
| Edit | Tab | Hex/characters or opcodes/assembly |
| View | F5 | Goto |
| View | F6 / Ctrl+F6 | References / next reference |
| View | F7 / Ctrl+Enter | Search / repeat |
| View | F8 | Executable headers |
| Headers | F5 / F6 / F7 / F9 | Entry / sections / imports / exports |
| Code | Branch numbers / Backspace | Follow / return |
| Code | Ctrl+F1 / Shift+F1 | Bitness / architecture |
| Code | / | Resynchronize decoding |
| View | * / [ / ] | Select / block boundaries |
| Blocks | Insert | Insert/overwrite |
| Blocks | F2 / Ctrl+F2 | Export/import |
| Blocks | Alt+F2 | Pattern fill |
| Code, no block | Alt+F2 | NOP instruction |
| Blocks | Shift+F2/F5/F6 | Delete/copy/move |
| View | F12 / Shift+F12 / ; | Names / name / comment |
| View | Alt+= | Calculator |
| View | Ctrl+. / Ctrl+- | Record/manage macros |
| View | F11 | Extensions |
| View | F9 / Tab | Open / next file |

- **KEY-001**: Commands have stable identifiers, independent of physical key events.
- **KEY-002**: Menus, shortcuts, macros, and automation invoke the same command handlers.
- **KEY-003**: Preserve goto syntax: hexadecimal default, decimal suffix t, relative +/-,
  and dotted virtual addresses; reject ambiguous or overflowing expressions.
- **KEY-004**: Preserve per-view navigation/bookmark behavior after recording its details.
- **KEY-005**: Verify edit-state F2 assembly, F3 byte restore, F10 truncation,
  exit/timestamp behavior, and modifier variants against the selected reference.
- **KEY-006**: Add recorded scenarios for every contextual collision, especially F2/F3/F9.
- **KEY-007**: Treat Esc as cancellation of the active operation before application exit.
- **KEY-008**: Macro recording stores meaningful commands and arguments, not terminal timing.

## 3. Document, editing, and persistence

All requirements in this section are proposed engineering contracts.

- **EDIT-001**: Raw bytes are authoritative; interpretations must not rewrite them implicitly.
- **EDIT-002**: Pending edits live in an overlay/transaction until an explicit save command.
- **EDIT-003**: Cancelling an edit transaction restores its starting state.
- **EDIT-004**: Distinguish pending transactions from previously saved changes.
- **EDIT-005**: Provide undo/redo, with defined boundaries for byte, block, and assembly edits.
- **EDIT-006**: Implement overwrite, insert, delete, copy, move, fill, import, and export.
- **EDIT-007**: Define overlapping copy/move behavior and cover it with independent fixtures.
- **EDIT-008**: Reject out-of-range operations before any document or filesystem mutation.
- **EDIT-009**: Make logical edit transactions atomic; define persistence guarantees per storage backend.
- **EDIT-010**: Make insert/overwrite state visible; never infer it from clipboard length.
- **EDIT-011**: Refresh affected disassembly, search results, and parsed structures after edits.
- **EDIT-012**: Save preserves untouched bytes exactly, including unknown fields and overlays.
- **EDIT-013**: Do not silently recalculate headers, relocations, or checksums during byte edits.
- **EDIT-014**: Expose deliberate format-aware repairs separately with a preview of changed bytes.
- **EDIT-015**: Read-only mode forbids persistence through every command and extension path.
- **EDIT-016**: Detect replacement/truncation of the backing file and avoid stale writes.
- **EDIT-017**: Failed saves retain recovery information and report the error plus any written/uncertain ranges; reconcile partial persistence before retrying.
- **EDIT-018**: Save semantics must cover permissions, symlinks, metadata, and partial I/O.
- **EDIT-019**: Preserve the executable bit on ordinary executable-file saves.
- **EDIT-020**: Display patch ranges and support exporting a reproducible patch description.
- **EDIT-021**: Define file-size-changing edits independently from executable relocation.
- **EDIT-022**: Treat malformed encodings as bytes; rendering must not destroy original data.

## 4. Addressing and large files

- **ADDR-001**: Use distinct types for file offsets, RVAs, VAs, and container/slice offsets.
- **ADDR-002**: Use checked arithmetic and 64-bit file positions throughout the core.
- **ADDR-003**: Display the address domain and preserve it in history and annotations.
- **ADDR-004**: Reject unmapped VAs explicitly; do not fabricate file offsets for zero-fill data.
- **ADDR-005**: Validate mapping ranges, overflow, overlaps, and truncated tables.
- **ADDR-006**: Resolve fat Mach-O addresses within the selected slice before global translation.
- **ADDR-007**: Allow explicit base-address, bitness, endian, and architecture overrides for raw data.
- **ADDR-008**: Keep an address override visible and separate from parsed metadata.
- **IO-001**: Opening and browsing must not require an allocation proportional to file size.
- **IO-002**: Bound viewport, parser, decoder, search, and background-analysis memory separately.
- **IO-003**: Support files beyond 4 GiB and offsets crossing 32-bit boundaries.
- **IO-004**: File access may use paging or mappings; document concurrent-truncation handling.
- **IO-005**: Searches operate incrementally and find matches crossing chunk boundaries.
- **IO-006**: Long operations provide progress and cancellation without blocking input.
- **IO-007**: Prefer measured performance budgets over an unqualified "unlimited size" claim.

## 5. Instruction-set capabilities

Host CPU and inspected CPU are independent. Every release must publish its capability table.
A decoder being available does not establish assembly, symbols, or reference-analysis support.
All rows below are **Planned**; the table does not claim backend availability.

| ID | Inspected ISA | Decode goal | Text assembly goal | Basis |
|---|---|---|---|---|
| ISA-001 | x86 16/32/64 | Required | Required | HIEW workflow baseline |
| ISA-002 | x86 extensions, including AVX | Required; versioned coverage | Required; versioned coverage | Official baseline [S1] |
| ISA-003 | ARMv6/ARM/Thumb | Required; explicit modes | Planned improvement | ARMv6 decode baseline [S1] |
| ISA-004 | AArch64/ARM64 | First-class requirement | First-class requirement | macOS improvement |
| ISA-005 | MIPS 32/64 | Tracked extension | Evaluate independently | Proposed |
| ISA-006 | PowerPC 32/64 | Tracked extension | Evaluate independently | Proposed |
| ISA-007 | RISC-V 32/64 | Tracked extension | Evaluate independently | Proposed |
| ISA-008 | Additional backend architectures | Explicit roadmap entries | No implied support | Proposed |

- **ISA-009**: Report architecture, instruction width, mode, endianness, and backend version.
- **ISA-010**: Report decoded, explicitly unsupported, or undecodable/unknown results; do not label unknown extensions definitively invalid without backend evidence.
- **ISA-011**: Preserve raw-byte editing when decoding is unavailable.
- **ISA-012**: Support branch-following and return history without executing inspected code.
- **ISA-013**: Reference analysis must identify its scope and incompleteness.
- **ISA-014**: Re-decode edited spans and invalidate dependent branch/reference information.
- **ISA-015**: Resynchronize at a user-selected offset in mixed code/data.
- **ASM-001**: Accept textual instructions; a programmatic encoder API alone is insufficient.
- **ASM-002**: Assemble using the selected ISA, mode, endian setting, and actual target address.
- **ASM-003**: Preview resulting bytes and affected span before committing a patch transaction.
- **ASM-004**: Reject an oversized patch unless the user explicitly enlarges its destination span.
- **ASM-005**: NOP padding must be explicit and architecture-appropriate.
- **ASM-006**: Syntax errors, unresolved symbols, and range failures leave all bytes unchanged.
- **ASM-007**: Define supported syntax, number notation, labels, and instruction-set coverage.
- **ASM-008**: Test relative branches at different addresses and mode/endian boundaries.

## 6. File-format capabilities

For each format, track detection, structure browsing, address translation, symbols,
imports/exports, editing, and malformed-input handling separately.
All capabilities are **Planned**; each requires independent fixtures and acceptance evidence.

| ID | Format family | Planned scope | Evidence |
|---|---|---|---|
| FMT-001 | Raw bytes / COM-style images | Always available; explicit interpretation overrides | Proposed foundation |
| FMT-002 | DOS MZ | Legacy headers and navigation; verify reference details | Verification goal |
| FMT-003 | PE32 / PE32+ | Headers, sections, directories, imports, exports, relocations, resources | Official family [S1] |
| FMT-004 | ELF32 / ELF64 | Headers, segments, sections, symbols, relocations, dynamic tables | LE official; BE improvement |
| FMT-005 | Mach-O 32/64 | Load commands, segments, sections, symbols, imports, entry information | LE official; expanded coverage proposed |
| FMT-006 | Universal/fat Mach-O | Slice selection with correct nested address mappings | macOS requirement |
| FMT-007 | NE / LE / LX | Explicit legacy parity work, with separate capability entries | Official [S1] |
| FMT-008 | TE / TE64 | Explicit firmware-format parity work | Official [S1] |
| FMT-009 | NetWare NLM / DSK / LAN | Explicit legacy parity work | Official [S1] |
| FMT-010 | Other formats | Extension-driven expansion; no automatic parity claim | Proposed |

- **FMT-011**: Do not drop legacy rows because an initial parser library does not support them.
- **FMT-012**: Detection failure must retain raw viewing and display the parser's limitation.
- **FMT-013**: Parsed header fields link to their exact backing byte ranges.
- **FMT-014**: Recognize zero-fill, overlays, and non-file-backed ranges without invented bytes.
- **FMT-015**: Bounds-check every table, count, string, and allocation against resource budgets.
- **FMT-016**: Editing a field is distinct from rebuilding a structurally valid executable.
- **FMT-017**: Changing signed binaries must not silently invoke signing or external repair tools.
- **FMT-018**: Prefer self-built redistributable fixtures; record provenance for imported samples.

## 7. Search, annotations, and advanced parity

- **SEARCH-001**: Support text, byte/wildcard, and instruction searches with explicit encodings.
- **SEARCH-002**: Support forward/backward, whole-file/selection, repeat, and cancellation.
- **SEARCH-003**: Support strings browsing and navigation from results.
- **SEARCH-004**: Track multi-file search/replacement as an explicit parity requirement [S1].
- **META-001**: Persist names, comments, bookmarks, and colored ranges separately from file bytes.
- **META-002**: Version sidecar data and bind it to an identifiable document revision.
- **META-003**: Record verification work for HIEW name/map import/export compatibility.
- **CALC-001**: Provide checked 64-bit expressions, bases, bit operations, and data reads.
- **CALC-002**: Make signedness, width, and endianness explicit in new capabilities.
- **CRYPT-001**: Track the HIEW transform-interpreter workflow as parity work [S1].
- **CRYPT-002**: Interpret transform instructions; never execute inspected bytes as host code.
- **CRYPT-003**: Bound transform execution and allow cancellation without partial persistence.
- **MACRO-001**: Support record, replay, save/load, and management of command sequences.
- **MACRO-002**: Replay must use the same validation and transaction rules as interactive commands.

## 8. Native terminal behavior, especially macOS

- **TERM-001**: Run natively on each supported build target; no Wine requirement.
- **TERM-002**: Test macOS Terminal.app, iTerm2, and a Kitty-protocol terminal.
- **TERM-003**: Test Windows Terminal/ConPTY and Linux terminal/PTY operation.
- **TERM-004**: Exercise SSH and tmux without dependence on local OS key-state polling.
- **TERM-005**: Negotiate enhanced keyboard protocols; retain a documented legacy fallback [S11].
- **TERM-006**: Handle Option/Alt, Esc prefixes, modified function keys, and missing Insert.
- **TERM-007**: Document Fn/media-key and OS-shortcut conflicts without changing system settings.
- **TERM-008**: Provide alternative chords and command-menu access to intercepted bindings.
- **TERM-009**: Distinguish pasted data from command input through bracketed-paste handling.
- **TERM-010**: Handle Unicode widths, combining characters, invalid bytes, and non-ASCII paths.
- **TERM-011**: Resize safely, including very small terminal dimensions.
- **TERM-012**: Restore cursor, alternate screen, raw mode, and negotiated input modes.
- **TERM-013**: Provide a reproducible key-diagnostic report for terminal-specific bugs.

## 9. Devices and extensions: explicit platform gaps

- **DEV-001**: Physical/logical device browsing and editing remain tracked HIEW parity goals [S1].
- **DEV-002**: Implement device access through platform-specific I/O adapters, not regular-file assumptions.
- **DEV-003**: Record alignment, seeking, permissions, sharing, flush, and failure semantics per OS.
- **DEV-004**: Never require device privileges for ordinary file inspection.
- **DEV-005**: Device writes need explicit target selection and read-only/write capability reporting.
- **DEV-006**: Mark unavailable platform operations visibly; do not silently omit their roadmap entries.
- **EXT-001**: Provide portable extensions for commands, annotations, views, formats, and transformations.
- **EXT-002**: Extensions share document bounds, transactions, and read-only enforcement.
- **EXT-003**: Version the extension API and describe lifecycle, errors, and compatibility.
- **EXT-004**: Preserve an F11-style extension discovery workflow.
- **EXT-005**: Original HEM files are Windows DLLs with a HIEW gateway [S3].
- **EXT-006**: Portable source/API compatibility and native HEM binary compatibility are different goals.
- **EXT-007**: Investigate a Windows HEM adapter separately; do not promise macOS DLL compatibility.
- **EXT-008**: An unavailable adapter must not block native hiewbie extensions.

## 10. Dependency candidates and licensing

Choices below are **candidates**, not committed dependencies or completed licence reviews.

- Ratatui/Crossterm: MIT terminal components; assess input coverage and terminal restoration [S4].
- iced-x86: MIT x86 decoder/encoder; its CodeAssembler API is not a text parser [S5].
- Capstone: multi-architecture decoder; preserve its distributed licence notices [S6].
- Goblin: MIT format-parser candidate; supplement missing formats and test malformed inputs [S7].
- object: MIT/Apache-2.0 candidate; upstream describes a trusted-input security model [S8].
- LLVM MC: textual assembly candidate; evaluate footprint, build matrix, and Apache/LLVM notices [S9].
- Keystone: GPLv2 with a FOSS exception listing MIT; examine conditions before selecting it [S10].
- Keep hiewbie-authored code MIT and preserve third-party licences and provenance.
- A dependency's architecture list is not evidence of hiewbie capability or release support.

## 11. Acceptance and completion evidence

Each requirement needs a ledger entry: status, evidence, fixture, test, and remaining difference.
Use Planned / Partial / Implemented / Verified / Intentional difference with explicit explanations.
A skipped platform test is not a successful test.

| Test ID | Required result |
|---|---|
| TEST-001 | Replay open, mode change, goto, branch, return, edit, save through a real PTY. |
| TEST-002 | Cancel editing; original bytes and modification time remain unchanged absent external writers; access time follows filesystem semantics. |
| TEST-003 | Save one-byte change; exactly one byte differs; reopen confirms it. |
| TEST-004 | Invalid/oversized assembly changes neither bytes nor dirty state. |
| TEST-005 | Relative patches encode correctly at distinct target addresses. |
| TEST-006 | All hosts inspect identical x86 PE, ARM64 Mach-O, and ELF fixtures identically. |
| TEST-007 | Navigate and edit a sparse file beyond 4 GiB without whole-file allocation. |
| TEST-008 | Search finds chunk-boundary matches and cancels responsively. |
| TEST-009 | Exercise overlapping blocks, EOF insertion/deletion, and transaction undo. |
| TEST-010 | Test malformed/truncated formats, arithmetic overflow, and unmapped VAs. |
| TEST-011 | Replay function-key/modifier/paste/resize cases and verify terminal cleanup. |
| TEST-012 | Verify raw fallback when format or ISA interpretation is unavailable. |
| TEST-013 | Exercise save failures and externally modified files without silent data loss. |
| TEST-014 | Launch each packaged artifact and perform an edit; include third-party notices. |

Use golden byte fixtures, parser/decoder corpus tests, fuzzing, and terminal scenario recordings.
Measure startup, viewport movement, memory, and cancellation on a documented reference machine.
Add a screenshot or terminal recording for workflow changes and document the before/after behavior.
No milestone may claim full HIEW parity while required legacy or contextual entries are untracked.

## Sources

- [S1] HIEW author: https://www.hiew.ru/
- [S2] Community Hiew Documentation Project, linked by S1: https://taviso.github.io/hiewdocs/index.htm
- [S3] HEM author documentation: https://www.hiew.ru/hem.html
- [S4] https://github.com/ratatui/ratatui and https://github.com/crossterm-rs/crossterm
- [S5] https://github.com/icedland/iced and https://docs.rs/iced-x86/latest/iced_x86/code_asm/index.html
- [S6] https://www.capstone-engine.org/ and https://www.capstone-engine.org/download
- [S7] https://github.com/m4b/goblin
- [S8] https://github.com/gimli-rs/object#security
- [S9] https://llvm.org/docs/CommandGuide/llvm-mc.html and https://llvm.org/docs/DeveloperPolicy.html
- [S10] https://github.com/keystone-engine/keystone and https://github.com/keystone-engine/keystone/blob/master/EXCEPTIONS-CLIENT
- [S11] https://sw.kovidgoyal.net/kitty/keyboard-protocol/
