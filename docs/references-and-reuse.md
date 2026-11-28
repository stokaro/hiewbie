# References and source reuse

Baseline research snapshot: 2026-11-27; analysis selection updated 2026-11-28.
These sources describe upstream behavior or inform
implementation choices; they are not evidence that hiewbie already implements it.

## HIEW behavior references

- [Supplied Hiew 8.63 distribution audit](reference/hiew-8.63.md): versioned
  author-documentation evidence, source hashes, known implementation differences
  and 25 reference scenarios. Original startup was blocked; no dynamic parity
  is claimed. The original distribution remains outside the repository.
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

- [Ratatui](https://github.com/ratatui/ratatui) and [Crossterm](https://github.com/crossterm-rs/crossterm): selected terminal components.
- [iced-x86](https://github.com/icedland/iced) and its [CodeAssembler API](https://docs.rs/iced-x86/latest/iced_x86/code_asm/index.html): selected x86 decoder; the assembler API is disabled and does not itself parse user-entered assembly text.
- [yaxpeax-arm](https://github.com/iximeow/yaxpeax-arm) and [yaxpeax-arch](https://github.com/iximeow/yaxpeax-arch): selected portable ARM/Thumb/AArch64 decoder and shared types, both declared 0BSD. Exact inspected revisions and notices are in [dependencies.md](dependencies.md).
- [Capstone](https://www.capstone-engine.org/) and [distribution/license guidance](https://www.capstone-engine.org/download): multi-architecture decoding and native-build considerations.
- [Goblin](https://github.com/m4b/goblin): executable format parsing candidate.
- [object security scope](https://github.com/gimli-rs/object#security): review its documented trusted-input assumptions before selecting it for arbitrary binary files.
- [Keystone](https://github.com/keystone-engine/keystone) and its [FOSS exception](https://github.com/keystone-engine/keystone/blob/master/EXCEPTIONS-CLIENT): GPLv2 plus exception, not MIT; MIT is listed in the exception, but all applicable conditions and distribution requirements need evaluation.
- [LLVM MC](https://llvm.org/docs/CommandGuide/llvm-mc.html) and [LLVM licensing policy](https://llvm.org/docs/DeveloperPolicy.html): text assembly candidate; evaluate deployment size and all required targets.
- [Kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/): optional richer key-event negotiation, with legacy fallback.

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
