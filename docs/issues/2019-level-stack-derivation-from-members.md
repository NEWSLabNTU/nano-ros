---
id: 2019
title: "A level's derived stack is the per-callback default plus a frame estimate, not its members' stacks"
status: open
type: enhancement
area: [codegen, memory, executor]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108]
---

## Measured

`NROS_LEVEL_STACK_BYTES(member)` exists on Zephyr, FreeRTOS and NuttX. It is
the largest member's stack plus the worker frame, but every generated entry
passes `NROS_LEVEL_STACK_BYTES(0u)`. No contract or `system.toml` key states a
member's stack, so every level gets the per-callback default plus a 4,096 B
frame estimate. That equals the old tier default (Zephyr
`CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE`, FreeRTOS 256 KiB, NuttX 16 KiB):
nothing got tighter.

Two further gaps:

- The worker frame (4,096 B) is an estimate. It has not been measured per
  port.
- The Rust pack has no derived-stack row. The Rust Zephyr and ThreadX level
  runners keep each tier's declared `stack_bytes`, or the pool or byte-pool
  default.

## Closes when

- A member's stack can be stated (contract or `system.toml`) and reaches
  `NROS_LEVEL_STACK_BYTES(member)`.
- The worker frame is measured on each port (the island's thread analyzer
  for Zephyr) and replaces the estimate.
- The Rust pack derives level stacks the same way.
- `mem-report --levels` checks all three.
