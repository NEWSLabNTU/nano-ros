---
id: 1598
title: "FreeRTOS tier task stacks still come from heap_4 via `xTaskCreate` — 256 KiB per tier, priced by nothing at link time"
status: open
type: tech-debt
area: [memory, freertos]
severity: medium
found: 2026-10-01
related: [issue-1568, issue-1571, issue-1146, issue-0667, issue-1232]
---

## What is left

After issues 1568 and 1571, a FreeRTOS tier's EXECUTOR storage is a `.bss`
static the entry owns. Its TASK STACK is not: each tier task is created with
`xTaskCreate`, which takes the stack from heap_4. The default is 256 KiB per
tier, from issue 1146's measurement, charged per task. Issue 1568 recorded
this and deliberately left it.

So the largest per-tier cost on FreeRTOS is still invisible to the linker and
to `mem-report`. An image that cannot hold its tiers' stacks fails at boot as
`*** MALLOC FAILED ***` (CLAUDE.md: an undersized stack lands as a malloc
failure, not a stack overflow), not at link.

## What moving them needs (from issue 1568's assessment)

- `configSUPPORT_STATIC_ALLOCATION 1` in each FreeRTOS board config
  (mps2-an385 currently has 0), plus the `vApplicationGetIdleTaskMemory` /
  `vApplicationGetTimerTaskMemory` hooks it then requires.
- The generated entry emitting `StackType_t __nros_tier_stacks[n][words]`
  plus `StaticTask_t[n]`, sized from each tier's `stack_bytes` (the floor
  rule of issue 0667 still applies: the port may raise it).
- The 256 KiB default moving from the runner to the emitter, so the number is
  stated where it is reserved.
- `xTaskCreateStatic` in the C and Rust FreeRTOS tier runners. The Rust
  `nros::main!` road needs the same static.

## Acceptance

- `mem-report` names the tier stacks on a FreeRTOS tiered image.
- The boot heap peak drops by the moved amount (a move, not a saving).
- An image whose stacks exceed RAM fails at link.
- Every FreeRTOS tiered fixture still boots and both tiers tick.
