# Third-party notices

hiewbie's original application code and documentation remain MIT-licensed.

## Application and workspace dependencies

The initial native application uses the locked registry packages below. These
licenses apply to the dependencies themselves. Exact source revisions, purposes,
inspected license files, and the native/core boundary are recorded in the
[dependency inventory](docs/dependencies.md).

| Component | Locked version | Published license options |
| --- | --- | --- |
| Ratatui | `0.29.0` | MIT |
| Crossterm, through Ratatui | `0.28.1` | MIT |
| tempfile | `3.23.0` | MIT OR Apache-2.0 |
| same-file | `1.0.6` | Unlicense OR MIT; the older manifest spells this `Unlicense/MIT`, and `COPYING` confirms the choice |
| libc, Unix only | `0.2.191` | MIT OR Apache-2.0 |
| winapi-util, Windows only | `0.1.11` | Unlicense OR MIT |
| Serde, `xtask` only | `1.0.229` | MIT OR Apache-2.0 |
| Serde JSON, `xtask` only | `1.0.154` | MIT OR Apache-2.0 |

The complete transitive graph is not exclusively MIT. Notable distinctions are
`foldhash 0.1.5` (Zlib), `unicode-ident 1.0.27` (`(MIT OR Apache-2.0) AND
Unicode-3.0`), and `ryu 1.0.23` (`Apache-2.0 OR BSL-1.0`). Their inspected notices
and dependency paths are documented in the inventory.

No application dependency source is vendored here. Release packaging and complete
target-specific notice aggregation remain unimplemented; this summary is not a
complete distribution notice bundle or a full audit of the dependency graph.

## Optional agent-development material

The following optional agent-development material retains its upstream license.
No component in this section is an application runtime dependency.

| Destination | Source revision | License and retained notice |
| --- | --- | --- |
| `.agents/skills/rust-best-practices/` | [Apollo skills at 8d0129fd](https://github.com/apollographql/skills/tree/8d0129fdc96a1b80d999a6682e48fc11e137dc09/skills/rust-best-practices) | [MIT; Apollo Graph, Inc.](.agents/skills/rust-best-practices/LICENSE) |
| `.agents/skills/gh-fix-ci/` | [OpenAI skills at 49f948fa](https://github.com/openai/skills/tree/49f948faa9258a0c61caceaf225e179651397431/skills/.curated/gh-fix-ci) | [Apache-2.0](.agents/skills/gh-fix-ci/LICENSE.txt) |
| `.agents/skills/systematic-debugging/` | [Superpowers at 22f0b463](https://github.com/obra/superpowers/tree/22f0b463e7a5e579904dd6211a520321035526f7/skills/systematic-debugging) | [MIT; Jesse Vincent](.agents/skills/systematic-debugging/LICENSE) |

Each skill's `UPSTREAM.md` records its source paths and local adaptations. The
Apollo reference chapters are unchanged; entry procedures are adapted for
focused reads and bounded validation. The GitHub diagnostic helper includes
correctness and output-volume fixes with offline regression coverage. These are
local adaptations, not upstream releases or endorsements.

The `.claude/skills/` entries are small hiewbie-authored adapters pointing to the
canonical `.agents/skills/` procedures. Keep the canonical license and provenance
files with any redistributed skill. Optional RTK binaries are not vendored;
their distribution and edition licenses are documented in [agent tools](docs/agent-tools.md).
