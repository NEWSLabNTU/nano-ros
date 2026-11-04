---
id: 2263
title: "`nros setup` refuses the board key `nros new` writes into `system.toml` (`mps2-an385`)"
status: open
type: bug
area: cli, setup
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I12/E9, installation.md "Provision a whole workspace": `nros setup` in the scaffolded project → ``./system.toml: `mps2-an385` is not an index board key and no board catalog was found``. `nros new` wrote `board = "mps2-an385"`; the index key is `mps2-an385-baremetal`.

Measured on the release; check whether `main`'s `nros new` and `nros setup` agree (phase-525 W2 changed the scaffold).

## Next

One board vocabulary between `nros new` and `nros setup`; a test that scaffolds and then provisions the result.
