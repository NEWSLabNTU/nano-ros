---
id: 2706
title: "Zephyr, ThreadX, NuttX and mps2 bare metal refuse an interrupt-safe executor wake until a test reaches each"
status: open
type: test-gap
severity: medium
area: [platform, freertos, zephyr, threadx, nuttx, boards]
found: 2026-11-18
related: [2696, phase-579]
---

**Home: [phase-579](../roadmap/phase-579-device-ports.md) W1**

## What

Issue 2696 gave the executor an interrupt-safe wake:
`GuardCondition::trigger_from_isr` reaches each port's
`nros_platform_wake_signal_from_isr`. A port claims that signal only where a
test takes a real interrupt (or, on POSIX, a real signal) and shows the wake
arriving. Two ports have one:

| Port | Interrupt-safe signal | Test |
| --- | --- | --- |
| FreeRTOS | `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` | `tests/isr_wake_rtos.rs`: a CMSDK TIMER1 interrupt on MPS2-AN385 |
| POSIX (Linux) | `sem_post` + pipe `write`, `errno` kept | `nros-platform-cffi/tests/c_port_posix_wake.rs`: a real `SIGUSR2` handler |

Four ports now REFUSE: `nros_platform_wake_signal_from_isr` returns -1, and
`trigger_from_isr` returns `false`. The flag is still set, so the executor
sees the trigger at its next deadline instead of at once.

| Port | What the body would be | The test it needs |
| --- | --- | --- |
| Zephyr | `nros_zephyr_wake_give` (`k_sem_give`, ISR-safe; a non-blocking eventfd write) | `irq_offload()` on `native_sim` or `qemu_cortex_m3` calling `trigger_from_isr` while the executor parks |
| ThreadX | `nros_platform_wake_signal` (`tx_semaphore_ceiling_put`, ISR-callable per the spec) | an interrupt on a ThreadX board image (`threadx-riscv64` QEMU timer), or a `TX_TIMER_PROCESS_IN_ISR` application timer |
| NuttX | `sem_post` only: `write` is not interrupt-safe, so a handle park needs the pipe byte through the work queue | a NuttX QEMU image with a timer interrupt handler, covering both a semaphore wait and a handle park |
| mps2-an385 bare metal | the existing flag (`wake::signal`) | a TIMER1 interrupt on the bare-metal mps2 image, as the FreeRTOS cell does |

## Closing it

Per port: give `nros_platform_wake_signal_from_isr` the body above, add the
test, and move the port's row to the first table. The FreeRTOS cell
(`isr-wake-freertos-mps2`) is the template: a hardware timer's handler
triggers, the guard callback reads the latency off the timer, and the test
asserts the interrupts are all served and the bound held.
