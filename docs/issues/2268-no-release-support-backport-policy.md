---
id: 2268
title: "No release-support, backport or asset-retention policy in the book, and the production checklist still says Jazzy has \"no public ETA\""
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

- I18, installation.md "Pinning a version", production-readiness §8–9: versioning is a link to `book-internals/src/dev-env/versioning.md`; the reader is told to "track the roadmap directory in the upstream repo"; there is no support window, backport/LTS policy, or promise that dist assets stay hosted. "Targets ROS 2 Humble today… Jazzy no public ETA" contradicts support-status, which lists Jazzy as supported.

## Next

A release-support statement in the book (what is supported for how long, how fixes reach a pinned release, how long assets stay hosted) and a production checklist that matches support-status.
