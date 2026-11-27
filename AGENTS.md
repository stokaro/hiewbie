# Working on hiewbie

## Project contract

The repository and application name is `hiewbie`, in lowercase. The project is a
Rust implementation of the console HIEW workflow, licensed under MIT. Native
macOS use and the complete ArtCraft-derived release matrix are requirements.
Read `docs/compatibility.md`, `docs/architecture.md`, and
`docs/build-and-release.md` before choosing implementation shortcuts.

This repository is initially a specification bootstrap. Do not describe planned
features or a proposed command as implemented, and do not create release badges
or installation claims for artifacts that do not exist.

## Implementation priorities

1. Preserve the HIEW command model, default bindings, modes, layout, and workflow.
   Improvements should be additive and configurable when they change familiar
   behavior.
2. Treat host targets, executable formats, and instruction architectures as
   separate capability matrices. A native backend must not restrict analysis to
   its own CPU architecture.
3. Keep document operations and commands independent of terminal, browser, and
   operating-system APIs. Frontends dispatch the same commands.
4. Start with the smallest useful modules; split crates only where an actual
   platform, dependency, test, or compilation boundary justifies it.
5. Use explicit types and checked arithmetic for file offsets, addresses, sizes,
   and slices. User-controlled files and lengths must not cause panics or
   unbounded allocations.
6. Preserve raw-byte access when format parsing or an instruction backend cannot
   handle a file. Report unsupported capabilities precisely.
7. Never silently drop legacy parity work, a release target, an instruction
   architecture, or an acceptance requirement because a selected dependency does
   not support it. Record the gap and the implementation plan.

## Licensing and reuse

- Keep original hiewbie source MIT-licensed.
- Before copying or substantially adapting upstream code, inspect the license at
  the exact source commit, check file-specific notices, and record provenance.
- Preserve upstream copyright and license notices in copied source and release
  distributions where required.
- ArtCraft is not one license grant. GridCraft offers an MIT option; the inspected
  `craft-common-actions` repository did not expose a license grant. Use the latter
  as platform evidence, not as an assumed MIT source donor.
- Do not copy HIEW proprietary code, binaries, graphics, or manual text.
- Inspect complete dependency licenses, including native libraries behind Rust
  wrappers. In particular, Keystone is not MIT, and iced-x86's code-assembler API
  is not a textual assembly parser.

## Verification

- Automate recurring validation. A manual screenshot or interactive check may
  discover a problem, but does not replace regression coverage.
- For behavioral fixes, add a regression test reproducing the actual defect.
  Avoid tests that merely duplicate implementation structure.
- Test editing against expected file bytes, not only UI state. Cover discard,
  failed saves, boundaries, large offsets, and relative instruction encoding.
- Run command scenarios headlessly and through PTY/ConPTY where appropriate;
  include rendered terminal snapshots for mode and keymap changes.
- Maintain redistributable fixtures and document their origin. Do not commit
  proprietary executable samples or the original HIEW application.
- When the workspace exists, use the documented formatter, linter, test, and
  packaging commands. Clearly state any unrun platform tests.
- Compile-only, emulated, VM-tested, and native-tested are different statuses.
  Do not convert a missing artifact, skipped target, or failed mandatory check
  into a successful release.

## Pull requests

Explain the user-visible problem, changed behavior, requirement IDs, and relevant
validation. Include a simple before/after diagram for a changed workflow,
architecture, or data flow. For a change without meaningful flow, use a compact
before/after table and explain why a diagram would add no information.

State remaining compatibility gaps precisely. Do not mark the implementation
epic complete until its required platform and feature matrices have evidence.
Update the specification when an implementation decision changes a documented
contract; retain the rationale and references.
