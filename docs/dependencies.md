# Dependency and license inventory

Review date: 2026-11-28. This inventory covers the native editor, portable analysis
core, and workspace tools at the versions resolved in [Cargo.lock](../Cargo.lock).
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
| `hiewbie-core` | Three exact-pinned Rust dependencies provide instruction interpretation: `iced-x86`, `yaxpeax-arm`, and `yaxpeax-arch`. Serde serializes the shared capability contract. The document model, bounded executable readers, address mapping, and application commands are hiewbie code. No native filesystem, terminal adapter, or C engine enters this crate. |
| `hiewbie` | Ratatui/Crossterm provide terminal rendering and input; `tempfile`, `same-file`, and target-specific platform helpers support native persistence. `time` formats Files timestamps; Serde JSON emits the core capability report. The application depends on the local MIT-licensed `hiewbie-core`. |
| `xtask` | Serde and Serde JSON read and validate the checked-in target manifest and per-build evidence. The core capability catalog validates backend claims; SHA-256 identifies inspected artifacts. These tool dependencies do not add binary inspection or hashing engines to the editor. |
| `fuzz/replay` | Development-only portable invariant handlers and generated corpus, depending on the local core. Isolated libFuzzer and WASM adapters reuse this helper; no fuzz engine enters the application dependency graph. |

The native editor enables only Ratatui's `crossterm` feature and disables
Ratatui's default features. It uses `ratatui::crossterm`, so terminal input and
the Ratatui backend share the same resolved Crossterm 0.29.0 instance. This selection
does not establish runtime support for every required platform; the complete
target and verification requirements remain in [build and release](build-and-release.md).

## Direct native dependencies

All direct third-party native dependencies below provide an MIT option.
The alternatives are recorded as published rather than relabeling the packages
as exclusively MIT. Native dependency versions are exact manifest pins.

