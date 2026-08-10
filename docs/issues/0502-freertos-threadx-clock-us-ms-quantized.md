---
id: 502
title: "nros_platform_clock_us is millisecond-quantized on FreeRTOS and ThreadX — 10% of a 10 ms timer period is quantization before scheduling starts"
status: open
type: bug
area: embedded
related: []
---

## Problem

`nros_platform_clock_us` is the one monotonic time source the whole stack
leans on: the executor's spin/timer accounting reads it (the extern in
`packages/core/nros-node/src/executor/spin.rs` documents "every platform
port exports `nros_platform_clock_us` through the same C ABI"), and
`nros-c` scales it for `clock_ns`. Two ports return tick-quantized time
under a microsecond signature:

- FreeRTOS (`packages/platform/nros-platform-freertos/src/platform.c:89`):
  `xTaskGetTickCount() * US_PER_TICK` — with the common 1 kHz tick this is
  a millisecond counter multiplied by 1000.
- ThreadX (`packages/platform/nros-platform-threadx/src/platform.c:52`):
  `tx_time_get() * MS_PER_TICK * 1000` — same shape.

Zephyr (`k_cyc_to_us_floor64(k_cycle_get_64())`) and POSIX
(`clock_gettime`) are genuinely sub-microsecond; the API contract those
two set is what the executor's arithmetic assumes.

## Why it matters

Every duration the executor computes on FreeRTOS/ThreadX moves in 1 ms
steps. For a 10 ms periodic callback that is a 10% error floor per
measurement: elapsed-time credit, timeout expiry, and any monitor
comparing achieved period against a declared one all inherit ±1 tick of
noise that has nothing to do with scheduling. Sub-millisecond periods
cannot be expressed at all. External cadence measurements on the
mps2-an385 FreeRTOS board had to bypass this clock entirely and
interpolate ticks with the SysTick down-counter to get usable numbers
(NEWSLabNTU/nano-ros-rt-eval, `src/freertos_entry/src/main.rs`,
`island_now_us`) — the same interpolation the Tonbandgeraet trace port
in `packages/boards/nros-board-mps2-an385-freertos/trace/tband_config.h`
already performs for trace timestamps.

## Fix direction

Sub-tick interpolation behind the same export, per port:

- Cortex-M (covers the FreeRTOS mps2 board and any SysTick-driven port):
  `tick * US_PER_TICK + (RELOAD - SysTick->VAL) / CYCLES_PER_US`, with a
  read-tick / read-VAL / re-read-tick guard against the tick-boundary
  race (the tband port documents the same backward-step hazard).
- ThreadX: equivalent using the port's timer hardware where available;
  where not, document the resolution honestly instead of implying us.

The platform crates are board-generic, so the sub-tick read belongs in a
small per-board hook (reload value + cycles-per-us constant) with the
tick-only version as the documented fallback. No API change; callers
already assume microseconds.
