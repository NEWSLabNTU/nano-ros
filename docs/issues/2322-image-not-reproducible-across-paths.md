---
id: 2322
title: "An image embeds its absolute workspace path, so the same project built in another directory is not byte-identical, and the book does not say so"
status: open
type: enhancement
area: build, reproducibility, book
purpose: [industrial]
related: [phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I5: the same scaffold and knob, built in `~/a/sensor_ws` and `~/b/sensor_ws`, give ELF `55f96fa6…` and `6fc6b81d…`. The `objcopy -O binary` images also differ. `strings` finds 69 occurrences of `/home/dev/a`. Rebuilding at the same path is byte-identical.
- E (note): two seats' firmware hashes differed for the same reason.
- The default build is cargo's `dev` profile, a 16 MB ELF. First Project does not mention this.
- Choose your path promises "the same inputs give the same image".

## Next

Remap paths by default (`--remap-path-prefix` for rustc and `-ffile-prefix-map` for C) so the image does not depend on where it was built. Say in the book which profile First Project builds, and what is expected to reproduce.
