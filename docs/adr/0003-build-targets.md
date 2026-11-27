# ADR 0003: Portable build tooling and explicit target evidence

Status: accepted for the native editor bootstrap, 2026-11-27.

## Context

M0 requires a working workspace and native CI, plus a build strategy or explicit
blocker for every target in [the build contract](../build-and-release.md).
Compilation, native execution, emulation, browser operation, packaging, and
release completeness are separate claims. No target may disappear because a
standard library, linker, runner, or adapter is missing.

## Decision

Use Rust **1.90.0**, edition **2024**, with the exact compiler version pinned in
`rust-toolchain.toml` and application dependencies locked in `Cargo.lock`.
The workspace's declared minimum Rust version matches the pinned compiler;
older compilers are not supported by this bootstrap.

`build/targets.json` is the machine-readable source of truth for required Rust
triples, planned artifact names, package scope, CI strategy, runtime plans, and
release blockers. The additional macOS universal archive references both macOS
rows; it is an artifact requirement, not an eleventh Rust target. The workflow
derives its matrices from the manifest instead of maintaining another list.
The manifest records configuration and remaining work, not successful builds.
CI logs identify actual evidence for each commit.

The portable `xtask` binary provides these commands:

| Command | Implemented behavior |
| --- | --- |
| `cargo xtask check` | Validate the target manifest; run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --locked`; stop on the first failure. |
| `cargo xtask build --target <triple>` | Select a declared target and build its application or portable core using `--release --locked --target`; report prerequisites and fail if compilation/linking fails. |
| `cargo xtask targets` | Validate and print the full JSON manifest. |
| `cargo xtask package ...` | Fail explicitly because packaging is not implemented. |
| `cargo xtask verify-release ...` | Fail explicitly because release verification is not implemented. |

The Cargo alias itself uses `--locked`. `xtask` invokes Cargo directly through
Rust process APIs, sets the workspace directory, preserves diagnostics, and
propagates failures. It requires no particular shell or Python installation.
The existing Python agent-tooling regression job remains a separate gate.
The small CI matrix reader and Unix PTY harness are CI/test adapters, not build
or release orchestration dependencies.

## Dependency boundary

The shared `hiewbie-core` crate uses the standard library without terminal or
native filesystem dependencies. The `hiewbie` crate owns native storage and
terminal APIs using Ratatui, Crossterm, and tempfile. `xtask` uses Serde and
serde_json for the target manifest.

This bootstrap does not add an embedded C/C++ disassembly or assembly engine,
or require building bundled C/C++ sources. Native binaries still depend on the
platform linker, system runtime, and operating-system APIs. Windows requires
the MSVC build tools and SDK; Unix targets require the applicable system linker
and runtime libraries. A future engine must be assessed against every target
and its own license before introduction. Rust wrappers alone do not resolve
native-engine portability or licensing.

## Target feasibility at the pinned compiler

The following summarizes the strategies in the manifest. These are configured
checks and implementation plans, not assertions that every job has executed.
Rust tier information is from the **1.90.0** source tag, not current stable docs.

| Required target | Rust 1.90.0 distribution | Bootstrap strategy | Remaining evidence |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | Tier 1 host tools and std | Native `macos-15`; workspace gates, help/version, Unix PTY smoke | Release packaging, supported minimum OS, complete terminal scenarios |
| `x86_64-apple-darwin` | Tier 2 host tools and std | Native `macos-15-intel`; same gates | Intel minimum OS, packaging, universal combination |
| `x86_64-pc-windows-msvc` | Tier 1 host tools and std | Native `windows-2022`; workspace gates and help/version | ConPTY interaction/cleanup and Windows/console baselines |
| `aarch64-pc-windows-msvc` | Tier 2 host tools and std | Native `windows-11-arm`; require an ARM64 Rust host | ConPTY interaction/cleanup, packaging, supported Windows baseline |
| `x86_64-unknown-linux-gnu` | Tier 1 host tools and std | Native `ubuntu-22.04`; workspace gates, help/version, Unix PTY smoke | Actual linked ABI floor, packaging, minimum-runtime testing |
| `aarch64-unknown-linux-gnu` | Tier 1 host tools and std | Native `ubuntu-22.04-arm`; same gates | Actual linked ABI floor, packaging, minimum-runtime testing |
| `riscv64gc-unknown-linux-gnu` | Tier 2 host tools and std | `cargo check` of the native app from Linux x86_64 | Pinned GNU linker/sysroot, linked ELF inspection, hardware or named emulation runtime tests |
| `x86_64-unknown-freebsd` | Tier 2 host tools and std | `cargo check` of the native app from Linux x86_64 | Pinned FreeBSD VM/sysroot, linked ELF inspection, guest runtime/PTY tests |
| `aarch64-unknown-freebsd` | Tier 3; std is not distributed by rustup 1.90.0 | Explicitly blocked; `xtask build` fails with the recorded reason | Matching compiler/std, verified sysroot and linker, ARM64 guest/hardware execution |
| `wasm32-unknown-unknown` | Tier 2 std, no host tools | Compile **only `hiewbie-core`** | Browser terminal/storage adapter, browser import/edit/export tests, usable static-site archive |

GitHub's official runner documentation lists the selected x64 and ARM64 labels.
Every native job installs the pinned toolchain for its expected host and checks
`rustc --version --verbose` before tests. This prevents an x64 compiler running
under Windows ARM64 emulation from being accepted as native ARM64 evidence.
Each job has a bounded timeout; older runs for the same ref are cancelled.
Workflow permissions are read-only, and checkout is pinned to an exact revision.

### Linked cross-build paths

`cargo check` exercises target-specific Rust code without a target linker. It
does not create a release executable and cannot validate system ABI or runtime
behavior. The RISC-V and FreeBSD x64 rows retain those release blockers.

For RISC-V, install the standard library with
`rustup target add riscv64gc-unknown-linux-gnu`, supply a GNU RISC-V linker and
matching libc sysroot, and set
`CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_LINKER` to that linker before running
`cargo xtask build --target riscv64gc-unknown-linux-gnu`. Toolchain/sysroot
versions and checksums must be fixed before this becomes release automation.
Inspect the resulting ELF machine and linked ABI, then run the editor and PTY
scenarios on hardware or in a named QEMU environment.

For FreeBSD x64, the straightforward linked-build path is a pinned FreeBSD VM
with the 1.90.0 host toolchain and system linker, running
`cargo xtask build --target x86_64-unknown-freebsd`. A cross-build is also
possible with a matching sysroot and linker. Guest/image provenance, supported
runtime release, and runtime/PTY checks must be fixed before claiming support.

### FreeBSD ARM64 blocker

At Rust 1.90.0, `aarch64-unknown-freebsd` is Tier 3. That release's FreeBSD
documentation lists only x86_64 and i686 artifacts as rustup-distributed.
Running `rustup target add aarch64-unknown-freebsd` is therefore not a build
strategy for this pin. Newer stable documentation may describe a different
support tier; it must not be used as evidence for the pinned release.

The retained implementation plan is to obtain an exact matching compiler and
ARM64 standard library from a pinned FreeBSD ports/source build, pin the guest
and ARM64 sysroot provenance with checksums, and link with clang/lld. The target
must reach Cargo and any future native backend build. Architecture inspection
must read the resulting ELF, never infer its machine from the x86_64 build host.
An ARM64 FreeBSD hardware, VM, or emulator environment must then exercise the
same document/command and terminal scenarios. A separately reviewed Rust
upgrade may simplify the standard-library acquisition, but does not supply the
missing linker, sysroot, runtime evidence, or packaging automatically.

Until these inputs are resolved, the manifest retains the required row as
blocked and no placeholder success job is created.

### WebAssembly scope

`cargo xtask build --target wasm32-unknown-unknown` builds the portable core as a
Rust library. It creates no browser UI or static-site archive. The native app
is deliberately excluded because its terminal and file APIs require a host OS.
The Rust standard library on this target does not supply ordinary native
filesystem or thread behavior. The browser adapter must implement import,
export, keyboard translation, and terminal display around the shared command
and document semantics, then be tested in a real browser.

## Runtime baselines and release status

Runner versions are development/test environments, not minimum supported OS
claims. No minimum macOS, Windows, Linux/glibc, FreeBSD, or browser version is
established by this ADR. Release work must inspect linked ABI requirements and
exercise the stated minimums. In particular, the Rust compiler's target floor
does not prove the application's dependency/runtime floor.

Packaging, SHA-256 generation, executable architecture inspection, dependency
notices, signing state, release completeness, and published-asset verification
remain required work. The full matrix and macOS universal archive remain
mandatory for a release claiming complete distribution support.

## Sources

- [Rust 1.90.0 platform support](https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support.md).
- [Rust 1.90.0 FreeBSD support](https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/freebsd.md).
- [Rust 1.90.0 RISC-V support](https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/riscv64gc-unknown-linux-gnu.md).
- [Rust 1.90.0 WebAssembly support](https://github.com/rust-lang/rust/blob/1.90.0/src/doc/rustc/src/platform-support/wasm32-unknown-unknown.md).
- [GitHub-hosted runner names and architectures](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), inspected 2026-11-27.

The implementation and workflow are independently written. No ArtCraft build
scripts or unlicensed shared-workflow code were copied.
