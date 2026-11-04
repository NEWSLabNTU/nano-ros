---
id: 2265
title: "Memory evidence needs `just mem-report` from a source checkout, and the book's 64 KB heap default disagrees with the image (132,816 B)"
status: open
type: bug
area: book, memory
purpose: [industrial]
related: [phase-523, rfc-0116]
found: 2026-11-05
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I15, internals/measuring-static-memory.md: `just mem-report path/to/your/binary` exists only in a checkout. configuration.md says HEAP defaults to 64 KB; the First Project image's `nros_platform_mps2_an385::memory::HEAP` measures 132,816 B (RAM 180,044 B total). No stack budget is stated anywhere.

## Next

A user-side `nros` verb (or a documented `arm-none-eabi-size`/`nm` recipe) for an image's static memory, a heap figure that matches the image or says what it depends on, and the stack budget.
