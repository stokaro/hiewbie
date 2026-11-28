# Working on hiewbie

`hiewbie` is an independent, MIT-licensed Rust implementation of console HIEW.
The workspace contains a portable byte/format/instruction core, a native terminal
editor, and `xtask`. Read `docs/compatibility-status.md` for implemented scope and
remaining requirements. Do not present planned commands, features, targets, or
releases as available or verified.

## Invariants

- Preserve HIEW's command model, default keys, modes, layout, and workflow.
  Keep improvements additive; make familiar-behavior changes explicit and
  configurable.
- Preserve native macOS support and the complete required release matrix.
  Keep host targets, file formats, and instruction architectures independent.
  Never silently drop legacy parity, targets, or acceptance requirements because
  a dependency lacks support; record the gap and implementation plan.
- Share document and command semantics across native, browser, and headless
  frontends. Isolate platform APIs. Start with small modules; split crates only
  for an actual platform, dependency, testing, or compilation boundary.
- Treat inspected files as untrusted. Use explicit address/offset types, checked
  arithmetic, bounded work and allocations, and precise errors without panics.
  Preserve raw-byte access when interpretation fails and untouched bytes on save.
- Keep original code MIT-licensed. Review exact source revisions, file-specific
  licenses, and native engines behind wrappers before reuse. Record provenance
  and retain required source and distribution notices. Do not copy proprietary
  HIEW code, binaries, graphics, or manual text.

## Read the affected contract

The specifications remain authoritative. Read relevant sections before changing
their subject; expand for dependencies or uncertainty. Reuse context already read.

| Change | Required context |
| --- | --- |
| Wording or links only | Edited document and affected references |
| Document model, commands, storage, persistence | `docs/architecture.md` sections 2–4; affected `docs/compatibility.md` requirements |
| Modes, keys, rendering, terminal lifecycle | `docs/compatibility.md` sections 2, 8, 11; `docs/architecture.md` section 6 |
| Formats, addresses, decoding, assembly | `docs/compatibility.md` sections 4–6; `docs/architecture.md` section 5 |
| Dependencies, upstream reuse, fixtures | `docs/references-and-reuse.md`; affected architecture and target requirements |
| Workspace, toolchain, CI, packaging, release | Affected `docs/build-and-release.md` sections and its complete required target matrix |

Update affected contracts, rationale, references, and acceptance criteria when a
decision changes documented behavior. See `docs/development-workflow.md` for check
selection and command status.

For HIEW command changes, read only the relevant scenarios in
`docs/reference/hiew-8.63.md`. Its author-documentation evidence and hiewbie tests
are distinct from original runtime observations; the recorded launch was blocked.
Keep unresolved search, Text, navigation and contextual-key differences explicit.

## Select skills by task

- Nontrivial Rust implementation or review: [rust-best-practices](.agents/skills/rust-best-practices/SKILL.md).
- Cause unclear after initial inspection, nondeterminism, or failed fixes: [systematic-debugging](.agents/skills/systematic-debugging/SKILL.md).
- Failing GitHub Actions checks: [gh-fix-ci](.agents/skills/gh-fix-ci/SKILL.md).

Load only relevant guidance and references. Reuse equivalent guidance already
loaded instead of loading duplicate personal and project copies.

## Verify and report

Automate recurring validation. Behavioral fixes need a regression reproducing
the defect; edit tests assert expected bytes and transaction state. Fixtures must
be generated or redistributable, with provenance. Manual checks do not replace
regression coverage. Run focused checks plus mandatory repository gates.

Report affected requirement IDs, checks actually run, and remaining gaps.
Distinguish compilation, emulation, VM, native, and browser evidence. Missing
artifacts and skipped mandatory checks remain incomplete. Full parity and epic
completion require evidence for the required feature and platform matrices.

Follow `.github/pull_request_template.md`: include a before/after diagram for
workflow, architecture, or data-flow changes; otherwise use a compact table and
explain why a diagram adds no information.
