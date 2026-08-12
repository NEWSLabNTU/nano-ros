---
id: 531
title: "nros_platform_clock_us() returns 0 forever on Zephyr Cortex-M boards under 60 MHz, so the executor's timers never fire"
status: open
type: bug
area: platform-zephyr
related: [issue-0502, issue-0532]
---

## Problem

The Zephyr port computes microseconds from the 64-bit cycle counter:

```c
/* packages/platform/nros-platform-zephyr/src/platform.c:49-51 */
uint64_t nros_platform_clock_us(void) {
    return (uint64_t) k_cyc_to_us_floor64(k_cycle_get_64());
}
```

`k_cycle_get_64()` is not unconditionally available:

```c
/* zephyr/include/zephyr/kernel.h:1838-1847 */
static inline uint64_t k_cycle_get_64(void) {
    if (!IS_ENABLED(CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER)) {
        __ASSERT(0, "64-bit cycle counter not enabled on this platform. ...");
        return 0;
    }
    return arch_k_cycle_get_64();
}
```

`__ASSERT` compiles out in a release build, so the failure mode is a
silent, permanent `0`.

On the Cortex-M SysTick driver, the symbol that selects it is
conditional on the CPU frequency:

```
# zephyr/drivers/timer/Kconfig.cortex_m_systick:30-34
config CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER
	depends on CORTEX_M_SYSTICK
	default y if (SYS_CLOCK_HW_CYCLES_PER_SEC > 60000000)
	select TIMER_HAS_64BIT_CYCLE_COUNTER
```

So any Zephyr Cortex-M board at or below 60 MHz gets a clock that
always reads zero unless something enables the symbol explicitly.
Nothing in this repo does: a git grep for
`CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER` / `TIMER_HAS_64BIT_CYCLE_COUNTER`
outside `zephyr-workspace/` returns nothing.

The tier-2 `ZephyrQemuCortexM` platform is exactly this case —
`qemu_cortex_m3_defconfig:4` sets `CONFIG_SYS_CLOCK_HW_CYCLES_PER_SEC=12000000`
(12 MHz).

## Why it matters

The executor defaults its timer accounting to this clock on
`no_std` + `rmw-cffi` builds (`nros-node/src/executor/spin.rs`,
`default_platform_clock_us`). With the clock stuck at 0 the per-spin
delta is `now - prev = 0` forever, so accumulated time never reaches
any period and **periodic callbacks never fire**. Subscriptions still
work, because they are data-driven — so the image looks alive while
every timer is dead, which is the same silent-failure shape as the
earlier spin-timeout accounting bug.

Boards NOT affected: `native_sim` / `native_posix`
(`Kconfig.native_posix:11` selects the symbol — which is why the
native_sim lane has always looked fine), ARM architected timer
(`Kconfig.arm_arch:11`, so the FVP board is fine), x86, ESP32,
nRF GRTC, cAVS, MTK ADSP. Cortex-M above 60 MHz picks up the default.

## Verification status

Static: the four links above are each verified by reading the vendored
Zephyr sources and the board defconfig. NOT yet confirmed by running a
`qemu_cortex_m3` image — worth doing before the fix lands, since it is
also the regression test.

## Fix direction

- Make the port not depend on a Kconfig it does not control. Either
  select `CORTEX_M_SYSTICK_64BIT_CYCLE_COUNTER` from nano-ros' Zephyr
  Kconfig, or compute microseconds from `k_uptime_ticks()` (always
  available, tick-resolution) and use cycles only when the 64-bit
  counter is actually enabled.
- Prefer a build-time assertion over a runtime zero: if the port keeps
  using `k_cycle_get_64`, a `BUILD_ASSERT(IS_ENABLED(
  CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER))` turns a silent dead-timer
  image into a compile error naming the missing symbol.
- `k_cycle_get_32()` is not a drop-in alternative: it wraps in ~25 s at
  168 MHz and ~171 s at 12-25 MHz, so it needs the same software
  wrap-extension the FreeRTOS and STM32F4 ports already carry.
- The general shape of this — a platform clock whose resolution and
  availability are per-board facts the ABI cannot express — is issue
  0532.
