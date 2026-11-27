# Third-party development material

hiewbie's original application code and documentation remain MIT-licensed.
The following optional agent-development material retains its upstream license.
No component listed here is an application runtime dependency.

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
