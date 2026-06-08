---
rfc: 0000
title: "RFC template — copy me"
status: Draft
since: YYYY-MM
last-reviewed: YYYY-MM
implements-tracked-by: []   # phase doc slugs that carry the work breakdown, e.g. [phase-206]
supersedes: []             # rfc numbers this replaces (rare, hard reversals only)
superseded-by: null        # set when this RFC is itself retired
---

# RFC-0000 — Title

> **How to use this template.** Copy to `docs/design/NNNN-slug.md` with the next free
> number (see [README index](README.md)). Fill the frontmatter. Delete this blockquote.
> RFCs are **living docs**: edit in place, flip `status` as the shape settles, and append
> to the Changelog. Design rationale lives here, never only in a phase doc.

## Status meaning

- `Draft` — shape still moving; open questions below are unresolved.
- `Stable` — shape settled; changes are refinements, tracked in the Changelog. Flipping to
  `Stable` **requires** updating the matching section of [ARCHITECTURE.md](ARCHITECTURE.md)
  in the same commit.
- `Superseded` — retired by a hard reversal. Set `superseded-by`, freeze the body, add a
  pointer at the top to the replacement RFC. Move the file to `archived/` only after the
  replacement is `Stable`.

## Summary

One paragraph: what this decision is and why it exists.

## Motivation / problem

What forces this decision. Constraints (no_std, embedded RTOS, wire-compat, …).

## Design

The decision. Interfaces, shapes, invariants. Keep units bounded — link sibling RFCs with
`RFC-NNNN` and a relative link rather than restating them.

## Alternatives considered

What else was on the table and why it lost.

## Open questions

Numbered, each resolvable. Empty once `Stable`.

## Changelog

- YYYY-MM — created.
