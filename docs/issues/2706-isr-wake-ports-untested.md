---
id: 2706
title: "Zephyr, ThreadX and NuttX refuse an interrupt-safe executor wake until a test reaches each"
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
arriving. Three ports have one:

| Port | Interrupt-safe signal | Test |
| --- | --- | --- |
| FreeRTOS | `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` | `tests/isr_wake_rtos.rs`: a CMSDK TIMER1 interrupt on MPS2-AN385 |
| POSIX (Linux) | `sem_post` + pipe `write`, `errno` kept | `nros-platform-cffi/tests/c_port_posix_wake.rs`: a real `SIGUSR2` handler |
| mps2-an385 bare metal | the wake flag (`wake::signal`), read by the park with interrupts masked | `tests/isr_wake_rtos.rs`: a CMSDK TIMER1 interrupt on the bare-metal image (`isr-wake-baremetal-mps2`) |

Three ports now REFUSE: `nros_platform_wake_signal_from_isr` returns -1, and
`trigger_from_isr` returns `false`. The flag is still set, so the executor
sees the trigger at its next deadline instead of at once.

| Port | What the body would be | The test it needs |
| --- | --- | --- |
| Zephyr | `nros_zephyr_wake_give` (`k_sem_give`, ISR-safe; a non-blocking eventfd write) | `irq_offload()` on `native_sim` or `qemu_cortex_m3` calling `trigger_from_isr` while the executor parks |
| ThreadX | `nros_platform_wake_signal` (`tx_semaphore_ceiling_put`, ISR-callable per the spec) | an interrupt on a ThreadX board image (`threadx-riscv64` QEMU timer), or a `TX_TIMER_PROCESS_IN_ISR` application timer |
| NuttX | `sem_post` only: `write` is not interrupt-safe, so a handle park needs the pipe byte through the work queue | a NuttX QEMU image with a timer interrupt handler, covering both a semaphore wait and a handle park |

## Closing it

Per port: give `nros_platform_wake_signal_from_isr` the body above, add the
test, and move the port's row to the first table. The FreeRTOS cell
(`isr-wake-freertos-mps2`) is the template: a hardware timer's handler
triggers, the guard callback reads the latency off the timer, and the test
asserts the interrupts are all served and the bound held.

## mps2-an385 bare metal (2026-11-19)

Done:
- `wake_signal_from_isr` is `wake::signal`, the same atomic store.
- The park itself needed a fix to be interrupt-safe. `wake::park_until_us`
  read the flag, then entered the board's idle, whose `wfi` runs inside its
  own critical section. A handler that ran between that read and the
  critical section set the flag after it was read, and the `wfi` then slept
  until the next interrupt. The flag is now read inside the same critical
  section as the idle, so such a handler is pending rather than taken, and
  the `wfi` returns at once.

Measured (`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_baremetal_mps2`,
50 samples per run, TIMER1 every 20 ms, aarch64 host):
- Fixed: 5 of 5 runs had 50 interrupts and 50 callbacks, with latency 182 to
  247 us.
- Mutation, the port refusing (main before this): 1700 interrupts were
  served by 50 callbacks, worst 12.5 ms, and every trigger was refused. Red.
- Mutation, the old park (flag read outside the critical section): first
  measured statistically, 3 runs passed with one 6.86 ms sample. The window
  is too narrow to be caught by timing, so the test now makes it
  deterministic. A test-only hook (`wake::set_park_probe`, feature
  `park-race-probe`, enabled only by this image) runs between the park's
  flag read and its idle. The image's race probe uses it to pend DUALTIMER,
  whose handler calls `trigger_from_isr`, for 10 rounds, with TIMER1 stopped
  so nothing else ends a park.
  - Fixed: 3 of 3 runs served all 10 rounds in 83 to 154 us.
  - Old ordering, with the hook at the same point: 3 of 3 runs red. Each
    time 6 of 10 rounds lost their wake and slept 326 to 671 ms (671 ms is
    the board's SysTick one-shot limit).
