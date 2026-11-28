# ADR 0005: add bounded, on-demand symbol and import/export navigation

Status: accepted for the next M2 analysis slice, 2026-11-28. This extends
[ADR 0004](0004-analysis-backends.md) with selected symbol-table readers. Complete
loader analysis, executable reconstruction, and the remaining HIEW baseline
continue to be tracked separately.

## Context and decision

Headers and load mappings can be valid even when a symbol table is malformed,
absent, stripped, or outside the implemented scope. Opening a file and rendering
Code must not require reading every symbol. An import name also does not by
itself identify bytes in the inspected file: a definition, an import pointer
slot, and an unresolved external reference have different navigation semantics.

Add the independent [`symbols` module](../../crates/hiewbie-core/src/symbols.rs)
to the portable core. It reads the current logical document through `ByteSource`
and `Document`, including pending edits, and requires executable metadata from
the same document generation and selected slice. It neither loads external
libraries nor executes inspected code. The implementation and fixtures are
independently written Rust; no parser dependency, upstream source, or bundled
executable sample is added.

`Editor::refresh_symbols` explicitly requests the tables. The existing header
and Code paths retain their own budgets and do not call the symbol reader.
`Analysis` caches either a table or a separate `symbol_problem` for the current
image. Repainting the same view does not repeat a failed parse. Byte edits,
undo/discard, raw interpretation changes, and slice selection invalidate the
appropriate analysis state; callers cannot retrieve stale symbol results.

Parsing is atomic at the symbol-result boundary: an invalid table or exhausted
budget makes that request unavailable with an error. A partial prefix is not
published as a complete table. The failure leaves header mappings, Code, raw
viewing/editing, logical bytes, and edit history available and unchanged.

The shared commands are `GotoSymbol` (`navigate.symbol`) and `GotoSymbolRecord`
(`navigate.symbol-record`). From executable metadata, the terminal exposes
imports with F7, exports with F9, and all parsed entries with N. Enter follows a
target; R follows its record. The headless `--symbols`, `--imports`, and
`--exports` modes use the same core result. These contextual keys do not replace
view-mode search or edit-mode save. The N/R bindings are additive hiewbie
controls, not a claim about original HIEW interaction.

## Records and targets are separate

Every displayed `Symbol` owns its name, kind, optional library/ordinal/size,
detail, target, and a nonempty `FileRange` for the actual metadata record decoded.
Records use global file offsets, including within a selected universal Mach-O
member. Array indices are positions in the current parsed table, not persistent
identities across edits. Duplicate names and export aliases retain their records.

| Target | Meaning | Navigation rule |
| --- | --- | --- |
| `Virtual` | Definition at an image virtual address | Translate through the current unambiguous, file-backed mapping |
| `Arm` | ELF ARM-state function established by symbol metadata | Translate the VA and supply an advisory ARM decoder hint |
| `Thumb` | ARM Thumb definition with an instruction-state hint | Remove the state bit for byte translation and supply an advisory Thumb decoder hint |
| `File` | Checked section-relative definition in a relocatable object | Use its global file position within the selected slice |
| `ImportSlot` | PE IAT pointer storage | Follow the slot bytes, without resolving or following the stored external address |
| `Unavailable` | Unresolved name, forwarder, alias, absolute value, or another value without an interpreted file target | Reject target navigation; keep record navigation available |

Target validation precedes cursor and decoder-context changes. Unmapped,
ambiguous, zero-fill, or invalid destinations produce an error. Some formats
retain an unmapped virtual value for display while others mark its target
unavailable; neither case creates a file position. Following a record exposes
metadata bytes and performs no structural repair, relocation, checksum update,
or implicit save. Names and details are escaped when displayed by the terminal
or headless output.

Decoder priority is an explicit user profile, then a current symbol-navigation
hint, then the image's inferred mode. Following mixed ARM/Thumb symbols never
converts a hint into a persistent explicit override. Absolute/entry/region/record
navigation, slice changes and logical-generation changes clear the hint;
incremental movement preserves the selected instruction state. Failed navigation
preserves the cursor and context. Branch return restores an automatic hint only
for the same logical generation, so edited symbol metadata cannot resurrect an
old inferred mode. Untyped definitions use image inference; relocatable ELF
file targets retain the manual-profile limitation described below.

## Implemented format scope

### PE32 and PE32+

