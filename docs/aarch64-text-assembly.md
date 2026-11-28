# AArch64 text assembly

The portable [assembly API](../crates/hiewbie-core/src/assembly.rs) takes a
source line, instruction mode, instruction byte order, actual instruction
address, and symbol resolver. For AArch64 it returns a single candidate of
four bytes in little-endian A64 instruction order. The address must be a
multiple of four. An explicit big-endian instruction profile is unsupported.
An ELF image may store its headers and data in either byte order.
See section B2.9.2 of the [Arm Architecture Reference Manual](https://developer.arm.com/documentation/ddi0487/latest).

The editor core obtains the current address from a file-backed image mapping
or from the checked raw base plus file offset. Executable symbols are resolved
on demand and refer to the inspected image. Obtaining candidate bytes leaves
the cursor, document bytes, pending patches, generation, and undo and redo
history intact. The interactive prompt and patch transaction belong to
[#78](https://github.com/stokaro/hiewbie/issues/78).

## Input

- Enter one ASCII instruction of up to 256 bytes. Mnemonics, registers, and
  fixed operand names are case-insensitive. A semicolon introduces a trailing
  comment. Directives, local label definitions, macros, and multiple source
  lines are unsupported.
- Use hexadecimal numbers by default. The `0x` prefix and `h` suffix also
  select hexadecimal. Use `t` for decimal. A number may include the HIEW
  backtick separator, as in ``00000001`40002028``. Use a leading `#` for ordinary
  immediate operands.
- Use an absolute address for a direct branch, ADR or ADRP, or a literal load.
  For example, `b 140002028` at `140001ff8` emits `0C 00 00 14`. An optional
  `#` on an absolute target leaves its meaning unchanged. PC-relative
  displacements use the current instruction address. ADRP uses the difference
  between the target and instruction 4 KiB pages.
- Use the symbol name directly or prefix it with `@` to force name lookup,
  even for a name that looks like a hexadecimal number. Symbol spelling is
  case-sensitive. An expression may add or subtract numeric terms, as in
  `ldr q9, @Table-4`. Unknown names, overflow, misalignment, and values beyond
  the selected field range return errors.
- Register choices include `w0`–`w30` and `x0`–`x30`, `wsp` and `sp`
  for stack pointer forms, `wzr` and `xzr` for zero register forms, and
  `fp` and `lr` as aliases for `x29` and `x30`. Memory operands use
  brackets, optional `!` for pre-index writeback, and a trailing immediate
  for post-index writeback. Vector operands require their element suffix
  where applicable.

## Supported families

| Family | Encoded forms |
| --- | --- |
| A64 control flow and literal addresses | B, BL, B.cond, CBZ/CBNZ, TBZ/TBNZ, BR/BLR/RET, ADR/ADRP, literal LDR to W/X/S/D/Q, literal LDRSW, literal PRFM with a named operation |
| A64 integer arithmetic | MOV and MOVZ/MOVN/MOVK, ADD/SUB and their flag-setting forms, CMP/CMN, AND/ANDS/ORR/EOR/TST, LSL/LSR/ASR/ROR, MUL/MADD/MSUB, SDIV/UDIV |
| A64 integer and FP/Advanced SIMD memory | LDR/STR with W/X/S/D/Q, LDRB/H and STRB/H, LDRSW, the corresponding LDUR/STUR forms, LDP/STP. Supported forms enforce the appropriate scale, register width, and writeback constraints |
| A64 system | NOP, BRK, SVC, DMB/DSB/ISB. MRS/MSR support TPIDR_EL0, FPCR, FPSR, and NZCV. TPIDRRO_EL0, CNTFRQ_EL0, and CNTVCT_EL0 are read-only |
| FP and Advanced SIMD | Register FADD/FSUB/FMUL/FDIV with matching H/S/D sizes, register vector ADD/SUB on integer arrangements, selected AES and SHA256 operations |
| Armv8 and later extensions | CRC32X/CRC32CX, W/X LSE LDADD variants, Pointer Authentication stack hints and authenticated branch/return/load forms, BTI, MTE IRG/STG, and HBC BC.cond |
| SVE | PTRUE, WHILELO/WHILELT, unpredicated vector ADD/SUB, and same-size single-register contiguous LD1B/H/W/D and ST1B/H/W/D with signed MUL VL offsets |
| SVE2 and SME | EOR3, SQXTNB/SQXTNT, SMSTART/SMSTOP with optional SM or ZA selector |

The forms above have fixed explicit operands. MOV chooses one legal
MOVZ, MOVN, or ORR immediate form. An address too far for ADR returns an error.
Extended system register ranges, other processor extensions, full SVE/SVE2
and SME matrix instruction coverage, relocations, and program-level assembly
remain outside the current scope. Availability of a candidate does not
establish that a particular physical CPU implements the selected extension.

## Validation and provenance

[The fixture](../tests/fixtures/aarch64-assembly.tsv) records the hexadecimal
PC, four expected bytes in file order, Hiewbie input, and independent LLVM MC
source. In the LLVM column, PC-relative operands use decimal displacements.
To reproduce the expected bytes, feed that column to LLVM MC 23.1.6 with
`-triple=aarch64-unknown-linux-gnu -show-encoding` and the `-mattr` field
in the fixture header. LLVM is a validation tool. The production encoder uses
in-tree Rust compiled with the same core on native and WASM targets.

The [corpus test](../crates/hiewbie-core/src/assembly/aarch64_tests.rs) checks
exact bytes, explicit rejections, and resolving the same symbol from distinct
instruction addresses. The [editor tests](../crates/hiewbie-core/src/analysis_tests.rs)
exercise mapped and raw addresses, both ELF data byte orders, and preservation
of edit state. [ADR 0006](adr/0006-text-assembler-selection.md) records the
evaluation and the production encoder decision.
