# Build and release specification

**Status: build foundation implemented; release distribution planned.** A pinned
Rust workspace, native editor, `xtask`, and matrix-derived CI are present.
[build/targets.json](../build/targets.json) records each required target's
configured strategy and remaining blockers. Actual verification belongs to the
identified commit's CI results. No release archives or full-platform claims are
made by this configuration.

## 1. Objective and reference baseline

hiewbie must provide a native terminal application across the operating systems and
CPU architectures shipped by the ArtCraft Crafting Apps, with macOS as a primary
development and usability target. Platform coverage is an explicit deliverable.

The inspected reference is GridCraft, an independent Rust application in ArtCraft's
`storytold` organization. Its release workflow delegates to `craft-common-actions`.
The matrix below is supported by both workflow source and published release assets.

Reference snapshot inspected on **2026-11-27**:

- GridCraft commit: `97f2754c1ddf4caddff865015563d7f342b34c52`.
- Shared workflows commit: `3fe7aa009c15e57932497b5e6d00bf20f781e5ec`.
- Published release evidence: GridCraft `v0.19.1`, published 2026-11-12.
- GridCraft's caller inherits the Web, RISC-V and FreeBSD release targets.
- macOS universal CLI archives are already a reference-suite artifact.

These references define the initial coverage baseline, not a dependency on ArtCraft's
services or a promise that changes to upstream `main` are automatically adopted.

## 2. Host platforms and binary-analysis architectures

Keep two separate compatibility matrices:

1. **Host/build target:** where the hiewbie executable or WebAssembly application runs.
2. **Analysis target:** the architecture, instruction mode and executable format of
   the file being inspected, disassembled, assembled or edited.

An ARM64 Mac must be able to inspect an x86 Windows executable. An x86 Linux build
must be able to inspect ARM64 Mach-O data. Do not infer analysis support from the host,
or claim an assembler backend supports every architecture handled by its disassembler.
Backend capabilities must be reported and tested independently for each build target.

## 3. Required build matrix

All rows remain required. ArtCraft arrangements describe the reference;
hiewbie uses its own runners while preserving the complete target contract.
The state column records configured checks, not successful releases.

| Host platform | Rust target | Reference build arrangement | hiewbie state |
| --- | --- | --- | --- |
| macOS, Apple Silicon | `aarch64-apple-darwin` | macOS build; combined into universal binary | Native tests and Unix PTY configured |
| macOS, Intel | `x86_64-apple-darwin` | macOS build; combined into universal binary | Native tests and Unix PTY configured |
| Windows, x64 | `x86_64-pc-windows-msvc` | Native Windows x64 runner | Native tests/CLI configured; ConPTY pending |
| Windows, ARM64 | `aarch64-pc-windows-msvc` | Native Windows ARM64 release runner | Native tests/CLI configured; ConPTY pending |
| Linux, x86_64 | `x86_64-unknown-linux-gnu` | Native Ubuntu 22.04 runner | Native tests and Unix PTY configured |
| Linux, ARM64 | `aarch64-unknown-linux-gnu` | Native Ubuntu 22.04 ARM runner | Native tests and Unix PTY configured |
| Linux, RISC-V 64 | `riscv64gc-unknown-linux-gnu` | Cross-build on Ubuntu 24.04 | Compile check configured; linking/runtime pending |
| FreeBSD, x86_64 | `x86_64-unknown-freebsd` | FreeBSD VM on a Linux runner | Compile check configured; VM/linking/runtime pending |
| FreeBSD, ARM64 | `aarch64-unknown-freebsd` | Cross-build inside an x86_64 FreeBSD VM | Blocked under pinned Rust 1.90.0 |
| Browser/WebAssembly | `wasm32-unknown-unknown` | Web build and static-site archive | Core compile check configured; browser product pending |

The GNU Linux triples describe the native reference runners; their native packaging
commands do not explicitly pass `--target`. hiewbie must record target triples
explicitly in its own build manifest and artifact metadata.

Provide both macOS architecture artifacts and a universal CLI artifact. Universal
packaging combines the two native binaries; it is not a separate Rust target.
Additional targets such as Linux musl may be added without substituting for this matrix.

## 4. Portable orchestration and repository structure

Use a Cargo workspace with independently testable document/editor, format and machine-
code components, thin terminal/platform adapters, and a Rust `xtask` command package.
Keep build orchestration portable across Windows, macOS and Unix environments.

