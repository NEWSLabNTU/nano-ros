---
id: 2324
title: "Offline, `nros run` fails on crates.io resolution without naming `NROS_OFFLINE=1`"
status: open
type: bug
area: cli, offline, ux
purpose: [industrial]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I7: in the air-gapped container, `nros run sensor_node` without `NROS_OFFLINE=1` fails: `refresh source metadata for sensor_node … metadata-mode harness failed (exit 101) … Could not resolve host: index.crates.io`.
- This was the only step that reached for the network. `nros setup` was offline-clean. With `NROS_OFFLINE=1` the run matched after 3.4 s.

## Next

When cargo fails on name resolution, add one line: "no network: set NROS_OFFLINE=1 to build from the carried stores". Better still, detect that the stores are complete and go offline without being told.
