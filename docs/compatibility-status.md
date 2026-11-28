# Compatibility implementation ledger

This ledger retains every requirement ID from [compatibility.md](compatibility.md).
It covers native byte editing, executable analysis and selected symbol tables. Full
HIEW parity remains the goal. **Implemented**
means the stated initial behavior exists with the linked regression evidence;
**Partial** means only the recorded subset exists; **Planned** means no application
implementation is claimed. Actual platform results belong to the identified
commit's CI runs. No row is promoted to cross-platform **Verified** by configuration
alone. A [versioned Hiew 8.63 comparison][Reference] and selected Windows runtime
[observations][Observation] are recorded; neither establishes full runtime parity.

Current fixtures are generated in the linked tests: byte vectors, PE/ELF/Mach-O
images, instruction encodings, temporary files, Unicode paths, sparse sources,
and PTY input. Workspace tests, native builds, real PTY scenarios, PTY-harness
regressions and integrated `wasm32-unknown-unknown` core compilation provide
separate evidence. The implementation PR and CI record actual commands, test
totals and available native-host results for each identified commit.

The [build strategy][Build] retains six native CI targets, compile-only checks
for RISC-V and FreeBSD x64, and the pinned-toolchain blocker for native FreeBSD
ARM64. WebAssembly core compilation establishes neither browser execution nor
a browser frontend. Release packaging remains open. M0-M2 remain partial; M3-M6
and the parent epic remain open.

## UI

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| UI-001 | Implemented | [A], [T], [P], [Observation]: Text/Hex/Code share the document and cursor; the observed original preserves position across Enter cycling. |
| UI-002 | Partial | [T], [P], [Commands], [Reference], [Observation]: separate F4 choice/Enter cycle, edit restores, Shift+F7 and Code/header/profile bindings; full modifier and command matrix remains unverified. |
| UI-003 | Partial | [T], [ACLI], [Hex observation]: Raw/PE Hex title and access states, VA/file-offset display, and changed-byte colors now have paired Windows evidence. Decoder profile, overrides, and full state coverage remain open. |
| UI-004 | Partial | [A], [T]: the cursor survives all three views. Branch return restores position and interpretation. Screen bookmarks restore mode-specific viewport state [Bookmark observation]. Other per-view navigation behavior remains open. |
| UI-005 | Partial | [T], [Observation]: view/edit/search/recovery and mode/header/profile dialogs resolve their own commands; the two-field search dialog has its own help and contextual bindings; block tracking, active-mark cancellation, scoped search, Write block, Calculator, and the file lists have tests. Transient modifier and other overlay details remain open. |
| UI-006 | Partial | [T], [Commands], [Hex observation]: Hex uses the observed F-key label positions and dims unsupported commands. Contextual Help uses the observed blank bar and wide slots. History and the argument list have their own bars. Other dialog bars, modes, and modifier layers remain open. |
| UI-007 | Implemented | [T], [P]: all currently implemented operations are keyboard-accessible. |
| UI-008 | Partial | [T], [P], [Hex observation], [Decode observation], [Dialog observation], [Text observation], [Help observation], [Calculator observation]: Paired Windows evidence covers Hex, initial raw/PE Decode, selected dialogs, single-byte Text, Help geometry, Calculator, and file lists. Complete Decode syntax and navigation, the Text scrollbar, Help content, and header fields remain open. |
| UI-009 | Partial | [T], [Observation]: blue/cyan default aligns with the observed palette; block bytes use yellow on magenta. Search focus, modal alerts, and Help use state-specific colors. Complete palette and accessible theme coverage remain open. |
| UI-010 | Partial | [T], [Commands]: Ctrl alternatives and keyboard header/profile dialogs; comprehensive command-menu and intercepted-key coverage pending. |
| UI-011 | Partial | [T], [ST], [Input], [Commands], [Reference], [Observation]: visible deviations and unavailable commands are explicit; the observed runtime now covers primary views and dialogs, while the remaining matrix stays open. |
| UI-012 | Partial | [T], [P], [Help observation], [Commands]: view, edit, Search, and Calculator Help render contextual rows and preserve modal state. Complete original command lists, a shared command registry, and key-event diagnostics remain open. |
| UI-013 | Implemented | [F], [D], [A], [T]: bounded viewport, metadata reads and per-row decoding; no eager whole-file disassembly or indexing. |
| UI-014 | Implemented | [CLI], [P]: ordinary editing has no network/service dependency. |
| UI-015 | Partial | [P], [Input]: acquired-mode guard and normal exit; ConPTY/signal/failure matrix pending. |

