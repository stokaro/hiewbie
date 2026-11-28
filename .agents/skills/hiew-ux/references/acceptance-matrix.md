# HIEW UX acceptance matrix

Use this matrix to turn an open observation into a bounded reference capture and
an automated hiewbie acceptance test. It defines the evidence needed for a
high-fidelity core UX replacement. It does not by itself establish complete
binary, extension, device, or platform compatibility.

## Evidence contract

Each row remains open until both gates are satisfied:

1. **Reference oracle:** record the initial state, one key or semantic action at
   a time, resulting terminal cells, colors, cursor, title and function bar,
   logical state, and resulting bytes where applicable.
2. **hiewbie gate:** replay the same semantic sequence against a generated or
   redistributable fixture and assert deterministic cells, command state,
   transaction state, and exact byte effects.

Public evidence must contain no proprietary binaries, raw captures, copied
manual text, proprietary-file fingerprints, local paths, hostnames, logins, or
environment-specific identifiers. Describe the reference environment only by
reported product version, OS family, terminal class, dimensions, and behaviorally
relevant settings. Keep raw reference material private.

Status vocabulary:

- **Open oracle:** reference behavior is not sufficiently observed.
- **Oracle captured:** a sanitized expected result exists, but hiewbie is not
  fully automated against it.
- **Implemented:** deterministic hiewbie coverage passes on the development host.
- **Verified:** the required terminal and platform matrix passes for the exact
  commit.
- **Intentional difference:** the deviation and its accessible fallback are
  documented and tested.

Priority `P0` is required for a credible core look-and-feel replacement. `P1`
completes familiar editing and navigation workflows. `P2` covers advanced and
platform breadth needed before an unqualified drop-in compatibility claim.

## Frame, rendering, and terminal input

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-FRM-01 | P0 | UI-003, UI-011 | Open the same raw fixture read-only, enter Hex edit, change one byte, save, then reopen. | Record the exact meanings and transitions of `FRO`, `FWO`, and `FUO`, including dirty state, file access, title address, and post-save token. | Cell snapshots and application-state assertions cover every transition; access tokens never imply a capability the document lacks. |
| UX-FRM-02 | P0 | UI-002, UI-005, UI-006 | Capture View, Edit, Block, Search, Help, Header, and modifier states at 80 by 25. | Record all ten function slots, blank slots, labels, colors, and the moment each bar replaces another. | One command registry produces dispatch, help, and bars; snapshots assert all ten slots for every state. |
| UX-FRM-03 | P1 | UI-008, TERM-011, TEST-011 | Start at 80 by 25, widen, narrow to the supported minimum, and resize with each dialog active. | Record clipping, reflow, minimum-size treatment, cursor retention, overlay placement, and restoration after resize. | Snapshot breakpoints include 80 by 25, wider, minimum supported, and resize-during-overlay; no panic or stale cells. |
| UX-FRM-04 | P1 | UI-009, TERM-010 | Render every state with the original-style palette, then an accessible theme. | Record semantic uses of selection, pending edit, finalized block, dialog field, disabled command, and error colors. | Theme-independent state tests plus cell-style snapshots prove color is redundant with text, cursor, counter, or label. |
| UX-FRM-05 | P1 | UI-006, UI-010, TERM-005, TERM-006 | Hold and release Shift, Ctrl, and Alt alone and with every contextual function key. | Record transient layer timing, labels, unavailable commands, chord completion, cancellation, and intercepted-key behavior. | A table-driven key suite covers enhanced and legacy protocols; every command has a documented reachable fallback. |
| UX-FRM-06 | P1 | UI-012, TERM-009, TERM-013 | Feed a key, an ambiguous escape sequence, bracketed paste, and an unsupported chord. | Record whether the event becomes a command, text, paste, diagnostic entry, or visible error. | Key diagnostics identify decoded events without sensitive environment data; paste never dispatches embedded command sequences. |
| UX-FRM-07 | P2 | UI-015, TERM-012, TEST-011 | Exit normally and trigger recoverable render, input, and save errors while modes are acquired. | Record cursor visibility, screen buffer, input mode, and error presentation before and after exit. | PTY or ConPTY tests assert restoration of raw mode, alternate screen, cursor, paste, mouse, and keyboard negotiation. |
| UX-FRM-08 | P2 | TERM-002..006, TERM-010 | Replay the P0 scenario set in the required macOS, Windows, Linux, SSH, and tmux terminals. | Record only behaviorally relevant transport differences and unavailable chords. | Exact-commit platform jobs pass the declared matrix or record an intentional difference with tested fallback. |
| UX-FRM-09 | P0 | UI-008, UI-009, UI-011 | Compare paired HIEW 8.63 and hiewbie frames at identical terminal dimensions and settings using generated fixtures. Include Hex edit and view, mode chooser, Search and progress, help, headers, file browser, and errors. | Record bounds, padding, border glyphs, shadow offset, colors, selected rows and fields, cursor form and cell, title, and function bar. The frames must look indistinguishable at first glance. | Assert glyph and style cells plus native caret position and shape. Inspect the paired frames on the reference Windows console and record terminal-specific limits. Report functional coverage separately. |

