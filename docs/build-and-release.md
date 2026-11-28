# Build and release specification

**Status: build and artifact-evidence foundation implemented; release distribution planned.** A pinned
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
cargo xtask inspect --target <triple> --artifact <path> --commit <full-sha>
cargo xtask validate-evidence --target <triple> --artifact <path> --commit <full-sha> --evidence <json>
```

The Cargo alias itself uses locked resolution. `check` validates the manifest,
formatting, workspace Clippy with denied warnings, workspace tests, and the offline
fuzz corpus replay. `build` selects the declared package, release profile, lockfile,
explicit target, and reviewed `build.environment` values. The macOS rows apply
`MACOSX_DEPLOYMENT_TARGET=11.0`; arbitrary environment injection is rejected.
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

## 7. Runtime baselines and artifact evidence

### Selected release intent

Schema **2** of [build/targets.json](../build/targets.json) selects the following
baselines. Selection is a requirement for future release builds, not a successful
execution claim. Every checked-in target and supplemental artifact remains
`evidence_status: pending`. Platform implementation stories and #98 own the
remaining exact-artifact/minimum-runtime proof.

| Targets | Selected runtime intent | Selected build inputs / remaining qualification |
| --- | --- | --- |
| Both macOS architectures | macOS **11.0** | Rust 1.90.0, Apple clang and macOS SDK; `MACOSX_DEPLOYMENT_TARGET=11.0` is applied by `xtask build` and derived CI settings. Exact Xcode/SDK versions belong to build evidence. |
| Windows x64 MSVC | Windows **10 1809 / build 17763**; Server 2019 is the corresponding server API generation | MSVC tools from Visual Studio 2022 or newer and Windows SDK; record the exact toolset in build evidence. ConPTY interaction, imports and minimum-runtime evidence remain required. |
| Windows ARM64 MSVC | Windows **11 / build 22000** | Native ARM64 MSVC toolchain, matching SDK, ConPTY and minimum-runtime evidence. |
| Linux x64 / ARM64 GNU | Ubuntu **22.04**, kernel **5.15**, glibc **2.35** | Matching GNU development sysroot; exact package/image and linker versions recorded per build. |
| Linux RISC-V GNU | Ubuntu **24.04**, kernel **6.8**, glibc **2.39** | GNU cross-linker and matching RISC-V sysroot remain #92. |
| Both FreeBSD architectures | FreeBSD **14.5-RELEASE** | Matching clang/lld and sysroot. The x64 linked/runtime path remains #93; ARM64 matching Rust compiler/std provenance remains #91. |
| Browser/WASM | Rust 1.90.0 default Wasm feature set plus DOM keyboard and local File/Blob import/download | Named minimum browser versions and a real browser frontend remain #95/#96. Core compilation is not a browser product. |

The [Rust 1.90.0 platform table][Rust targets] gives compiler target floors;
application dependencies and APIs can impose newer requirements. The chosen
macOS floor applies to both universal inputs and follows the documented
[deployment-target/SDK controls][Rust macOS]. The Windows floor accounts for
[ConPTY availability][ConPTY] and the [MSVC toolchain contract][Rust Windows].
The Linux selections use [Ubuntu 22.04 release facts][Ubuntu 22.04], the
[24.04 kernel selection][Ubuntu 24.04 kernel] and its [glibc package][Ubuntu glibc].
FreeBSD 14.5 is a selected release from the [release inventory][FreeBSD releases]
and [support schedule][FreeBSD support]; Rust/std acquisition still follows the
[pinned compiler's FreeBSD rules][Rust FreeBSD].

The browser feature baseline is deliberately more specific than “WebAssembly
1.0”: Rust 1.90.0 defaults include `multivalue`, `mutable-globals`,
`reference-types`, `sign-ext`, `nontrapping-fptoint`, and `bulk-memory`
([pinned target documentation][Rust WASM]). The planned frontend additionally
needs DOM keyboard events and the [File/Blob/slice/read and blob-URL
contracts][File API]. These feature requirements and the unresolved browser
version qualification are stored in the manifest.

Every target records compiler/MSRV, linker/sysroot selection, native-library
requirements, input-provenance obligations, required runtime scenarios and source
links. A pending sysroot digest or runner image does not become a pinned input
merely because its description is present. Build producers record the actual
versions and immutable input identities before claiming artifact evidence.

### Capabilities and download metadata

`cargo xtask targets` validates and emits the one operational target manifest.
CI, packaging and any future download metadata consume that output; no download
is advertised while no release archive exists. Validation preserves all ten
required targets, Windows ZIP / Unix tar.gz formats, and the required universal
macOS archive with both inputs. The specification's required matrix is retained
as a regression contract.

Capability profiles list **requirements**, including unfinished stories. Each ID
is checked against the [shared production registry](capabilities.md), with
decoding, text assembly, references, symbols, extensions and devices independently
represented. Browser native-device unavailability is explicit. The application
reports current implementation availability through `--capabilities [--json]`.
Neither a capability requirement nor a usable compiled-core descriptor proves
that a particular target artifact was built or executed. Per-artifact compiled
and runtime-tested capability IDs belong in the records below; an unavailable
or unknown implementation cannot be promoted by writing its ID into evidence.

### Metadata inspection

After building from an identified source commit, retain the exact executable and
run (replace the placeholders with the actual target, path and full Git SHA):

```sh
cargo xtask inspect --target x86_64-unknown-linux-gnu --artifact target/release/hiewbie --commit FULL_40_CHARACTER_SHA > inspection.json
```

The command hashes the artifact with streaming SHA-256, reads checked binary
headers and invokes an available metadata tool. It **never executes the supplied
artifact** and never calls `ldd`.

| Artifact | Current inspection hook | Scope and limits |
| --- | --- | --- |
| GNU Linux / FreeBSD ELF64 | `readelf` or `llvm-readelf` | Machine/type/OSABI, interpreter, DT_NEEDED and GLIBC version requirements. Linux and FreeBSD cannot qualify each other merely because the CPU matches. Static, sectionless or other-libc cases without recoverable ABI evidence remain unsupported. |
| Thin macOS Mach-O | `otool -l` | Executable kind, CPU, macOS platform/deployment commands and dylib dependencies. iOS/tvOS/watchOS, conflicting deployment commands, and a higher deployment floor cannot qualify. Universal slice combination remains #94. |
| Windows PE32+ | `llvm-readobj --file-headers --coff-imports` | Executable kind, machine, subsystem version and DLL imports. DLLs do not qualify as the CLI. PE metadata cannot itself prove MSVC provenance or minimum Windows API availability. |
| WASM | Header recognition only | Feature/import inspection and a real browser producer remain #96; header recognition does not pass native inspection. |

Tool output is bounded to **2 MiB** and each invocation to **20 seconds**. Errors
and timeouts terminate the child. Missing tools, unsupported metadata and target
mismatch have explicit JSON diagnostics and a failing exit status. An observed
GLIBC or Mach-O deployment requirement above the selected floor also fails.
`compatible-observed-metadata` means only that the inspected ABI fields fit;
`unknown` retains unresolved minimum-runtime questions. Neither result means the
program has run. PE and FreeBSD can have complete metadata inspection with an
unknown minimum-runtime verdict.

The report binds target, declared source commit, artifact size and digest to its
baseline ID, parsed facts and bounded tool output. The artifact is hashed again
after inspection to detect changes. SHA-256 identifies the bytes; the command's
`--commit` argument is a **producer declaration**, not a proof that arbitrary
bytes were compiled from that revision. Build evidence must originate from a
controlled checkout of that exact commit, with its dirty-tree policy and actual
compiler/linker/sysroot inputs recorded in the build log.

### Attaching and validating evidence

An artifact evidence envelope has this schema (all nested records carry the
same full `identity` object):

| Field / record | Required contents |
| --- | --- |
| Envelope | `schema_version: 1`, `identity`, `state`, optional `build`, `inspection`, `runtime` |
| Identity | `target`, full lowercase `source_commit`, lowercase 64-digit `sha256`, exact `size` |
| Build | `identity`, exact `compiler`, executed `command` array, `inputs` with `rustc`, `linker`, `sysroot` provenance, `compiled_capabilities`, `log_ref`, `passed` |
| Inspection | The complete JSON object emitted by `xtask inspect`; do not reconstruct its parsed fields by hand |
| Runtime | `identity`, `baseline_id`, `environment`, completed `checks`, `verified_capabilities`, `minimum_runtime_tested`, `log_ref`, `passed` |
| Runtime environment | `class` (`native`, `virtual-machine`, `emulated`, `browser`), `system`, `machine`, numeric `versions`, exercised `features`, descriptive runner/VM/emulator/browser details |

`state` is one of `proposed`, `built`, `inspected`, `runtime-verified`. Each
successive state requires its corresponding successful results; absence of a
result never becomes success. A newer-runner result can be attached while the
state remains `inspected`, but cannot set `minimum_runtime_tested: true`. That
flag requires every selected version component at the selected major/minor
floor (patch suffixes are allowed) and the selected feature set. Thus running
with glibc 2.40 does not prove the 2.35 minimum. Browser runtime records have an
explicit browser class/version; full promotion remains blocked until #96
supplies the currently missing Wasm inspection hook.

A concrete pending envelope can be produced from an actual inspection without
inventing build or runtime results:

```python
import json
from pathlib import Path