The native Files panel has separate [Windows observations][Files observation]
and a [PTY replay][Files replay] of the startup and F9 workflow. The
[history observations][History observation] and [history replay][History replay]
cover visiting, switching, and reopening files.

## KEY

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| KEY-001 | Implemented | [C], [A], [SN], [Commands]: Stable IDs include source-byte restores, block selection/export, profiles, mappings, branches and separate symbol-target/record navigation. |
| KEY-002 | Partial | [A], [T], [ACLI]: terminal dialogs, shortcuts and headless inspection use shared application commands; macro/browser callers pending. |
| KEY-003 | Partial | [C], [A], [T], [ACLI], [Reference]: hexadecimal, decimal t, relative and dotted absolute-VA syntax reject overflow/unavailable mappings; original current-section-relative dotted interpretation remains open. |
| KEY-004 | Partial | [A], [T]: shared position, bounded branch and reference navigation history, an independent search resume point, file history, and eight screen bookmarks per view [Bookmark observation]. General navigation return remains open. |
| KEY-005 | Partial | [C], [T], [P], [Reference], [Observation]: observed Hex input is nibble-oriented and F3 restores the current source byte then advances; buffer boundaries, Backspace, assembly, truncation and timestamps remain open. |
| KEY-006 | Partial | [C], [T], [P], [Commands], [Files observation], [Files replay], [Reference observations]: F3/F9, file navigation, F4 mode and search area, F2 export, Shift+F7, headers, and Hex/Decode x86 reference keys have tests. F2 textual assembly and broader reference scenarios remain open. |
| KEY-007 | Implemented | [T], [P]: Esc cancels search/dialog, discards edits, then exits view. |
| KEY-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## EDIT

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| EDIT-001 | Implemented | [C], [N]: raw source plus overlay are authoritative. |
| EDIT-002 | Implemented | [C], [N], [P]: pending changes do not write until explicit save. |
| EDIT-003 | Implemented | [C], [N], [P]: discard restores original bytes and does not rewrite mtime. |
| EDIT-004 | Implemented | [C], [N], [T]: successful save rebases and clears transaction/history. |
| EDIT-005 | Partial | [C], [A], [T]: bounded overwrite/restore command undo/redo refreshes Code and remains distinct from source-byte restoration; blocks and textual assembly pending. |
| EDIT-006 | Partial | [C], [T], [Block export], [Block observation]: overwrite and native raw block export implemented. Insert/delete/copy/move/fill/import remain open. |
| EDIT-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-008 | Partial | [C], [N]: ranges and limits validated before mutation; later operations pending. |
| EDIT-009 | Partial | [C], [N], [Storage]: atomic logical edits and documented replacement commit point. |
| EDIT-010 | Partial | [T]: edit entry column visible; size-changing insert mode unavailable. |
| EDIT-011 | Implemented | [C], [F], [A], [SN], [T]: generation changes invalidate search, metadata, symbols, decoding and direct targets after overwrites, source-byte restores, undo/redo, discard and save. |
| EDIT-012 | Implemented | [N], [P]: saved logical bytes preserve untouched ranges, including copy boundaries. |
| EDIT-013 | Implemented | [A], [SN], [N], [ST], [P]: header-field and symbol-record navigation lead to ordinary byte edits; no format repair or checksum rewrite is invoked. |
| EDIT-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-015 | Partial | [C], [T], [P]: read-only mode blocks source edits and saves. Raw export rejects the source as a destination. Extension paths remain open. |
| EDIT-016 | Partial | [N], [Storage]: identity/metadata conflict detection; final-check/rename race documented. |
| EDIT-017 | Partial | [N], [T], [Storage]: precommit errors retain edits; postcommit uncertainty requires reopen; recovery export pending. |
| EDIT-018 | Partial | [N], [Storage]: ordinary modes/links/partial-I/O policy; ACL/xattrs/ADS and durability gaps remain. |
| EDIT-019 | Implemented | [N]: Unix executable permission test; special metadata is outside this save strategy. |
| EDIT-020 | Partial | [C]: bounded sorted patch snapshots; display/export pending. |
| EDIT-021 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| EDIT-022 | Partial | [T], [CLI], [D]: Text view renders CP437 controls and high bytes without changing the document. Code retains undecodable raw bytes. Unicode document decoding and encoding-aware Text editing remain pending. |

