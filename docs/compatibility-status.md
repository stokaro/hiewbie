# Compatibility implementation ledger

This ledger retains every requirement ID from [compatibility.md](compatibility.md).
It describes the first native raw-byte editor, not full HIEW parity. **Implemented**
means the stated initial behavior exists with the linked regression evidence;
**Partial** means only the recorded subset exists; **Planned** means no application
implementation is claimed. Actual platform results belong to the identified
commit's CI runs. No row is promoted to cross-platform **Verified** by configuration
alone. A [versioned Hiew 8.63 documentation comparison][Reference] is recorded;
the original failed to start, so no original-program runtime parity is claimed.

Current fixtures are generated in the linked tests (byte vectors, temporary files,
Unicode paths, sparse files, and PTY input). The native Linux run and source-level
checks are recorded in the implementation PR; CI supplies the other native hosts.
M0/M1 remain partial; M2-M6 and the parent epic remain open.

## UI

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| UI-001 | Partial | [T], [C]: shared Hex/Text; Code remains planned. |
| UI-002 | Partial | [T], [P], [Reference]: separate F4 choice/Enter cycle, edit restores and Shift+F7; full keymap and original runtime parity unverified. |
| UI-003 | Partial | [T]: file offset, mode, edit/readonly/dirty state; bitness/architecture await analysis. |
| UI-004 | Partial | [T], [C]: shared cursor survives Hex/Text changes; other views/history pending. |
| UI-005 | Partial | [T]: view/edit/dialog/search/recovery contexts; selections pending. |
| UI-006 | Partial | [T]: context-sensitive function bar; modifier-aware hints pending. |
| UI-007 | Implemented | [T], [P]: all currently implemented operations are keyboard-accessible. |
| UI-008 | Partial | [T], [P]: 80-column cells, dynamic rows, resize; full reference comparison pending. |
| UI-009 | Partial | [T]: blue/cyan default; accessible configurable themes pending. |
| UI-010 | Partial | [T]: Ctrl alternatives; command menu and all intercepted-key cases pending. |
| UI-011 | Partial | [Input], [Reference]: unavailable commands and documented deviations are explicit; original runtime observations pending. |
| UI-012 | Partial | [T], [Commands]: help and core command inventory; key diagnostics/menu pending. |
| UI-013 | Implemented | [C], [T], [N]: bounded initial viewport; no eager whole-file scan. |
| UI-014 | Implemented | [CLI], [P]: ordinary editing has no network/service dependency. |
| UI-015 | Partial | [P], [Input]: acquired-mode guard and normal exit; ConPTY/signal/failure matrix pending. |

