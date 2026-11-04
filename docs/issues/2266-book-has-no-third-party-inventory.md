---
id: 2266
title: "The book has no third-party inventory or patch list for an image; the zenoh-pico fork is not mentioned at all"
status: open
type: bug
area: book, compliance
purpose: [industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I16, support-status.md "RMW backends": the Cyclone fork delta is a link into the repository; nothing names the zenoh-pico fork (`github.com/jerry73204/zenoh-pico`); production-readiness points at `third-party/*/LICENSE` in the source tree. There is no per-image list of compiled-in third-party code.

The fork-delta pages exist (`docs/reference/forks/`, phase-523 W4) but are not in the book.

## Next

The book carries (or generates) the fork-delta pages and a per-image third-party list a reviewer can attach.
