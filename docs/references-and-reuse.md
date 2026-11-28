# References and source reuse

Baseline research snapshot: 2026-11-27; analysis and symbol readers updated
2026-11-28.
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

- [Ratatui](https://github.com/ratatui/ratatui) and [Crossterm](https://github.com/crossterm-rs/crossterm): selected terminal components.
- [iced-x86](https://github.com/icedland/iced) and its [CodeAssembler API](https://docs.rs/iced-x86/latest/iced_x86/code_asm/index.html): selected x86 decoder; the assembler API is disabled and does not itself parse user-entered assembly text.
- [yaxpeax-arm](https://github.com/iximeow/yaxpeax-arm) and [yaxpeax-arch](https://github.com/iximeow/yaxpeax-arch): selected portable ARM/Thumb/AArch64 decoder and shared types, both declared 0BSD. Exact inspected revisions and notices are in [dependencies.md](dependencies.md).
- [Capstone](https://www.capstone-engine.org/) and [distribution/license guidance](https://www.capstone-engine.org/download): multi-architecture decoding and native-build considerations.
- [Goblin](https://github.com/m4b/goblin): executable format parsing candidate.
- [object security scope](https://github.com/gimli-rs/object#security): review its documented trusted-input assumptions before selecting it for arbitrary binary files.
- [Keystone](https://github.com/keystone-engine/keystone) and its [FOSS exception](https://github.com/keystone-engine/keystone/blob/master/EXCEPTIONS-CLIENT): GPLv2 plus exception, not MIT; MIT is listed in the exception, but all applicable conditions and distribution requirements need evaluation.
- [LLVM MC](https://llvm.org/docs/CommandGuide/llvm-mc.html) and [LLVM licensing policy](https://llvm.org/docs/DeveloperPolicy.html): text assembly candidate; evaluate deployment size and all required targets.
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
