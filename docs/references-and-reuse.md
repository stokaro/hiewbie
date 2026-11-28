# References and source reuse

Baseline research snapshot: 2026-11-27; analysis, symbol, and structure-record
readers updated 2026-11-28.
These sources describe upstream behavior or inform
implementation choices; they are not evidence that hiewbie already implements it.

## HIEW behavior references

- [Supplied Hiew 8.63 distribution audit](reference/hiew-8.63.md): versioned
  author-documentation evidence, source inventory, known implementation differences,
  25 reference scenarios, and a later Windows observation of selected workflows.
  The original distribution and raw captures remain outside the repository.
- [Official HIEW site](https://www.hiew.ru/): console HIEW VIII is the primary
  product reference. Hiewix is a separate WinAPI presentation.
- [Hiew Documentation Project](https://taviso.github.io/hiewdocs/index.htm):
  community documentation linked from the official site, useful for contextual
  command and key behavior. Verify ambiguities against a named reference version.
- [Official HEM documentation](https://www.hiew.ru/hem.html): extension interface
  and Windows DLL model; not a promise of portable binary ABI compatibility.
- [7ingsong/hiew](https://github.com/7ingsong/hiew): a separate MIT-licensed Python
  implementation to examine as a secondary behavior and implementation reference.
  It is not the authoritative definition of original HIEW behavior.

Use observed behavior and independently written specifications. Do not copy the
proprietary application, assets, or manual text into this repository. Fixtures
must be generated or redistributable, with documented provenance.

## ArtCraft references

The relevant reference is the [ArtCraft application suite](https://getartcraft.com/apps)
under the `storytold` organization. GridCraft supplies a concrete inspected
example of the portable Rust workspace and release coverage.

| Reference | Inspected revision | Purpose |
| --- | --- | --- |
| [storytold/gridcraft](https://github.com/storytold/gridcraft/tree/97f2754c1ddf4caddff865015563d7f342b34c52) | `97f2754c1ddf4caddff865015563d7f342b34c52` | App workspace, local packaging, license, release caller |
| [storytold/craft-common-actions](https://github.com/storytold/craft-common-actions/tree/3fe7aa009c15e57932497b5e6d00bf20f781e5ec) | `3fe7aa009c15e57932497b5e6d00bf20f781e5ec` | Observed platform matrix and build constraints |
| [GridCraft v0.19.1](https://github.com/storytold/gridcraft/releases/tag/v0.19.1) | Published 2026-11-12 | Evidence of actual release artifacts |

GridCraft's [workspace manifest](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/Cargo.toml)
declares `MIT OR Apache-2.0`. Its [MIT license](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/LICENSE-MIT)
provides an MIT reuse option with the upstream copyright and permission notice.
Inspect file-specific notices before copying or substantially adapting a file.

The inspected shared `craft-common-actions` repository had no root license file
and its GitHub license metadata was `null`. Its workflow source is used here as
evidence of platform coverage, not as code authorized for copying into this MIT
project. Write independent workflows or use clearly licensed app-local material.
Re-evaluate if upstream later adds a license.

Useful app-local examples:

- [macOS packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/macos/package.sh), including universal CLI distribution.
- [Linux packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/linux/package.sh).
- [Windows packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/windows/package.ps1).
- [FreeBSD packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/freebsd/package.sh).
- [Web packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/web/package.sh).

The shared ArtCraft workflow also includes suite-specific services, assets,
signing, and paid-runner assumptions. Platform coverage is the requirement;
those operational dependencies are not hiewbie requirements.

## Dependency research references

Dependencies must be evaluated at a pinned revision before inclusion. A Rust
wrapper's license does not replace the native engine's license. The selected
portable decoders and bounded format readers are documented in
[ADR 0004](adr/0004-analysis-backends.md), including primary PE/ELF/Mach-O
specifications, exact source commits, feature choices, and known limitations.
The independently written, on-demand symbol readers and their separate resource
budgets are documented in [ADR 0005](adr/0005-symbol-navigation.md). They add no
parser dependency or bundled upstream implementation.
The [capability contract](capabilities.md) records a finite evaluation of optional
ISAs and formats with exact candidate versions, license constraints and deferred
experiments. It does not promote an upstream architecture list into product
support. The [fuzzing guide](fuzzing.md) records the separate development engine,
generated corpus provenance and bounded reproduction commands.

- [Ratatui](https://github.com/ratatui/ratatui) and [Crossterm](https://github.com/crossterm-rs/crossterm): selected terminal components.
- [iced-x86](https://github.com/icedland/iced) and its [CodeAssembler API](https://docs.rs/iced-x86/1.22.0/iced_x86/code_asm/index.html): selected x86 decoder; the production assembler API is disabled and does not itself parse user-entered assembly text. The independently locked probe in ADR 0006 evaluates an in-tree parser and encoder path.
- [yaxpeax-arm](https://github.com/iximeow/yaxpeax-arm) and [yaxpeax-arch](https://github.com/iximeow/yaxpeax-arch): selected portable ARM/Thumb/AArch64 decoder and shared types, both declared 0BSD. Exact inspected revisions and notices are in [dependencies.md](dependencies.md).
- [Capstone](https://www.capstone-engine.org/) and [distribution/license guidance](https://www.capstone-engine.org/download): multi-architecture decoding and native-build considerations.
- [Goblin](https://github.com/m4b/goblin): executable format parsing candidate.
- [object security scope](https://github.com/gimli-rs/object#security): review its documented trusted-input assumptions before selecting it for arbitrary binary files.
- [Keystone](https://github.com/keystone-engine/keystone) and its [FOSS exception](https://github.com/keystone-engine/keystone/blob/master/EXCEPTIONS-CLIENT): GPLv2 plus exception, not MIT; MIT is listed in the exception, but all applicable conditions and distribution requirements need evaluation.
- [LLVM MC](https://llvm.org/docs/CommandGuide/llvm-mc.html) and [LLVM licensing policy](https://llvm.org/docs/DeveloperPolicy.html): independent encoding oracle in the [text assembler decision](adr/0006-text-assembler-selection.md).
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/): optional richer key-event negotiation, with legacy fallback.

## Symbol-table format references and fixture provenance

The symbol-reader work uses published layout facts from these primary sources:

| Source | Inspected scope |
| --- | --- |
| Microsoft [PE/COFF specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format) | Ordinary import descriptors, lookup/IAT metadata, name/ordinal imports, export aliases, and bounded forwarders |
| System V gABI 4.2 [sections](https://gabi.xinuos.com/v42/elf/03-sheader.html), [string tables](https://gabi.xinuos.com/v42/elf/04-strtab.html), and [symbol tables](https://gabi.xinuos.com/v42/elf/05-symtab.html) | ELF32/64 section-backed symbol records, links, bindings, visibility, values, and string boundaries |
| Arm [ELF32 symbol values](https://github.com/ARM-software/abi-aa/blob/main/aaelf32/aaelf32.rst#553symbol-values), consulted 2026-11-28 | ARM/Thumb instruction-state encoding in function symbol values |
| Apple XNU [loader.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/loader.h) and [nlist.h](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/nlist.h) at `43a90889846e00bfb5cf1d255cdc0a701a1e05a4` | Mach-O section/nlist layouts, symbol flags, dylib dependency ordinals, and explicit dyld-metadata gaps |

The [readers](../crates/hiewbie-core/src/symbols.rs), their per-format tests, and
the shared [fixture generators](../tests/fixtures/symbol_images.rs) are original
hiewbie code. Tests construct byte arrays or sparse byte sources from documented
integer fields; no upstream header text, binary samples, proprietary HIEW code,
or manual text is copied. This work introduces no new dependency or third-party
source distribution. The existing MIT licensing of hiewbie's original code
continues to apply.

Fixture coverage establishes the implemented table subset and failure handling.
It does not establish a complete PE loader inventory, sectionless ELF dynamic
symbol resolution, dyld export/binding reconstruction, native-platform execution,
or original-HIEW runtime parity. Those limitations and the distinction between
metadata records and navigable targets are explicit in ADR 0005.

## Structure-record format references and provenance

The [bounded structure-record contract](format-records.md) adds TE firmware,
Mach-O commands/sections/thread states, and selected relocation/debug metadata.
These primary specifications and published source layouts were consulted on
2026-11-28. They establish field meanings and implementation decisions; they do
not establish original-HIEW runtime equivalence or complete loader conformance.

| Primary source and inspected revision | Facts used |
| --- | --- |
| UEFI PI 1.8A [TE header](https://uefi.org/specs/PI/1.8A/V1_TE_Image.html), [creation](https://uefi.org/specs/PI/1.8A/V1_TE_Image_Creation.html), and [loading](https://uefi.org/specs/PI/1.8A/V1_TE_Image_Loading.html), chapter 17 | One 40-byte header, original PE values, stripped-header bias, the two directories, and the nonzero-RVA/zero-size position-independent convention |
| UEFI 2.10 [Overview §2.1.1](https://uefi.org/specs/UEFI/2.10/02_Overview.html) | Firmware machine tags, particularly `0x01C2` as ARMTHUMB_MIXED rather than a mandatory Thumb-only entry |
| Microsoft [PE/COFF specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format) | Section headers, machine values, relocation blocks, HIGHADJ's second word, ARM/Thumb MOVW/MOVT pairs, and debug-directory records |
| EDK2 [`PeImage.h`](https://github.com/tianocore/edk2/blob/cd6f70671097dcccc9b70a4dfcd2d037a51f453c/MdePkg/Include/IndustryStandard/PeImage.h), [`BasePeCoff.c`](https://github.com/tianocore/edk2/blob/cd6f70671097dcccc9b70a4dfcd2d037a51f453c/MdePkg/Library/BasePeCoffLib/BasePeCoff.c), and [`GenFw.c`](https://github.com/tianocore/edk2/blob/cd6f70671097dcccc9b70a4dfcd2d037a51f453c/BaseTools/Source/C/GenFw/GenFw.c), `cd6f70671097dcccc9b70a4dfcd2d037a51f453c` | Retained section headers, unchanged original RVAs, loaded header base, full retained header prefix, adjusted raw/debug pointers, and loader-specific restrictions |
| EDK2 `edk2-stable202402` [`Elf32Convert.c`](https://github.com/tianocore/edk2/blob/edk2-stable202402/BaseTools/Source/C/GenFw/Elf32Convert.c) and [`GenFw.c`](https://github.com/tianocore/edk2/blob/edk2-stable202402/BaseTools/Source/C/GenFw/GenFw.c) | ARM ELF conversion preserves the entry state bit, emits ARMTHUMB_MIXED, and copies the entry into TE |
| LLVM `llvmorg-20.1.8` [`CVDebugRecord.h`](https://github.com/llvm/llvm-project/blob/llvmorg-20.1.8/llvm/include/llvm/Object/CVDebugRecord.h) | RSDS/PDB70 and NB10/PDB20 fixed-header field order and age/signature layout |
| Apple XNU `43a90889846e00bfb5cf1d255cdc0a701a1e05a4`, [`loader.h`](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/loader.h), [`reloc.h`](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/reloc.h), [`x86_64/reloc.h`](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/x86_64/reloc.h), [`arm/reloc.h`](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/arm/reloc.h), and [`arm64/reloc.h`](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/EXTERNAL_HEADERS/mach-o/arm64/reloc.h) | Load-command/section layouts, SG_HIGHVM, ordinary/scattered relocations, PAIR/SUBTRACTOR rules, and ARM HALF length-bit meanings |
| Apple XNU `f6217f891ac0bb64f3d375211650a4c1ff8ca1ea`, [`i386/thread_status.h`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/mach/i386/thread_status.h), [`i386/_structs.h`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/mach/i386/_structs.h), [`arm/thread_status.h`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/mach/arm/thread_status.h), and [`arm/_structs.h`](https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/mach/arm/_structs.h) | Explicit x86/x64/ARM/ARM64 thread flavors, 32-bit-word counts, PC offsets, and ARM CPSR state; cross-checked against the earlier XNU layout reference |
| Apple cctools [`arm64/reloc.h`](https://github.com/apple-oss-distributions/cctools/blob/e0d56624eca2a76c2ace4c21850df9e666de4ca5/include/mach-o/arm64/reloc.h), `e0d56624eca2a76c2ace4c21850df9e666de4ca5` | Updated ARM64 ADDEND description and examples, including BRANCH26 as well as PAGE21/PAGEOFF12 |
| Apple ld64 [`macho_relocatable_file.cpp`](https://github.com/apple-oss-distributions/ld64/blob/f60a74eaa2c99585de1dc0f2820e7a9f8aaf522c/src/ld/parsers/macho_relocatable_file.cpp), `f60a74eaa2c99585de1dc0f2820e7a9f8aaf522c` | Independent confirmation of relocation pair constraints and signed addend interpretation |

The inspected EDK2 files carry BSD-2-Clause-Patent identifiers. LLVM's CodeView
header carries Apache-2.0 with the LLVM exception. The Apple loader, relocation,
and thread headers and the inspected cctools/ld64 files carry APSL 2.0 notices.
File-specific additional notices matter: XNU's generic `reloc.h` also contains
Christopher G. Demetriou's three-clause BSD notice, while
`i386/thread_status.h` additionally carries OSF/Carnegie Mellon notices and the
Carnegie Mellon permission/retention terms. These source notices were reviewed
as part of source selection; they are not relabelled as the project's MIT
license. No upstream header text, implementation, or binary was copied or
substantially adapted into hiewbie by this work, so it introduces no new
third-party source distribution or runtime dependency.

The new [TE reader](../crates/hiewbie-core/src/formats/te.rs),
[Mach-O helpers](../crates/hiewbie-core/src/formats/macho/), shared record model,
and [TE](../tests/fixtures/te_images.rs),
[Mach-O image](../tests/fixtures/macho_images.rs), and
[Mach-O record](../tests/fixtures/macho_records.rs) fixture generators are
independently written MIT project code. Fixtures construct original byte arrays
from published integer fields and instruction encodings. They contain no
redistributed firmware, proprietary HIEW content, third-party executable, or PDB.

The official [HIEW format list](https://www.hiew.ru/) establishes that TE/TE64 and
Mach-O are advertised formats. It does not establish precise TE/TE64 label
semantics or the added T/R structure controls. Hiewbie's documented common-header
and machine-width interpretation is specification-based. The generated tests
establish its own metadata subset and byte-preservation behavior; firmware
loadability, complete dyld/fixup resolution, the complete host matrix, and
original-HIEW runtime equivalence remain distinct evidence requirements.

## Provenance policy

No upstream ArtCraft or HIEW implementation code or build scripts were copied.
The original specifications, editor, and build orchestration are independently
written. Selected Cargo dependency versions and license review are recorded in
[dependencies.md](dependencies.md); optional development-skill adaptations retain
their separate [provenance and notices](../THIRD_PARTY_NOTICES.md).

For future copied or substantially adapted material, record the source repository,
file, exact commit, original license, retained notice, destination file, and local
changes. Include required notices in source and release artifacts. Keep hiewbie's
original code MIT-licensed and describe third-party licenses accurately.

Hex byte display uses characters defined by the PC glyph and encoding
standards. The editor keeps byte values authoritative across rendering and
editing. [IBM PC display glyph mapping](https://www.unicode.org/Public/MAPPINGS/VENDORS/MISC/IBMGRAPH.TXT)
and [Microsoft CP437 mapping](https://www.unicode.org/Public/MAPPINGS/VENDORS/MICSFT/PC/CP437.TXT).
