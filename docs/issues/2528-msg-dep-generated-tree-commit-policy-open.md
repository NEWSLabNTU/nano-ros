---
id: 2528
title: "Which in-tree leaves commit their generated message tree, and does a committed tree over-assert the ROS edition?"
status: open
type: design-question
severity: low
area: [codegen, build]
related: [rfc-0067]
found: 2026-11-14
---

**Moved here from RFC-0067's open questions** when the RFC became Stable
(phase-555 W3, rule A1(a)): its decisions D1-D5 landed (phases 333, 335, 465),
and these two policy questions are what remained.

1. **Should every in-tree testing or bench leaf commit its `generated/` tree**,
   so it can build and commit a lock without a ROS host, or only the leaves that
   need offline reproducibility? A committed `generated/` is edition-pinned
   content in the tree; the alternative is a leaf that builds only after
   `nros configure`. Today the rule is the invariant `check-leaf-lockfiles`
   holds: a tracked lock requires no message deps or a committed `generated/`.
2. **Multi-edition.** A committed `generated/` is one ROS edition's field set.
   Cross-edition leaves pick an edition per build; confirm that the path-dep
   lock does not over-assert the edition.