## ADDR

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| ADDR-001 | Implemented | [F], [A]: distinct FileOffset, VirtualAddress, Rva and SliceOffset types with explicit translations. |
| ADDR-002 | Implemented | [C], [N], [F], [A], [D]: checked u64 file/mapping arithmetic, offsets beyond 4 GiB, and decoder PC-width semantics kept separate. |
| ADDR-003 | Partial | [A], [T], [ACLI]: FO, mapped VA and raw-base context are visible; branch history restores interpretation; annotations pending. |
| ADDR-004 | Implemented | [F], [A], [T]: unmapped, zero-fill and ambiguous virtual destinations fail explicitly without moving the cursor. |
| ADDR-005 | Implemented | [F]: current readers check extents, overflow and truncated tables; overlapping mappings remain inspectable but translation reports ambiguity. |
| ADDR-006 | Implemented | [F], [A], [T], [ACLI]: selected fat member maps through its validated slice to global file offsets; nested fat containers explicitly unsupported. |
| ADDR-007 | Implemented | [A], [T], [ACLI]: explicit raw base, x86 bitness, architecture and instruction-endian overrides; unsupported decoding orders remain visible. |
| ADDR-008 | Implemented | [A], [T], [ACLI]: effective overrides are shown separately from parsed machine metadata; changing fat slice clears the prior raw base. |

## IO

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| IO-001 | Implemented | [C], [N], [T]: bounded reads; sparse-file access beyond 4 GiB. |
| IO-002 | Partial | [C], [F], [S], [D], [A], [Analysis], [Symbols]: independent viewport, image/symbol reader, decoder, edit/history and search budgets; symbols are lazy; background analysis remains future work. |
| IO-003 | Implemented | [N], [F], [A], [T]: real Unix sparse file, synthetic 1 TiB source and bounded code reads beyond 4 GiB. |
| IO-004 | Implemented | [N], [Storage]: seek/read strategy and truncation checks; no unsafe file mapping. |
| IO-005 | Implemented | [C]: bidirectional KMP with cross-chunk matches and bounded steps. |
| IO-006 | Partial | [C], [T], [Analysis]: search and block output staging provide cancellation and progress. Final export commit, save, and bounded format/decoder reads remain synchronous. |
| IO-007 | Partial | [Storage], [Analysis]: explicit read, table, row, text and history budgets; reproducible latency/RSS benchmarks pending. |