## Modes, navigation, and file selection

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-NAV-01 | P0 | UI-001, UI-004 | Position mid-row, scroll off the first screen, then use Enter and F4 across Text, Hex, and Code. | Record logical byte, viewport origin, selected subfield, decoder alignment, title address, and F4 cancel behavior. | Command tests and snapshots prove one shared cursor/document while preserving any observed per-view viewport state. |
| UX-NAV-02 | P0 | KEY-003, ADDR-001..006 | On generated raw and PE fixtures enter `220`, decimal suffix, `+/-`, absolute dotted VA, and current-section-relative dotted forms, including overflow and unmapped values. | Record accepted grammar, resolved address, field/error presentation, cursor movement, history effect, and bytes remaining unchanged. | Table-driven parser and UI tests assert checked resolution and exact rejection; no ambiguous form is silently reinterpreted. |
| UX-NAV-03 | P1 | UI-004, KEY-004 | Store, restore, overwrite, and delete eight bookmark slots independently in each mode. | Record saved cursor, viewport, mode, bitness, format context, selection, slot display, and rollover behavior. | Application tests serialize logical bookmark state and snapshots cover store/restore/delete feedback for all modes. |
| UX-NAV-04 | P1 | KEY-004, KEY-007 | Navigate by goto, search, data reference, code target, header table, and nested target; then press Backspace or Escape as appropriate. | Record which operations push return state, nesting depth, restored mode/address/viewport, and empty-history behavior. | A bounded return-stack suite distinguishes view Backspace from edit restore and overlay Escape from application exit. |
| UX-NAV-05 | P1 | UI-005, KEY-006 | Start with multiple arguments, open Files, traverse directories, use history, filter/sort, switch next file, and cancel. | Record selection retention, path presentation, sort/filter state, per-file offset/mode restoration, empty directory, and inaccessible entry errors. | Generated directory trees and file lists replay deterministically without embedding local paths; commands preserve configured per-file state. |
| UX-NAV-06 | P2 | UI-003, KEY-004 | Change display base at an absolute and current-relative position, then navigate and switch modes. | Record title/address formatting, persistence scope, reset command, and confirmation that bytes are unchanged. | Address-display tests separate presentation base from document offsets and format mappings. |

