---
id: 2067
title: "No FreeRTOS level image has been booted to show the kernel's time slicing off"
status: resolved
type: tech-debt
area: [freertos, executor, testing]
severity: low
found: 2026-10-26
resolved: 2026-10-27
resolved_in: "PR #2709 (2026-10-27)"
related: [phase-505, phase-509, rfc-0108, 2017]
---

**Home:** [phase-505](../../roadmap/archived/phase-505-ci-lanes-reach-and-infra-robustness.md) W1, since 2026-10-26.

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

## Resolution

Booted and asserted on 2026-10-27 (QEMU mps2-an385). The three FreeRTOS
level rows (`workspace-{c,cpp,rust}-freertos-realtime`) print, after
`nros: multi-level run -- 2 level(s) over one executor`:

```
nros: kernel time slicing OFF
```

The Rust row's image comes from the cargo road (`level-image` through
`nros-board-freertos`'s build.rs), so its kernel is checked by a booted image
now, not only by the feature-selection test.

`tests/realtime_tiers_e2e.rs` asserts it. `require_kernel_slicing_off` runs
in both FreeRTOS proofs, `SerialTicks` (C, C++) and `SerialDispatch` (Rust).
It waits for either spelling, so a slicing kernel fails on its line instead
of timing out. Mutation check: with `level_image` forced false in
`nros-board-freertos/build.rs`, the Rust row prints
`nros: kernel time slicing ON (issue 2017)` and the cell fails naming it.

The negative control is `tests/rtos_e2e.rs::test_rtos_pubsub_e2e` on
FreeRTOS, whose talker and listener are single-executor images. Their console
must carry no slicing line, or one that reads ON. The runner for those images
prints none; the Rust cell passed 70/70 with the line absent. The markers live
in `nros_tests::output` (`KERNEL_TIME_SLICING_{PREFIX,OFF,ON}`).

## Closes when

A FreeRTOS QEMU level image row, C and Rust, asserts
`nros: kernel time slicing OFF` on its console. A non-level FreeRTOS row
asserts the line is absent, or that it reads ON.
