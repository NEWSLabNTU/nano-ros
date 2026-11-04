---
id: 2272
title: "The serial QEMU flow needs `socat`, which no prerequisite list names"
status: open
type: bug
area: book, serial
purpose: [education]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- E14, serial-transport.md "Create the PTY pair": the flow runs `socat`; it is in no prerequisite list, so a fresh seat stops at `socat: command not found`. Reached because `nros run` is absent from the release (issue 2255).

## Next

Add `socat` to the prerequisites (and the index's apt rows if a runner needs it), or confirm `nros run` on `main` needs none.
