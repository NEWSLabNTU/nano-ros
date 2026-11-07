---
id: 2331
title: "The education route's pages cite RFC numbers and link into the repository instead of staying in the book"
status: open
type: enhancement
area: book
purpose: [education]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E2: Choose your path › Design background and First Project › Scaffold and Design background (also Project layout § `src/lib.rs`) say "The three purposes and their budgets are RFC-0116 D1". They link `github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/…` and `examples/…` in the repository.
- `user-surface-ids` forbids the `RFC-NNNN` spelling in body text, so these sit in a "Design background" form the gate allows.

## Next

The book work (phase-537 owns the user book): keep the user book self-contained, and move the design background to the Internals book behind one link.