## ISA

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| ISA-001 | Partial | [D], [A], [ACLI]: iced-x86 1.22.0 decodes 16/32/64-bit modes with automatic raw x86-32, manual overrides, and a native Decode viewport [Decode observation]. Textual assembly remains planned. |
| ISA-002 | Partial | [D], [x86 Decode coverage]: 58 exact scalar, x87, SIMD, and extension encodings exercise iced-x86 1.22.0 in 16/32/64-bit modes. The table records which samples worked in HIEW 8.63. A complete extension inventory and textual assembly remain open. |
| ISA-003 | Partial | [D], [A], [Analysis]: separate ARM/Thumb modes, strict decoding and direct mode exchange; precise ARMv6/revision filtering, IT-state propagation and big-endian instructions remain gaps. |
| ISA-004 | Partial | [D], [A], [ACLI]: AArch64 decoding and checked direct targets independent of host CPU; textual assembly, SVE/SVE2 and complete extension coverage pending. |
| ISA-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-006 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| ISA-009 | Implemented | [D], [T], [ACLI]: mode/bitness, endian, instruction lengths and backend version are reported; parsed CPU and explicit profile remain distinct. |
| ISA-010 | Implemented | [D], [T], [ACLI]: Decoded, Unknown, InvalidForMode, Truncated, and Unsupported preserve raw bytes and guarantee progress for nonempty input. InvalidForMode covers recognized legacy-only opcodes in long mode. |
| ISA-011 | Implemented | [A], [T], [ACLI]: unknown machines, malformed metadata and unsupported instruction order preserve raw viewing and byte editing. |
| ISA-012 | Partial | [D], [A], [T], [P]: visible direct branches have 1–9/A–Z labels and return via 0 or Backspace. Includes ARM/Thumb mode exchange. Indirect and far resolution remain open. |
| ISA-013 | Partial | [A], [T], [Analysis], [Symbols], [Reference observations]: raw x86 near branches and selected mapped memory and literal operands are searchable in bounded steps. General references, loader symbol resolution and control-flow analysis remain open. |
| ISA-014 | Implemented | [A], [T]: current logical bytes drive re-decoding after edits and undo. Generation and profile changes invalidate dependent reference continuation. Profile changes and resynchronization clear saved viewport labels. |
| ISA-015 | Implemented | [A], [T], [Commands]: Code-view / resynchronizes at the selected byte; instruction navigation retraces known boundaries or uses a bounded byte/alignment fallback. |

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
| FMT-001 | Partial | [A], [T], [ACLI]: raw viewing/editing and explicit mode/base interpretation are available; COM-specific reference parity remains unverified. |
| FMT-002 | Planned | [Analysis]: MZ identification routes to PE when present; DOS MZ headers and legacy navigation are not implemented. |
| FMT-003 | Partial | [F], [S], [SN], [SCLI], [P]: PE32/PE32+ headers, mappings, ordinary named/ordinal imports and exports, aliases and explicit forwarders; delay imports, bound-directory resolution, COFF/PDB, relocations and resources pending. |
| FMT-004 | Partial | [F], [S], [Symbols]: ELF32/64 headers, PT_LOAD and section-backed SYMTAB/DYNSYM in both byte orders; ET_REL file targets; sectionless dynamic tables, symbol versions/providers and relocations pending; extended numbering unsupported. |
| FMT-005 | Partial | [F], [S], [Symbols]: thin Mach-O32/64 headers, segments, LC_MAIN, sections for target validation and LC_SYMTAB/nlist in both byte orders; export tries, dyld bindings/chained fixups, indirect-symbol bindings, relocations and LC_UNIXTHREAD pending; high-VM segments unsupported. |
| FMT-006 | Implemented | [F], [A], [T], [ACLI]: fat32/fat64 descriptors, explicit selection and global mapping; deterministic first-slice default independent of host, selected thin members only. |
| FMT-007 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-008 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-009 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-010 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| FMT-011 | Partial | [Analysis]: DOS MZ, NE/LE/LX, TE/TE64 and NetWare rows remain required; initial modern readers do not remove legacy work. |
| FMT-012 | Implemented | [A], [T], [ACLI]: failed/unsupported detection retains raw access and a diagnostic; --headers fails clearly while --dump remains available. |
| FMT-013 | Implemented | [F], [S], [T], [ST]: interpreted fields and symbol records retain exact backing ranges; header and symbol dialogs navigate to those bytes. |
| FMT-014 | Partial | [F], [A]: zero-fill, overlays and disk padding never acquire invented bytes or VAs; dedicated overlay browsing and broader non-file-backed metadata pending. |
| FMT-015 | Implemented | [F], [S], [ACLI], [SCLI], [Analysis], [Symbols]: current readers bound reads, parsed tables, counts, names, fields and retained text; malformed/truncated fixtures, real budget exhaustion and bounded mutations exercise failures. |
| FMT-016 | Implemented | [A], [T], [README]: navigating and editing backing bytes is explicitly separate from executable rebuilding; no structural rebuild action exists yet. |
| FMT-017 | Partial | [N]: save invokes no signer or repair command; format-aware workflows pending. |
| FMT-018 | Implemented | [F], [D], [A], [ACLI], [P]: executable/instruction fixtures are generated or independently written; no proprietary binary samples are required. |

## SEARCH

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| SEARCH-001 | Partial | [C], [T], [Reference]: exact bytes and UTF-8 with ASCII case folding; legacy code-page case mapping, wildcard and instruction search remain open. |
| SEARCH-002 | Partial | [C], [T], [P], [Reference]: forward/backward, repeat from the last hit, explicit resets and cancellation; F4 selects whole file or a completed block. [Block observation] records the first original hit. Original repeat, no-hit, boundary, and Unicode-alignment details remain open. |
| SEARCH-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| SEARCH-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## META

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| META-001 | Partial | [A], [T], [Bookmark observation], [Bookmark replay]: in-memory screen bookmarks are separate from navigation return. Persistent annotations, color ranges, and bookmark storage remain open. |
| META-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| META-003 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## CALC

