# Compiled capabilities and the finite extension roadmap

This is the contract for [#81](https://github.com/stokaro/hiewbie/issues/81).
The native report and portable query describe what the current build implements.
The roadmap records evaluated possibilities. A candidate's upstream architecture
list does not enable a hiewbie operation.

## Query the current build

```sh
hiewbie --capabilities
hiewbie --capabilities --json
```

Neither command opens a file or initializes a terminal. Files and inspection
options are rejected instead of being silently ignored. `--json` is scoped to
`--capabilities`. The JSON comes directly from
[`hiewbie_core::capabilities::current()`](../crates/hiewbie-core/src/capabilities.rs);
native CLI and headless callers share one catalog.

The same ordinary Rust API is available when the core is compiled for WASM.
There is no browser frontend, JavaScript binding or browser runtime claim here.
The future adapter in [#96](https://github.com/stokaro/hiewbie/issues/96) must
query its loaded core, then add only its own implemented file/terminal
capabilities. It must not copy a native feature list.

### Schema 1

| Field | Meaning |
| --- | --- |
| `schema_version` | Integer contract version; currently 1 |
| `version` | Compiled hiewbie package version |
| `scope` | Explicit distinction between implementation availability and target runtime evidence |
| `capabilities[].id` | Stable, case-sensitive identifier for one operation |
| `category` | `raw`, `decode`, `assembly`, `references`, `symbols`, `format`, `extensions`, or `devices` |
| `status` | `available`, `limited`, or `unavailable` |
| `backend` | Loaded implementation's name and version, or null |
| `modes`, `byte_orders` | Implemented interpretation scope; empty for unavailable operations |
| `limitations` | Concrete incomplete coverage or the reason the operation is unavailable |
| `evidence` | Repository test/contract paths or tracked implementation issue links |

`available` means the named operation is implemented. `limited` means its
listed subset is usable, with the recorded gaps. Neither means every instruction,
format variant, operating system or release artifact is verified.
`unavailable` means the current product has no loaded implementation for that
operation. Research-only assemblers remain unavailable.

Unknown IDs are distinct from known unavailable IDs. `report.get(id)` returns
`None` only for an absent ID. `report.require(id)` returns
`LookupError::Absent` or `LookupError::Unavailable` as appropriate;
`Status::is_usable()` accepts Available and Limited. A caller must also honor
the selected mode, byte order and limitations; a usable little-endian decoder
does not make a big-endian request valid.

New IDs and evidence details may be added within schema 1. Removing/renaming an
ID or changing its meaning requires a schema migration. Consumers must tolerate
additional IDs and fields while rejecting unknown required schema versions.

### Independent production operations

| Operation | Compiled implementation | Present scope | Separate missing work |
| --- | --- | --- | --- |
| Raw read and overwrite | hiewbie-core | Bounded logical bytes, pending overwrite, undo/redo | Insert/delete, recovery, device persistence |
| x86-16/32/64 decoding | iced-x86 1.22.0 | Little-endian bounded decoding | Exhaustive versioned extension qualification |
| ARM and Thumb decoding | yaxpeax-arm 0.5.0 | Little-endian bounded decoding and selected direct branches | Revision filtering, complete IT-state handling, instruction byte-order variants |
| AArch64 decoding | yaxpeax-arm 0.5.0 and hiewbie-core | Little-endian A64 instructions in either container data order, checked PC-relative operands, and versioned scalar and extension families | Wider scalable-vector, matrix and other extension coverage |
| x86 textual assembly | iced-x86 1.22.0 and hiewbie-core | Little-endian x86-16/32/64 core API and [native Decode assembler](commands.md#decode-assembler) | Full HIEW grammar |
| AArch64 textual assembly | hiewbie-core | [One checked little-endian A64 word](aarch64-text-assembly.md) with explicit scalar and extension forms and native patch review | Wider ISA coverage |
| x86 reference search | iced-x86 and hiewbie-core | Bounded near branches, selected mapped memory and literal operands | General references, loader resolution and control-flow analysis |
| ARM/Thumb/AArch64 reference search | None | Unavailable | Direct branch following does not establish reference search |
| PE/ELF structures | hiewbie-core | Bounded headers and load mappings | Remaining per-format directories and relocations |
| Mach-O structures | hiewbie-core | Load-command records/status, sections, old thread entries, high-VM mappings, section relocations and selected fat slices | Dyld binding/chained fixups, loader-specific relocation targets, unknown optional types |
| TE/TE64 structures | hiewbie-core | Common header, machine width, stripped-header mapping, section/entry and relocation/debug records | Unknown types remain raw; no PE reconstruction, PDB loading or relocation application |
| PE/ELF/Mach-O symbols | hiewbie-core | Ordinary PE imports/exports; section-backed ELF symbols; Mach-O nlist | Delay/bound imports, sectionless dynamic ELF, dyld metadata |
| Portable extensions | None | A descriptive registration contract exists | Loader, executable ABI, permissions, isolation and lifecycle in #82 |
| Original Windows HEM binaries | None | Unavailable independently of the portable API | Compatibility investigation in #84 |
| Raw devices | None | Unavailable | Shared contract #85 and OS adapters #86, #87, #88 and #90 |

The authoritative per-build entries are the report; this table explains their
separation. Backend names/versions are checked against the actual decoder API.
Generated decoder fixtures establish usable little-endian cases and explicit
unsupported big-endian results. An unknown machine can still retain parsed
headers and raw bytes. Unavailable optional decoding does not disable raw
overwrite, undo or the existing image reader.

`format.macho.executable`, `format.macho.object` and `format.macho.core` distinguish
entry navigation, link-time section/relocation inspection and core segment/state
inspection. They remain Limited; recognizing `MH_CORE` does not establish process
restoration, and object metadata does not establish linking. `format.te` likewise
describes the implemented firmware metadata scope independently of an instruction
decoder. See [format records](format-records.md) for exact source/target semantics,
supported entry layouts and the remaining format-specific limits.

The [target manifest](../build/targets.json) and
[build contract](build-and-release.md) record host triples and evidence separately.
A RISC-V Linux host does not provide inspected RISC-V decoding. A WASM core build
does not provide WebAssembly module inspection. The complete required host
matrix remains unchanged.

## Registration contract for future extensions

`Registry` is a bounded collection of host-supplied descriptors, not an extension
loader. A host first validates and loads an implementation, then registers only
the operations it can dispatch. A registry is independent of `current()`;
registration never alters the built-in catalog or an open document.

The contract is deliberately small:

1. One stable ID names one operation. Decode, assembly, references, symbols and
   format support are registered independently. New extension-owned IDs should
   include a provider prefix after the category.
2. A maximum of 128 entries bounds storage. IDs are at most 96 ASCII bytes;
   metadata fields are at most 512 bytes and reject control characters.
   Each detail list has at most 16 items. Descriptors borrow backend-owned
   metadata rather than accepting executable bytes or filesystem paths.
3. Duplicate IDs are rejected. Failed registration preserves every prior entry.
   Registry output is ordered by ID, independent of loading order.
4. Usable entries require a backend name/version and evidence. Limited and
   unavailable entries require an explicit limitation. An unavailable entry
   cannot advertise implemented modes or byte orders.
5. Unknown IDs and known unavailable implementations produce different lookup
   errors. Neither authorizes an implicit substitute, network service or host
   architecture fallback.
6. A descriptor is serializable for reporting, but it is not deserializable as
   a trusted registration request. Text in a third-party manifest is not proof
   of a loaded implementation or a grant of file/device access.

[#82](https://github.com/stokaro/hiewbie/issues/82) still owns the portable ABI,
version negotiation, command dispatch, resource enforcement and security
boundaries. This contract neither enables F11 nor loads HEM DLLs.
[#83](https://github.com/stokaro/hiewbie/issues/83) and
[#84](https://github.com/stokaro/hiewbie/issues/84) remain separate.

## Evaluated extension decisions

Evaluation date: 2026-11-28. These are finite tracked extensions under ISA-005
through ISA-008 and FMT-010. **All rows below are deferred for production
implementation.** None is promoted to a mandatory delivery gate or counted as
usable by the compiled catalog.

The existing mandatory x86/ARM/AArch64 work and DOS MZ, NE, LE/LX, TE/TE64 and
NetWare stories remain unchanged. Closing this roadmap does not complete
[#66](https://github.com/stokaro/hiewbie/issues/66) through
[#70](https://github.com/stokaro/hiewbie/issues/70).

### Named ISA rows

| Inspected ISA and use case | Decoder evaluation | Text assembler evaluation | Mode/byte-order boundary to qualify | References / branch analysis | Symbols | Format-machine integration |
| --- | --- | --- | --- | --- | --- | --- |
| MIPS32: legacy router/embedded firmware | Capstone 5.0.6 exposes MIPS32, microMIPS and selected revision modes [C] | LLVM 20.1.8 has MIPS32 BE/LE target registrations [L-MIPS]; asm-rs 0.2.0 has no MIPS feature [A] | Separate classic MIPS32, revision and microMIPS; qualify BE/LE independently | No hiewbie MIPS reference model; delayed transfers need their own tests | MIPS ABI symbol interpretation is unqualified; generic format readers remain independent | ELF machine tag and ELF32 class need explicit mapping; currently retained as an unknown machine |
| MIPS64: older 64-bit appliances and Unix binaries | Capstone's separate MIPS64 mode is a candidate [C] | LLVM's MIPS64 BE/LE registrations are candidates [L-MIPS] | 64-bit registers/addresses must not inherit MIPS32 assumptions; BE/LE and revision are separate | No qualified MIPS64 references or delayed-transfer model | MIPS64 ABI symbol handling is unqualified | ELF64 and MIPS ABI flags require a separate fixture from MIPS32 |
| PowerPC32: older Mac and embedded firmware | Capstone PPC mode is a candidate, with separately named BookE/SPE options [C] | LLVM registers PPC32 and PPC32LE [L-PPC]; asm-rs has no PPC feature [A] | Qualify 32-bit BE first; LE, BookE and SPE are separate decisions | No PPC reference model; direct/conditional/link branches and register-indirect forms need distinct results | PPC symbol and relocation interpretation is unqualified | ELF32 and Mach-O PPC machine tags need explicit mapping; existing readers do not select a PPC decoder |
| PowerPC64: server and appliance binaries | Capstone exposes a separate 64-bit PPC mode [C] | LLVM registers PPC64 and PPC64LE [L-PPC] | BE/LE, instruction families and ABI version must be explicit | No qualified PPC64 references; function descriptors must not be treated automatically as code pointers | ELFv1/ELFv2 function-symbol interpretation needs separate ABI qualification | ELF64 and ELFv1/ELFv2 function-symbol targets require independent evidence |
| RISC-V32: embedded firmware | Capstone has RV32G plus an independent compressed-mode flag [C] | asm-rs has a RISC-V feature; LLVM registers a separate RISCV32 target [A, L-RV] | Start with a finite RV32I subset; C/F/D/V and privilege revisions need separate capability rows | No RISC-V reference model; AUIPC pairs and compressed instruction boundaries need independent work | RISC-V symbol relocations and ABI interpretation are unqualified | ELF class, ISA flags and machine mapping are not established by host RISC-V builds |
| RISC-V64: newer Linux/firmware binaries | Capstone has a separate RV64G mode [C] | asm-rs and LLVM are candidates, not selected production implementations [A, L-RV] | RV64I and RV32-invalid instructions must be tested separately; no unqualified extension list | No qualified RV64 references or relaxation analysis | No ABI-aware RISC-V64 symbol resolution is qualified | ELF64 mapping and relocation semantics require their own tests |
| ARM A32 textual assembly: deliberate improvement to existing inspection | Existing yaxpeax decoding stays unchanged | asm-rs `arm` feature or LLVM ARM target are candidates [A, L-ARM] | ARMv6/revision constraints, PC bias, conditional encodings and BE8/BE32 must be explicit | Existing direct-branch decode is independent of assembly qualification | Existing format-level symbols do not qualify assembler symbol resolution or relocations | Existing ARM image mappings do not qualify emitted instructions |
| Thumb textual assembly: compact firmware patching | Existing Thumb decoding stays unchanged | asm-rs ARM module or LLVM Thumb targets are candidates [A, L-ARM] | 16/32-bit selection, branch range, IT context and ARM/Thumb exchange require separate proof | No additional reference or linker support is implied by an assembler candidate | Thumb symbol mode bits remain distinct from aligned addresses; assembly symbol resolution is unqualified | A mode bit in a symbol must remain distinct from the aligned file address |

These are source-level candidate evaluations. Upstream mode names do not prove
hiewbie adapter semantics. No candidate from this table is accepted for delivery,
so this change introduces no new optional implementation child. Reconsideration
requires an implementation issue with a bounded ISA family list, licensing and
target plan before the status can become Planned for delivery.

### Smallest validating experiments before selection

Each experiment must use generated bytes and an independent known-answer oracle.
It must run in a standalone crate/tool, leave the production manifest unchanged,
and record the exact backend revision. Syntax/encoding failures must return no
patch bytes. A positive single instruction is not a full ISA claim.

| Row | Smallest decision experiment | Failure/boundary case required for the decision |
| --- | --- | --- |
| MIPS32 | Decode one immediate arithmetic instruction and a direct branch in both byte orders; assemble the equivalent bounded text at two PCs using LLVM MC | Truncated word; unsupported microMIPS/revision selection; branch delay semantics and out-of-range destination |
| MIPS64 | Decode/assemble one 64-bit-only arithmetic instruction plus a branch near a 32-bit address boundary | Reject the 64-bit instruction in a 32-bit profile; preserve 64-bit target arithmetic without wrapping file offsets |
| PowerPC32 | Decode/assemble `addi`, a conditional branch and `blr` in the selected 32-bit profile | Truncation; conditional/link metadata; unresolved indirect target; no fabricated host-endian default |
| PowerPC64 | Add a 64-bit load and branch in BE and LE at two PCs | 32-bit profile rejection where defined; ELF function-descriptor target must remain distinguishable from code |
| RISC-V32 | Decode/assemble integer immediate, direct branch and compressed NOP with C enabled/disabled | Reject a 64-bit-only load; distinguish 2/4-byte and truncated lengths; unresolved relocations cannot become raw patch bytes |
| RISC-V64 | Decode/assemble a 64-bit load, JAL and AUIPC pair at two PCs | RV32 mismatch, range/alignment failure and unsupported extension; no claim that AUIPC alone resolves a destination |
| ARM A32 text | Normalize one HIEW-style immediate and branch through the in-tree grammar; compare asm-rs and LLVM results | Revision mismatch, actual-PC branch bias, unresolved symbol and out-of-range branch; no document mutation |
| Thumb text | Compare one 16-bit and one 32-bit instruction plus a branch through the same bounded grammar | Explicit width/range, alignment, IT-state dependence and mode exchange; reject unsupported byte-order policy |

Before selecting a native C/C++ backend, link and run the smallest relevant
probe on native macOS/Windows/Linux x64 and ARM64, check the remaining required
cross targets, and demonstrate a WASM route or record a concrete alternative.
Cross compilation is not runtime evidence. A future browser candidate needs
browser execution evidence, not just a successful native probe.

### Additional finite candidates

| Candidate and use case | Evaluation and explicit decision | Smallest future experiment |
| --- | --- | --- |
| eBPF bytecode inspection for kernel-program analysis | Deferred. Capstone exposes BPF with separate classic/extended modes [C]; no eBPF engine or kernel execution is introduced | Generated 8-byte instruction, double-width immediate, branch and truncation; verify bounds and distinguish classic BPF. Textual assembly is a separate deferred operation |
| WebAssembly module inspection for browser artifacts | Deferred. A bounded module/container reader would be separate from a WASM host build and from an opcode backend | Generated module with one function, bounded LEB128 sizes, unknown section and malformed/truncated section; preserve exact byte offsets |
| ar archive-member inspection for static libraries | Deferred. Start with ordinary archive members; thin-archive external paths require a separate policy | Generated two-member archive, odd-byte padding, long-name table and malformed size; never follow external member paths implicitly |

For the two format candidates, the prospective implementation is an original
MIT-licensed bounded reader in portable Rust. No third-party parser source,
crate or license is selected for reuse. Each reader would need the full native
host matrix and WASM compilation/runtime qualification before selection for
delivery; neither requires executing the inspected module or archive member.
The primary format references are the [WebAssembly binary specification][WASM],
FreeBSD's [BSD and SVR4/GNU archive-format description][AR], and GNU binutils'
[thin-archive behavior][AR-THIN]. The ar variants must be identified explicitly;
the format description does not establish a single universal archive standard.

This is the full evaluated optional list for this story. Other architectures
and formats require a new finite decision; the extension API is not a promise
to support every upstream architecture.

## Candidate license and target constraints

| Candidate | Exact source reviewed | License and integration constraint | Decision |
| --- | --- | --- | --- |
| Capstone 5.0.6 | [`accf4df62f1fba6f92cae692985d27063552601c`][C] | Root license has BSD 3-clause redistribution conditions [C-LICENSE]. Native C code and architecture feature switches require explicit builds; the supplied MSVC instructions describe Win32/Win64 and do not establish ARM64 or WASM coverage [C-WIN]. Inspect selected source-file notices and wrapper terms before any redistribution | Source-reviewed decoder candidate only; not linked or distributed |
| LLVM 20.1.8 | [`87f0227cb60147a26a1eeb4fb06e3b505e9c7261`][L-LICENSE] | Apache-2.0 with LLVM exceptions; identified third-party portions have separate terms. Target registration is not proof of a bounded HIEW text adapter. Object/relocation handling, native C++ build footprint and a browser route remain separate requirements | Independent assembler/oracle candidate; no production dependency added |
| asm-rs 0.2.0 | [`8dfe5d86db18d3d861b53e6db1be0816a177715a`][A] | MIT OR Apache-2.0 as reviewed in ADR 0006; pure Rust, optional ARM/RISC-V features, no native engine. Its parser/macros/directives must be bounded behind the chosen HIEW grammar. ARM/Thumb/RISC-V are not enabled by the existing AArch64 experiment | Optional modes deferred; existing x86/AArch64 research decision is unchanged |

The asm-rs repository now resolves as `hupe1980/asm-kit`; the reviewed source
commit and crate version remain explicit. No candidate source or binary is
copied by this capability work. Existing dependency notices remain applicable
to the production decoders and shared serialization crates.

The already accepted x86 and AArch64 research paths have runnable encodings,
PC-relative checks and a WASM probe in
[ADR 0006](adr/0006-text-assembler-selection.md) and
[experiments/assembler](../experiments/assembler/README.md).
Their production implementation issues are
[#76](https://github.com/stokaro/hiewbie/issues/76),
[#77](https://github.com/stokaro/hiewbie/issues/77) and
[#78](https://github.com/stokaro/hiewbie/issues/78).
The report exposes the compiled x86 and AArch64 assembly subsets. The native
[Decode assembler](commands.md#decode-assembler) uses their shared patch API.

## Automated evidence

- [Core capability tests](../crates/hiewbie-core/src/capabilities_tests.rs):
  unique/sorted bounded registration, absent/unavailable distinctions,
  actual decoder/backend/order checks, invalid descriptor/duplicate/capacity
  failures and raw editing of an image with an unsupported machine.
- [Machine-readable cases](../tests/fixtures/capabilities.json) and
  [CLI integration tests](../crates/hiewbie/tests/capabilities_cli.rs):
  schema 1, independent operation categories, unavailable research paths,
  text/JSON consistency, option errors and no terminal escapes.
- [Native writer test](../crates/hiewbie/src/capability_output.rs):
  I/O failure propagation including JSON BrokenPipe.
- Target build/runtime evidence remains in the target manifest and identified
  CI run. No test here promotes an optional ISA, an unimplemented browser, or a
  research assembler into production support.

[C]: https://github.com/capstone-engine/capstone/blob/accf4df62f1fba6f92cae692985d27063552601c/include/capstone/capstone.h
[C-LICENSE]: https://github.com/capstone-engine/capstone/blob/accf4df62f1fba6f92cae692985d27063552601c/LICENSE.TXT
[C-WIN]: https://github.com/capstone-engine/capstone/blob/accf4df62f1fba6f92cae692985d27063552601c/COMPILE_MSVC.TXT
[A]: https://github.com/hupe1980/asm-kit/blob/8dfe5d86db18d3d861b53e6db1be0816a177715a/crates/asm-rs/Cargo.toml
[L-LICENSE]: https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/LICENSE.TXT
[L-MIPS]: https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/lib/Target/Mips/TargetInfo/MipsTargetInfo.cpp
[L-PPC]: https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/lib/Target/PowerPC/TargetInfo/PowerPCTargetInfo.cpp
[L-RV]: https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/lib/Target/RISCV/TargetInfo/RISCVTargetInfo.cpp
[L-ARM]: https://github.com/llvm/llvm-project/blob/87f0227cb60147a26a1eeb4fb06e3b505e9c7261/llvm/lib/Target/ARM/TargetInfo/ARMTargetInfo.cpp
[WASM]: https://webassembly.github.io/spec/core/binary/index.html
[AR]: https://github.com/freebsd/freebsd-src/blob/fa9896e082a1046ff4fbc75fcba4d18d1f2efc19/share/man/man5/ar.5
[AR-THIN]: https://sourceware.org/binutils/docs/binutils/ar.html
