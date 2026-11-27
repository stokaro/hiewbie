# hiewbie

An open-source HIEW-compatible terminal hex editor, disassembler, and binary
patcher, to be implemented in Rust. Native macOS use is a first-class goal.

**Status: project bootstrap and implementation specification.** There is no
working editor or published application binary yet. Features and platform support
below are requirements, not completed capabilities.

The project aims to preserve HIEW's compact, keyboard-driven workflow: open a
binary, switch between text, hex, and code, follow an address or branch, change
bytes or instructions, and save. Familiar keys, contextual function-key hints,
layout, navigation, and executable awareness are part of compatibility.

## Project specification

- [Implementation epic](https://github.com/stokaro/hiewbie/issues/1)
- [HIEW compatibility and acceptance requirements](docs/compatibility.md)
- [Architecture and implementation plan](docs/architecture.md)
- [Build, release, and platform matrix](docs/build-and-release.md)
- [Reference projects and reuse policy](docs/references-and-reuse.md)
- [Contributor and agent instructions](AGENTS.md)
- [Development workflow and verification](docs/development-workflow.md)

## Product goals

- Reproduce the console HIEW experience in Rust, with native macOS, Windows,
  Linux, and FreeBSD executables.
- Support raw bytes, executable structures, disassembly, and textual assembly
  patches through one consistent document model.
- Make Apple Silicon and Intel Macs equally usable, including function keys,
  Option/Alt, keyboard layouts, Unicode paths, and terminal restoration.
- Keep host CPU support separate from the instruction sets in files. A macOS
  ARM64 executable must still analyze and patch x86/x64 Windows binaries.
- Preserve the complete HIEW parity backlog, including advanced and legacy
  features, while delivering useful increments.
- Improve on the original with ARM64 analysis, undo/redo, reproducible patch
  workflows, and automated compatibility tests.

## Required release platforms

The target set follows the inspected ArtCraft application release matrix.
Every target is **planned** at this stage.

| Platform | Required architectures / delivery |
| --- | --- |
| macOS | Apple Silicon and Intel; universal terminal binary |
| Windows | x64 and ARM64 |
| Linux | x86_64, AArch64, and RISC-V 64 |
| FreeBSD | x86_64 and AArch64 |
| Web | WebAssembly core plus an actual browser terminal frontend |

See the [build specification](docs/build-and-release.md) for exact Rust target
triples, reference commits, validation levels, and release artifacts. Browser
limitations on filesystem and device access must be explicit; native builds
remain standalone terminal applications.

## Development approach

The intended design has a portable Rust document/command core, format and
instruction backends, a thin terminal frontend, and a separate browser adapter.
Build orchestration belongs in Rust `xtask` commands with small platform adapters.
The initial implementation must establish a buildable workspace and automated
tests before adding editor functionality. Commands documented in design files
are proposed interfaces until implemented.

AI-assisted contributions are welcome and must satisfy the same review,
licensing, compatibility, and automated verification requirements as other work.

## References and license

[HIEW](https://www.hiew.ru/) is the behavior reference. This is an independent
implementation, with no affiliation or endorsement implied. Its proprietary
source, binaries, and assets are not part of this repository.

[ArtCraft's applications](https://getartcraft.com/apps) inform the portable Rust
architecture and release coverage. Their licensing is evaluated per repository
and per reused file; see [reuse notes](docs/references-and-reuse.md).

hiewbie's original source and documentation are licensed under the [MIT
License](LICENSE). Third-party components retain their own licenses and notices;
see [third-party development material](THIRD_PARTY_NOTICES.md).
