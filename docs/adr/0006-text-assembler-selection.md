# ADR 0006: Select a portable text assembly path

Accepted for implementation on 2026-11-28 under
[#27](https://github.com/stokaro/hiewbie/issues/27), part of
[#1](https://github.com/stokaro/hiewbie/issues/1).

## Decision

Use a bounded, in-tree MIT parser for HIEW-style assembly. Its input includes
the inspected ISA/profile, byte order, and actual instruction address. The
parser resolves numeric tokens and symbols. The backend returns candidate
bytes and diagnostics without access to a document.

- [#76](https://github.com/stokaro/hiewbie/issues/76) uses iced-x86 **1.22.0**
  encoder and opcode metadata. Lower parsed operands to structured
  instructions and enumerate legal encodings in stable order. Implement
  x86-16, x86-32, and x86-64 with checked symbols, sized memory operands,
  explicit branch widths, and a versioned AVX table.
- [#77](https://github.com/stokaro/hiewbie/issues/77) uses asm-rs **0.2.0**
  with only its AArch64 feature and defaults disabled. Lower one validated
  instruction and pass resolved destinations as external absolute symbols.
  Reject directives, macros, and multi-line input before lowering. Require
  exactly four output bytes. Start with little-endian AArch64 and checked
  PC-relative encodings. Qualify the production instruction-family table
  before enabling it in the editor.
- [#78](https://github.com/stokaro/hiewbie/issues/78) integrates the HIEW
  dialog, encoding choices, preview, span validation, NOP padding, and
  atomic undoable overwrite.

The product retains its existing decoder features. The standalone
[research probe](../../experiments/assembler/README.md) has a locked
manifest and runs without any native assembler or network service.
Production qualification, packaging, and notices belong to the respective
implementation stories and [#94](https://github.com/stokaro/hiewbie/issues/94)
and [#97](https://github.com/stokaro/hiewbie/issues/97).

## Reproduced grammar and encodings

`hiewbie-assembler-probe 0.0.0` accepts one ASCII instruction up to 128
bytes. Unsuffixed numbers are hexadecimal. `0x` and `h` specify hexadecimal,
and `t` specifies decimal. Its grammar covers selected register immediates,
explicit jump widths, one AVX form, and AArch64 B and NOP. It rejects
unresolved names. The production parser will supply a resolved-symbol
interface and publish its operand and instruction-family coverage.

| Probe input and actual PC | In-tree parser and encoder | LLVM MC with equivalent source |
| --- | --- | --- |
| x86-16 `mov ax, 10t` | `B8 0A 00` | Same with `0xa` |
| x86-32 `mov eax, 12345678` | `B8 78 56 34 12` | Same with `0x12345678` |
| x86-64 `jmp 1010` at `1000` | `EB 0E`, `E9 0B 00 00 00` | `EB 0E` with a local label |
| x86-64 `jmp 1010` at `1001` | `EB 0D`, `E9 0A 00 00 00` | `EB 0D` with a local label |
| x86-64 `vaddps xmm1,xmm2,xmm3` | `C5 E8 58 CB` | Same |
| AArch64 `b 1010` at `1000` and `1004` | `04 00 00 14`, `03 00 00 14` | Same with local labels |

The x86 displacement starts at the end of the encoded instruction. The
AArch64 displacement starts at the instruction address. An unresolved name,
out-of-range short jump, unaligned AArch64 target, or AArch64 displacement
of +128 MiB returns an error. The pure probe cannot write a document.

On the generated HIEW 8.63 Windows fixture, F2 and Enter seeded the Code
edit prompt from the current instruction. F4 toggled CommandSelect.
Replacing a five-byte MOV with one NOP advanced one byte and preserved the
other four. Esc closed the prompt, and a second Esc discarded the pending
edit. The original program and captures remain private.

## Alternatives and provenance

**Direct asm-rs text.** Version 0.2.0 rejected `10t` and interpreted the
unsuffixed x86-32 immediate as decimal. Direct numeric `jmp 1010` emitted
the same relative encoding for PCs `1000` and `1001`. After normalization
and explicit absolute-label binding, it matched expected MOV and branch
bytes. The AArch64 adapter also rejected the out-of-range destination.
The tested interface returns one encoding. The x86 path needs explicit
candidate enumeration.

The [asm-rs 0.2.0 package](https://crates.io/crates/asm-rs/0.2.0) identifies
[8dfe5d86db18d3d861b53e6db1be0816a177715a](https://github.com/hupe1980/asm-rs/tree/8dfe5d86db18d3d861b53e6db1be0816a177715a/crates/asm-rs)
as its source revision. The crate declares MIT OR Apache-2.0. We inspected
the pinned [MIT](https://github.com/hupe1980/asm-rs/blob/8dfe5d86db18d3d861b53e6db1be0816a177715a/LICENSE-MIT)
and [Apache-2.0](https://github.com/hupe1980/asm-rs/blob/8dfe5d86db18d3d861b53e6db1be0816a177715a/LICENSE-APACHE)
texts. The probe enables x86, x86-64, AVX, and AArch64 with defaults off.
Both Rust candidates use no native engine underneath. The pinned iced-x86
source and MIT notice are in [dependencies.md](../dependencies.md).

**LLVM MC oracle.** The tested version 23.1.6 encoded the fixtures through
relocatable objects and rejected `10t`. It assembled an unresolved branch
successfully with a relocation. An editor adapter would need a checked
relocation or link stage before returning patch bytes. On arm64 macOS, the
tested `llvm-mc` file measured 107,272 bytes and loaded the 169,354,800-byte
`libLLVM.23.1.dylib`. These sizes describe one installed toolchain.
No linked browser MC route was demonstrated. Distribution would require
the applicable Apache-2.0 with LLVM exceptions engine notices.
[LLVM MC](https://llvm.org/docs/CommandGuide/llvm-mc.html)
[LLVM licensing](https://llvm.org/docs/DeveloperPolicy.html#copyright-license-and-patents)

**Keystone.** The underlying engine uses GPLv2 with a FOSS exception.
The proposed path leaves Keystone out.
[Keystone FOSS exception](https://github.com/keystone-engine/keystone/blob/master/EXCEPTIONS-CLIENT)

## Build and runtime routes

The [target manifest](../../build/targets.json) drives the Rust workflow and
its probe steps. The compatibility ledger keeps assembly Planned.

| Required targets | Evidence route and remaining qualification |
| --- | --- |
| macOS arm64 and x64 | Native compiler and corpus on both architectures. Universal release packaging belongs to [#94](https://github.com/stokaro/hiewbie/issues/94). |
| Windows x64 and ARM64 MSVC | Native compiler and corpus on both architectures. ConPTY and packaging belong to [#39](https://github.com/stokaro/hiewbie/issues/39) and [#94](https://github.com/stokaro/hiewbie/issues/94). |
| Linux x64 and arm64 | Native compiler and corpus on both architectures. Archive validation belongs to [#94](https://github.com/stokaro/hiewbie/issues/94). |
| Linux RISC-V | Pinned std and compile-check. GNU linker, matching sysroot and runtime belong to [#92](https://github.com/stokaro/hiewbie/issues/92). |
| FreeBSD x64 | Pinned std and compile-check. Matched link and runtime belong to [#93](https://github.com/stokaro/hiewbie/issues/93). |
| FreeBSD ARM64 | Rust 1.90.0 lacks distributed std. [#91](https://github.com/stokaro/hiewbie/issues/91) owns a matching compiler/std and runtime. |
| WebAssembly | Link the standalone Rust library and instantiate with no host imports. The corpus exercises both selected paths. Browser integration and packaging belong to [#95](https://github.com/stokaro/hiewbie/issues/95) and [#96](https://github.com/stokaro/hiewbie/issues/96). |

On Rust 1.90.0, the release WASM probe with both candidates linked measured
846,823 bytes. Node ran the fixed corpus and checked the external-label
branches with an empty import object. Package size for the final editor
depends on its feature set and release profile.
