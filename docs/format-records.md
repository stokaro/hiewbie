# Bounded executable structure records

The portable format reader exposes navigable records for TE firmware and selected
Mach-O load commands, sections, thread states, and relocations. The native
terminal and `--structures` use the same result from the current logical document,
including pending byte edits. This extends header and address inspection under
FMT-005/006/008/011/013/014/015 and ADDR-001/004. It does not complete the broader
format-inspector issue #59 or establish full HIEW format parity.

The implementation is independent MIT-licensed Rust and adds no parser dependency.
Primary specifications, inspected source revisions, and fixture provenance are
listed in [References and source reuse](references-and-reuse.md#structure-record-format-references-and-provenance).

## Inspecting and following records

Open executable headers in the native terminal, then press **T** to display
structure records. T is an additive hiewbie control. It is not an original-HIEW
runtime observation.

| Control in the structure dialog | Action |
| --- | --- |
| Up/Down, PgUp/PgDn, Home/End | Select a record, including records beyond the visible rows |
| Enter | Follow the selected record's checked target bytes |
| R | Follow the selected metadata record's own bytes |
| Left/Right | Scroll the selected detail text |
| F8 | Return to the header overview |
| Esc | Close the dialog |

Names, status, source range, optional target range, and details remain separate.
An unsupported payload still has a raw metadata record. A record with no target
explains that limitation; Enter fails without moving the cursor, while R can
still follow its record. The same distinction applies to zero-fill memory,
unresolved references, padding relocations, and uninterpreted loader metadata.

Headless output prints complete details for at most 64 records per page:

```sh
hiewbie --structures firmware.te
hiewbie --structures --structure-page 1 image.bin
hiewbie --structures --slice 1 universal.bin
```

Pages are zero-based. Each page reports the total record count, page count,
coverage notes, and the next-page command when applicable. Every retained record
can be reached by paging. A missing or overflowing page is an error;
`--structure-page` requires `--structures`. A valid image whose format exposes no
structure records has an empty page 0. A failed format interpretation explains
the error and leaves `--dump` available for raw bytes. Displayed metadata is
escaped in both terminal and headless output.

## Shared record and navigation contract

[`StructureRecord`](../crates/hiewbie-core/src/formats.rs) owns the following
information:

| Member | Meaning |
| --- | --- |
| `name`, `kind` | Display label and load-command, section, relocation, debug-directory, or debug-data category |
| `record` | Exact range of metadata bytes used for this record |
| `target` | Optional validated range of destination bytes; its meaning is stated in the detail |
| `status` | Interpreted metadata or an explicitly unsupported interpretation |
| `detail` | Bounded explanation of values, references, pairing, and coverage limitations |

Ranges use complete-file offsets, including a selected universal Mach-O member's
prefix. Source and destination spans must lie entirely inside that selected
image. A numeric header field retains its original on-disk value and backing
range; a derived destination does not replace that value. A relocation may have
one record for its stored value and another for its reference, with different
destinations. For example, a reference can target an nlist record rather than the
symbol's eventual runtime address.

`Executable::structure_ref(index)` returns a `StructureRef` containing the
document generation, selected image range, selected fat-member index, and record
index. `GotoStructure` (`navigate.structure-target`) and `GotoStructureRecord`
(`navigate.structure-record`) refresh analysis and validate all those identifiers
before using the selection. A byte edit or slice change therefore invalidates an
old selection, even when the same array index exists in the new result. The
exhausted generation value is also rejected. Empty, missing, stale, overflowing,
or out-of-document destinations fail before cursor and decoder-context changes.

Successful navigation clears any previous symbol-derived decoder hint and moves
to the chosen span. It does not edit bytes, apply relocations, repair headers,
load an image, or save the document. Entry navigation retains its separate
checked VA translation; an unbacked declared entry remains visible as metadata
without acquiring a file destination.

## TE and TE64

### Header layout and machine interpretation

TE uses the UEFI PI 40-byte header beginning with `VZ`. Its `ImageBase` field is
64 bits for every machine. There is no second invented TE64 header layout.
Hiewbie uses TE and TE64 as machine-width labels according to the following
mapping. This interpretation comes from primary format specifications; the
official HIEW format list alone does not establish the details of HIEW's labels.

| On-disk machine | Family | Width/label | Initial decoder interpretation |
| --- | --- | --- | --- |
| `0x014C` | x86 | 32 / TE | x86-32 |
| `0x8664` | x86-64 | 64 / TE64 | x86-64 |
| `0x01C0` | ARM | 32 / TE | ARM, with the entry state bit respected |
| `0x01C2` | UEFI ARMTHUMB_MIXED | 32 / TE | ARM or Thumb from the entry low bit |
| `0x01C4` | ARMNT / Thumb-2 | 32 / TE | Thumb |
| `0xAA64` | AArch64 | 64 / TE64 | AArch64 |
| Other values | Preserved numeric machine | Unknown width | No host-derived decoder; explicit override remains available |

UEFI assigns `0x01C2` to a mixed ARM/Thumb image. Its relocation instruction
type is independent of the entry's execution state. The original entry RVA and
machine field remain unchanged in metadata. Entry navigation removes the ARM
state bit only when translating that entry to instruction bytes; arbitrary VA
navigation does not silently clear address bits.

### Stripped-header addressing

Let `bias = StrippedSize - 40`. A retained section's original raw pointer becomes
`TE file offset = PointerToRawData - bias`; its linked address stays
`VA = ImageBase + original RVA`. The retained header begins at
`ImageBase + bias`. Subtracting bias again from the section VA would be wrong.

The retained header prefix extends to `BaseOfCode - bias`, including padding.
That range must contain the complete TE and section headers and fit the file.
Explicit sections take precedence over the prefix's padding mappings: their
virtual extents and full raw-file spans are excluded from those mappings. This
allows data before code without duplicate translations and prevents section
zero-fill or raw padding from acquiring invented header backing. The EDK2 loader
has stricter acceptance rules for sections before code; successful metadata
inspection is not a claim that a particular firmware loader accepts the image.

For the generated fixture, `StrippedSize = 0x120`, `bias = 0xF8`, and
`ImageBase = 0x400000`:

| Location | Original value | TE file position | VA |
| --- | --- | --- | --- |
| TE header | Replaces the stripped prefix | `0` | `0x4000F8` |
| Retained header padding | Offset within the new header prefix | `0x400` | `0x4004F8` |
| `.text` section | RVA/raw pointer `0x1000` | `0xF08` | `0x401000` |
| Entry | RVA `0x1010` | `0xF18` | `0x401010` |
| Relocation target | RVA `0x1020` | `0xF28` | `0x401020` |

`StrippedSize == 40` is accepted as zero bias when the remaining layout is valid.
Smaller values, truncated header/table ranges, raw-pointer underflow, and address
overflow fail before navigation or mutation. Empty sections do not subtract a
bias from a nonexistent raw pointer. RVA targets use the validated image mappings;
matching section/file alignment does not justify treating every RVA as a file
offset. A complete target span must fit one unique file-backed mapping, including
any conflicting mapping in the middle of the span.

### Relocation and debug metadata

The two TE data directories retain their original PE RVAs and sizes. A relocation
RVA that is nonzero with size zero denotes a position-independent image. It
produces a PIC note without reading a target, including when that RVA is smaller
than the stripped bias. Both fields zero mean no relocation table. A nonzero
size with zero RVA is invalid.

Relocation blocks and their original type/offset words have exact record ranges.
The reader recognizes ABSOLUTE padding, HIGH, LOW, HIGHLOW, HIGHADJ, ARM_MOV32,
THUMB_MOV32, and DIR64. HIGHADJ consumes two words as one four-byte record with
a two-byte target. Its second word is adjustment data, not another relocation.
ARM/Thumb MOV32 targets span the eight-byte MOVW/MOVT pair. Numeric relocation
types shared by other machines are not assigned ARM semantics. Unsupported
types remain visible without a fabricated target width. No fixup is evaluated
or applied.

Debug directory records expose their timestamps, versions, type, data size, RVA,
and original raw pointer. A raw pointer subtracts the stripped bias. Data whose
RVA is zero can remain a checked raw-file overlay outside section mappings. When
both a mapped RVA and raw pointer are present, their complete ranges must agree.
RSDS and NB10 CodeView fixed headers expose signature, age, and GUID words or
offset/timestamp fields. PDB filenames and remaining payload bytes stay raw;
external PDBs are not opened. Other debug types and unrecognized CodeView
signatures are explicit unsupported payloads with raw records available. TE
symbol/import/export reconstruction remains outside this reader.

## Mach-O32, Mach-O64, and universal members

The selected thin image is interpreted in its declared byte order and width.
Fat32/fat64 descriptors retain explicit member selection and global file ranges;
the host CPU does not choose a different member or thread-state layout.

| Structure | Implemented inspection | Explicit limits |
| --- | --- | --- |
| Every load command | Original type, size, and complete bounded raw command range | Unknown commands remain unsupported, including commands marked required by dyld; inspection does not establish loadability |
| `LC_SEGMENT`, `LC_SEGMENT_64` | Segment fields, protection, flags, full section headers, known section types and instruction flags | Unknown section backing or unavailable section mappings retain a raw record without invented target bytes |
| `SG_HIGHVM` | Zero-fill prefix and file-backed suffix | The prefix has no file target |
| `MH_OBJECT` sections | Section-specific file/VA mappings and relocation value locations | Compact object sections need not share the aggregate segment displacement |
| `LC_MAIN` | Checked entry through the selected `__TEXT` mapping | Nonzero `__TEXT` file offset remains explicitly unsupported; ambiguous/unbacked entry is rejected |
| `LC_UNIXTHREAD` | Supported PC state selects the entry when `LC_MAIN` is absent | Unknown flavors remain raw; no state is decoded using a host structure layout |
| `LC_THREAD` | Supported PC/state fields and checked PC target | The context does not select the process entry |
| `LC_SYMTAB` | Descriptor and checked table ranges, referenced nlist metadata | Optional malformed/unavailable symbol backing can leave image mappings available with an unsupported record |
| `LC_DYSYMTAB` | Descriptor fields and external/local relocation words | Loader-dependent dynamic patch bases and module/TOC/indirect tables are not interpreted |

Supported thread states use fixed counts of 32-bit words, including for 64-bit
machines: x86 state32 flavor 1/count 16, x86 state64 flavor 4/count 42, ARM
state32 flavor 1 or 9/count 17, and ARM64 state64 flavor 6/count 68. ARM CPSR's
state bit supplies the entry ARM/Thumb hint. Duplicate supported PC states and
malformed flavor/count spans fail. `LC_MAIN` takes entry precedence when both
entry mechanisms are present; the thread-state record remains inspectable.
The raw thread PC is a register value, separate from CPSR state. Misaligned
ARM/Thumb thread PCs retain their raw metadata but cannot be followed; they are
not silently normalized like the tagged ELF/PE/TE entry pointers.

Section relocation inspection retains the original eight-byte words, plain
endianness-dependent bitfields, 32-bit scattered `r_value`, type names, stored
values, and reference provenance. Supported families include generic x86,
x86-64, ARM, and ARM64. Generic/ARM PAIR relationships, ARM HALF's repurposed
length bits, x86-64/ARM64 SUBTRACTOR plus UNSIGNED pairs, and signed ARM64 ADDEND
plus BRANCH26/PAGE21/PAGEOFF12 pairs are described. PAIR and ADDEND payloads do
not acquire independent patch addresses. Orphan or malformed pairs and unknown
types remain explicitly unsupported records without inferred patch targets.

Relocation references distinguish absolute values, section ordinals, nlist
indices, and scattered values. A reference target can be the section header or
the exact nlist record; it is never presented as a resolved external address.
Invalid or unavailable references can produce local unsupported reference rows
while preserving the useful record and mappings. Referenced names are read in a
bounded prefix and may be explicitly abbreviated. Dyld export tries, bindings,
rebases, chained fixups, indirect/stub resolution, relocation application, and
runtime symbol resolution remain separate work.

## Bounds, failures, and verification

The common format request enforces these independent limits:

| Resource | Limit |
| --- | ---: |
| Cumulative bytes requested, including repeated reads | 1 MiB |
| Table entries per applicable bounded traversal | 4,096 |
| Retained load regions across the image | 4,096 |
| Retained numeric header fields | 32,768 |
| Retained structure records across all kinds | 4,096 |
| Inspection notes | 256 |
| Each structure name, detail, or note | 1,024 UTF-8 bytes |
| Universal Mach-O descriptors | 64 |
| Headless records per page | 64 |

Mach-O sections and relocation entries also have cumulative 4,096-entry bounds.
Reference rows consume the shared structure-record budget, so a file can reach
that limit before exhausting a raw relocation count. These are application
limits, not assertions that larger files violate their format specification.
The limits bound work and retained metadata; they are not a latency guarantee.
The on-demand symbol reader has its own scope and budgets described in
[ADR 0005](adr/0005-symbol-navigation.md).

Mandatory header/mapping failures expose a format error and preserve raw bytes.
Unsupported optional semantics retain explicit raw records when their bounds are
known. Neither outcome modifies the logical document or edit history. Editing,
undo/redo, discard, and persistence continue through the existing document and
native file adapter; no parser silently repairs or reconstructs an executable.

Regression sources are:

- [TE parser tests](../crates/hiewbie-core/src/formats/te_tests.rs): 21 focused
  cases covering machine widths, original address arithmetic, retained header
  padding, data-before-code, BSS, PIC, pair handling, CodeView/unknown records,
  overlays, all fixture truncations, range/budget failures, and exact logical
  patch/reopen bytes.
- [Mach-O parser tests](../crates/hiewbie-core/src/formats/macho_tests.rs):
  generated command, section, thread, relocation, endianness, and fat-member
  coverage. Fixtures are constructed in [macho_images.rs](../tests/fixtures/macho_images.rs)
  and [macho_records.rs](../tests/fixtures/macho_records.rs).
- [Shared command tests](../crates/hiewbie-core/src/structure_navigation_tests.rs):
  source/target separation, rejected target preservation, stale generation and
  slice selection, unbacked entries, and mixed ARM/Thumb entry behavior.
- [Terminal tests](../crates/hiewbie/src/terminal/structure_tests.rs): keys,
  long-list/detail navigation, tiny screens, raw fallback, and refreshed fields.
- [Native CLI tests](../crates/hiewbie/tests/structures_cli.rs): all output pages,
  selected-member global offsets, seven TE machine/entry cases, exact native
  save/reopen bytes, all baseline Mach-O thread modes in both byte orders,
  relocation-record save/reopen, malformed fallback, and unknown-machine overrides.
- [PTY scenarios](../scripts/pty_smoke.py) and
  [terminal recording](evidence/format-structures.cast): TE entry/target/record
  navigation, unsupported relocation refusal, and fat-member record navigation,
  with exact saved bytes and terminal restoration assertions.

These tests establish the implemented metadata and editing subset. They do not
demonstrate execution of the fixtures by firmware or a platform loader, the
complete required host matrix, or original HIEW runtime equivalence. Per-commit
validation and remaining requirements stay in the PR and
[compatibility status](compatibility-status.md).