- Put filesystem operations, manifests, archive assembly and checksums in Rust tooling.
- Use small platform scripts only where an external platform tool requires them.
- Provide a single documented entry point for checks, target builds and packaging.
- Commit the application lockfile and use locked dependency resolution in CI/releases.
- Pin the Rust toolchain and document the minimum supported Rust version.
- Record native library/toolchain requirements per backend and per target.
- Select dependencies with the complete host matrix and WebAssembly boundary in mind.
- Separate compilation, packaging, signing, validation and publication steps.
- Do not make ordinary local builds depend on signing credentials or hosted services.

Implemented command surface:

```text
cargo xtask check
cargo xtask build --target <rust-triple>
cargo xtask targets
```

The Cargo alias itself uses locked resolution. `check` validates the manifest,
formatting, workspace Clippy with denied warnings, and workspace tests. `build`
selects the declared package, release profile, lockfile, and explicit target.
The WebAssembly target selects `hiewbie-core` only and emits a limitation notice.
Toolchain and MSRV are 1.90.0 / 1.90; see [ADR 0003](adr/0003-build-targets.md).

`cargo xtask package --target TRIPLE` and
`cargo xtask verify-release --manifest PATH` deliberately return failure until
packaging and release verification exist. They cannot produce a false pass.

The manifest should be the shared source of truth for target names, artifact names,
build expectations and release completeness checks. Do not duplicate divergent lists
across workflows, packaging scripts and download documentation.

## 5. Compilation and runtime validation

A successful cross-compilation proves that an artifact builds, not that it runs.
Each target must separately record compilation and runtime-validation results.

| Validation class | Required evidence |
| --- | --- |
| Native | Run the executable and applicable tests on the named OS/CPU |
| Virtual machine | Run inside the stated guest OS; record guest architecture/version |
| Emulated | Run under a named emulator; retain that distinction in results |
| Cross-built only | Inspect executable format/architecture; mark runtime unverified |
| Browser | Exercise the shipped WASM application in a real browser test harness |

Native desktop coverage must include startup, version/help, terminal setup and cleanup,
opening a fixture, navigation, editing a copy, saving and reopening the saved result.
Automate terminal interactions using PTYs on Unix and appropriate Windows console
support. Test resize, Unicode paths, keyboard modifiers and restoration after errors.
Test both macOS architectures, rather than treating `lipo -info` as a runtime test.

Cross-built RISC-V and FreeBSD ARM64 artifacts need explicit runtime-validation plans
using suitable hardware, VMs or emulation. Until executed, report the limitation in
the manifest. Never turn a missing runner into a passing runtime check.
Format/architecture inspection must reject a host binary packaged under a target name.

## 6. FreeBSD ARM64 toolchain investigation

For hiewbie's pinned **Rust 1.90.0**, `aarch64-unknown-freebsd` does not have a
rustup-distributed standard library. This is a version-specific blocker;
current stable documentation may describe a newer support level. A workflow
that only runs `rustup target add` under this pin is not an implementation plan.
See the exact Rust source references and alternatives in
[ADR 0003](adr/0003-build-targets.md), including a separately reviewed toolchain
upgrade as an alternative to a pinned FreeBSD ports compiler/std route.

The reference cross-builds in an x86_64 FreeBSD VM. It obtains matching FreeBSD
`lang/rust` compiler and ARM64 standard-library packages, supplies an ARM64 sysroot,
and uses clang/lld for target linking. Investigate and document an equivalent supported
route for hiewbie, preferably with fewer native dependencies than the GUI reference.

- Verify compiler and target standard-library versions match exactly.
- Pin the guest release and sysroot provenance, and verify downloaded inputs.
- Propagate the target into Cargo, C/C++ backend builds and linker configuration.
- Derive artifact architecture from the declared target, never the build host's `uname`.
- Test architecture inspection and produce a runnable ARM64 smoke-test plan.
- Record any unsupported backend explicitly instead of silently omitting functionality.

## 7. Runtime baselines

Choose and document minimum OS/runtime versions before publishing compatibility claims.
The following are reference facts and starting points, not verified hiewbie minimums:

