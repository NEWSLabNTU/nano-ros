---
id: 502
title: "nros_platform_clock_us is millisecond-quantized on FreeRTOS and ThreadX — 10% of a 10 ms timer period is quantization before scheduling starts"
status: resolved
resolved_in: "same-day fix; see platform.c #502 comment"
type: bug
area: embedded
related: [issue-0503, issue-0504]
---

## Resolution (2026-08-11)

FreeRTOS (`nros-platform-freertos/src/platform.c`): on Cortex-M the
standard port drives the tick from SysTick, so `clock_us` now adds the
sub-tick fraction from the SysTick down-counter. Hazards handled:
tick-boundary race (snapshot with tick read on both sides, retry on
change), counted-to-zero-but-ISR-pending (ICSR.PENDSTSET sampled on
both sides of the VAL read; credit one tick when stable-set), tickless
idle / non-SysTick tick sources (reload register read at runtime;
LOAD == 0 falls back to tick-only). Non-Cortex-M ports and builds
defining `NROS_PLATFORM_FREERTOS_NO_SUBTICK` keep the tick-quantized
value. Verified on the mps2-an385 QEMU lane (SysTick VAL reads proven
against an independent guest-clock instrument).

ThreadX: NOT sub-tick — the tick source is port-defined, so there is
no portable counter to interpolate. The coarseness is now documented
at the implementation (`nros-platform-threadx/src/platform.c`); a
per-port hardware-timer hook remains future work and can reopen a
scoped issue if a ThreadX consumer needs it.

## Original problem (condensed)

Both ports returned `ticks * US_PER_TICK` under a microsecond
signature (1 ms steps at a 1 kHz FreeRTOS tick; 10 ms steps at the
default 100 Hz ThreadX tick). The executor's spin/timer accounting
runs on this clock, so every measured duration carried a 1-tick error
floor — 10% of a 10 ms period — and sub-ms periods were
inexpressible. Zephyr (`k_cyc_to_us_floor64`) and POSIX
(`clock_gettime`) always delivered genuine microseconds.