| Requirement | Status | Evidence and remaining difference |
| --- | --- | --- |
| CALC-001 | Implemented | [Calculator contract], [Calculator tests], [A], [T], [Calculator observation], [Calculator replay]: wrapping and checked 64-bit expressions with a documented precedence/base/operator corpus, bounded cursor reads and precise errors; regression coverage preserves pending bytes, undo/redo, cursor and search state. The title identifies checked evaluation. Broader original-runtime differential capture remains in [#24](https://github.com/stokaro/hiewbie/issues/24); independent corpus results are not new reference observations. |
| CALC-002 | Implemented | [Calculator contract], [Commands], [Calculator tests], [T], [Calculator replay]: tag case and b/w/d/q identify signedness and 8/16/32/64-bit width. Explicit per-read :le/:be suffixes select byte order; bare tags preserve legacy LE. All widths, sign extension, mixed orders, unaligned/EOF reads and invalid qualifiers have generated tests. |

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
| TERM-001 | Partial | [Build]: six native CI jobs run shared fixtures; actual results belong to the identified commit's PR/CI, and the full required host/runtime matrix remains incomplete. |
| TERM-002 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TERM-003 | Partial | [P], [T]: Unix PTY now exercises Code/headers/branch navigation as well as editing; Windows ConPTY pending. |
| TERM-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TERM-005 | Partial | [Input]: legacy fallback documented; enhanced protocol negotiation pending. |
| TERM-006 | Partial | [T], [P]: ordinary F-keys, profile/bitness modifier handling and Ctrl alternatives; terminal-specific Option/Alt/modified-key matrix pending. |
| TERM-007 | Implemented | [Input], [README]: Fn/media-key conflicts and Ctrl fallbacks documented; no settings changed. |
| TERM-008 | Partial | [T], [Input], [Commands]: Ctrl alternatives and keyboard profile/header dialogs; comprehensive command-menu access pending. |
| TERM-009 | Implemented | [T], [P]: bracketed paste identified and rejected without command execution. |
| TERM-010 | Partial | [T], [CLI], [ACLI], [N]: native/Unicode paths and escaping of metadata, decoder text and errors; Unicode document decoding and full width corpus pending. |
| TERM-011 | Implemented | [T], [P]: tiny/narrow/large cells and real PTY resize. |
| TERM-012 | Partial | [P], [PH], [Input]: raw/alternate/paste/cursor cleanup and native caret output covered in Unix PTY. Configured shape reset, caret visibility, and nibble/field coordinates have [Cursor replay] evidence. Native ConPTY and emulator appearance remain open. |
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
| TEST-001 | Implemented | [P], [Recording], [Observation]: generated scenarios replay mode choice, source-byte restore, entry/VA goto, Code branch/return, byte edit and save through a real PTY; selected original behavior is recorded, while automated differential replay remains pending. |
| TEST-002 | Implemented | [N], [P]: discard checks expected bytes and mtime on generated ordinary files. |
| TEST-003 | Implemented | [N], [P]: exact persisted byte changes; reopen via file reads. |
| TEST-004 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TEST-005 | Planned | No implementation or test evidence yet; the full specification entry remains required. |
| TEST-006 | Partial | [F], [D], [A], [ACLI]: host-independent x86 PE, ARM64 Mach-O and ELF fixtures are shared by six native CI jobs; per-commit results are recorded in PR/CI, while the full required host/runtime matrix remains incomplete. |
| TEST-007 | Partial | [N], [C], [F], [A], [T]: real Unix sparse file, synthetic 1 TiB format source and large-offset code windows; interactive large-file benchmark pending. |
| TEST-008 | Partial | [C], [T]: chunk-boundary literal matches in both directions, cancellation, and [native PTY search replay][Search replay]; latency benchmark pending. |
| TEST-009 | Partial | [C], [T], [Block export], [Block replay]: overlapping overwrites and history, reversed marks, fixed range search, export cancellation, exact output and append. Overlapping block edits and EOF insertion/deletion remain open. |
| TEST-010 | Implemented | [F], [D], [A], [ACLI]: generated malformed/truncated formats, bounded mutation corpora, arithmetic overflow, overlaps, zero-fill and unmapped VAs; not exhaustive fuzzing or format conformance. |
| TEST-011 | Partial | [P], [T], [Cursor replay], [Dialog observation]: F-keys/Ctrl/paste/resize/cleanup and native cursor focus subset; terminal application/modifier matrix pending. |
| TEST-012 | Implemented | [A], [T], [ACLI]: malformed formats, unknown machines and unavailable decoding retain raw inspection/editing; explicit profile override can decode raw bytes. |
| TEST-013 | Partial | [N], [T], [Storage]: writer/conflict/temp-path failures and postcommit UI state; fault/crash matrix pending. |
| TEST-014 | Planned | No implementation or test evidence yet; the full specification entry remains required. |

## Evidence index

[C]: ../crates/hiewbie-core/src/tests.rs
[F]: ../crates/hiewbie-core/src/formats/tests.rs
[D]: ../crates/hiewbie-core/src/instructions/tests.rs
[A]: ../crates/hiewbie-core/src/analysis_tests.rs
[S]: ../crates/hiewbie-core/src/symbols
[SN]: ../crates/hiewbie-core/src/symbol_navigation_tests.rs
[ST]: ../crates/hiewbie/src/terminal/symbol_tests.rs
[SCLI]: ../crates/hiewbie/tests/symbol_cli.rs
[N]: ../crates/hiewbie/tests/persistence.rs
[T]: ../crates/hiewbie/src/terminal/tests.rs
[CLI]: ../crates/hiewbie/tests/cli.rs
[ACLI]: ../crates/hiewbie/tests/analysis_cli.rs
[P]: ../scripts/pty_smoke.py
[PH]: ../scripts/test_pty_smoke.py
[Block observation]: evidence/hiew-8.63-windows-blocks.json
[Block export]: ../crates/hiewbie/tests/block_export.rs
[Block replay]: evidence/block-workflow.cast
[Calculator observation]: evidence/hiew-8.63-windows-calculator.json
[Calculator replay]: evidence/calculator.cast
[Calculator tests]: ../crates/hiewbie-core/src/calculator_tests.rs
[Calculator contract]: calculator.md
[Cursor replay]: evidence/native-cursor.cast
[Dialog observation]: evidence/hiew-8.63-windows-dialogs.json
[Hex observation]: evidence/hiew-8.63-windows-hex.json
[Help observation]: evidence/hiew-8.63-windows-help.json
[Files observation]: evidence/hiew-8.63-windows-files.json
[Files replay]: evidence/files.cast
[History observation]: evidence/hiew-8.63-windows-history.json
[History replay]: evidence/file-history.cast
[Bookmark observation]: evidence/hiew-8.63-windows-bookmarks.json
[Bookmark replay]: evidence/bookmarks.cast
[Decode observation]: evidence/hiew-8.63-windows-decode.json
[x86 Decode coverage]: x86-decode-coverage.md
[Reference observations]: evidence/hiew-8.63-windows-references.json
[Decode replay]: evidence/decode.cast
[Text observation]: evidence/hiew-8.63-windows-text.json
[Search replay]: evidence/search-repeat.cast
[Recording]: evidence/executable-analysis.cast
[Storage]: adr/0001-document-storage.md
[Input]: adr/0002-terminal-input.md
[Observation]: evidence/hiew-8.63-windows-observation.json
[Build]: adr/0003-build-targets.md
[Analysis]: adr/0004-analysis-backends.md
[Symbols]: adr/0005-symbol-navigation.md
[Commands]: commands.md
[README]: ../README.md
[Reference]: reference/hiew-8.63.md

- [Core unit/command/search tests][C]
- [Generated format/mapping fixtures][F], [decoder fixtures and bounded corpora][D], and [shared analysis/navigation regressions][A]
- [Symbol reader fixtures][S], [generation/navigation commands][SN], [symbol dialogs][ST], [symbol CLI][SCLI], and [symbol PTY recording](evidence/symbol-navigation.cast)
- [Native persistence and sparse-file tests][N] and [adapter fault tests](../crates/hiewbie/src/file.rs)
- [Key/rendering tests][T], [built CLI tests][CLI], and [executable-analysis CLI tests][ACLI]
- [Real Unix PTY scenarios][P], [PTY-harness regressions][PH], [initial editor recording](evidence/native-editor.cast), and [PE analysis recording][Recording]
- [Storage guarantees and limits][Storage], [input behavior][Input], [build strategies][Build]
- [Selected readers/decoders, limits and remaining analysis decisions][Analysis]
- [Versioned original-documentation audit and open differential scenarios][Reference]
