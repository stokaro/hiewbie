# ADR 0004: select bounded executable readers and portable instruction decoders

Status: accepted for the first M2 analysis slice, 2026-11-28. Textual assembly
remains an open decision gate. This decision implements part of the analysis
contract; it does not mark M2, the complete HIEW baseline, or browser execution
as supported.

## Constraints

The complete [format and instruction contract](../compatibility.md) and
[host matrix](../build-and-release.md) remain required. A backend's advertised
instruction coverage is not equivalent to a hiewbie capability. Decoding,
textual assembly, address mapping, branch/reference analysis, and browser
availability have separate acceptance criteria.

The editor inspects arbitrary bytes, including incomplete files and pending
edits. Analysis must use bounded reads, preserve raw editing after errors, and
keep file offsets distinct from addresses in the inspected program. Neither the
host architecture nor the host byte order selects the program interpretation.

## Decision

Keep executable metadata, address mapping, decoder adapters, and analysis
commands in `hiewbie-core`, shared by terminal and headless callers. The format
readers are independently written Rust code with no additional parser dependency.
Instruction decoding uses three exact-pinned Rust packages:

| Package | Version | Selected role | License |
| --- | --- | --- | --- |
| `iced-x86` | `=1.22.0` | x86 16/32/64-bit decode, Intel formatting, and direct near-branch metadata | MIT |
| `yaxpeax-arm` | `=0.5.0` | ARM, Thumb, and AArch64 decode and formatting | 0BSD |
| `yaxpeax-arch` | `=0.3.3` | Decoder traits, byte reader, and instruction lengths | 0BSD |

Defaults are disabled for all three packages. iced-x86 enables `std`, `decoder`,
`intel`, and `instr_info`; yaxpeax-arm enables `std`, `fmt`, and
`non-exhaustive-enums`; yaxpeax-arch enables `std`. No assembler API is exposed.
Exact source commits, archive provenance, transitive packages, and license-file
distinctions are in the [dependency inventory](../dependencies.md). No engine
source is copied, patched, or vendored.

### Executable readers and address domains

Focused readers establish the minimum metadata needed for headers, mapped
addresses, entry navigation, and code interpretation:

This table describes the eager `formats` layer selected by this decision.
[ADR 0005](0005-symbol-navigation.md) subsequently adds separate, lazy
symbol/import/export readers; opening an image still performs only the header
and mapping work below.

| Container | Implemented scope | Remaining limits |
| --- | --- | --- |
| PE32 / PE32+ | DOS and PE identification, COFF and optional headers, section descriptors, preferred image base, entry RVA, and file-backed mappings | Data-directory bounds are checked, but imports, exports, relocations, and other directory contents are not interpreted. DOS MZ, NE, LE, and LX remain outside this reader's supported scope |
| ELF32 / ELF64 | Little- and big-endian headers, program headers, `PT_LOAD` mappings, and entry address | Section-table extents are checked; section contents, symbols, relocations, and extended numbering are not interpreted |
| Thin Mach-O32 / Mach-O64 | Little- and big-endian headers, bounded load commands, segments, and `LC_MAIN` entry mapping | Internal sections, symbols, imports, and relocations are not interpreted. `LC_UNIXTHREAD` entry extraction and high-VM core segments remain unsupported |
| Universal Mach-O | 32- and 64-bit fat descriptors, slice validation, selection, and translation to global file offsets | Only a selected thin Mach-O member is parsed. Nested fat containers and non-Mach-O members are unsupported |

Universal files default to slice index zero, in file-table order. The selected
slice and available descriptors remain visible, and callers can select another
index. This result is deterministic across hosts; it does not prefer the machine
running hiewbie.

`FileOffset`, `VirtualAddress`, `Rva`, and `SliceOffset` are separate types.
Translation uses checked arithmetic and explicit errors for unmapped, zero-fill,
ambiguous, or out-of-range addresses. Overlays and raw alignment padding do not
acquire invented virtual addresses. Metadata retains the backing byte ranges of
displayed fields so navigation lands on actual file bytes. The generated fixture
suite covers both widths, both relevant data byte orders, malformed extents,
overlapping mappings, fat selection, truncations, and a sparse terabyte source.

Metadata is derived from the current logical document, including pending edits,
and guarded by its generation. Editing, undoing, or discarding bytes invalidates
stale interpretation. The parsed machine is separate from an explicit decoder
override. PE Thumb/ARMNT machine values infer Thumb directly; generic ARM metadata
can use an odd entry address to indicate Thumb. Per-region ARM mapping symbols
and exhaustive mixed ARM/Thumb analysis remain future work.

