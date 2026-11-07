---
id: 2325
title: "Board pages and `nros board show` give a verification cadence, but not the date or result of the last run"
status: open
type: enhancement
area: book, support-status
purpose: [industrial]
related: [phase-523, issue-2267]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I8: `nros board show mps2-an385-baremetal` and `nros doctor --report` say `Verified in emulator (daily) … cadence daily`. No page or command gives the last run's date, its result or a link to it.
- The same release's First Project did not compile on that board (issue 2317), so "daily" did not cover the user's road.

## Next

Stamp each release's support table with the date, commit and result of the last verifying run, and make the daily lane run the book's First Project (the `probe=` blocks) from the release asset.
