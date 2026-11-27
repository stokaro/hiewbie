---
name: rust-best-practices
description: Review or implement nontrivial Rust ownership, error handling, APIs, unsafe boundaries, or measured performance changes. Use for Rust design/refactoring and Rust correctness reviews; skip documentation-only edits, simple renames, and non-Rust tasks.
---

# Focused Rust best practices

Follow the user's scope and repository rules, toolchain, feature matrix, and
existing design before applying this adapted Apollo guidance.

1. Inspect the changed code, its callers, and relevant tests. State the concrete
   invariant or problem. Read a reference below only to resolve a relevant design
   question; search its headings first and load the required section.
2. Prefer borrowed input when ownership is unnecessary. Justify clones and shared
   ownership by lifetime/API needs; do not replace clear code with clever generic
   abstractions or type states without an actual invariant to enforce.
3. Propagate expected failures with explicit errors and useful context. Validate
   external bytes, offsets, arithmetic, and sizes at boundaries. Document unsafe
   preconditions and keep unsafe operations small. Respect existing error types.
4. Optimize from measurements in representative release builds. Consider memory,
   allocations, dispatch, and build time together. Treat reference suggestions
   (Copy size cutoffs, one assertion per test, blanket bans on dyn/unwrap, or lint
   choices) as contextual advice, not universal Rust rules.
5. Add regression coverage for changed behavior and meaningful boundary cases.
   Use the repository's check command, or target the affected package/features
   with Cargo. Do not assume --all-features is valid, add a framework for a small
   test, or change the supported Rust version for a convenience idiom.
6. Before completion, check the diff and run the relevant required gates once on
   the final state. Repeat only checks invalidated by later changes. Report the
   commands, exit status, and coverage limits; do not equate compilation with
   runtime verification or a filtered run with the entire test suite.

## Read only the relevant reference section

| Design question | Reference |
| --- | --- |
| Borrowing, cloning, iteration, abstraction | [Idioms](references/chapter_01.md) |
| A Clippy warning or lint configuration | [Clippy](references/chapter_02.md) |
| A measured performance issue | [Performance](references/chapter_03.md) |
| Error boundaries and Result types | [Errors](references/chapter_04.md) |
| Behavior, regressions, and snapshots | [Tests](references/chapter_05.md) |
| Generics versus trait objects | [Dispatch](references/chapter_06.md) |
| A concrete invalid-state problem | [Type states](references/chapter_07.md) |
| Public API documentation | [Documentation](references/chapter_08.md) |
| Pointer choice or Send/Sync boundary | [Pointers](references/chapter_09.md) |

Keep full command logs available and show only actionable diagnostics plus the
result. Use an existing output filter when suitable; inspect raw logs if it hides
needed evidence. Do not read every chapter or activate other skills by default.

See [UPSTREAM.md](UPSTREAM.md) for the pinned source and adaptation notes.
