# Development workflow

The compatibility, architecture, release, and reuse specifications remain
authoritative. This guide bounds routine development work without reducing their
acceptance requirements.

## Scope and current commands

Identify changed behavior and requirement IDs. Read existing code and nearby
tests first. Use `rg` and bounded reads; follow dependencies when relevant. Avoid
unrelated refactors, speculative abstractions, and repeated context dumps.

The workspace pins Rust 1.90.0. `cargo xtask check` validates the target manifest
and runs `cargo fmt --all -- --check`, workspace Clippy with warnings denied,
and locked workspace tests. `rust.yml` runs native and compile-only jobs derived
from `build/targets.json`; inspect the identified commit's runs for actual
results. The separate `agent-tooling.yml` workflow runs the CI helper's offline
regressions when that helper or its workflow changes.
For documentation changes, review affected links, requirement references, and
status claims; run `git diff --check` in a Git checkout. Do not create tooling
merely to validate a documentation edit.

Run the same helper checks locally with Python 3.10+:

```sh
python3 -m unittest discover -s .agents/skills/gh-fix-ci/scripts -p 'test_*.py'
```

`cargo xtask build --target TRIPLE` builds the declared application in release
mode with locked dependencies; the WebAssembly row builds only the core.
`cargo xtask targets` prints the validated manifest. `package` and
`verify-release` fail explicitly because release tooling is not implemented.

Run the real Unix terminal scenarios after building the CLI:

```sh
cargo build --locked --package hiewbie
python3 -m unittest discover -s scripts -p 'test_pty_smoke.py' -v
python3 scripts/pty_smoke.py --binary target/debug/hiewbie
```

If `CARGO_TARGET_DIR` is set, pass the executable from that directory. The PTY
script uses Python's standard library and generated temporary fixtures. Its
`--record PATH` option records the successful edit workflow as asciinema v2;
`--record-analysis PATH` records the generated PE header/entry/Code/branch/return
workflow followed by dotted-VA navigation and an exact one-byte save. The
`--record-symbols PATH` option records generated PE import-slot, export-target
and forwarded-export record navigation with exact byte-save assertions.
`--record-search PATH` records the two-field dialog and backward repeat after
independent cursor movement, followed by an exact one-byte save.
`--record-block PATH` records reverse block marking, scoped search, an exact
edit, export cancellation, raw export, and confirmed append. All seven
PTY scenarios run in the existing native Unix CI jobs even without recording
options. The symbol fixture contains no external executable bytes. The
checked-in [editing](evidence/native-editor.cast) and
[analysis](evidence/executable-analysis.cast) recordings, together with the
[symbol navigation](evidence/symbol-navigation.cast),
[search repeat](evidence/search-repeat.cast), and
[block workflow](evidence/block-workflow.cast) recordings, are evidence of those
scenarios. Byte assertions, state assertions, and cleanup checks are the
regression gate.
The PTY harness keeps a supervisor alive as the controlling session leader
while the editor exits. It checks the editor's exact exit status, restored
termios attributes, and terminal cleanup sequences before releasing that
supervisor. Exit actions wait for the editor status even when terminal output
has gone quiet. This preserves an inspectable terminal for macOS as well as Linux;
an unavailable post-exit terminal is never treated as a successful cleanup check.
The lifecycle rule is documented in the inspected
[Darwin session-leader exit implementation](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/bsd/kern/kern_exit.c).
That exit path also waits for terminal output to drain. The parent therefore
keeps consuming master output while reaping the supervisor on both successful
completion and cancellation, with bounded deadlines and cleanup escalation.
Before comparing every termios field, one nonblocking slave read lets Darwin
process its transient `PENDIN` state after returning to canonical input, as
described in the inspected
[terminal read implementation](https://github.com/apple-oss-distributions/xnu/blob/43a90889846e00bfb5cf1d255cdc0a701a1e05a4/bsd/kern/tty.c).
The read rejects any returned input and does not reset, mask, or flush terminal
attributes; the descriptor's original file-status flags are restored afterwards.
The harness regressions use real child processes to check supervisor lifetime,
nonzero/signal exit status, missing raw-mode restoration, unexpected queued input,
and timeout cleanup.

## Select meaningful checks

| Change | Relevant evidence |
| --- | --- |
| Core behavior | Focused unit or command scenarios; a defect-reproducing regression for fixes |
| Editing or storage | Expected bytes and state; applicable discard, undo, failed-save, external-change, boundary, and large-offset cases |
| Formats or instructions | Redistributable fixtures, malformed inputs, address mapping, raw fallback; relative-address and patch-span assembly cases |
| Commands, keys, rendering | Headless scenarios, deterministic cell snapshots, appropriate PTY/ConPTY cases, terminal cleanup |
| Browser adapter | Actual browser import, navigation, edit, export, and key translation |
| Dependencies or toolchains | Affected consumers, supported feature combinations, license review, implicated target checks |
| Packaging or release | Artifact inspection and launch/edit/save evidence; mandatory release gates |

Select concrete scenarios from `compatibility.md`. Mode/keymap changes need
terminal snapshots. Editor workflow changes need the specified screenshot or recording;
manual evidence supplements automated regression coverage. Assert observable
behavior, not implementation structure.

Use the repository's formatter, linter, focused tests, and
required gates with its pinned toolchain and locked dependency resolution. Do not
assume package names or blindly enable all features. Tests provide compilation
evidence only for the code and configuration they build; additional checks need
a configuration, target, or gate to justify them.

Bound fuzzing and performance experiments to a stated purpose, inputs, limits,
and environment. Expand checks for concrete remaining risks. Rerun affected checks
after changes; stop optional repetition once evidence is sufficient. A timeout,
missing runner, or skipped mandatory check is never a pass.

## Sources and tooling

Use repository code, manifests, tests, and pinned references first. Consult
upstream primary sources for unresolved API, capability, or license decisions.
Distinguish observed HIEW behavior, upstream capability, planned requirements,
implementation, and verification. HIEW comparisons need the reference version,
terminal, fixture, and date.
The [Hiew 8.63 scenario inventory](reference/hiew-8.63.md) identifies documented
expectations and original-side observations still needed. Cite its scenario ID
for affected commands; passing hiewbie tests does not promote a documented
expectation into observed reference behavior. Do not retry an environment-blocked
reference launch as part of routine code checks.

Record reused material's repository, path, exact commit, license, retained notice,
destination, and changes. Record fixture provenance. GridCraft's MIT option does
not license `craft-common-actions`. Keystone is not MIT; iced-x86's assembler API
is not a textual parser. Native engine licenses require separate review. See
`references-and-reuse.md` for the complete policy.

RTK is optional developer tooling, not an application dependency. See
[agent tools](agent-tools.md) for setup, raw-log retention, and exit-status checks.
Preserve complete diagnostics when reducing output.

## Completion

Report changed behavior, requirements, actual checks and results, and unverified
capabilities. Distinguish native, VM, emulated, cross-built-only, and browser
evidence. Focused checks do not reduce the full release matrix, runtime evidence,
checksums, notices, or published-asset verification required for release claims.
