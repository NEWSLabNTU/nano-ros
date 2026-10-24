---
id: 2019
title: "A level's derived stack is the per-callback default plus a frame estimate, not its members' stacks"
status: resolved
type: enhancement
area: [codegen, memory, executor]
severity: low
found: 2026-10-24
resolved: 2026-10-24
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

## Resolution

Resolved in two parts.

**Member stacks** (PR #2594). A component states its callbacks' stack in
`system.toml` (`[[component]] stack_bytes = <bytes>`). It travels in the
nano-ros overlay (`nros.toml`, `[component."<name>"] stack_bytes`), and a C or
C++ entry passes a spawned level that declares no `stack_bytes` as
`NROS_LEVEL_STACK_BYTES(<largest member>u)`. The Rust pack derives the same
way (`nros_orchestration_ir::derive_level_stacks`, called by `nros::main!`).
The priority record carries `member_stack_bytes`, and `mem-report --levels`
checks Zephyr, FreeRTOS and NuttX against the same rule.

**Measured worker frames.** A spawned level runs its own setup
(`open_level`, its members' registration) on its own stack before its first
pass. That setup, not any pass, is the stack's peak, so the 4,096 B estimate
was too small on every port measured. Method: QEMU, the `realtime-c` and
`realtime-rust` workspaces' spawned `high` level, the high-water mark after
setup and after 300 and 3,000 passes (FreeRTOS
`uxTaskGetStackHighWaterMark`; NuttX stack coloration through
`nxsched_get_stackinfo` + `arm_stack_check`). No pass went deeper than setup.
Frame = peak rounded up to the next KiB:

| Port | Runner | Setup peak | Steady (3,000 passes) | Frame |
|------|--------|-----------:|----------------------:|------:|
| FreeRTOS, mps2-an385 | C | 9,448 B | 9,448 B | 10,240 B |
| FreeRTOS, mps2-an385 | Rust | 46,840 B | 46,840 B | 48,128 B |
| NuttX, virt cortex-a7 | C | 10,384 B | 10,384 B | 11,264 B |
| NuttX, virt cortex-a7 | Rust | 32,384 B | 32,384 B | 32,768 B |

The C frames are the header defaults (`NROS_FREERTOS_LEVEL_WORKER_FRAME`,
`NROS_NUTTX_LEVEL_WORKER_FRAME`) and `mem-report`'s `LEVEL_HEADER_DEFAULTS`;
the level defaults stay 256 KiB and 16 KiB, so only the per-callback default
shrinks. The Rust frames are
`level_worker_frame_bytes(rtos, LevelRunner::Rust)`; the function now takes
the runner, because the two runners' setup depths differ by 3-5x. A Rust
test reads the C headers and the `mem-report` selftest reads them too, so
the three copies cannot drift. Each peak includes the fixture's own
callbacks, so it bounds the frame from above for that fixture.

## Still estimates

- **Zephyr**: 4,096 B (`CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME`), for both
  runners. There is no local west workspace to measure it; a board measures
  its own with `CONFIG_THREAD_ANALYZER` and sets the knob (the island,
  S32K344, does so on hardware). Given the FreeRTOS and NuttX numbers, a
  Zephyr image whose levels derive from member stacks should measure before
  trusting the default, the Rust runner especially.
- **ThreadX**: 4,096 B. The Linux port runs ThreadX threads on host
  pthreads with no `TX_ENABLE_STACK_CHECKING`, so there is no high-water
  mark to read there.