## Search and textual presentation

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-SRCH-01 | P0 | SEARCH-001, UI-005 | Open Search in Text, Hex, and Code; switch ASCII/Hex fields and toggle direction, scope, case, even alignment, and assembler input. | Record one-query versus two-field semantics, focus, conversion rules, validation, contextual bar, and cancel restoration. | A structured search request drives both dialog and engine; snapshots and state tests cover every toggle and field transition. |
| UX-SRCH-02 | P0 | SEARCH-001, TEST-008 | Search generated mixed-case ASCII and legacy-code-page text through character and exact-byte fields. | Record encoding, folding policy, matched offsets, displayed query, unsupported-character behavior, and whether hex is always exact. | Generated fixtures assert offsets for each supported policy; unsupported folding is explicit rather than approximated silently. |
| UX-SRCH-03 | P0 | SEARCH-001, KEY-004 | Find a hit, move the cursor, repeat, then reset with Ctrl+Home, Ctrl+End, F5, and a new F7 request. | Record the independent continuation point, direction, last-match state, reset behavior, and return-history interaction. | Search state is separate from cursor state; command tests assert every repeat and reset transition. |
| UX-SRCH-04 | P1 | SEARCH-001 | Search forward/backward in whole file and selected block, including no hit, first/last boundary, optional wildcard, and even alignment. | Record wrap policy, boundary prompt/error, wildcard syntax and match width, scope endpoint inclusion, and post-failure state. | Table-driven engine and dialog tests cover all combinations with exact offsets and deterministic no-result cells. |
| UX-SRCH-05 | P1 | SEARCH-002, TEST-008 | Search a pattern spanning a read chunk in a large sparse fixture, then cancel during a no-hit scan. | Record progress or busy feedback, cancellation latency, cursor and continuation state after cancel, and memory bounds. | Bounded-work tests prove chunk-boundary matches, responsive cancellation, unchanged document state, and capped allocation. |
| UX-TEXT-01 | P1 | UI-001, TERM-010 | Display generated LF, CRLF, CR, tabbed, wrapped, long-line, invalid-byte, and multibyte text fixtures. | Record line-ending selection, tab stops, wrap/unwrap, translation table, byte-to-cell mapping, cursor movement, and invalid-byte glyphs. | A bounded line index and deterministic snapshots preserve raw-byte addressing across layout settings. |
| UX-TEXT-02 | P1 | KEY-005, EDIT-002..006 | Enter Text edit, replace and insert representable and multibyte characters, cross line endings, then save and discard. | Record availability, cursor unit, byte encoding, length changes, pending colors, invalid input, and transaction boundaries. | Generated byte assertions prove exact encoding and untouched regions; unrepresentable input fails without dirtying the transaction. |

## Editing, persistence, and blocks

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-EDIT-01 | P0 | KEY-005, TEST-002, TEST-003 | Edit at offset zero, row end, EOF, and both nibbles; use Left/Right, Tab, F3, and Backspace before save/discard. | Record nibble cursor, field switch, partial-nibble cancellation, source-byte restore, preceding-byte restore, advancement, colors, and title address. | A table-driven transaction suite asserts bytes, pending nibble, cursor, dirty ranges, and screen cells after every action. |
| UX-EDIT-02 | P0 | KEY-007, TEST-002 | Make changes in multiple rows, press Escape, and reopen the file. | Record whether confirmation appears, restored mode/cursor, access token, mtime policy, and any retained source values. | Exact byte and transaction assertions prove discard leaves content unchanged and clears pending state. |
| UX-EDIT-03 | P0 | KEY-005, TEST-003, TEST-013 | Save one and several noncontiguous changes with F9, then reopen. | Record save feedback, cursor and mode, access-token transition, dirty-color clearing, failure message, and retry behavior. | Exactly intended bytes differ; save is atomic or reports its limits, and a failed save retains recoverable pending changes. |
| UX-EDIT-04 | P1 | KEY-005, TEST-004, TEST-005 | In Code edit assemble valid, alternate-length, relative, invalid, and oversized instructions; choose variants, NOP, save, and cancel. | Record assembler form, candidate selection, address-dependent encoding, size policy, errors, cursor movement, and pending-byte colors. | Known encodings at multiple addresses assert exact bytes; invalid or oversized input changes neither bytes nor dirty state. |
| UX-EDIT-05 | P1 | KEY-005 | Invoke F10 at start, middle, EOF, inside a pending edit, and on read-only input. | Record confirmation text structure, target length, cancel path, cursor correction, access state, and error presentation. | Temporary-file tests assert exact length and bytes for confirm/cancel/failure without truncating before confirmation. |
| UX-EDIT-06 | P1 | EDIT-005, TEST-002, TEST-003 | Edit, save, discard, and exit with timestamp preservation enabled and disabled. | Record view Escape/F10 policy, mtime/access-time behavior, prompt sequence, and configuration scope. | Filesystem-aware tests distinguish guaranteed mtime behavior from platform-defined access time and external writers. |
| UX-EDIT-07 | P1 | EDIT-003, EDIT-006, TEST-013 | Trigger permission denial, short write, full destination, rename failure, and external modification. | Record visible error, retry/discard choices, dirty transaction retention, original-file integrity, and capability tokens. | Fault-injected persistence tests prove no silent data loss and preserve untouched bytes and recoverable pending state. |
| UX-BLK-01 | P0 | UI-005, TEST-009 | Start, extend, reverse, shrink, finalize, cancel, and select all at offset zero and EOF. | Record inclusive/exclusive endpoints, anchor/cursor roles, range counter, reversed-range display, colors, and bar transitions. | Selection-state tests and snapshots assert exact ranges and counters independent of movement direction. |
| UX-BLK-02 | P1 | TEST-009 | Insert/overwrite, delete, copy, and move blocks before, after, and overlapping the source, including EOF. | Record confirmation, destination semantics, resulting cursor/selection, length changes, overlap policy, and cancellation. | Generated byte vectors assert exact results and transaction rollback for every overlap and boundary class. |
| UX-BLK-03 | P1 | TEST-009 | Export, append, import, and reuse a prior-file block with empty, existing, read-only, and failing destinations. | Record form defaults, append marker, offset interpretation, translation-table effect, overwrite prompt, and errors. | Isolated generated files assert exported/imported bytes and unchanged sources; displayed paths are sanitized in fixtures. |
| UX-BLK-04 | P2 | UI-005 | Apply fill, XOR, crypt, translation, and other exposed transforms to selected ranges and cancel midway where supported. | Record parameter forms, preview or progress, repeat semantics, pending colors, and unavailable-operation presentation. | Each supported transform has known-vector tests, bounded work, cancellation, and exact untouched-byte assertions. |

