---
name: hiew-ux
description: Implement or review hiewbie terminal UX against observed HIEW 8.63 screens, state transitions, contextual keys, dialogs, editing, blocks, file selection, and PE navigation.
---

# HIEW UX reference

Use this skill for hiewbie changes that affect modes, keys, rendering, dialogs,
editing feedback, block selection, file selection, or executable-header workflows.

Before changing behavior, read the relevant requirements in
[`docs/compatibility.md`](../../../docs/compatibility.md), frontend design in
[`docs/architecture.md`](../../../docs/architecture.md), and the observed UX in
[`references/observed-ux.md`](references/observed-ux.md). When planning parity,
closing a gap, or reviewing a compatibility claim, also read the applicable rows
in [`references/acceptance-matrix.md`](references/acceptance-matrix.md). Read only
the relevant scenarios in
[`docs/reference/hiew-8.63.md`](../../../docs/reference/hiew-8.63.md) when command
semantics or remaining unknowns matter.

Treat the observed runtime as behavioral evidence for HIEW 8.63 on the recorded
Windows console. Preserve the command model and information hierarchy while
adapting cell measurements to the active terminal. Do not reproduce proprietary
code, binary assets, graphics, or manual prose.

## Apply the reference

1. Identify the complete UI state, including view, edit transaction, block,
   active dialog, file format, address representation, and modifier state.
2. Match the observed screen hierarchy and contextual command availability.
   Keep familiar labels where they communicate established commands.
3. Route keys through stable semantic commands. The main function-key bar,
   modifier bar, dialog bar, and help must describe the same dispatch table.
4. Preserve the shared cursor and document across Text, Hex, and Code. Keep
   current address, access state, edit state, format, and decoding profile visible.
5. Make cancellation restore the prior state. Make saving explicit and expose
   pending changes through color and state text before persistence.
6. Add deterministic cell snapshots and command-level regressions for every
   changed state transition. Editing regressions must assert resulting bytes and
   transaction state.

When an observed detail conflicts with current hiewbie behavior, update the
affected requirement, rationale, ledger entry, and regression together. Record
intentional improvements as hiewbie behavior instead of attributing them to HIEW.

Report requirement IDs, checks actually run, and any reference behavior that
remains unobserved.
