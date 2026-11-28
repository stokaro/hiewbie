# x86 Decode capability for HIEW 8.63

The portable adapter uses iced-x86 1.22.0. The
[fixture corpus](../tests/fixtures/x86-decode-8.63.tsv) contains 58 rows in
16-, 32-, and 64-bit modes. An additional
[31-row 16-bit corpus](../tests/fixtures/x86-16-hiew-8.63.tsv) records portable
operand text, HIEW opcode boundaries, and HIEW instruction cells. Core tests
assert the complete operand text,
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
`llvm-objcopy --only-section=.text -O binary`. For the 16-bit corpus, assemble
each source line independently and pad it to 16 bytes with `90`. Test each
group as a separate raw file at the offsets in its rows.

| Family | Representative fixture | Adapter modes exercised | HIEW 8.63 observations |
| --- | --- | --- | --- |
| Integer and addressing | `mov`, segment overrides, scaled index, RIP/EIP-relative memory | 16/32/64 | Decoded in all three modes on the applicable samples. |
| Legacy-only and implicit operands | `pusha`, `rep movsb`, `cmovz` | 16/32 | `60` is `pusha` in 16-bit mode, `pushad` in 32-bit mode, and `#UD(64)` in 64-bit mode. String and conditional move samples decoded in 16-bit mode. |
| x87 | `fld1`, `fld` with 64-bit and 80-bit memory | 16/32/64 | Decoded in all three modes. |
| System and protected-mode instructions | `sgdt`, `lar`, `smsw`, `cpuid`, `rdtsc`, `rdtscp`, `sysenter`, `invlpg`, `mov eax,cr0`, `xgetbv` | 16 | Decoded in the selected 16-bit view. |
| MMX and 3DNow! | `paddb`, `pfadd` | 16/32/64 | Decoded in all three modes. |
| SSE through SSE4.2 | `movaps`, `paddq`, `haddps`, `pabsb`, `pmulld`, `pcmpistri` | 16/32/64 | Decoded in all three modes. |
| POPCNT, AES-NI, PCLMULQDQ | `popcnt`, `aesenc`, `pclmulqdq` | 16/32/64 | Decoded in all three modes. |
| AVX | `vzeroupper`, `vaddps` | 16/32/64 | `vaddps` decoded in 32/64. The observed 16-bit VEX forms start with `C5 #UD(mod)`. |
| FMA3 and RDRAND | `vfmadd132ps`, `rdrand` | 16/32/64 | RDRAND decoded in all three modes. FMA3 decoded in 32/64, and its 16-bit sample starts with `C4 #UD(mod)`. |
| AVX2, BMI1, BMI2, XOP, SHA | `vpbroadcastd`, `andn`, `pext`, `vpcmov`, `sha256rnds2` | 32/64 | HIEW displayed `#UD` at the tested instruction starts. The adapter decoded all samples. |
| AVX-512F | masked and zeroing `vaddps` | 64 | HIEW displayed `62 #UD(64)`. The adapter decoded the complete EVEX instruction. |

The table records individual encodings. Broader family coverage and the
exhaustive 8.63 inventory remain open. The locations and byte spans behind
these observations are recorded in the
[general Windows evidence](evidence/hiew-8.63-windows-x86-coverage.json) and
[16-bit Windows evidence](evidence/hiew-8.63-windows-x86-16.json).

The native viewport has tests for the observed segmented and split memory
operands, encoded displacement widths, scaled indexes, x87 operands, repeat
and lock prefixes, conditional-move mnemonics, and vector selectors. The two
Windows comparisons matched 42 original corpus rows and an additional 93
selected instruction rows. The latter includes 85 rows in 16-bit mode and
eight address-displacement rows in 32-bit mode. Headless output retains the
adapter's Intel syntax. Full mnemonic and operand parity and original
rejection lengths for unrecognized extensions remain open.

[Mapped operand observations](evidence/hiew-8.63-windows-x86-operands.json)
cover numbered memory and literal references, a file-backed indirect slot,
and return from Hex to Decode. The original used separate numbers for a
memory operand and a direct branch that shared the same address.

Complete, unambiguous legacy opcodes rejected in long mode have an
`InvalidForMode` status. Interactive 16-bit Decode interprets C4, C5, and
62 as LES, LDS, and BOUND. In their register-only ModR/M forms the row contains
the legacy prefixes and opcode, displays `#UD(mod)` or `#UD(lock)`, and leaves
the ModR/M byte for the next row. Instruction movement and aligned F6 scans
use that same boundary. The portable adapter interprets valid modern
encodings in the shared opcode space using iced-x86.

`Unknown` records an undecoded byte and
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