### Decoder contract and progress

`decode_one` takes bytes, a typed virtual address, an explicit `DecodeMode`, and
instruction endianness. Supported mode names are `x86-16`, `x86-32`, `x86-64`,
`arm`, `thumb`, and `aarch64`. Its result contains the consumed raw bytes,
instruction length, bounded text, status, and an optional direct target with its
destination mode.

An empty input returns no row. Every nonempty input makes positive progress,
including a truncated tail or an encoding the backend rejects. Fallback consumes
one byte for x86, up to two for Thumb, and up to four for ARM/AArch64, limited by
the available input. Status distinguishes `Decoded`, `Unknown`, `Truncated`, and
`Unsupported`. `Unknown` means unknown or rejected by the selected backend; it
does not prove that the bytes are invalid for every CPU revision.

The adapter currently accepts little-endian instruction interpretation only.
An explicit big-endian interpretation produces `Unsupported` rows with the raw
bytes preserved. Successful parsing of big-endian ELF or Mach-O metadata does not
establish support for that image's instruction order. ARM BE8/BE32 and AArch64
instruction order need separate handling from container data endianness.

Decoding may start at an unaligned byte address for deliberate resynchronization
while inspecting a file. A recognized row does not assert that a CPU could execute
that instruction at the supplied address. The adapter also does not maintain
Thumb IT-block condition state between rows.

### Direct targets and navigation

x86 direct near targets retain the decoder's architectural 16/32/64-bit address
semantics. Far and register-indirect targets are not resolved. The ARM/Thumb
adapter independently extracts direct-branch fields from the original bytes,
applies the ARM PC+8 or Thumb PC+4 bias, handles Thumb BLX PC alignment, and records
mode exchange. This avoids relying on inconsistent relative-operand biases in
the upstream formatter. The formulas were reviewed against LLVM's ARM
disassembler; no LLVM implementation was copied.

ARM/Thumb architectural PC arithmetic wraps modulo 2^32 when the supplied
starting address fits that address space. An out-of-domain starting address does
not produce an invented target. AArch64 target arithmetic uses checked signed
offsets in the `u64` address domain. File offsets and mappings always use checked
arithmetic; architectural PC wrap is not file-position wrap.

Code rows stop at a mapping discontinuity. Branch following redecodes the current
logical bytes, resolves the target through the selected mapping, and then changes
location. Unmapped, ambiguous, and zero-fill targets cannot silently become file
positions. Return history restores both the prior file position and its
interpretation. These operations are shared application commands, independent
of terminal layout.

### Resource limits

| Resource | Limit |
| --- | --- |
| Bytes read during one format inspection | 1 MiB |
| Entries in a format table | 4,096 |
| Recorded header fields | 32,768 |
| Universal Mach-O slices | 64 |
| Code rows requested from shared analysis | 256 |
| Input passed to a decoder for one row | 15 bytes maximum; 4 for ARM/Thumb/AArch64 |
| Formatted instruction text | 256 UTF-8 bytes |
| Return history and instruction-step history | 256 locations each |
| Headless disassembly output | 32 instructions |
| Headless header output | 64 items per category |

These are application limits, not assertions about the largest valid executable
or every backend's complete internal implementation. Exceeding a reader limit
produces an explicit analysis error while raw file access remains available.

## Evidence behind the selection

The version-selection experiments ran with Rust 1.90.0 on Linux x86_64. Native
fixtures exercised x86 widths, AVX decoding, relative branches, the 4 GiB address
boundary, ARM/Thumb lengths and mode exchange, and AArch64 scalar/direct branches.
The same exact dependency graph compiled for `wasm32-unknown-unknown`. This is
compilation evidence; there is no browser frontend or browser execution result.
The complete native target and runtime gates remain those in
[build and release](../build-and-release.md).

