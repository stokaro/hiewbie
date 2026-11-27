# Optional agent tools

[RTK](https://github.com/rtk-ai/rtk) can reduce the command output an AI agent
reads. It is optional contributor tooling, with no role in hiewbie's runtime,
build requirements, or release matrix. Direct command prefixes work without
generating agent configuration files.

The reference version is [RTK v1.2.1](https://github.com/rtk-ai/rtk/releases/tag/v1.2.1),
checked on 2026-11-27. Prefer its Apache-2.0 **OSS edition**, which includes the
command filters used here. Both editions install a binary named `rtk`; see
[upstream editions](https://github.com/rtk-ai/rtk/blob/v1.2.1/docs/guide/resources/editions.md).

## Installation

Check an existing installation before adding another:

```sh
rtk --version
rtk gain --text
```

`gain` identifies the token-filtering project. An unrelated Rust package also
uses the name `rtk`; avoid installing an unqualified crate by that name.

The [upstream installation guide](https://github.com/rtk-ai/rtk/blob/v1.2.1/docs/guide/getting-started/installation.md)
documents these package-manager commands:

| Platform | Command | Edition |
| --- | --- | --- |
| macOS with Homebrew | `brew install rtk-ai/tap/rtk-oss` | OSS |
| Windows with winget | `winget install rtk-ai.rtk` | Standard distribution |

Package managers may install a newer version. For the exact OSS baseline on
Windows, use `rtk-oss-x86_64-pc-windows-msvc.zip` from the v1.2.1 release, verify
its checksum, and put the extracted `rtk.exe` on `PATH`.

For Linux, the pinned prebuilt OSS archives require no Rust compiler:

| Architecture | v1.2.1 archive | SHA-256 |
| --- | --- | --- |
| x86_64 | [rtk-oss-x86_64-unknown-linux-musl.tar.gz](https://github.com/rtk-ai/rtk/releases/download/v1.2.1/rtk-oss-x86_64-unknown-linux-musl.tar.gz) | `44aa460c4827d206d11405c652832c9f7082edccfbc3a9f15a69a851e8a9e8b4` |
| AArch64 | [rtk-oss-aarch64-unknown-linux-musl.tar.gz](https://github.com/rtk-ai/rtk/releases/download/v1.2.1/rtk-oss-aarch64-unknown-linux-musl.tar.gz) | `b01860ae2d3f44e0f7fc4431a95dd66b29b4bd87ec7f5e60aff13681b05db5dd` |

Compare the archive's SHA-256 with the pinned value before extracting it.
The release's [checksums.txt](https://github.com/rtk-ai/rtk/releases/download/v1.2.1/checksums.txt)
also covers macOS and Windows archives. Keep a repository-local binary at
`.hiewbie-local/bin/rtk`; this directory is ignored by Git. Examples below assume
`rtk` is on `PATH`; otherwise use that relative binary path directly.

## Direct use

```sh
rtk git status
rtk git diff
rtk git log -5
```

For exact patch review, use native `git diff` or `rtk bypass git diff`.
`bypass` runs the command without output filtering. Prefer native structured
output for scripts that parse command results.

**Future Cargo examples:** this repository does not yet contain a Cargo
workspace. Once one exists, use its documented checks, optionally prefixed with
RTK:

```sh
rtk cargo check --workspace --locked
rtk cargo test --workspace --locked
```

RTK does not install Cargo, create the workspace, or implement the proposed
`cargo xtask` commands in the [build specification](build-and-release.md).

## Full output and exit status

When filtered output lacks needed details or has inconsistent counts, follow its
recovery hint, such as
`rtk recall HASH --full`, replacing `HASH` with the printed value. Recovery
storage has size and retention limits; preserve a separate raw log for
verification that must remain inspectable. See the
[upstream recall configuration](https://github.com/rtk-ai/rtk/blob/v1.2.1/docs/guide/getting-started/configuration.md#recall-system).

This Bash/POSIX-shell example is for the **future Cargo workspace**. It executes
the check once, retains its raw output, and returns the check's exit status even
when a separate filter succeeds:

```sh
mkdir -p .hiewbie-local/logs
(
    log=.hiewbie-local/logs/cargo-test.log
    if cargo test --workspace --locked >"$log" 2>&1; then
        result=0
    else
        result=$?
    fi
    if command -v rtk >/dev/null 2>&1; then
        rtk pipe --filter cargo-test <"$log" || cat "$log"
    else
        cat "$log"
    fi
    exit "$result"
)
```

Use the raw log for exact failure counts and diagnostics. Do not infer a
successful check from a filtering command's exit status. Recheck
failure propagation when upgrading RTK or adopting another command filter.
A Linux x86_64 smoke check of OSS v1.2.1 confirmed version/help startup and that
both native Git and `rtk git` return exit code 128 with the same diagnostic for a
nonexistent working directory. This checks one failure path; macOS and Windows
runtime behavior has not been tested for this repository.

## Measuring usefulness

Use `rtk gain --text` and `rtk gain --history` to inspect recorded commands.
The stdin-only `rtk pipe` calls above are not recorded in v1.2.1's gain history.
The default history spans projects. RTK estimates command-output tokens from
bytes rather than using the model's tokenizer; an output-reduction percentage
does not establish the same reduction in session tokens, cost, or elapsed time.
See [how RTK savings work](https://github.com/rtk-ai/rtk/blob/v1.2.1/docs/guide/resources/savings-explained.md)
and the [analytics reference](https://github.com/rtk-ai/rtk/blob/v1.2.1/docs/guide/analytics/gain.md).

Keep using bounded searches, focused file reads, and the smallest relevant
checks. Evaluate RTK on representative commands and include any extra raw-output
reads when deciding whether it helps.