The [PE reader](../../crates/hiewbie-core/src/symbols/pe.rs) reads ordinary import
descriptors, lookup thunks, hint/name strings, ordinal imports, and the export
directory. An absent lookup table can use a structurally valid, unbound IAT.
A bound descriptor without a lookup table produces an explicit omission note;
its external addresses are not interpreted as names. Valid imports target IAT
slots. Their records are the lookup entries, or IAT entries for the fallback.

Exports preserve named aliases and ordinal-only entries. Records identify EAT
entries. Empty unnamed EAT holes are skipped; a named empty entry is displayed
without a target. Forwarder strings must terminate within the export-directory
RVA interval and remain unresolved. Other exports retain their VA, with checked
file translation when followed.

Delay-load imports, bound-directory resolution, COFF symbols, and debug/PDB
symbols remain unsupported. Applicable directory/COFF declarations produce
coverage notes. This result is not a complete Windows loader inventory.

### ELF32 and ELF64

The [ELF reader](../../crates/hiewbie-core/src/symbols/elf.rs) reads named
section-backed `SHT_SYMTAB` and `SHT_DYNSYM` entries in either data byte order.
It validates table strides, the linked string table, the null entry, local-symbol
partition, name indices, termination, and section references. Declared extended
strides are respected, but record navigation covers only the decoded standard
16/24 bytes; a note identifies uninterpreted trailing bytes.

Undefined global/weak entries are imports. Defined global/weak dynamic entries
with default/protected visibility are labelled exports; other named entries
remain ordinary symbols. These labels describe the parsed table and do not
resolve dynamic interposition, symbol versions, or providers. Duplicate records
remain distinct and names are not decorated with guessed versions or libraries.

Linked definitions acquire a navigable target only when the section-backed byte
agrees with the unique `PT_LOAD` mapping. `ET_REL` definitions use checked
section-relative file offsets. Linked ARM Thumb functions retain their state
hint, and even ARM function values supply ARM-state hints. For `ET_REL` Thumb symbols the file position excludes the state bit, but
the `File` target does not automatically choose a decoder mode; the detail
identifies Thumb state.

ABS, COMMON, TLS, NOBITS, compressed sections, section-end markers, unresolved
names, and unsupported type/index semantics do not receive invented file
destinations. Extended section numbering, `SHN_XINDEX`, and compressed
symbol/string tables are unsupported. Sectionless `PT_DYNAMIC` reconstruction,
hash-based dynamic table discovery, versions, `DT_NEEDED` provider resolution,
relocations/GOT/PLT slots, load bias, and IFUNC execution/resolution remain gaps.
An empty section-backed result explicitly does not establish an empty runtime
import/export inventory. Unnamed entries are omitted with a coverage note.

### Mach-O32, Mach-O64, and universal members

The [Mach-O reader](../../crates/hiewbie-core/src/symbols/macho.rs) reads
`LC_SYMTAB` nlist records and their string table in either data byte order, for
the selected thin image or fat member. Public external definitions are labelled
exports; local/private definitions remain symbols. Undefined/prebound external
names are imports without fabricated binding slots or runtime addresses.

Two-level imports can retain library ordinals and names from ordered normal,
weak, re-exported, upward, and lazy dylib dependencies. Common-symbol size and
alignment fields are not treated as library ordinals. Definitions validate their
section ordinals, section bounds, and backing bytes. Mapped definitions use
checked VAs and ARM Thumb hints; relocatable sections can use their file offsets
when the segment mapping does not agree with the section.

Common, absolute, indirect, TLS, zero-fill, end-of-section, unknown-type, and
unresolved resolver values remain without a navigable definition. Indirect aliases
are displayed without recursively resolving chains. All TLS section types remain
unavailable as process addresses, including file-backed templates and descriptors.
STABS and unnamed records are omitted with explicit notes.

Export tries, `LC_DYLD_INFO` binding/rebase streams, chained fixups, and
`LC_DYSYMTAB` indirect/stub bindings are not decoded. Their presence produces
specific coverage notes; applicable dyld/link-edit range bounds are still
checked. Missing or empty nlist data does not establish that an image has no
imports or exports. Modern stripped Mach-O files particularly require this
distinction.

## Resource and failure boundaries

Symbol inspection has its own limits, independent of the 1 MiB header-reader
budget established in ADR 0004. Counts and arithmetic are checked before table
allocation. Reads use bounded chunks and do not allocate in proportion to total
file size. Mapped table spans must fit one unambiguous contiguous backed mapping;
strings additionally stop at their declared table/directory or mapping boundary.

