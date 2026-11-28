# x86 Decode capability for HIEW 8.63

The portable adapter uses iced-x86 1.22.0. The
[fixture corpus](../tests/fixtures/x86-decode-8.63.tsv) contains 58 encodings in
16-, 32-, and 64-bit modes. Core tests assert the complete operand text,
opcode length, preserved bytes, absence of direct branch metadata, and
one-byte recovery when the final byte of a multi-byte encoding is missing.
Separate branch tests check architectural PC wrap and file-offset validation.

The reference observations came from the licensed HIEW 8.63 executable on
an x64 Windows Server Core console in a Hyper-V VM. All inspected files were
generated data and were opened for disassembly only. The corpus records the
Intel source assembled by LLVM MC 23.1.6. To regenerate a raw stream for a given
mode, prepend `.intel_syntax noprefix`, `.text`, and `.code16`, `.code32`,
or `.code64` to that mode's source column. Assemble with
`llvm-mc -triple=x86_64 -filetype=obj` and extract `.text` with
`llvm-objcopy --only-section=.text -O binary`.

| Family | Representative fixture | Adapter modes exercised | HIEW 8.63 observations |
| --- | --- | --- | --- |
| Integer and addressing | `mov`, segment overrides, scaled index, RIP/EIP-relative memory | 16/32/64 | Decoded in all three modes on the applicable samples. |
| Legacy-only and implicit operands | `pusha`, `rep movsb`, `cmovz` | 16/32 | `60` is `pusha` in 16-bit mode, `pushad` in 32-bit mode, and `#UD(64)` in 64-bit mode. String and conditional move samples decoded in 16-bit mode. |
| x87 | `fld1`, `fld` with 64-bit and 80-bit memory | 16/32/64 | Decoded in all three modes. |
| MMX and 3DNow! | `paddb`, `pfadd` | MMX in 16/32/64, 3DNow! in 32/64 | Decoded in each exercised mode. |
| SSE through SSE4.2 | `movaps`, `paddq`, `haddps`, `pabsb`, `pmulld`, `pcmpistri` | SSE in 16/32/64, all others in 32/64 | Decoded in each exercised mode. |
| POPCNT, AES-NI, PCLMULQDQ | `popcnt`, `aesenc`, `pclmulqdq` | 32/64 | Decoded in both modes. |
| AVX | `vzeroupper`, `vaddps` | 16/32/64 | `vaddps` decoded in 32/64. The 16-bit `vzeroupper` sample started with `C5 #UD(mod)`. |
| FMA3 and RDRAND | `vfmadd132ps`, `rdrand` | 32/64 | Both samples decoded in 32/64. |
| AVX2, BMI1, BMI2, XOP, SHA | `vpbroadcastd`, `andn`, `pext`, `vpcmov`, `sha256rnds2` | 32/64 | HIEW displayed `#UD` at the tested instruction starts. The adapter decoded all samples. |
| AVX-512F | masked and zeroing `vaddps` | 64 | HIEW displayed `62 #UD(64)`. The adapter decoded the complete EVEX instruction. |

The table records individual encodings. Broader coverage within these
families, further 16-bit extension probes, and an exhaustive 8.63 inventory
remain open. Extension samples that HIEW rejects can still be decoded by the
adapter. The exact rejection boundaries observed on the console are recorded
in the [Windows evidence](evidence/hiew-8.63-windows-x86-coverage.json).

The native viewport has tests for the observed segmented and split memory
operands, signed displacements, scaled indexes, x87 size markers, string and
conditional-move mnemonics, and vector selectors. A paired Windows run matched
the opcode bytes, mnemonic and operand cells, and instruction colors for 42
rows before the first rejected encoding in each stream. Headless output
retains the adapter's Intel syntax. Full mnemonic and operand parity, original
rejection lengths for unrecognized extensions, and memory-reference arrows
remain open.

Complete, unambiguous legacy opcodes rejected in long mode have an
`InvalidForMode` status. Ambiguous opcode space such as legacy BOUND and EVEX
uses ordinary backend results. `Unknown` records an undecoded byte and
`Truncated` records incomplete input. These results preserve the first raw
byte and allow the next offset to be inspected. The original terminal can
continue past the end of a file with a different interpretation. For example,
it displays `sldt [rax]` for a lone trailing `0F` on the observed fixture.

Near branches keep their architectural 16-, 32-, or 64-bit destination.
Mapping that destination back to the file still requires a real in-range
offset. Numbered rows are refreshed after buffer changes, raw-base and profile
changes, slice selection, and explicit resynchronization.

Textual assembly is tracked separately through the
[assembler decision](adr/0006-text-assembler-selection.md) and
[#76](https://github.com/stokaro/hiewbie/issues/76).
[Reference search](commands.md#x86-reference-search) has its own coverage
under [#75](https://github.com/stokaro/hiewbie/issues/75). Executable format
recognition and address mapping are tracked by the format entries in the
[compatibility ledger](compatibility-status.md). Browser integration is
covered by [#95](https://github.com/stokaro/hiewbie/issues/95) and
[#96](https://github.com/stokaro/hiewbie/issues/96).