| Package and API reference | Locked version | Purpose in hiewbie | Declared license and inspected package files |
| --- | --- | --- | --- |
| [ratatui](https://docs.rs/ratatui/0.30.2/ratatui/) | `0.30.2` | Terminal cells, widgets, bounded viewport rendering, and deterministic `TestBackend` tests | `MIT`; `LICENSE`, including Florian Dehau and The Ratatui Developers notices |
| [tempfile](https://docs.rs/tempfile/3.23.0/tempfile/) | `3.23.0` | Sibling temporary files, replacement persistence, and generated test fixtures | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE`; MIT notice names Steven Allen |
| [same-file](https://docs.rs/same-file/1.0.6/same_file/) | `1.0.6` | Compare opened-file identity with path identity before reads and replacement | Manifest spells the choice `Unlicense/MIT`; `COPYING` explicitly permits either license. Inspected `LICENSE-MIT` and `UNLICENSE`; MIT notice names Andrew Gallant |
| [time](https://docs.rs/time/0.3.55/time/) | `0.3.55` | Checked date conversion for Files timestamps. Uses `std` and enables `local-offset` on Windows. | `MIT OR Apache-2.0`. Inspected `LICENSE-MIT` and `LICENSE-Apache`. |
| [libc](https://docs.rs/libc/0.2.191/libc/) | `0.2.191` | `cfg(unix)` only: supplies `O_NONBLOCK` to avoid blocking while opening a path that may be a FIFO | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE`; MIT notice names The Rust Project Developers |
| [winapi-util](https://docs.rs/winapi-util/0.1.11/winapi_util/) | `0.1.11` | `cfg(windows)` only: safe file information used to reject replacement saves through hard-linked files | `Unlicense OR MIT`; `COPYING`, `LICENSE-MIT`, and `UNLICENSE`; MIT notice names Andrew Gallant |
| [winconsole](https://omarkmu.github.io/docs/winconsole/console/index.html) | `0.11.1` | `cfg(windows)` only: safe wrappers for [reading](https://learn.microsoft.com/en-us/windows/console/getconsolecursorinfo) and [setting](https://learn.microsoft.com/en-us/windows/console/setconsolecursorinfo) console cursor height | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE`; MIT notice names Omar M. |

The registry archive for [time](https://github.com/time-rs/time/tree/857d9c404c5b6f6cb64d1bfa604c695be3369e12)
records that revision. Its date conversion dependencies in the lockfile are
[time-core 0.1.9](https://github.com/time-rs/time/tree/5d8737c39b9170c36c99cfc1f9f49cf0c059c63a),
[num-conv 0.2.2](https://github.com/jhpratt/num-conv/tree/b022fe87ac3ad5b0d7606ea7465549eaa92efe97),
[powerfmt 0.2.1](https://github.com/jhpratt/powerfmt/tree/3dd45162a7e64d6b354675e4282d0550dce8bfc8),
and [deranged 0.5.9](https://github.com/jhpratt/deranged/tree/aeb03a4be17fe287d5fdd509e9ae134d4b9999fc).
Each archive includes `LICENSE-MIT` and `LICENSE-Apache` and declares
`MIT OR Apache-2.0`.

The Windows-only cursor dependency also resolves `rgb 0.8.53` (MIT, `LICENSE`,
Kornel) and its `bytemuck 1.25.2` dependency (Zlib OR Apache-2.0 OR MIT,
`LICENSE-ZLIB`, `LICENSE-APACHE`, and `LICENSE-MIT`, Daniel "Lokathor" Gee).

`libc` licenses the Rust bindings and constants. Its package license does not
replace the licenses of host system libraries or toolchains.

## Direct portable analysis dependencies

The first analysis slice adds the three direct packages below and eight
transitive packages. Existing locked packages were not upgraded as part of this
selection. These engines are Rust implementations; their use does not require a
native C disassembler build. The accepted scope, capability gaps, malformed-input
experiments, and separate textual-assembly gate are recorded in
[ADR 0004](adr/0004-analysis-backends.md).

| Package and API reference | Exact version | Enabled features and purpose | Declared license and inspected source |
| --- | --- | --- | --- |
| [iced-x86](https://docs.rs/iced-x86/1.22.0/iced_x86/) | `=1.22.0` | Defaults disabled. `std`, `decoder`, `intel`, `instr_info`, `encoder`, and `op_code_info` support x86 16/32/64-bit decoding, Intel text, branch metadata, and checked textual assembly via the in-tree parser | `MIT`. The crate archive does not include a license file; the repository's [LICENSE.txt](https://github.com/icedland/iced/blob/cae65b1e90451f22c65f580d4acc752c8a486754/LICENSE.txt) was inspected at the package's reported revision |
| [yaxpeax-arm](https://docs.rs/yaxpeax-arm/0.5.0/yaxpeax_arm/) | `=0.5.0` | Defaults disabled; `std`, `fmt`, `non-exhaustive-enums`. ARM, Thumb, and AArch64 decoding and text; ARM/Thumb use strict nonconforming-input checks | `0BSD` in the pinned [Cargo.toml](https://github.com/iximeow/yaxpeax-arm/blob/a2723ae2043777472dd373feef4b9c40d52569bb/Cargo.toml). No standalone license file was found in this crate archive or at that repository revision |
| [yaxpeax-arch](https://docs.rs/yaxpeax-arch/0.3.3/yaxpeax_arch/) | `=0.3.3` | Defaults disabled; `std`. Shared decoder traits, bounded byte reader, and instruction lengths | `0BSD`; inspected [LICENSE](https://github.com/iximeow/yaxpeax-arch/blob/d369694a22e2b10aca1c497fc31cdc94fa9a6f41/LICENSE), with the iximeow copyright notice |

No decoder implementation was copied or patched into hiewbie. Cargo retrieves
the original published archives, identified by their checksums in `Cargo.lock`.
The production core uses iced-x86 encoding and an in-tree AArch64 word
encoder. [ADR 0006](adr/0006-text-assembler-selection.md) records the
independently locked evaluation of asm-rs 0.2.0. That package is confined to
the [research probe](../experiments/assembler/README.md).

## Terminal backend and workspace tools

| Package and API reference | Locked version | Relationship and purpose | Declared license and inspected package files |
| --- | --- | --- | --- |
| [crossterm](https://docs.rs/crossterm/0.29.0/crossterm/) | `0.29.0` | Transitive through Ratatui and used through its re-export for key events, paste handling, raw mode, cursor, and alternate-screen lifecycle | `MIT`; `LICENSE`, with the Timon notice |
| [serde](https://docs.rs/serde/1.0.229/serde/) | `1.0.229` | Core capability serialization and typed `xtask` target/evidence validation, with `derive` | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE` |
| [serde_json](https://docs.rs/serde_json/1.0.154/serde_json/) | `1.0.154` | Native `--capabilities --json` and `xtask` manifest/evidence input and output | `MIT OR Apache-2.0`; `LICENSE-MIT` and `LICENSE-APACHE` |

The split Ratatui core, Crossterm backend, and widget crates ship the
same MIT notice as Ratatui. The layout package resolves `lru 0.18.5`
through `ratatui-core`. The `lru` archive ships `LICENSE` under MIT with
Jerome Froelich's notice. The locked version includes the fixes for
[RUSTSEC-2026-0002](https://rustsec.org/advisories/RUSTSEC-2026-0002.html)
and [RUSTSEC-2026-0253](https://rustsec.org/advisories/RUSTSEC-2026-0253.html).

The workspace manifests allow the Serde `1.0` version line; the committed
lockfile fixes the versions recorded here. Use locked dependency resolution for
verification and update this inventory when the lockfile changes.

## Artifact evidence hashing

`xtask` pins `sha2 =0.10.9` to calculate SHA-256 while streaming an artifact.
The digest binds inspection and runtime records to the same bytes; it does not
prove who built them or turn a declared result into an independent attestation.
This dependency is confined to build tooling. The native editor and portable core
do not depend on it. Existing application dependencies were not upgraded.
On Unix, `xtask` also uses the already reviewed `libc =0.2.191` constant
`O_NONBLOCK` to reject non-regular artifact inputs without blocking on a FIFO.

The exact registry archives below were inspected, including their package
manifests, `.cargo_vcs_info.json` and shipped license files. All provide an MIT
option. No upstream implementation was copied or vendored into hiewbie.

| Package | Reported source revision | Declared license and shipped notices |
| --- | --- | --- |
| `sha2 0.10.9` | [RustCrypto/hashes, `82c36a4`](https://github.com/RustCrypto/hashes/tree/82c36a428f8d6f05f3bfccdedb243e9d1f85359d/sha2) | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; Graydon Hoare, Mozilla Foundation and Artyom Pavlov |
| `digest 0.10.7` | [RustCrypto/traits, `3443894`](https://github.com/RustCrypto/traits/tree/344389411fd9718a0742435152e933a9e71461ee/digest) | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; Artyom Pavlov |
| `block-buffer 0.10.4` | [RustCrypto/utils, `6d35952`](https://github.com/RustCrypto/utils/tree/6d35952d3d3b124bc1049ad6fb406b42b1ce4bfe/block-buffer) | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; RustCrypto Project Developers |
| `crypto-common 0.1.7` | [RustCrypto/traits, `66a997c`](https://github.com/RustCrypto/traits/tree/66a997ccf5776ce873da5a48e2885a9908e940f6/crypto-common) | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; RustCrypto Developers |
| `cpufeatures 0.2.17` | [RustCrypto/utils, `9d92d5e`](https://github.com/RustCrypto/utils/tree/9d92d5e95ab4c07c5d8bfd024bf2a17e96d20feb/cpufeatures) | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; RustCrypto Project Developers |
| `generic-array 0.14.7` | [fizyk20/generic-array, `1397d2b`](https://github.com/fizyk20/generic-array/tree/1397d2bb40c406901b7827161b73b34b056cbe3a) | `MIT`; `LICENSE`; Bartłomiej Kamiński |
| `typenum 1.20.1` | [paholg/typenum, `0db9a0f`](https://github.com/paholg/typenum/tree/0db9a0f731981f29266b63586c29fa07e4477b1a) | `MIT OR Apache-2.0`; `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`; Paho Lurie-Gregg |
| `version_check 0.9.5` | [SergioBenitez/version_check, `d77ef9f`](https://github.com/SergioBenitez/version_check/tree/d77ef9f27cc336719b2d839d09ee6635dd22f758) | Manifest spells `MIT/Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE`; Sergio Benitez |

The actual binary metadata tools (`readelf`, `otool` and `llvm-readobj`) are
external developer/runner tools. They are invoked for inspection and are not
linked into or distributed with the application. Their availability, versions
and output remain distinct from runtime compatibility evidence.

## Isolated fuzzing dependencies

The stable `hiewbie-fuzz-replay` helper has no registry dependency beyond the
portable core. Its WASM export adapter has a separate lockfile and uses the same
helper. The guided engine is isolated in `fuzz/libfuzzer`: `libfuzzer-sys =0.4.13`
wraps bundled native LLVM libFuzzer code and is driven by the development command
`cargo-fuzz 0.12.0` on the separately pinned nightly toolchain.

The [fuzzing provenance review](fuzzing.md#provenance-and-license-boundary)
records exact revisions, wrapper and engine notices, including the distinction
between the crate's NCSA metadata and newer Apache/LLVM source headers. This
engine is not an MIT-only dependency and is never linked into the shipped editor
or its portable core. No third-party binary corpus or proprietary HIEW fixture
is used. Existing release-notice work must account for actual distributed
components if development tools are ever packaged.

## Inspected source provenance

These links identify the source revisions reported by the corresponding
published packages. The shipped license filenames in the tables above refer to
the package root, which can differ from the repository root.

| Package | Reported upstream revision |
| --- | --- |
| `ratatui 0.30.2`, `ratatui-core 0.1.2`, `ratatui-crossterm 0.1.2`, `ratatui-widgets 0.3.2` | [e665c36cb14752a61cd777fbd06dbef8474f2add](https://github.com/ratatui/ratatui/tree/e665c36cb14752a61cd777fbd06dbef8474f2add) |
| `crossterm 0.29.0` | [36d95b26a26e64b0f8c12edfe11f410a6d56a812](https://github.com/crossterm-rs/crossterm/tree/36d95b26a26e64b0f8c12edfe11f410a6d56a812) |
| `lru 0.18.5` | [f1e972197053a6814e77b77afacb51f03fc03170](https://github.com/jeromefroe/lru-rs/tree/f1e972197053a6814e77b77afacb51f03fc03170) |
| `tempfile 3.23.0` | [fe9f4a305b785abd1f4f912c8304bcc90024f695](https://github.com/Stebalien/tempfile/tree/fe9f4a305b785abd1f4f912c8304bcc90024f695) |
| `same-file 1.0.6` | [5799cd323b8eefd17a089c950dac113f66c89c9e](https://github.com/BurntSushi/same-file/tree/5799cd323b8eefd17a089c950dac113f66c89c9e) |
| `libc 0.2.191` | [ccc2ff4aaab962f7fde7b8edda7379c045b1ca12](https://github.com/rust-lang/libc/tree/ccc2ff4aaab962f7fde7b8edda7379c045b1ca12) |
| `winapi-util 0.1.11` | [803874c57dc1f10ecd42f7c86d9de72f53818432](https://github.com/BurntSushi/winapi-util/tree/803874c57dc1f10ecd42f7c86d9de72f53818432) |
| `winconsole 0.11.1` | [1b8304d1486c0e827aa493d16f54dffc41700b7c](https://github.com/omarkmu/winconsole/tree/1b8304d1486c0e827aa493d16f54dffc41700b7c) |
| `rgb 0.8.53` | [f453adedbbd1a672c654239b0eb0576de606d7d2](https://github.com/kornelski/rust-rgb/tree/f453adedbbd1a672c654239b0eb0576de606d7d2) |
| `bytemuck 1.25.2` | [f363643e951a7ac9e4b9921de982f2b0918902e3](https://github.com/Lokathor/bytemuck/tree/f363643e951a7ac9e4b9921de982f2b0918902e3) |
| `serde 1.0.229` | [7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8](https://github.com/serde-rs/serde/tree/7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8), package path `serde` |
| `serde_json 1.0.154` | [20572a6d066958542d79cdd6cf1a7cd9f0c8e1f8](https://github.com/serde-rs/json/tree/20572a6d066958542d79cdd6cf1a7cd9f0c8e1f8) |
| `iced-x86 1.22.0` | [cae65b1e90451f22c65f580d4acc752c8a486754](https://github.com/icedland/iced/tree/cae65b1e90451f22c65f580d4acc752c8a486754), package path `src/rust/iced-x86` |
| `yaxpeax-arm 0.5.0` | [a2723ae2043777472dd373feef4b9c40d52569bb](https://github.com/iximeow/yaxpeax-arm/tree/a2723ae2043777472dd373feef4b9c40d52569bb) |
| `yaxpeax-arch 0.3.3` | [d369694a22e2b10aca1c497fc31cdc94fa9a6f41](https://github.com/iximeow/yaxpeax-arch/tree/d369694a22e2b10aca1c497fc31cdc94fa9a6f41) |

The two yaxpeax packages report repositories at `git.iximeow.net`; the links
above use the inspected GitHub mirrors at those exact reported commits. The
archive checksums remain the package identity used by Cargo.

## Analysis transitive dependencies

These eight packages are the additional transitive dependencies introduced by
the decoder selection. Their versions are fixed by the lockfile. All listed
license files were inspected in the downloaded packages.

| Locked package and reported upstream revision | Dependency path | Declared license and package files |
| --- | --- | --- |
| [lazy_static 1.5.1](https://github.com/rust-lang-nursery/lazy-static.rs/tree/4c1b9a170c157d679592e2682b13cc780c1db814) | `iced-x86` → `lazy_static` | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE` |
| [bitvec 1.1.1](https://github.com/bitvecto-rs/bitvec/tree/f90d81cac0eb1dfa24be848405d64f3a321b2502) | `yaxpeax-arm` → `bitvec` | `MIT`; `LICENSE.txt` |
| [funty 2.0.0](https://github.com/myrrlyn/funty/tree/25ca048375e389d703a0cefb377e32c4cc0a9dc5) | `bitvec` → `funty` | `MIT`; `LICENSE.txt` |
| [wyz 0.5.1](https://github.com/myrrlyn/wyz/tree/83f076970d745dc1706b46126d5700aa072af031) | `bitvec` → `wyz` | `MIT`; `LICENSE.txt` |
| [tap 1.0.1](https://github.com/myrrlyn/tap/tree/f5315f0f5ca90ce6399daac76c1fe3ba645a4e4e) | `bitvec` and `wyz` → `tap` | `MIT`; `LICENSE.txt` |
| [radium 0.7.0](https://github.com/bitvecto-rs/radium/tree/3f27e0d827338aee919213fd071b99819a1b9fff) | `bitvec` → `radium` | `MIT`; `LICENSE.txt` |
| [num-traits 0.2.19](https://github.com/rust-num/num-traits/tree/7ec3d41d39b28190ec1d42db38021107b3951f3a) | `yaxpeax-arch` → `num-traits` | `MIT OR Apache-2.0`; `LICENSE-MIT`, `LICENSE-APACHE` |
| [autocfg 1.5.1](https://github.com/cuviper/autocfg/tree/2799b09c24e6632f8e653c5cd8fc303e85a906ba) | `num-traits` build dependency | `Apache-2.0 OR MIT`; `LICENSE-MIT`, `LICENSE-APACHE` |

## Transitive license distinctions

Most other locked registry packages declare an MIT option. That does not make
the complete dependency graph MIT-licensed. The metadata inventory identified
the following distinctions, and the listed license texts were inspected:

| Locked package | Declared license | Dependency path and inspected notices |
| --- | --- | --- |
| [foldhash 0.2.0](https://github.com/orlp/foldhash/tree/8f878c636fda9c9e93384824ea45e06d03f009f5) | `Zlib` | Ratatui → `ratatui-core` → `lru` → `hashbrown` → `foldhash`; package `LICENSE` names Orson Peters and retains the Zlib origin, altered-source, and notice conditions |
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
cargo tree --locked -p hiewbie-core
cargo tree --locked -p hiewbie
cargo tree --locked -p xtask
cargo tree --locked -i lru
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
components, including the transitive distinctions above and the iced-x86
repository license that is absent from its package archive. The yaxpeax-arm
license declaration must remain distinguishable from a shipped license file;
an absent file must not be represented as an inspected notice. This inventory and
the repository's own MIT license do not substitute for that release bundle.
