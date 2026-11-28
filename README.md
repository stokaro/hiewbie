# hiewbie

An open-source HIEW-inspired terminal hex editor and disassembler in Rust.
Native macOS use is a first-class goal.

**Status: native byte editing and initial executable analysis.** Text/Hex/Code,
PE/ELF/Mach-O headers and load mappings, x86/ARM-family decoding, direct branch
navigation, selected symbol/import/export tables, literal search, overwrite
transactions, undo/redo and explicit save are implemented. Complete loader
metadata, textual assembly, advanced/legacy
parity, browser UI and release packages remain planned. See the
[compatibility ledger](docs/compatibility-status.md).

The project aims to preserve HIEW's compact, keyboard-driven workflow: open a
binary, switch between text, hex, and code, follow an address or branch, change
bytes or instructions, and save. Familiar keys, contextual function-key hints,
layout, navigation, and executable awareness are part of compatibility.

## Build and run

Install Rust through [rustup](https://rustup.rs/). The repository selects Rust
1.90.0 and commits its dependency lockfile. A native system linker is required;
macOS uses Xcode command-line tools and Windows uses the MSVC build tools.

```sh
git clone https://github.com/stokaro/hiewbie.git
cd hiewbie
cargo build --release --locked --package hiewbie
cargo run --release --locked --package hiewbie -- path/to/file.bin
```

The command is `target/release/hiewbie` (`hiewbie.exe` on Windows). Read-only and
noninteractive inspection are also available:

```sh
cargo run --locked --package hiewbie -- --read-only path/to/file.bin
cargo run --locked --package hiewbie -- --dump --offset 0x100 path/to/file.bin
cargo run --locked --package hiewbie -- --headers path/to/program
cargo run --locked --package hiewbie -- --imports path/to/program.exe
cargo run --locked --package hiewbie -- --symbols --slice 1 path/to/universal-macho
cargo run --locked --package hiewbie -- --disassemble --entry path/to/program
cargo run --locked --package hiewbie -- --disassemble raw.bin
cargo run --locked --package hiewbie -- --disassemble --arch x86-64 --base 0x1000 raw.bin
cargo xtask check
```

`--dump` prints at most 256 bytes; it does not require a terminal. Offsets are
hexadecimal by default; append `t` for decimal. A leading dot selects an absolute
virtual address, for example `--offset .401000`. Use `--` before a filename that
starts with a hyphen. Files are opened with bounded reads, including offsets
beyond 4 GiB, without loading the complete file into memory.

## Current controls

| Context | Keys | Behavior |
| --- | --- | --- |
| View | Enter | Cycle Text / Hex / Code, keeping the file offset |
| View | F4 | Choose Text, Hex or Code; arrows select, Enter applies, Esc cancels |
| View | F3 / Ctrl+E | Begin a pending overwrite transaction |
| View | F5 / Ctrl+G | Goto: hex, decimal `t`, relative `+` / `-`, or absolute dotted VA |
| View | F7 / Ctrl+F | Search exact hex bytes or UTF-8 text with ASCII case folding |
| Search | Tab / F2 / F4 / F5 | Switch field / reverse direction / file or completed block / last hit |
| View | Shift+F7 / Ctrl+Enter / Ctrl+R | Repeat in the same direction from the last hit, without wrapping |
| Hex / Code view | `*` / `[` / `]` | Start, finish, or clear a block / first byte / last byte |
| Hex / Code view | Alt+`*` / Ctrl+`*` or Ctrl+K | Move the marked endpoint / select the whole file |
| Completed block | F2 | Write raw bytes to a file. Confirm its path, offset, and format. Use `*` as the offset to append |
| View | F9 | Browse and open a file |
| View | Ctrl+Backspace / Tab | Open session history / cycle its files |
| View | Ctrl+F9 / Ctrl+F11 / Ctrl+F12 | Select / previous / next command-line file |
| Files / History | F9 | Switch between the directory and history lists |
| History | F2 / F5 / F8 | Toggle full paths / mark for Tab / remove a history entry |
| Files | Arrows / Enter / Esc | Select / open a directory or file / cancel |
| Files | F2 / F3–F8 / F10 | Toggle hidden entries / sort and reverse / set a filename mask |
| View | F8 / Ctrl+O | Executable headers; F5 entry, F6 regions, F4 fields, S slices |
| Headers | F7 / F9 / N | Parsed imports / exports / all symbols; tables load on request |
| Symbols | Enter / R | Follow target bytes / the exact metadata record |
| View / Edit | Alt+= / Ctrl+L | Open the 64-bit Calculator |
| Calculator | Enter / Insert / Page Up | Evaluate / toggle insertion / clear the expression |
| Calculator | Ctrl+E | Evaluate with checked signed 64-bit arithmetic |
| View | Shift+F1 / Ctrl+A | Decoder profile, byte order and explicit raw base |
| Code | Ctrl+F1 / Ctrl+B | Cycle x86 16/32/64 bitness |
| Hex / Decode view | F6 / Ctrl+D | Find the first x86 reference to the current address |
| Decode, viewing | Ctrl+F6 / Ctrl+T | Find the next x86 reference |
| Decode, viewing | Alt+F9 | Toggle instruction-aligned and byte-by-byte reference scanning |
| Decode, viewing | 1–9 / A–Z / 0 / Backspace | Follow a displayed direct destination, return, or cycle through reference hits |
| Code, viewing | / | Restart decoding at the current byte |
| Edit | Two hex digits / Tab | Write a byte / select printable ASCII entry |
| Edit | F3 / Backspace | Restore the original current / previous byte |
| Edit | Ctrl+Z / Ctrl+Y | Undo / redo pending byte changes |
| Edit | F9 / Ctrl+S | Save and complete the transaction |
| Edit | Esc | Discard the transaction without writing |
| Hex/Text | Arrows, PgUp/PgDn, Home/End | Move by byte, row, page, or row edge |
| Code, viewing | Up/Down, PgUp/PgDn | Traverse instruction rows; reverse movement uses known boundaries or the documented byte/alignment fallback |
| Code | Left/Right | Move by byte for explicit resynchronization |
| Any view | Ctrl+Home / Ctrl+End | Move to the first / last byte |
| Context | F1 / Esc | Help / cancel the active operation before exiting |

Run `hiewbie` without a filename to browse the working directory. F9 opens
the same browser from a view, saves in edit mode, and opens parsed exports
in headers. Give multiple paths to browse files in argument order. History
retains the mode and offset of each visited file for the session. Text displays
printable ASCII bytes, not a decoded
Unicode document. A single pending hex digit never changes a byte. Bracketed
paste is recognized and rejected. macOS function keys may require Fn; the Ctrl
alternatives avoid changing system keyboard settings. Detailed behavior and
terminal gaps are in [the input decision](docs/adr/0002-terminal-input.md).
In Code edit mode, digits write bytes, Backspace cancels a pending digit or
restores the previous byte and moves there, and vertical movement uses the byte
grid rather than instruction rows. Backspace in Code view returns from a branch.

F3 restores the selected byte from the current transaction's source; it is
different from undoing the most recent command. Backspace first cancels an
incomplete hex digit, otherwise restores and moves to the previous byte.
Ctrl+Z/Y additionally undo/redo commands, including restores. The
[Hiew 8.63 comparison](docs/reference/hiew-8.63.md) records documentation-derived
expectations, observed Windows console behavior, and remaining search, Text,
and navigation differences.

Saving streams the logical document into a sibling temporary file before
replacement. Conflict checks refuse an externally changed source; ordinary
permissions and executable bits are preserved. Symlink and hard-link saves are
refused in this initial strategy. Save is synchronous; advanced metadata,
concurrent-writer races, and power-loss durability have explicit limits in
[the storage decision](docs/adr/0001-document-storage.md).

The [recorded Unix terminal scenario](docs/evidence/native-editor.cast) shows
the initial workflow. It is generated from a temporary fixture by the automated
PTY test, not a comparison recording of proprietary HIEW.

## Executable analysis

The inspected architecture is independent of the host: the same ARM64 Mac build
can decode an x86 PE and an ARM64 Mach-O. Automatic selection uses image metadata
and starts ordinary unrecognized raw files in x86-32. Ctrl+F1 cycles x86 modes
in Decode. Profiles are `x86-16`, `x86-32`, `x86-64`,
`arm`, `thumb` and `aarch64`. The profile dialog offers automatic selection,
little/big/automatic instruction byte order, raw addressing and an explicit base.
The title and Decode rows distinguish mapped VAs, raw-based addresses and unmapped
file offsets. Use F4 then Decode, or press Enter from Hex. In the default x86
view, adjacent single-byte NOPs share a row and Page Up and Page Down move by the
visible rows. The headless listing reports each instruction individually.
[Windows Decode evidence](docs/evidence/hiew-8.63-windows-decode.json)

| Input | Current interpretation |
| --- | --- |
| PE32 / PE32+ | Headers, mappings, entry, ordinary name/ordinal imports and exports, explicit forwarders |
| ELF32 / ELF64 | Headers, load segments and section-backed `SYMTAB`/`DYNSYM` in either byte order |
| Mach-O32 / Mach-O64 | Headers, segments, `LC_MAIN` entry and `LC_SYMTAB`/nlist in either byte order |
| Universal Mach-O | Fat32/fat64 tables, explicit `--slice N` or header-dialog selection; indices start at zero |

The first fat slice is the default, independent of the computer's CPU. Invalid
metadata leaves raw viewing and byte editing available. Edited headers and
instructions are reinterpreted from the current logical bytes; undo, discard and
save invalidate dependent results. Selecting a field or region navigates to its
actual backing bytes and never rebuilds the executable.

`--headers` prints at most 64 fields, regions and slices per category;
`--disassemble` prints at most 32 rows. `--symbols`, `--imports` and `--exports`
print at most 64 entries and every coverage note. These six output modes are
mutually exclusive. `--arch`, `--endian`, `--raw` and `--base`
are explicit interpretation controls. Setting a base selects raw addressing;
it cannot be combined with slice selection. Code editing remains byte editing,
without textual assembly. Unknown/truncated instructions retain their bytes.

Symbol tables load only when requested and have separate read, entry and text
budgets. Their results follow logical byte edits, undo, restore, discard, save
and selected Mach-O slice. A table error leaves headers, Code and raw access
available. Enter follows a definition's backed bytes or a PE import's IAT pointer
slot; it does not resolve an external function. R opens the source metadata
record, including unresolved names, forwarders and definitions without file
bytes. Names are escaped before terminal or CLI output.

Coverage is deliberately explicit: PE delay imports and bound-only reconstruction,
ELF sectionless dynamic lookup, symbol versions and GOT/PLT relocations, and
Mach-O export tries, dyld binding streams and chained fixups remain open. Parsed
Mach-O nlist exports do not establish the complete runtime export inventory.
See [symbol interpretation and bounds](docs/adr/0005-symbol-navigation.md) and
the [automated symbol-navigation recording](docs/evidence/symbol-navigation.cast).

Instruction decoding currently accepts little-endian interpretations. Explicit
big-endian configurations report unsupported; format data endianness does not
establish instruction-order support. ARM version filtering, ARMv7 NEON, SVE/SVE2,
Thumb IT-state propagation, indirect/far branch resolution and complete symbol or
reference analysis remain gaps. See the [analysis decision and bounds](docs/adr/0004-analysis-backends.md)
and the [recorded PE analysis workflow](docs/evidence/executable-analysis.cast).

## Project specification

- [Implementation epic](https://github.com/stokaro/hiewbie/issues/1)
- [HIEW compatibility and acceptance requirements](docs/compatibility.md)
- [Architecture and implementation plan](docs/architecture.md)
- [Build, release, and platform matrix](docs/build-and-release.md)
- [Reference projects and reuse policy](docs/references-and-reuse.md)
- [Contributor and agent instructions](AGENTS.md)
- [Development workflow and verification](docs/development-workflow.md)
- [Implemented command inventory](docs/commands.md)
- [Hiew 8.63 reference comparison and 25 scenarios](docs/reference/hiew-8.63.md)

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

The required target set follows the inspected ArtCraft application release
matrix. The [machine-readable manifest](build/targets.json) retains every row
and records its configured CI strategy and release blockers. Native CI is
configured for macOS, Windows, and Linux on x64/ARM64. RISC-V and FreeBSD x64
receive compile checks, WebAssembly compiles the core only, and FreeBSD ARM64
has a pinned-toolchain blocker. None of those checks produces a release package.

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

The workspace contains `hiewbie-core`, the native `hiewbie` application, and
`xtask`. The core contains the bounded readers and pure-Rust decoders, with no
terminal or native filesystem dependency. A browser adapter remains planned.
`cargo xtask check`, `build --target TRIPLE`, and `targets` work;
`package` and `verify-release` explicitly fail until implemented.

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