| Resource | Limit for one symbol request |
| --- | --- |
| Cumulative bytes requested, including repeated reads | 8 MiB |
| Displayed entries | 16,384 across all kinds |
| Raw bytes in one name | 1,024 before the required NUL |
| Retained UTF-8 text | 4 MiB across names, libraries, details, notes, and charged dependency metadata |
| Distinct coverage notes | 32 |
| ELF sections; Mach-O load commands/sections | 4,096 per applicable bounded traversal |
| ELF raw symbol records | 16,384 cumulatively across symbol sections |
| Mach-O nlist records | 16,384 |
| PE export address/name arrays | 16,384 entries per array; aliases share the output-row budget |
| PE import descriptors / thunks | 16,384 non-null entries per bounded traversal, plus a terminating read |
| Headless displayed rows | 64 after filtering, with the omitted count and coverage notes |

These are application limits rather than claims that larger valid files are
malformed. Malformed data, unsupported interpretation, unmapped addresses, and
resource exhaustion retain distinct errors. Name decoding may use replacement
characters for invalid UTF-8 without altering original bytes; retained-text
accounting includes that expansion.

The initial request is synchronous and bounded. Region translation can scan the
bounded mapping list for each definition. The limits establish bounded work and
memory, not a measured latency guarantee. Background progress/cancellation for
larger analysis remains open under IO-006.

## Provenance and verification scope

Format facts come from primary specifications and published layout definitions:

- Microsoft [PE/COFF](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format):
  ordinary import/export directories, lookup/IAT entries, ordinals, and forwarders.
- System V gABI 4.2 [sections](https://gabi.xinuos.com/v42/elf/03-sheader.html),
  [string tables](https://gabi.xinuos.com/v42/elf/04-strtab.html), and
  [symbol tables](https://gabi.xinuos.com/v42/elf/05-symtab.html).
- Arm [ELF32 symbol values](https://github.com/ARM-software/abi-aa/blob/main/aaelf32/aaelf32.rst#553symbol-values),
  consulted 2026-11-28, for the Thumb state bit.
- Apple XNU at `43a90889846e00bfb5cf1d255cdc0a701a1e05a4`:
  [loader.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/loader.h)
  and [nlist.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/nlist.h).

The source text, upstream implementations, and proprietary HIEW material were
not copied. The [reuse policy](../references-and-reuse.md) and dependency/license
inventory continue to apply; this slice adds no dependency or upstream notice.

Generated [PE](../../crates/hiewbie-core/src/symbols/pe_tests.rs),
[ELF](../../crates/hiewbie-core/src/symbols/elf_tests.rs), and
[Mach-O](../../crates/hiewbie-core/src/symbols/macho_tests.rs) fixtures exercise
the supported widths/orders, aliases and unresolved values, malformed counts,
indices, strings and extents, mapping/overflow rejection, and resource limits.
A sparse fat64 source places its selected member beyond 4 GiB while requiring
only bounded reads. [Shared tests](../../crates/hiewbie-core/src/symbols/tests.rs)
cover reader budgets, generation checks, records, notes, and rejected navigation.
The common [fixture generators](../../tests/fixtures/symbol_images.rs) supply
application/frontend scenarios without distributing foreign executable binaries.
The [shared navigation tests](../../crates/hiewbie-core/src/symbol_navigation_tests.rs)
cover lazy loading, current logical bytes, records, failed destinations and
generation refresh. [CLI](../../crates/hiewbie/tests/symbol_cli.rs) and
[terminal](../../crates/hiewbie/src/terminal/symbol_tests.rs) regressions verify
bounded escaped output and contextual key routing. The generated
[PTY scenario](../../scripts/pty_smoke.py) and
[recording](../evidence/symbol-navigation.cast) follow an IAT slot, a definition
and an unresolved forwarder's record, then verify exact saved bytes and terminal
restoration.

These tests provide generated-fixture and local command evidence. Native runtime
checks, cross-target CI, browser execution, and original-HIEW differential
observations remain separate evidence categories under the
[development workflow](../development-workflow.md) and
[release gates](../build-and-release.md). This decision does not declare those
gates passed or M2 complete.

The affected contracts are FMT-003--006 and FMT-012--018; ADDR-001--006;
IO-001--003; EDIT-001/011--013; and KEY-001/002/006. Existing architecture,
legacy-format, assembly, search, reference-analysis, and host-matrix requirements
remain in force.
