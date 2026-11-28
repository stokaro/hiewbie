# x86 text assembly

The portable core assembles one instruction for an explicit little-endian
x86-16, x86-32, or x86-64 profile. The resolver supplies virtual addresses
for symbols. The editor adapter computes the instruction address from the
current executable mapping or raw base and file offset. It reads executable
symbol tables on demand from the logical document generation. Successful and
failed requests preserve document bytes, pending edits, undo and redo history,
and the file cursor. The edit dialog and patch transactions belong to #78.
[Portable API](../crates/hiewbie-core/src/assembly.rs)
[Shared editor adapter](../crates/hiewbie-core/src/command.rs)

## Syntax

| Element | Accepted forms |
| --- | --- |
| Source | One ASCII instruction, up to 256 bytes. Mnemonics, registers, and size qualifiers are case-insensitive. A semicolon outside quoted text begins a comment. |
| Numbers | Default hexadecimal, hexadecimal `0x` prefix or `h` suffix, decimal `t` suffix. A backtick can divide a hexadecimal address into high and low 32-bit words. |
| Constants | Signed terms, addition and subtraction, and quoted ASCII immediates up to eight bytes packed in little-endian order. |
| Registers | General-purpose, segment, control, debug, x87 `st(n)`, MMX, XMM, and YMM where the selected opcode accepts them. |
| Memory | Brackets with an absolute address or a base, optional index, scale of 1, 2, 4 or 8, and displacement. 16-bit index forms use scale 1. The signed displacement in `[rip - 4]` is relative to the end of the instruction. |
| Memory width | Short and long qualifiers including `b`/`byte`, `w`/`word`, `d`/`dword`, `fword`, `q`/`qword`, `tbyte`, `oword`/`xmmword`, and `yword`/`ymmword`. HIEW also writes a separate size before the bracket, as in `lock add w,[bx],1`. Optional `ptr` suffix and segment before the bracket. An ambiguous width requires a qualifier. |
| Names | Case-sensitive names resolved by the caller. Use `@name` to force lookup of names that also parse as hexadecimal or a register. The editor resolves mapped definitions, section-relative file targets, and import slots. |
| Prefixes | `lock` on a supported memory destination, and `rep`, `repe` or `repne` on compatible string instructions. |
| Aliases | `jmps`, `jmpf`, `callf`, `jz`, `jnz`, `jc`, `jnc`, `sal`, and `retn`. |

For example, `mov eax,"sign"` assembles to `B8 73 69 67 6E` and
`mov ax,10t` to `B8 0A 00` in x86-16. For a target of hexadecimal 1010
and PC of hexadecimal 1000, `jmp 1010` gives `E9 0B 00 00 00` in
x86-32 and x86-64. `jmps 1010` gives `EB 0E`. Moving the PC by one byte
changes the respective displacements to `0A` and `0D`. Relative encodings
check signed displacement against the next instruction in the full linear
address space, including when the 16- or 32-bit architectural IP uses the
low bits of a higher raw base.

A plain direct `jmp` selects the mode's near width, and `jmps` selects
the one-byte displacement. Conditional branches can expose both matching
widths. Immediate far `segment:offset` selects 16:16 in x86-16 and 16:32
in x86-32. `jmpf` and `callf` select indirect far memory forms with
an explicit pointer width, as in `jmpf f,[eax]`. HIEW's 32-bit prefilled
far forms use a separate size operand. The core also accepts named sizes,
such as `fword`, supported by the same operand-width checks.
Intel `jmp short target` is outside the
accepted grammar. HIEW 8.63 rejected that spelling in the sampled x86-32
session.

Each candidate contains the entire instruction and is valid for the
request's mode and address. The API orders distinct candidates by encoded
length and then by bytes. The observed `nop` sequence exposes one candidate
in x86-16 and nine in x86-32/64. No candidate writes to a document.

## Instruction families

The in-tree parser supplies typed operands to iced-x86 1.22.0. The encoder
checks the opcode's mode, operand form, prefix legality, and feature family.
The following table describes the current operand grammar and versioned
family policy. The golden tests exercise the stated representative forms.

| Family | Supported forms | Examples |
| --- | --- | --- |
| Legacy scalar | Integer arithmetic, shifts, register transfers, stack, comparison, basic string and selected system forms | `mov`, `lea`, `cmp`, `enter`, `rep movsb`, `lock add` |
| Control transfer | Checked near relative, conditional, segmented far and sizes for indirect memory | `jmp`, `jmps`, `jz`, `jmpf` |
| x87, MMX, SSE through SSE4.2 | Typed registers, sized memory, and supported immediates in allowed legacy opcode forms | `fadd st(0),st(1)`, `paddb mm1,mm2`, `addps xmm1,xmm2` |
| AES, PCLMULQDQ and selected system extensions | Legacy forms selected by the explicit CPUID-family filter and supported operands | Legacy vector forms and integer/system forms with supported operands |
| Baseline AVX | VEX instructions tagged AVX by iced-x86 with supported XMM/YMM, memory and immediate operands in x86-32/64 | `vaddps`, `vblendps` |

The family filter omits AVX2, EVEX/AVX-512, XOP, MVEX, and other unqualified
extensions. Remaining grammar work includes wider HIEW alias coverage,
expression operators beyond addition and subtraction, and exact choice
order for instructions beyond the measured cases. Macros, directives, and
multi-instruction source are outside the single-instruction API.
[Versioned opcode filter](../crates/hiewbie-core/src/assembly/x86.rs)

## Verification

The checked-in tests use independently derived instruction bytes and generated
raw and PE fixtures. They check first and alternate encodings, interpreted
addresses in all three modes, symbol-generation refresh, exact branch limits,
overflows, invalid syntax, missing names and unchanged editor state.
Native HIEW 8.63 Windows observations cover `jmp` versus `jmps`,
two `vaddps` widths, representative scalar encodings in all three modes,
and `nop` choices. [Reference evidence](evidence/hiew-8.63-windows-x86-assembly.json)
[Core regressions](../crates/hiewbie-core/src/assembly/tests.rs)
