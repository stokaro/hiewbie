# Hiew 8.63 reference comparison

Comparison date: **2026-11-28**. Audited hiewbie baseline:
[`3410f36`](https://github.com/stokaro/hiewbie/tree/3410f369e0abdc185409c646524f8f221a39b2fd).
This is a comparison of the supplied original documentation with hiewbie's
source, followed by hiewbie regression tests. **The original application did
not start in the available environment; there is no observed runtime parity.**

Related contracts: UI-001/002/004/005/011, KEY-003..008, EDIT-002..006,
ADDR-001..006, SEARCH-001/002, META-001, MACRO-001/002, TEST-001/003/011.

## Reference identity and evidence

The owner supplied a local Hiew distribution containing `hiew32.exe`, the
English and Russian manuals, contextual help, and configuration. The manuals
identify **8.63**, with a 2018-12-31 release entry. Static file inspection
identifies the executable as PE32, i386, Windows console. Its running version
and enabled edition have not been verified. The presence of `hiew32demo.txt`
does not establish that the executable is the demo edition.

Sources in this report are paraphrased from these original files, decoded as
CP866. Section names and original line numbers make the findings reproducible
for an owner with the same distribution; the proprietary files are not included.

| Source | SHA-256 |
| --- | --- |
| `hiew_en.txt` | `f05897d99a12a4220c0e2a7467751fc0f53dcc338d9e78ebc7b0bc4859e2de24` |
| `hiew_ru.txt` | `837f03e31d3f9ace3bff21a62e5b73e4a138476426c735163ae0921f4bcd3716` |
| `hiew8.hlp` | `5d6623cc927b04268145ff7a6d269f1b8908d75e025b8b49d1055449a8c2006e` |
| `hiew8.ini` | `ae454b45a483e383f5bc70347a928cf371206643d34f93a8f46a3e71812f5f2d` |

Evidence labels have distinct meanings:

- **Documented**: a behavior described by the supplied author documentation.
- **Implemented/tested**: a hiewbie behavior covered by its own tests; the PR's
  validation records the actual commands and host.
- **Observed**: a behavior recorded while running the original. No scenario in
  this report has this label yet.
- **Open**: a detail requiring a reference run, or an unimplemented parity gap.

### Runtime attempt

Wine 11.0 amd64 WoW64 itself reported its version. Both the Windows command
interpreter probe and `wine hiew32.exe /Oh=0 probe.bin` failed before the
reference program started:

```text
wineserver: socket: Operation not permitted
```

Exit status was 1. The generated 1,024-byte fixture remained unchanged. The
[attempt record](../evidence/hiew-8.63-runtime-attempt.json) identifies the
runtime and input without distributing either binary. This establishes a local
execution restriction, not an application defect or a Hiew/Wine incompatibility.

## Corrections to the existing editor

These changes address existing M1 commands; they do not claim completion of
the Code/format work in [issue #5](https://github.com/stokaro/hiewbie/issues/5).

| Command | Baseline behavior | Refined hiewbie behavior and evidence |
| --- | --- | --- |
| Edit Backspace | Moves left and leaves the edited byte changed | Restore the preceding byte from the transaction's source, then move there. A pending first hex digit is cancelled separately. Documented: `hiew8.hlp`, `[HexEdit]`, line 337. |
| Edit F3 | Reports byte restore as unavailable | Restore the byte at the cursor to its source value, preserving other edits. Documented: `[HexEdit]`, line 323. |
| View F4 | Shares Enter's immediate mode cycle | Hiew documents F4 as mode selection. Hiewbie implements an explicit chooser using the shared `view.set-mode` command; Enter still cycles. The chooser is a hiewbie design decision; the original F4 interaction remains unobserved. Sources: H `[Text]`, `[Hex]`, `[Code]`. |
| View Shift+F7 | Rejected as an unsupported modified key | Invoke the same repeat-search path as Ctrl+Enter and Ctrl+R. Documented: `[Text]`, line 232; `[Hex]`, line 295. Original search continuation semantics remain a separate gap. |

Restore is deliberately distinct from undo history. For example, change byte 0
twice, change byte 1, return to byte 0, and press F3: byte 0 must return to its
source value while byte 1 stays changed. Undoing the most recent edit would act
on the wrong byte. Hiewbie keeps Ctrl+Z/Y as additive command-history operations;
restores themselves are undoable. A successful save establishes a new source
baseline for later transactions.

The hiewbie boundary policy is explicit: previous-byte restore at offset zero is
a no-op; a source read failure changes neither bytes, history, nor cursor. The
exact original offset-zero, nibble, buffer-boundary and cursor behavior still
needs observation. Code remains unavailable in the chooser, and Text editing
remains a hiewbie addition rather than a verified original Text-mode command.

### Validation of these changes

On Linux x86_64 with pinned Rust 1.90.0:

- `cargo xtask check` passed formatting, workspace Clippy with warnings denied,
  target-manifest validation and all **79 Rust tests**.
- `python3 -m unittest discover -s scripts -p 'test_pty_smoke.py' -v` passed
  all **8 harness lifecycle tests**.
- `python3 scripts/pty_smoke.py --binary <built-hiewbie> --record
  docs/evidence/native-editor.cast` passed **4 real PTY sessions**: mode
  selection/byte restoration/save, Shift+F7 repeat/save, discard/resize, and
  read-only refusal. They also checked exact terminal restoration.
- The updated [terminal recording](../evidence/native-editor.cast) shows the
  hiewbie workflow on a generated fixture. It is not a recording of Hiew.

This is native Linux hiewbie evidence. It does not establish original Hiew,
Windows ConPTY, macOS terminal-application, browser or release-matrix parity.

## Next development priorities

### Search must retain a structured request and its own continuation point

The English manual describes case-insensitive search when confirmed in the
ASCII field and exact matching when confirmed in the hex field
(`hiew_en.txt`, Search and replace, lines 255-258). The Russian manual also
mentions Russian letters and documents a continuation point independent of
ordinary cursor movement (`hiew_ru.txt`, lines 265-274). Ctrl+Home, Ctrl+End,
F5 and a new F7 search reset that continuation state. Translation tables contain
uppercase mappings used by search (`hiew_en.txt`, lines 516-521).

Hiewbie currently searches literal bytes, including explicitly entered UTF-8,
case-sensitively, and repeats from the visible cursor plus one. Adding Shift+F7
does not resolve these differences. The next search change needs a request
holding bytes/text, encoding, case policy, direction, scope and last-match state;
repeat must preserve that request. Test `ABC abc AbC`, cursor movement after a
hit, each reset command, and a pattern crossing a read-chunk boundary. Do not
equate ASCII folding with arbitrary Unicode or legacy-code-page folding.

Whether Tab converts between two representations of one query, and what happens
at an unsuccessful search boundary, require a reference run. No wrap behavior is
inferred here.

### Text is a line-oriented view, with encoding and layout settings

The original Text help describes line wrapping, selectable line endings,
tab expansion and translation tables (`hiew8.hlp`, `[Text]`, lines 211-226;
`hiew8.ini`, lines 19-44). Hiewbie's current Text mode is an ASCII byte grid.
Keep that limitation visible until line indexing, bounded scanning and byte-to-
display-position mapping are implemented. Editing must still address raw bytes.

The supplied profile selects Text initially and overrides the default decoding
width to 16 bits. These are settings of this distribution, not proof of built-in
defaults. The 8.63 release notes limit UTF-8 handling to one- and two-byte
sequences; hiewbie should not adopt that old limitation. Complete Unicode
display remains planned; existing UTF-8 input and Unicode paths stay supported.

### Navigation state is richer than a single cursor

Original bookmarks preserve eight screen states independently for each mode.
Backspace in view returns after navigation/search; it is distinct from edit
Backspace (`hiew_en.txt`, lines 211-217; `hiew8.hlp`, `[Bookmark]`, lines 137-144).
Implement per-mode bookmarks and navigation returns as shared state, with
bounded histories and separate tests for each key context.

Address work belongs to issue #5. A dotted PE address can mean a VA or an
offset within the current section under the original's documented alignment
rules (`hiew_en.txt`, Local and Global offsets, lines 345-379). Display base is
another independent operation. Test raw `220`, VA `.401020` and section-relative
`.20` on a generated PE with raw section offset `0x200`, RVA `0x1000` and image
base `0x400000`. A parser that accepts only absolute dotted VAs does not complete
the original goto contract. Exact ambiguity/boundary handling needs observation.

### Preserve contextual and version-specific scope

F8 in Text selects a translation table; in Hex/Code it opens executable
information. F9 in view opens the file list; in edit it saves changes. F10 in
edit truncates at the cursor; it must never be treated as a universal Quit key.
The view Escape/F10 timestamp wording and the state immediately after edit F9
need measurement before changing persistence policy.

The distribution's `file_id.diz` predates documented Mach-O support (8.51) and
TE support (8.60). Use the manuals, contextual help and release notes together.
Retain older formats, indirect references, instruction-pattern search, assembly
variants, blocks, macros, transforms and extensions in the existing epic.

## Differential scenario inventory

All original-side expectations below are **Documented**, with open details
called out explicitly. They become **Observed** only after a recorded reference
run. Existing hiewbie tests are implementation evidence, not original captures.

Abbreviations: H = `hiew8.hlp`; E = `hiew_en.txt`; R = `hiew_ru.txt`;
I = `hiew8.ini`.

| ID | Controlled scenario / assertion | Original source |
| --- | --- | --- |
| H01 | Start in explicit Text/Hex/Code; distinguish Enter's cycle from F4 selection; record cursor/scroll and selector UI. | H `[Text]`, `[Hex]`, `[Code]`; E 528-558 |
| H02 | Change a byte, cancel editing, compare every byte and record resulting mode/cursor. | H `[HexEdit]` |
| H03 | Repeatedly edit a byte and a second byte; F3 restores the selected original byte, Backspace the preceding one; save and compare exact bytes. Observe partial-nibble and offset-zero cases. | H 323, 328, 337 |
| H04 | Tab switches edit entry field; view Tab selects a historical file. Record cursor/nibble position. | H 253, 320 |
| H05 | Exercise view/edit F9 and F10 separately; record save transition, truncation confirmation and timestamps. | H 252, 268-269, 328-329; I 102-103 |
| H06 | Search mixed-case ASCII/Russian data from character and hex fields; capture active encoding and hit offsets. | E 255-258, 516-521; R 265-267 |
| H07 | Move the cursor after a search hit and repeat; test continuation resets with Ctrl+Home/End, F5 and F7. | R 271-274 |
| H08 | Exercise search direction, selection scope and multiple input files; discover end-of-search behavior. | H 21-29, 277; E 262-263, 294-300 |
| H09 | Search an arbitrary-byte wildcard between exact bytes; record insertion and matching semantics. | H 28; E 298-300 |
| H10 | Input dialog clearing/history/insertion/cancel; record whether representation switches preserve one query. | H `[StringEdit]`, `[FindEdit]` |
| H11 | Store/restore/delete eight screen bookmarks per mode; test navigation return and slot rollover. | E 211-217; H `[Bookmark]` |
| H12 | Select/resize/all-select a block; measure endpoint inclusion and reversed ranges. | H `[Block]` |
| H13 | Export/append/import a block; test the previous-file block source and destination mode. | E 150-161; I 94-97, 113-116 |
| H14 | Copy/move/delete/fill/insert blocks; compare bytes, lengths, overlapping ranges and cancellation. | H 273, 290-294 |
| H15 | Distinguish argv file navigation from history navigation, including offset/state preservation settings. | H 253, 286-289; E 663; I 77-78 |
| H16 | Translate raw, VA and current-section-relative dotted addresses in generated executable fixtures. | E 345-379 |
| H17 | Change display base, including a current-position-relative base; verify bytes stay unchanged. | E 143-147; R 144-152 |
| H18 | Follow labelled direct/indirect branches and return, including nested targets and configured jump tables. | E 25-29, 220-227; R 224-238; H 426-428 |
| H19 | Search instruction patterns and slash-separated sequences; distinguish forced assembly and command/byte stepping. | E 265-285; H 57-62, 388 |
| H20 | Assemble an instruction, choose an encoding variant, NOP the current instruction and save/cancel; inspect lengths and bytes. | E 121-140; H `[CodeEdit]` |
| H21 | Browse generated PE/ELF/Mach-O and legacy headers, tables and byte ranges; include malformed input. | H header sections `[HeaderMZ]`, `[HeaderNE]`, `[HeaderLX]`, `[HeaderPE]`, `[HeaderELF]`, `[HeaderMacho]`, `[HeaderNLM]` |
| H22 | Extract strings by encoding/length/scope; distinguish string goto from reference search; persist annotations separately. | E 432-470; H 881-906 |
| H23 | Evaluate 64-bit expressions and typed dereferences; measure overflow, division and invalid-address handling. | H `[Calculator]` |
| H24 | Record/save/replay a macro, use startup macro/config/state options, capture the screen and compare fixture bytes. | E 391-429, 500-504, 528-558; H 205, 254, 361 |
| H25 | Empty/truncated/large files, odd Unicode offsets, line endings/tabs/wrapping and resizing; measure resource bounds. | E 382-388, 659; H 25, 871-875; I 19-44 |

## Repeatable reference capture

Run the original locally on Windows or another environment that actually starts
it. Record executable hash/version/edition, OS and terminal, configuration hash,
fixture bytes/hash, actions, resulting cells/bytes and pass/fail/open status.
Keep failures and timeouts distinct from observed results.

The author documents `/Oh=0` (Hex at offset zero), `/Ot=END`, `/Oc=OEP`, separate
`/INI=` and `/SAV=` paths, `/MACRO0=` startup playback and Alt-P screen export.
The supplied profile has `KbdFlush=On`, which discards buffered typeahead before
reads. A keyboard-injection runner should use a separate documented test profile
with `KbdFlush=Off`, bounded waits for screen states and isolated fixture copies.

No macro file was supplied, and its serialization format is not documented.
Record the first macro through Hiew itself before using startup playback; do not
invent a macro encoding. Screen-export format also needs an initial capture.
On Windows, a future runner can use console input/output APIs to deliver keys,
capture cells, and verify file changes. That runner and native reference results
remain open work, not delivered automation in this change.

Keep the original program and full screen captures local. Public test fixtures
should be generated; publish independently written expectations and sanitized
observations. Repeated hiewbie validation uses its existing core, frontend and
PTY regression suites.