## KEY

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| KEY-001 | Implemented | [C], [Commands]: stable IDs for the initial core command set. |
| KEY-002 | Partial | [C], [T]: terminal and headless tests share dispatch; macros/menu/browser pending. |
| KEY-003 | Partial | [C], [T]: hex, decimal t, relative syntax; dotted VAs explicitly unavailable. |
| KEY-004 | Partial | [T]: cursor preserved across two views; reference navigation/bookmarks pending. |
| KEY-005 | Partial | [C], [T], [P], [Reference]: F3/current and Backspace/previous source-byte restores; exact original nibble/buffer/cursor behavior, F2/assembly, F10/truncation and timestamps remain open. |
| KEY-006 | Partial | [C], [T], [P]: F3/F9 collisions, F4 view selection and Shift+F7 context covered; other contextual commands remain pending. |
| KEY-007 | Implemented | [T], [P]: Esc cancels search/dialog, discards edits, then exits view. |
| KEY-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## EDIT

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| EDIT-001 | Implemented | [C], [N]: raw source plus overlay are authoritative. |
| EDIT-002 | Implemented | [C], [N], [P]: pending changes do not write until explicit save. |
| EDIT-003 | Implemented | [C], [N], [P]: discard restores original bytes and does not rewrite mtime. |
| EDIT-004 | Implemented | [C], [N], [T]: successful save rebases and clears transaction/history. |
| EDIT-005 | Partial | [C], [T]: bounded overwrite/restore command undo/redo, distinct from source-byte restoration; blocks/assembly pending. |
| EDIT-006 | Partial | [C], [T]: overwrite implemented; insert/delete/copy/move/fill/import/export pending. |
| EDIT-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-008 | Partial | [C], [N]: ranges and limits validated before mutation; later operations pending. |
| EDIT-009 | Partial | [C], [N], [Storage]: atomic logical edits and documented replacement commit point. |
| EDIT-010 | Partial | [T]: edit entry column visible; size-changing insert mode unavailable. |
| EDIT-011 | Partial | [C]: generation invalidates search; parser/disassembly invalidation pending. |
| EDIT-012 | Implemented | [N], [P]: saved logical bytes preserve untouched ranges, including copy boundaries. |
| EDIT-013 | Implemented | [C], [N]: byte edits never invoke format repair or checksum changes. |
| EDIT-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-015 | Partial | [C], [T], [P]: readonly blocks current command/save paths; extension paths pending. |
| EDIT-016 | Partial | [N], [Storage]: identity/metadata conflict detection; final-check/rename race documented. |
| EDIT-017 | Partial | [N], [T], [Storage]: precommit errors retain edits; postcommit uncertainty requires reopen; recovery export pending. |
| EDIT-018 | Partial | [N], [Storage]: ordinary modes/links/partial-I/O policy; ACL/xattrs/ADS and durability gaps remain. |
| EDIT-019 | Implemented | [N]: Unix executable permission test; special metadata is outside this save strategy. |
| EDIT-020 | Partial | [C]: bounded sorted patch snapshots; display/export pending. |
| EDIT-021 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-022 | Partial | [T], [CLI]: nonprintable bytes render as dots without changing source; Unicode decoding pending. |

## ADDR

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| ADDR-001 | Partial | [C]: FileOffset introduced; RVA/VA/slice types await mapping backend. |
| ADDR-002 | Implemented | [C], [N], [T]: checked u64 boundaries and offsets beyond 4 GiB. |
| ADDR-003 | Partial | [T]: file-offset display; domain selection/history/annotations pending. |
| ADDR-004 | Partial | [C]: dotted VA input explicitly rejected; actual VA mapping pending. |
| ADDR-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ADDR-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ADDR-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ADDR-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## IO

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| IO-001 | Implemented | [C], [N], [T]: bounded reads; sparse-file access beyond 4 GiB. |
| IO-002 | Partial | [C], [T], [Storage]: independent viewport/edit/history/search bounds; analysis budgets pending. |
| IO-003 | Implemented | [C], [N], [T]: fake large source, real Unix sparse file, large-offset rendering. |
| IO-004 | Implemented | [N], [Storage]: seek/read strategy and truncation checks; no unsafe file mapping. |
| IO-005 | Implemented | [C]: incremental KMP with cross-chunk matches and bounded steps. |
| IO-006 | Partial | [C], [T]: cancellable search/progress; save is synchronous, analysis pending. |
| IO-007 | Partial | [Storage]: explicit resource limits; reproducible latency/RSS benchmarks pending. |

## ISA

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| ISA-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-009 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-010 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-011 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-012 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-013 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-015 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## ASM

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| ASM-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ASM-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## FMT

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| FMT-001 | Partial | [C], [T], [CLI]: raw byte inspection; no executable parser is selected. |
| FMT-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-009 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-010 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-011 | Partial | [Analysis]: every legacy format row retained; parsers remain planned. |
| FMT-012 | Partial | [T]: raw-only access works; parser detection/fallback diagnostics pending. |
| FMT-013 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-015 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-016 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-017 | Partial | [N]: save invokes no signer or repair command; format-aware workflows pending. |
| FMT-018 | Partial | [C], [N], [T], [P]: current byte fixtures are generated; format fixtures pending. |

