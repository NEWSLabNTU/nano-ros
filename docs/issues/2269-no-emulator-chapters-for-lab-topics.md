---
id: 2269
title: "Five of the seven lab topics an instructor needs have no emulator chapter (QoS, service, parameters, lifecycle, real-time tiers)"
status: open
type: enhancement
area: book, education
purpose: [education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E11, task 2's coverage table: publish (talker only, no subscriber walkthrough) and the ROS 2 graph are covered; QoS (API page only), services (`nros new --template server/client` plus GitHub pointers), parameters and lifecycle (Linux-host pages only) and real-time tiers (one ThreadX paragraph; rt-execution-model uses contributor-only `cargo nextest -p nros-tests`) have none.

This is phase-523 W1's work list; the probe confirms the gap from the reader's side.

## Next

phase-523 W1 labs 1–6.
