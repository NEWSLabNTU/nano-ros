---
id: 2267
title: "The book never points a reviewer at a release-specific provenance report, and support-status describes `main`, not the installed release"
status: open
type: bug
area: book, release
purpose: [industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I17: no attachable document of toolchain, pins and board support level is reachable from the book. support-status.md describes `main` (QEMU `11.0.0-nros8`), not the installed release (`nros7`). The board page shows a cadence but no last-run date or link.

`nros doctor --report` (phase-523 W4) landed after the release and no book page names it (`git grep 'doctor --report' book/src` is empty).

## Next

The book names `nros doctor --report`, and support status is stated per release with dated evidence.
