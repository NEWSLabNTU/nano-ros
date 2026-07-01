---
id: 126
title: "Embedded C/C++ run_tiers (FreeRTOS) does not run: tier-task stack overflow + shared session never connects under the multi-task structure"
status: open
type: bug
area: freertos
related: [phase-274, rfc-0015, rfc-0016]
---

## Summary

phase-274 W3 landed the embedded C/C++ `run_tiers` path for FreeRTOS/mps2-an385 (RFC-0015 Model 1):
`freertos_run_tiers.c` (one FreeRTOS task per tier over a shared session, gated), `FreertosBoard::
run_tiers`, the codegen gate flip, and the `ws-realtime-cpp-mps2` fixture. It **compiles and links**
(verified: `arm-none-eabi-gcc -mcpu=cortex-m3` clean; the entry links + the ELF contains
`nros_board_freertos_run_tiers`; the generated entry uses `run_tiers` with the two tiers baked). But
it **does not run** under QEMU — two runtime defects, so the embedded Model-1 e2e is NOT yet proven.

## Defects (QEMU mps2-an385, verified)

**A — tier-task stack overflow → HardFault (fixable).** The default 64 KiB tier-task stack overflows:
`-d int` shows a Prefetch Abort at `0xa5a5a5a4` (FreeRTOS `tskSTACK_FILL_BYTE`) right after a context
switch. A diagnostic bump to 256 KiB removes the fault. Cause: the borrowed executor's spin/dispatch
needs more stack than 64 KiB (the boot tier survives on the 512 KiB `app_task`). Fix: size the
tier-task stack from `TierSpec.stack_bytes` (bake a sufficient value in `[tiers.<name>.freertos]`
stack) and/or raise the board default — mindful of QEMU/mps2 RAM (256 KiB × N tiers may not fit;
tune the executor arena + stack).

**B — shared session never connects under `run_tiers` (the deeper blocker).** Even with no fault
(256 KiB stack), the `run_tiers` boot **session never connects**: zenohd shows zero connections, no
`[ctrl]`/`[telem]` ticks, though the CPU is alive + context-switching. The single-tier
`run_components` control fixture connects + publishes `Published: 0..71` in the IDENTICAL environment
(network/zenohd/QEMU all work) — so this is **`run_tiers`-specific**, consistent with the
shared-session-across-FreeRTOS-tasks (zenoh-pico) soundness concern flagged in phase-274. Likely: the
boot task must bring the session fully up + connected BEFORE spawning tier tasks (or the zenoh-pico
read/lease task interaction with the borrowed executors on separate FreeRTOS tasks is unsound). Needs
dedicated debugging of the zenoh-pico session lifecycle under the multi-task run_tiers structure.

## Impact

Embedded C/C++ multi-tier (Model 1) is code-complete + builds/links but unproven at runtime on
FreeRTOS. Native C/C++ Model 1 (phase-274 W2) + Rust are proven. Single-tier embedded C/C++ is
unaffected (uses the existing `run_components` path). Zephyr/NuttX/ThreadX embedded `run_tiers` were
deferred to a follow-up and will likely hit the same session-lifecycle question (B).

## Fix direction

1. **B first** (the blocker): ensure the boot task's session is fully open + connected before tier
   tasks spawn; audit zenoh-pico session concurrency across FreeRTOS tasks (the Rust FreeRTOS
   `run_tiers` boots — compare its session bring-up ordering vs the C path). This is the load-bearing
   item.
2. **A**: bake/size the tier-task stack (from `TierSpec.stack_bytes`) to fit the executor; tune RAM.
3. Then the QEMU e2e (`ws-realtime-cpp-mps2`, both tiers scheduling) closes it.
