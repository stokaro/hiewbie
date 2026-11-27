# Dependency and license inventory

Review date: 2026-11-27. This inventory covers the initial native editor and its
workspace tools at the versions resolved in [Cargo.lock](../Cargo.lock).
The review used the exact downloaded package manifests, their shipped license
files, and their `.cargo_vcs_info.json` provenance records. Registry checksums in
the lockfile identify the package archives; upstream revisions below are the
revisions reported by those archives.

This is a bounded dependency review, not a complete audit of every transitive
source file or a finished distribution notice bundle. No application dependency
source is vendored in this repository. Cargo retrieves these packages from the
registry. The separately retained agent skills have their own provenance and
notices in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

## Dependency boundaries

| Workspace component | Third-party dependency boundary |
| --- | --- |
| `hiewbie-core` | No third-party Cargo dependencies. Byte sources, transactions, commands, offsets, and incremental search use the Rust standard library. No native filesystem or terminal adapter enters this crate. |
| `hiewbie` | Ratatui/Crossterm provide terminal rendering and input; `tempfile`, `same-file`, and target-specific platform helpers support native persistence. The application depends on the local MIT-licensed `hiewbie-core`. |
| `xtask` | Serde and Serde JSON read and validate the checked-in target manifest. These are workspace-tool dependencies, not direct dependencies of the editor executable. |

The native editor enables only Ratatui's `crossterm` feature and disables
Ratatui's default features. It uses `ratatui::crossterm`, so terminal input and
the Ratatui backend share the same resolved Crossterm version. This selection
does not establish runtime support for every required platform; the complete
target and verification requirements remain in [build and release](build-and-release.md).

## Direct native dependencies

All five direct third-party native dependencies below provide an MIT option.
The alternatives are recorded as published rather than relabeling the packages
as exclusively MIT. Native dependency versions are exact manifest pins.