inspection = json.loads(Path("inspection.json").read_text())
report = {
    "schema_version": 1,
    "identity": inspection["identity"],
    "state": "proposed",
    "build": None,
    "inspection": inspection,
    "runtime": None,
}
Path("artifact-evidence.json").write_text(json.dumps(report, indent=2) + "\n")
```

Attach actual build/runtime records from the named producers as available, then:

```sh
cargo xtask validate-evidence --target x86_64-unknown-linux-gnu --artifact target/release/hiewbie --commit FULL_40_CHARACTER_SHA --evidence artifact-evidence.json
```

Validation compares every record with the supplied artifact's actual digest,
size, target and requested commit. It checks the baseline ID, compiler, scenario
coverage, capability claims and minimum-runtime context. Recorded tool arguments
are allowlisted; ABI fields are reparsed from the bounded raw output and checked
against the structured fields and actual binary header. Altering a GLIBC floor
in JSON while retaining different tool output is rejected. Failed, unsupported,
stale or incomplete records cannot qualify a successful state.

This is consistency validation for **trusted producer logs**, not cryptographic
attestation or automatic authentication of a CI URL. It does not execute commands
stored in JSON. The final release gate #98 still checks complete feature coverage,
all exact packaged artifacts, universal inputs, notices, runtime environments and
published inventory. The checked-in manifest stays pending rather than embedding
stale success from one developer machine.

[Rust targets]: https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support.md
[Rust macOS]: https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/apple-darwin.md
[Rust Windows]: https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/windows-msvc.md
[Rust FreeBSD]: https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/freebsd.md
[Rust WASM]: https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/wasm32-unknown-unknown.md
[ConPTY]: https://learn.microsoft.com/en-us/windows/console/createpseudoconsole
[Ubuntu 22.04]: https://discourse.ubuntu.com/t/jammy-jellyfish-release-notes/24668/1
[Ubuntu 24.04 kernel]: https://discourse.ubuntu.com/t/introducing-kernel-6-8-for-the-24-04-noble-numbat-release/41958
[Ubuntu glibc]: https://packages.ubuntu.com/noble/libc6
[FreeBSD releases]: https://www.freebsd.org/releases/
[FreeBSD support]: https://www.freebsd.org/security/
[File API]: https://www.w3.org/TR/FileAPI/

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
