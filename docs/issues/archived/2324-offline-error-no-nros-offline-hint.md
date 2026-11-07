---
id: 2324
title: "Offline, `nros run` fails on crates.io resolution without naming `NROS_OFFLINE=1`"
status: resolved
type: bug
area: cli, offline, ux
purpose: [industrial]
related: [phase-523]
found: 2026-11-07
resolved_in: "adoption-r2-cli-ux branch: a name-resolution failure in the metadata probe names NROS_OFFLINE=1"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I7: in the air-gapped container, `nros run sensor_node` without `NROS_OFFLINE=1` fails: `refresh source metadata for sensor_node … metadata-mode harness failed (exit 101) … Could not resolve host: index.crates.io`.
- This was the only step that reached for the network. `nros setup` was offline-clean. With `NROS_OFFLINE=1` the run matched after 3.4 s.

## Next

When cargo fails on name resolution, add one line: "no network: set NROS_OFFLINE=1 to build from the carried stores". Better still, detect that the stores are complete and go offline without being told.

## Resolution (2026-11-07)

`rust_toolchain::offline_hint` returns one line, "no network: set
NROS_OFFLINE=1 to build from the carried stores", when a failed cargo step's
output shows a name-resolution failure (`Could not resolve host`,
`Couldn't resolve host`, `Temporary failure in name resolution`) and
`NROS_OFFLINE` is not already set. A refused connection or a 5xx gets
nothing, since going offline is not the remedy for those. The metadata-mode
harness failure, the step I7 measured, appends it. The build's own cargo
step inherits stdio by design (RFC-0024), so its output cannot be scanned;
cargo's own text is shown there. Detecting complete stores and going offline
unasked is not done.
