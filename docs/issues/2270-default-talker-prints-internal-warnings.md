---
id: 2270
title: "The default talker's sync and boot print internal issue numbers and a \"contract violation\" a newcomer cannot act on"
status: open
type: bug
area: cli, ux, logging
purpose: [education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E12, first-project.md "Run it": the first `nros sync` (the release's spelling of `nros configure`) prints ``take buffer not derived, so `RX_BUF` keeps the crate default (issue 1233)…`` and ``…stay on the `--config` road (issue 1381)``; at runtime the board prints `[WARN] nros: … contract violation: release-jitter-runtime spin measured=10556 declared=10000`.

On a scaffolded project with nothing changed, these read as errors and name internal tracker ids. Measured on the release; the sync messages' code is still on `main` (`leaf_settings.rs` cites 1381).

## Next

A scaffold's default build is silent unless the user can act; internal ids stay out of user-facing text; the default talker's timing contract matches what the board achieves under QEMU.