## SEARCH

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| SEARCH-001 | Partial | [C], [T], [Reference]: case-sensitive literal bytes/UTF-8; original character-field case policy, encoding mappings and wildcard/instruction search remain open. |
| SEARCH-002 | Partial | [C], [T], [P], [Reference]: forward/repeat/cancel plus Shift+F7; cursor+1 repeat still differs from documented independent last-match state; backward/scope/boundary policies pending. |
| SEARCH-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| SEARCH-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## META

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| META-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| META-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| META-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## CALC

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| CALC-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| CALC-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## CRYPT

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| CRYPT-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| CRYPT-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| CRYPT-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## MACRO

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| MACRO-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| MACRO-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## TERM

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| TERM-001 | Partial | [Build]: six native CI jobs configured; full target/runtime matrix remains incomplete. |
| TERM-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TERM-003 | Partial | [P], [T]: Unix PTY and headless cells; Windows ConPTY pending. |
| TERM-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TERM-005 | Partial | [Input]: legacy fallback documented; enhanced protocol negotiation pending. |
| TERM-006 | Partial | [T], [P]: ordinary F-keys and modifier rejection; Option/Alt/modified-key matrix pending. |
| TERM-007 | Implemented | [Input], [README]: Fn/media-key conflicts and Ctrl fallbacks documented; no settings changed. |
| TERM-008 | Partial | [T], [Input]: Ctrl alternatives; command-menu access pending. |
| TERM-009 | Implemented | [T], [P]: bracketed paste identified and rejected without command execution. |
| TERM-010 | Partial | [T], [CLI], [N]: Unicode/native paths and control escaping; decoded text and full width corpus pending. |
| TERM-011 | Implemented | [T], [P]: tiny/narrow/large cells and real PTY resize. |
| TERM-012 | Partial | [P], [Input]: raw/alternate/paste/cursor guard; wider failure and ConPTY coverage pending. |
| TERM-013 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## DEV

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| DEV-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| DEV-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| DEV-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| DEV-004 | Implemented | [N], [P]: ordinary file access needs no device adapter or device privileges. |
| DEV-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| DEV-006 | Partial | [Input], [Build]: unsupported operations and host blockers documented; capability UI pending. |

## EXT

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| EXT-001 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EXT-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## TEST

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| TEST-001 | Partial | [P]: open/mode-choice/goto/edit/restore/save subset; branch/return await analysis; original runtime comparison blocked. |
| TEST-002 | Implemented | [N], [P]: discard checks expected bytes and mtime on generated ordinary files. |
| TEST-003 | Implemented | [N], [P]: exact persisted byte changes; reopen via file reads. |
| TEST-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TEST-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TEST-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TEST-007 | Partial | [N], [C], [T]: real Unix sparse file and fake large sources; interactive large-file benchmark pending. |
| TEST-008 | Partial | [C], [T]: chunk-boundary literal matches and cancellation; latency benchmark pending. |
| TEST-009 | Partial | [C]: overlapping overwrites and history; blocks/EOF insertion/deletion pending. |
| TEST-010 | Partial | [C]: offset overflow and unsupported dotted addresses; malformed executable corpus pending. |
| TEST-011 | Partial | [P], [T]: F-keys/Ctrl/paste/resize/cleanup subset; terminal application/modifier matrix pending. |
| TEST-012 | Partial | [T], [CLI]: raw inspection available; format/ISA failure fallback requires backends. |
| TEST-013 | Partial | [N], [T], [Storage]: writer/conflict/temp-path failures and postcommit UI state; fault/crash matrix pending. |
| TEST-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## Evidence index

[C]: ../crates/hiewbie-core/src/tests.rs
[N]: ../crates/hiewbie/tests/persistence.rs
[T]: ../crates/hiewbie/src/terminal/tests.rs
[CLI]: ../crates/hiewbie/tests/cli.rs
[P]: ../scripts/pty_smoke.py
[Storage]: adr/0001-document-storage.md
[Input]: adr/0002-terminal-input.md
[Build]: adr/0003-build-targets.md
[Analysis]: adr/0004-analysis-backends.md
[Commands]: commands.md
[README]: ../README.md
[Reference]: reference/hiew-8.63.md

- [Core unit/command/search tests][C]
- [Native persistence and sparse-file tests][N] and [adapter fault tests](../crates/hiewbie/src/file.rs)
- [Key/rendering tests][T] and [built CLI tests][CLI]
- [Real Unix PTY scenarios][P] and [recording](evidence/native-editor.cast)
- [Storage guarantees and limits][Storage], [input behavior][Input], [build strategies][Build]
- [Analysis selection remains open][Analysis]
- [Versioned original-documentation audit and open differential scenarios][Reference]