## Analysis, formats, and advanced workflows

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-FMT-01 | P0 | FMT-001, UI-005 | Open generated PE32/PE32+ summary, sections, imports, exports, directories, entry point, and referenced bytes; return one level at a time. | Record columns, address forms, selection, contextual bars, navigation target, unavailable tables, and overlay stack. | Permissively generated fixtures drive parser assertions and cell snapshots for every implemented PE table and return path. |
| UX-FMT-02 | P1 | FMT-001..006, TEST-006 | Repeat the header workflow on generated ELF and Mach-O fixtures for each supported architecture. | Record format-specific hierarchy, labels, address mapping, entry navigation, symbols, and unsupported controls. | Cross-host application results are identical for the same fixtures; intentional UI differences are documented by format. |
| UX-FMT-03 | P2 | FMT-001, UI-011 | Exercise MZ, NE, LE/LX, NLM, TE, and every retained legacy target with minimal generated fixtures. | Record detection, summary/tables, navigation, raw fallback, and visibly unavailable operations. | Each promised format has a provenance-safe fixture, parser test, snapshot, and explicit capability ledger entry. |
| UX-FMT-04 | P1 | FMT-007, TEST-010, TEST-012 | Open empty, truncated, contradictory, overlapping, overflowing, and unknown-format fixtures. | Record error placement, retained raw-byte access, mode availability, address fallback, and recovery navigation. | Corpus and fuzz regressions prove bounded parsing, no panic, precise errors, and unchanged raw access. |
| UX-CODE-01 | P1 | ISA-001..010, KEY-004 | Follow direct/indirect targets, nested labels, configured jump-table entries, references, next reference, and return. | Record target labels, selection, instruction alignment, history, unresolved target errors, and data/code distinction. | Generated instruction fixtures assert address-neutral command results and bounded reference work across supported ISAs. |
| UX-CODE-02 | P2 | SEARCH-001, ISA-001..010 | Search instruction patterns and slash-separated sequences with forced assembly and byte/command stepping. | Record grammar, candidate encoding policy, alignment, match presentation, continuation, and invalid-pattern errors. | Pattern tests assert exact matched ranges per backend and preserve raw-byte fallback where assembly is unavailable. |
| UX-ADV-01 | P1 | ANNO-001..004 | Create, edit, delete, navigate, save, and reload names and comments at raw and mapped addresses. | Record forms, inline markers, collisions, persistence scope, invalid addresses, and behavior after file edits. | Sidecar or project-state tests keep annotations separate from source bytes and round-trip stable identifiers. |
| UX-ADV-02 | P1 | UI-005 | Evaluate signed, unsigned, binary, split-hex, character, arithmetic, overflow, divide-by-zero, and typed dereference expressions. | Record live-update timing, formatting widths, separators, invalid input, mapped/unmapped dereferences, and cancel state. | Expression tests assert checked results and snapshot all display rows without reading beyond document bounds. |
| UX-ADV-03 | P2 | KEY-008, MACRO-001..004 | Record, stop, save, replay, cancel, and startup-play a macro containing navigation, dialog input, and an edit. | Record command/argument capture, prompts, recursion/error handling, timing independence, and final screen/bytes. | Macros store semantic commands, replay deterministically, enforce bounds, and never depend on terminal event timing. |
| UX-ADV-04 | P2 | EXT-001..008, DEV-001..006 | Open extension discovery and unavailable device/adapter commands on supported and unsupported platforms. | Record discovery layout, capability reporting, lifecycle errors, permission failures, and fallback commands. | Portable extensions and platform adapters have separate tests; unavailable native compatibility is explicit and nonblocking. |

