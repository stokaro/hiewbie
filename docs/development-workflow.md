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
python3 scripts/pty_smoke.py --binary target/debug/hiewbie
```

If `CARGO_TARGET_DIR` is set, pass the executable from that directory. The PTY
script uses Python's standard library and generated temporary fixtures. Its
`--record PATH` option records the successful edit workflow as asciinema v2.
The checked-in [recording](evidence/native-editor.cast) is evidence of that
scenario; byte/state assertions and cleanup checks remain the regression gate.

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
