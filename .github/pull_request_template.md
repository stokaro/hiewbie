## Problem

Describe the user-visible problem and relevant requirement IDs or issue.

## Changes

Describe the resulting behavior and compatibility implications.

## Before / after

Include a small Mermaid diagram for a changed workflow, architecture, or data
flow. Use a fenced `mermaid` block and verify that GitHub renders it. The raw
Markdown must retain literal Mermaid arrows and quotes.
For a change without a meaningful flow, use a concise before/after table.

## Validation

List the relevant automated checks and the platforms actually exercised.
Distinguish compilation, emulation, VM execution, and native execution.

## Goal progress

Report the percentage of requirements marked Implemented or Verified in
`docs/compatibility-status.md`. Include the count and total, plus any change
to those counts in this PR. Leave Partial and Planned requirements out of
the numerator. Report platform and release verification separately.

## Remaining gaps

List any affected compatibility or platform requirements that remain incomplete.
If source or dependencies were added, link their provenance and license review.
