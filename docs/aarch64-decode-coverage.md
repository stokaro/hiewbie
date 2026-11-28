# AArch64 Decode coverage

AArch64 is a first-class hiewbie extension. The same portable core serves Code
view and `--disassemble` independently of the host CPU. The primary decoder is
yaxpeax-arm 0.5.0, with in-tree decoding for the explicitly listed supplemental
forms. See [#73](https://github.com/stokaro/hiewbie/issues/73).

## Instruction and data byte order

A64 encodes fixed four-byte, little-endian instructions. Automatic
interpretation chooses that instruction order for raw A64, Mach-O, and both
ELF data orders. The executable retains its declared order for metadata and
data. The CLI prints the image data byte order separately when it differs
from the selected instruction order. An explicit big-endian instruction
override yields raw `Unsupported` rows. [Arm instruction endianness](https://support.arm.com/documentation/ddi0487/mb/-Part-B-The-AArch64-Application-Level-Architecture/-Chapter-B2-The-AArch64-Application-Level-Memory-Model/-B2-9-Endian-support/-B2-9-2-Instruction-endianness)

## Checked addresses

| Encoding | PC base and target | Code view |
| --- | --- | --- |
| B, BL, B.cond, BC.cond, CBZ/CBNZ, TBZ/TBNZ | Current instruction VA plus the signed byte displacement | Direct branch stays in Code |
| ADR | Current instruction VA plus the signed byte displacement | A backed address opens Hex |
| ADRP | Page containing the instruction VA plus the signed page displacement. The architectural page is 4 KiB | A backed page start opens Hex |
| LDR and LDRSW literal, PRFM literal | Current instruction VA plus signed, scaled literal displacement | A backed address opens Hex |

The result retains a checked virtual address for valid arithmetic, including
targets in zero-fill regions. Navigation requires backing bytes and a valid
mapping, or an in-file address under the explicit raw profile. An address
outside the 64-bit domain is rendered as `<address out of range>`. Returning
from Hex restores the Code location. Re-decoding after edits uses the logical
bytes. Unaligned resynchronization remains available for inspection.
[Arm ISA overview](https://developer.arm.com/-/media/Files/pdf/graphics-and-multimedia/ARMv8_InstructionSetOverview.pdf)

## Versioned coverage

The corpus qualifies concrete forms and operand variations. Expanded
upstream feature tables, general references, loader-resolved addresses,
and architecture-revision filtering remain separate work.

| Family | Checked coverage | Current limits |
| --- | --- | --- |
| Armv8-A scalar baseline | Register and immediate arithmetic, stack pairs and immediate-offset memory operations, system register access, barriers, direct branches, and PC-relative address/literal forms | Further baseline instructions and system register names need qualification |
| Floating point and Advanced SIMD | Representative scalar and vector arithmetic | Full formats and optional revisions need qualification |
| AES, SHA2, CRC32 and LSE | Representative crypto, checksum and atomic encodings | Other encodings require separate qualification |
| Pointer authentication and BTI | PACIASP, AUTIASP, RETAA, BRAA, LDRAA and all four BTI hint encodings | Authenticated control-flow targets are dynamic |
| MTE | IRG and STG examples | Inspection does not interpret tags or tagged pointers |
| SVE | PTRUE patterns, scalar WHILELT and WHILELO, unpredicated vector ADD/SUB, and same-size contiguous LD1/ST1 for byte/halfword/word/doubleword with scalar base and signed vector-length immediate | Predicated arithmetic, widening loads, gather/scatter, and other forms remain open |
| SVE2 | EOR3 and signed saturating narrow-bottom/top SQXTNB/SQXTNT | Other SVE2 families remain open |
| SME | SMSTART and SMSTOP with the default, SM, and ZA selectors | Matrix and tile instruction decoding remains open |

Unknown/reserved words preserve bytes and advance by four. When the backend
reports an unimplemented encoding class, the status is `Unsupported`
with the encoding-class reason. Incomplete words preserve one to three bytes
and report `Truncated`. The existing repository decoder and the in-tree paths
each have bounded output.

## Reproduce the corpus

The [baseline and extension fixture](../tests/fixtures/aarch64-decode.tsv)
contains family, bytes in file order, LLVM source, and exact expected
hiewbie text. Assemble column 3 with LLVM MC 23.1.6, target
`aarch64-unknown-linux-gnu`, options `-show-encoding` and
`-mattr=+v8.6a,+aes,+sha2,+crc,+lse,+pauth,+bti,+mte,+sve2,+sme`.
The [PC-relative fixture](../tests/fixtures/aarch64-pc-relative.tsv) uses
column 5 as LLVM source, the same target, and `-show-encoding`. Its independent
PC and expected target columns exercise branch limits, page transitions,
literal load sizes, unaligned inspection, and checked 64-bit boundaries.
The [generated ELF fixture](../tests/fixtures/aarch64_images.rs) reuses the
same architectural byte order in both ELF variants and encodes a zero-fill
target separately from a backed target. Raw, ELF, Mach-O, and terminal
regressions cover the production routes.

The supplemental encodings were independently checked using Arm instruction
semantics and LLVM MC output across operands, widths, patterns, signed
displacements and reserved neighbors. No external decoder or ISA table was
incorporated. The reviewed disarm64 0.2.0 release points to generated input
from a binutils-derived extractor. Provenance of the generated production
tables requires resolution before adoption for wider coverage.
[Arm SVE guide](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/SVE%20programmers%20guide/102476_0001_00_en_introduction-to-sve.pdf)
[disarm64 0.2.0](https://docs.rs/crate/disarm64/0.2.0)
[Extractor](https://github.com/kromych/opcodes-lab)