| Package and API reference | Locked version | Purpose in hiewbie | Declared license and inspected package files |
| --- | --- | --- | --- |
| [ratatui](https://docs.rs/ratatui/0.29.0/ratatui/) | `0.29.0` | Terminal cells, widgets, bounded viewport rendering, and deterministic `TestBackend` tests | `MIT`; `LICENSE`, including Florian Dehau and The Ratatui Developers notices |
| [tempfile](https://docs.rs/tempfile/3.23.0/tempfile/) | `3.23.0` | Sibling temporary files, replacement persistence, and generated test fixtures | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE`; MIT notice names Steven Allen |
| [same-file](https://docs.rs/same-file/1.0.6/same_file/) | `1.0.6` | Compare opened-file identity with path identity before reads and replacement | Manifest spells the choice `Unlicense/MIT`; `COPYING` explicitly permits either license. Inspected `LICENSE-MIT` and `UNLICENSE`; MIT notice names Andrew Gallant |
| [libc](https://docs.rs/libc/0.2.191/libc/) | `0.2.191` | `cfg(unix)` only: supplies `O_NONBLOCK` to avoid blocking while opening a path that may be a FIFO | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE`; MIT notice names The Rust Project Developers |
| [winapi-util](https://docs.rs/winapi-util/0.1.11/winapi_util/) | `0.1.11` | `cfg(windows)` only: safe file information used to reject replacement saves through hard-linked files | `Unlicense OR MIT`; `COPYING`, `LICENSE-MIT`, and `UNLICENSE`; MIT notice names Andrew Gallant |

`libc` licenses the Rust bindings and constants. Its package license does not
replace the licenses of host system libraries or toolchains. The initial editor
does not select or embed a third-party disassembler or assembler engine.

## Terminal backend and workspace tools

| Package and API reference | Locked version | Relationship and purpose | Declared license and inspected package files |
| --- | --- | --- | --- |
| [crossterm](https://docs.rs/crossterm/0.28.1/crossterm/) | `0.28.1` | Transitive through Ratatui and used through its re-export for key events, paste handling, raw mode, cursor, and alternate-screen lifecycle | `MIT`; `LICENSE`, with the Timon notice |
| [serde](https://docs.rs/serde/1.0.229/serde/) | `1.0.229` | Direct `xtask` dependency with `derive`; typed target-manifest serialization and validation | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE` |
| [serde_json](https://docs.rs/serde_json/1.0.154/serde_json/) | `1.0.154` | Direct `xtask` dependency; target-manifest JSON input and output | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE` |

The workspace manifests allow the Serde `1.0` version line; the committed
lockfile fixes the versions recorded here. Use locked dependency resolution for
verification and update this inventory when the lockfile changes.

## Inspected source provenance

These links identify the source revisions reported by the corresponding
published packages. The shipped license filenames in the tables above refer to
the package root, which can differ from the repository root.

| Package | Reported upstream revision |
| --- | --- |
| `ratatui 0.29.0` | [28732176e1adb4cddba45c2d6b2b27abf7a46f79](https://github.com/ratatui/ratatui/tree/28732176e1adb4cddba45c2d6b2b27abf7a46f79) |
| `crossterm 0.28.1` | [492b764f9f132d302903ff6f2f55e96c1b8cd0ec](https://github.com/crossterm-rs/crossterm/tree/492b764f9f132d302903ff6f2f55e96c1b8cd0ec) |
| `tempfile 3.23.0` | [fe9f4a305b785abd1f4f912c8304bcc90024f695](https://github.com/Stebalien/tempfile/tree/fe9f4a305b785abd1f4f912c8304bcc90024f695) |
| `same-file 1.0.6` | [5799cd323b8eefd17a089c950dac113f66c89c9e](https://github.com/BurntSushi/same-file/tree/5799cd323b8eefd17a089c950dac113f66c89c9e) |
| `libc 0.2.191` | [ccc2ff4aaab962f7fde7b8edda7379c045b1ca12](https://github.com/rust-lang/libc/tree/ccc2ff4aaab962f7fde7b8edda7379c045b1ca12) |
| `winapi-util 0.1.11` | [803874c57dc1f10ecd42f7c86d9de72f53818432](https://github.com/BurntSushi/winapi-util/tree/803874c57dc1f10ecd42f7c86d9de72f53818432) |
| `serde 1.0.229` | [7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8](https://github.com/serde-rs/serde/tree/7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8), package path `serde` |
| `serde_json 1.0.154` | [20572a6d066958542d79cdd6cf1a7cd9f0c8e1f8](https://github.com/serde-rs/json/tree/20572a6d066958542d79cdd6cf1a7cd9f0c8e1f8) |

## Transitive license distinctions

Most other locked registry packages declare an MIT option. That does not make
the complete dependency graph MIT-licensed. The metadata inventory identified
the following distinctions, and the listed license texts were inspected:

| Locked package | Declared license | Dependency path and inspected notices |
| --- | --- | --- |
| [foldhash 0.1.5](https://github.com/orlp/foldhash/tree/42461756c3760f9165c90d9626a49e36980a1e47) | `Zlib` | Ratatui → `lru` → `hashbrown` → `foldhash`; package `LICENSE` names Orson Peters and retains the Zlib origin, altered-source, and notice conditions |
| [unicode-ident 1.0.27](https://github.com/dtolnay/unicode-ident/tree/c1ac3afb721f3cea6527bc2255340a04d0de3ca0) | `(MIT OR Apache-2.0) AND Unicode-3.0` | Procedural-macro parsing dependencies used by Ratatui and Serde derives; inspected `LICENSE-MIT`, `LICENSE-APACHE`, and `LICENSE-UNICODE`. Choosing MIT does not remove the additional Unicode-3.0 notice |
| [ryu 1.0.23](https://github.com/dtolnay/ryu/tree/f0b52bb194befe6fd242154f2182fafd43a819b8) | `Apache-2.0 OR BSL-1.0` | Ratatui → `compact_str` → `ryu`; inspected `LICENSE-APACHE` and `LICENSE-BOOST`. This package does not offer an MIT option |

These observations include build-time dependencies and do not assert that every
locked package is linked into every target artifact. Target-specific native
libraries, generated code, file-specific notices, and actual shipped content
must be accounted for when producing release notices.

## Reproduction and remaining distribution work

Use the pinned toolchain and committed lockfile to inspect the current graph:

```sh
cargo metadata --locked --format-version 1
cargo tree --locked -p hiewbie
cargo tree --locked -p xtask
cargo tree --locked -i foldhash
cargo tree --locked -i unicode-ident
cargo tree --locked -i ryu
```

For target-specific decisions, inspect the graph for that target rather than
treating the host graph as the release matrix. `cargo metadata` provides each
resolved package's `manifest_path`; inspect the adjacent shipped license files
and `.cargo_vcs_info.json`, not only the manifest's license expression.

Release packaging and complete notice aggregation are unimplemented. Before a
release can pass its distribution gate, packaging must retain the applicable
license texts, copyrights, and notices for its actual resolved and shipped
components, including the transitive distinctions above. This inventory and
the repository's own MIT license do not substitute for that release bundle.