iced-x86 1.22.0 includes an upstream Rust decoder fix for integer underflow when
crossing a 4 GiB boundary. A dedicated adapter regression keeps that behavior
covered; the [release notes](https://github.com/icedland/iced/releases/tag/v1.22.0)
record the upstream fix.

The initial yaxpeax ARM probe found a deterministic panic on word `0x4120ffd2`
(bytes `d2 ff 20 41`): permissive decoding constructed an out-of-range paired
register. Source inspection showed that the supported
`.allow_nonconforming(false)` setting validates those operands before register
construction. Production ARM and Thumb adapters require that strict setting.
Repeating the same 65,536 generated words and their short input tails passed;
the exact failing word is retained as a regression. No panic recovery or local
upstream patch is used as an input-validation mechanism.

Reproducible coverage is committed in
[format tests](../../crates/hiewbie-core/src/formats/tests.rs),
[instruction tests](../../crates/hiewbie-core/src/instructions/tests.rs), and
[shared analysis tests](../../crates/hiewbie-core/src/analysis_tests.rs).
Fixtures are generated or independently written; no proprietary executable
samples are required. Bounded corpus success is useful regression evidence,
not an exhaustive proof of decoder safety or ISA coverage.

## Capability gaps and alternatives

yaxpeax-arm's ARM revision filters are not enforced by the selected release.
Common ARMv6 encodings can be decoded through its ARMv7 implementation, but the
adapter does not promise strict ARMv6-only validation. ARMv7 NEON and AArch64
SVE/SVE2 remain upstream gaps. Thumb IT state, segmented x86 address resolution,
indirect targets, complete cross references, and the unimplemented executable
structures remain explicit work in the compatibility ledger. This selection
does not remove them from the baseline.

Capstone was considered using the Rust wrapper at commit
`f4ae71a22d2a159238e4cafc7762a32be9b17a93`. Its `capstone-sys` build script compiles
the C engine through `cc::Build`; the wrapper's MIT declaration does not replace
the engine's BSD and additional source notices. That route needs separate C
cross-compiler, sysroot, and WebAssembly packaging experiments. Its `arch_wasm`
feature names an inspected instruction architecture, not browser-host support.
It is not selected in this slice.

Goblin and other general executable parsers remain possible future choices if
broader structure support justifies a change. The initial mapping surface uses
focused readers with explicit budgets instead. The `object` crate's documented
trusted-input scope still requires an arbitrary-input assessment before adoption.

### Textual assembly remains a decision gate

No textual assembly backend is accepted. iced-x86's encoder/CodeAssembler API
does not itself parse user-entered assembly. A future choice must demonstrate
syntax handling, explicit current address/mode/endianness, preview, instruction
length rejection, atomic document transactions, licensing, and packaging for the
full target matrix. LLVM MC integration or a separate parser and encoder may be
evaluated against those requirements. Keystone's GPLv2/FOSS-exception terms must
not be represented as MIT; no Keystone dependency is introduced.

## Primary references

Format layouts and address rules were checked against:

- Microsoft's [PE format specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format)
  and [image machine constants](https://learn.microsoft.com/en-us/windows/win32/sysinfo/image-file-machine-constants).
- System V gABI 4.2 [ELF headers](https://gabi.xinuos.com/v42/elf/02-eheader.html)
  and [program headers](https://gabi.xinuos.com/v42/elf/07-pheader.html).
- Apple XNU at `43a90889846e00bfb5cf1d255cdc0a701a1e05a4`:
  [loader.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/loader.h)
  and [fat.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/fat.h).
- LLVM at `24a30daaa559829ad079f2ff7f73eb4e18095f88`:
  [MachO.h](https://github.com/llvm/llvm-project/blob/24a30daaa559829ad079f2ff7f73eb4e18095f88/llvm/include/llvm/BinaryFormat/MachO.h)
  for the 64-bit fat descriptor layout.

Decoder selection and limits were checked against:

- iced-x86's [1.22.0 release](https://github.com/icedland/iced/releases/tag/v1.22.0)
  and [pinned manifest](https://github.com/icedland/iced/blob/cae65b1e90451f22c65f580d4acc752c8a486754/src/rust/iced-x86/Cargo.toml).
- yaxpeax-arm's pinned [README](https://github.com/iximeow/yaxpeax-arm/blob/a2723ae2043777472dd373feef4b9c40d52569bb/README.md)
  and [ARM implementation](https://github.com/iximeow/yaxpeax-arm/blob/a2723ae2043777472dd373feef4b9c40d52569bb/src/armv7.rs).
- LLVM 21.1.0's [ARM disassembler](https://github.com/llvm/llvm-project/blob/llvmorg-21.1.0/llvm/lib/Target/ARM/Disassembler/ARMDisassembler.cpp)
  for branch displacement, PC bias, and Thumb BLX alignment.
- The pinned Capstone Rust wrapper's [build script](https://github.com/capstone-rust/capstone-rs/blob/f4ae71a22d2a159238e4cafc7762a32be9b17a93/capstone-sys/build.rs)
  and [engine license](https://github.com/capstone-rust/capstone-rs/blob/f4ae71a22d2a159238e4cafc7762a32be9b17a93/capstone-sys/capstone/LICENSE.TXT).

Earlier candidate constraints and the reuse policy remain in
[references and reuse](../references-and-reuse.md).
