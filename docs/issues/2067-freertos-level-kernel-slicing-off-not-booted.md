---
id: 2067
title: "No FreeRTOS level image has been booted to show the kernel's time slicing off"
status: open
type: tech-debt
area: [freertos, executor, testing]
severity: low
found: 2026-10-26
related: [phase-505, phase-509, rfc-0108, 2017]
---

**Home:** [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W1, since 2026-10-26.

## Measured

Issue 2017 (PR #2618) builds a FreeRTOS level image's kernel with
`configUSE_TIME_SLICING 0` via `NROS_FREERTOS_LEVEL_IMAGE`. The following
check it:

- the boards' `FreeRTOSConfig.h`, run through the host preprocessor both
  ways;
- the CLI test that holds the `level-image` feature to the board crates;
- `#error` guards in `tier_task_memory_freertos.h` and
  `freertos_run_tiers.c`.

The QEMU realtime rows passed.

Both level runners print the kernel's setting at boot
(`nros: kernel time slicing OFF`), but no test asserts that line, and no one
read it from a booted image when the PR landed. The cargo road (the Rust
`level-image` feature through `nros-board-freertos`'s build.rs) is checked
only by the feature-selection test, not by a built kernel.

## Closes when

A FreeRTOS QEMU level image row, C and Rust, asserts
`nros: kernel time slicing OFF` on its console. A non-level FreeRTOS row
asserts the line is absent, or that it reads ON.