## Startup, configuration, scale, and product substitution

| ID | Pri | Requirements | Controlled setup and action | Observable oracle | Automation and completion gate |
| --- | --- | --- | --- | --- | --- |
| UX-OPS-01 | P1 | UI-011, UI-014 | Launch with explicit Text/Hex/Code, start offset, entry point, separate configuration/state paths, multiple files, invalid options, and no file. | Record option precedence, initial mode/address, file browser entry, diagnostics, exit status, and which state persists. | CLI integration tests use isolated generated state and document every supported compatibility option and intentional difference. |
| UX-OPS-02 | P1 | UI-009, TERM-010 | Change key bindings, colors, text layout, translation tables, state preservation, and keyboard-flush-related settings. | Record load precedence, live versus restart application, invalid-setting errors, and separation of configuration from saved state. | Round-trip tests use a documented hiewbie format; proprietary configuration syntax is supported only when explicitly implemented. |
| UX-OPS-03 | P1 | UI-013, TEST-007 | Open empty, one-byte, page-boundary, very large, and sparse files beyond 4 GiB; move to beginning, middle, and end. | Record first-screen latency, EOF layout, address width, scrollbar behavior, edit boundaries, and cancellation. | Performance tests enforce bounded startup, viewport work, allocation, checked arithmetic, and exact sparse edits. |
| UX-OPS-04 | P2 | TEST-001, TEST-011 | Replay the complete P0 sequence through native PTY/ConPTY, headless commands, and browser terminal adapters. | Record only frontend-specific key transport or file-transfer differences; logical states and bytes remain identical. | Shared scenario vectors assert command, document, and transaction parity across frontends; differences have explicit capability flags. |
| UX-OPS-05 | P2 | TEST-014 | Install each declared package, launch with a generated fixture, perform one edit, save, reopen, and exit. | Record startup diagnostics, terminal restoration, packaged notices, and resulting bytes without local identifiers. | Exact-release artifacts pass the required target matrix; missing artifacts or skipped launches block a drop-in claim. |

## Claim gates

Use the narrowest accurate claim:

| Claim | Minimum evidence |
| --- | --- |
| **HIEW-inspired interface** | No matrix threshold; deviations remain explicit. |
| **High-fidelity core UX replacement** | Every P0 row is at least Implemented, with reference-oracle coverage for the behavior attributed to HIEW and a passing visual comparison on the stated Windows reference console. |
| **Behaviorally compatible core workflow** | Every P0 and P1 row is Verified on the declared terminal/platform subset; intentional differences are listed. |
| **Drop-in replacement** | Every applicable P0, P1, and P2 row is Verified for the declared CLI, format, terminal, platform, frontend, packaging, extension, and device scope. The claim must name exclusions. |

Do not aggregate an open row into a percentage. Report it by ID, status, exact
evidence, tested platforms, and remaining difference.
