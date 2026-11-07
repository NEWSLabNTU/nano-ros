---
id: 2326
title: "The attributed memory report ships only in the source tree, and the book's `llvm-tools` recipe needs the network"
status: open
type: enhancement
area: release, memory, book
purpose: [industrial]
related: [phase-523, issue-2265]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I9: Measure static memory's `llvm-size -A` / `llvm-nm --size-sort` recipe works only after `rustup component add llvm-tools`. That needs the network once, and Working offline does not list it.
- The attributed report (by crate and by pool) "ships in the nano-ros source tree, not in a release". The book sends the reader to Internals for it.
- The heap-section claim the probe flagged was wrong and has been corrected (issue 2309).

## Next

Ship the memory report in the release as an `nros` verb over a built image, or carry `llvm-tools` in the SDK and list it under Working offline.
