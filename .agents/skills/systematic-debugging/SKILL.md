---
name: systematic-debugging
description: Investigate a nontrivial failure whose cause remains unclear after initial inspection, a nondeterministic defect, or a bug that persists after a fix. Use for root-cause debugging; skip obvious syntax errors, simple mechanical edits, and already-understood fixes.
---

# Bounded root-cause debugging

Honor the user's task, existing authorization, and repository contracts. Use
this focused adaptation of Superpowers without loading a general planning,
TDD, verification, or subagent framework automatically.

1. Establish the exact symptom, expected behavior, revision, input, and relevant
   environment. Reuse existing evidence. Reproduce the smallest failing case;
   if reproduction is unavailable, separate observations from hypotheses.
2. Trace the failing value or state back to its first invalid boundary. Inspect
   the relevant callers, configuration, and a working comparison. Read only the
   necessary code and log sections; retain a full log or durable source link.
3. Write one falsifiable hypothesis with the evidence supporting it. Choose the
   cheapest diagnostic that distinguishes it from the next plausible cause.
   Change one variable at a time; use bounded temporary instrumentation when
   needed and remove it after the diagnosis.
4. Fix the cause at the responsible boundary, preserving unrelated behavior.
   For a behavioral bug, add a focused regression that exposes the original
   failure and checks the real outcome. Use existing test tools. Do not add a
   framework or require a ritual test sequence for a reversible wording edit.
5. Run that regression and affected mandatory gates on the final code. Reuse
   unchanged relevant results; rerun checks invalidated by subsequent edits.
   Bound nondeterministic tests by attempts or time and report that bound.
6. If a hypothesis fails, update the evidence before trying another. After two
   failed hypotheses, reassess the reproduction and system boundary instead of
   stacking speculative patches. Continue when new evidence yields a useful
   experiment; report a concrete blocker when progress needs unavailable input.
7. Report the root cause, minimal fix, evidence, and remaining uncertainty.
   Distinguish tested behavior from inference and unavailable checks. Never
   claim successful tests from a code edit, a timeout, or an unverified assertion.
   Reuse delegated results when their output, exit status, revision, and relevant
   configuration are available for inspection and still match the changed code.

For a failing remote GitHub job, use the CI skill only if its log retrieval
procedure is needed; do not load both workflows to repeat the same investigation.
Delegate only an independent bounded question that is worth the context cost.

See [UPSTREAM.md](UPSTREAM.md) for the pinned source and adaptation notes.