| Platform | Observed ArtCraft baseline | hiewbie requirement |
| --- | --- | --- |
| macOS | GridCraft packaging sets deployment target 11.0 | Decide and test minimum OS for both architectures |
| Linux x86_64/ARM64 | Ubuntu 22.04 build runners | Inspect linked ABI requirements and test an explicit minimum |
| Linux RISC-V | Ubuntu 24.04; shared workflow sets glibc floor 2.39 | State actual glibc requirement or verify an improved baseline |
| Windows | x64/ARM64 release runners, ARM64 verification on Windows 11 | Define supported OS and console/terminal combinations |
| FreeBSD | Shared workflow defaults to 15.1-STABLE at inspected commit | Pin a supported guest/runtime baseline and test it |
| Browser | WebAssembly application | Define browser/file/terminal capabilities and automated coverage |

Inspect linked libraries and packaging metadata for consistency with each declared
baseline. A package dependency field alone is not evidence of runtime compatibility.

## 8. Terminal-first release artifacts

The primary deliverable is a usable terminal command, without a GUI runtime prerequisite.
Package the executable, license information, concise usage instructions and any required
runtime components. Document installation onto `PATH` and clean uninstallation.

- macOS: architecture-specific portable archives plus a universal CLI archive.
- Windows: x64 and ARM64 portable ZIPs; installer/package-manager support may follow.
- Linux: portable archives for x86_64, ARM64 and RISC-V; deb/rpm packaging as appropriate.
- FreeBSD: separate x86_64 and ARM64 portable archives with runtime requirements.
- Web: a self-contained static-site archive with the usable browser terminal adapter.

ArtCraft's DMGs, desktop launchers, GUI font bundles and application menus need not be
copied into a terminal editor. Preserve OS/CPU coverage while adapting packaging to
the actual application. Optional signing must report its real state; unsigned builds
must remain buildable without credentials and must never be described as signed.

## 9. WebAssembly deliverable

Web support must produce a usable browser application, not merely a successful core
library compilation. Implement a browser terminal adapter around the same command and
document model, with keyboard navigation, local-file import, editing and file export.
Keep native terminal/file APIs behind adapters so they do not leak into the portable core.

Document browser capability limits explicitly: native raw-device access and arbitrary
filesystem access cannot be assumed. Confirm which disassembly/assembly backends work
in WASM, and test that advertised operations preserve the expected semantics.
The native terminal application remains the primary experience on desktop systems.

## 10. Delivery phases and release gates

Implement the native macOS experience early, alongside shared-core and desktop CI.
Extend the same system to the full native matrix and the browser adapter in tracked
implementation phases. Sequencing does not remove any row from the required matrix.
Early alpha releases may identify incomplete coverage; they must not claim full parity.

Before a release claiming the complete platform matrix is published:

- Build every required target from the same identified commit and locked dependency set.
- Validate artifact names, executable formats, architecture and version metadata.
- Record runtime evidence and any approved qualification for each target separately.
- Produce and verify SHA-256 checksums for every distributed artifact.
- Include hiewbie's MIT license and all required dependency/upstream notices.
- Fail completeness checks if an expected artifact or required validation is absent.
- Treat skipped, cancelled or unavailable jobs as incomplete, never implicit success.
- Verify the published asset list against the manifest after upload.

## 11. Reuse and source references

GridCraft declares `MIT OR Apache-2.0` and supplies an MIT license. Its own applicable
code/scripts may be adapted under the MIT option while retaining required notices.
At the inspected snapshot, `craft-common-actions` has no top-level license file and
GitHub reports no detected license. Do not assume an MIT grant for copying that code.
Write independent orchestration or reuse explicitly licensed app-local material.
**No upstream ArtCraft code has been copied.** The workspace, `xtask`, and CI
are independently written implementations of this target contract.

- [GridCraft workspace and license declaration](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/Cargo.toml)
- [GridCraft MIT license](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/LICENSE-MIT)
- [GridCraft release caller](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/.github/workflows/release.yml)
- [Shared release matrix and toolchain reference](https://github.com/storytold/craft-common-actions/blob/3fe7aa009c15e57932497b5e6d00bf20f781e5ec/.github/workflows/release.yml)
- [Published GridCraft v0.19.1 artifacts](https://github.com/storytold/gridcraft/releases/tag/v0.19.1)
- [GridCraft macOS packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/macos/package.sh)
- [GridCraft Linux packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/linux/package.sh)
- [GridCraft FreeBSD packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/freebsd/package.sh)
- [GridCraft Web packaging](https://github.com/storytold/gridcraft/blob/97f2754c1ddf4caddff865015563d7f342b34c52/packaging/web/package.sh)
- [Shared workflow repository at the inspected commit](https://github.com/storytold/craft-common-actions/tree/3fe7aa009c15e57932497b5e6d00bf20f781e5ec)
