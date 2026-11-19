---
id: 2706
title: "Zephyr, ThreadX and NuttX refused an interrupt-safe executor wake until a test reached each"
status: resolved
type: test-gap
severity: medium
area: [platform, freertos, zephyr, threadx, nuttx, boards]
found: 2026-11-18
resolved: 2026-11-20
resolved_in: "fix(nuttx, issue 2706): an interrupt wakes a parked NuttX executor and a handle park, tested from a watchdog"
related: [2696, 2736, 2737, 2745, 2748, phase-579]
---

**Home: [phase-579](../../roadmap/phase-579-device-ports.md) W1**

## What

Issue 2696 gave the executor an interrupt-safe wake:
`GuardCondition::trigger_from_isr` reaches each port's
`nros_platform_wake_signal_from_isr`. A port claims that signal only where a
test takes a real interrupt (or, on POSIX, a real signal) and shows the wake
arriving. Six ports have one:

| Port | Interrupt-safe signal | Test |
| --- | --- | --- |
| FreeRTOS | `xSemaphoreGiveFromISR` + `portYIELD_FROM_ISR` | `tests/isr_wake_rtos.rs`: a CMSDK TIMER1 interrupt on MPS2-AN385 |
| POSIX (Linux) | `sem_post` + pipe `write`, `errno` kept | `nros-platform-cffi/tests/c_port_posix_wake.rs`: a real `SIGUSR2` handler |
| mps2-an385 bare metal | the wake flag (`wake::signal`), read by the park with interrupts masked | `tests/isr_wake_rtos.rs`: a CMSDK TIMER1 interrupt on the bare-metal image (`isr-wake-baremetal-mps2`) |
| Zephyr | `nros_zephyr_wake_give` (`k_sem_give`; a non-blocking eventfd write) | `tests/isr_wake_rtos.rs`: a `k_timer` expiry, in the system clock's interrupt handler, on `native_sim` (`isr-wake-zephyr-native-sim`) |
| ThreadX (bare-metal ports) | `tx_semaphore_ceiling_put`, inside the port's interrupt context save/restore | `tests/isr_wake_rtos.rs`: the RISC-V `virt` machine-timer interrupt, through the board's tick hook (`isr-wake-threadx-riscv64`) |
| NuttX | `sem_post`; when a handle park has made the wake object's pipe, its byte through the low-priority work queue, written through the pipe's `struct file` | `tests/isr_wake_rtos.rs`: a watchdog (timer interrupt) on the QEMU ARM `virt` image (`isr-wake-nuttx-qemu-arm`), over a semaphore park and a handle park |

One port variant still REFUSES, and is tracked on its own:
[issue 2745](../2745-threadx-linux-isr-wake-refused.md), the hosted ThreadX
Linux port (`threadx-linux`), whose "interrupt" is a host thread. There
`trigger_from_isr` returns `false`, and the executor sees the trigger at its
next deadline.

## Closing it (as it was done)

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

## Zephyr (2026-11-19)

Done:
- `nros_platform_wake_signal_from_isr` calls `nros_zephyr_wake_give`.
  `k_sem_give` is ISR-safe. The eventfd is opened `ZVFS_EFD_NONBLOCK`, so its
  write takes the non-blocking path: a spinlock and `k_poll_signal_raise`.
- The test image is `isr-wake-zephyr-native-sim`, a workspace whose bringup
  names an owned entry (`isr_wake_zephyr_entry`) and declares no component.
  A `k_timer` expiry runs in the system clock's interrupt handler and calls
  `trigger_from_isr`. The image counts expiries that ran outside interrupt
  context (`k_is_in_isr`), and the test requires that count to be 0, so it
  cannot pass on a thread wake.
- Building it needed issue 2737: its west message-bound seed refused an
  owned entry with no model.

Measured (`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_zephyr_native_sim`,
50 samples per run, a 20 ms timer):
- Fixed: 3 of 3 runs had 50 interrupts and 50 callbacks, none outside
  interrupt context. Latency read 0 us: native_sim's simulated clock does
  not advance while the CPU runs, so on this board the latency bound holds
  trivially. The served-count assertion (`fires <= callbacks + 1`) and the
  refusal count are what have teeth here.
- Mutation, the port refusing (`return -1`, main before this): 2500
  interrupts were served by 50 callbacks, every trigger refused. Red.

## ThreadX (2026-11-19)

Done:
- `nros_platform_wake_signal_from_isr` calls `tx_semaphore_ceiling_put` on
  the bare-metal ThreadX ports. The ThreadX spec allows the call from an ISR
  that runs inside the port's context save/restore, which takes any
  preemption it causes at the restore. The RISC-V `virt` board's trap entry
  does that save before `trap_handler`.
- The hosted Linux port (`NROS_THREADX_HOSTED_PORT`) still refuses. Its
  "interrupt" is the port's timer thread, and no test triggers from it.
- The `rv-virt-threadx` board's trap handler calls a weak
  `nros_rv_virt_tick_hook` after ThreadX's tick, in interrupt context. The
  default does nothing; the test image `isr-wake-threadx-riscv64` defines it
  and triggers the guard every second tick (20 ms), stamping `mtime`.

Measured (`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_threadx_riscv64`,
50 samples per run):
- Fixed: 5 of 5 runs had 50 interrupts and 50 callbacks, with latency 38 to
  927 us. The test needs 95 % of samples within 500 us and every sample
  under 10 ms, and each run met both.
- Mutation, the port refusing (main before this): 1010 interrupts were
  served by 50 callbacks, and every trigger was refused. Red.

## NuttX (2026-11-20)

Done:
- `nros_platform_wake_signal_from_isr` posts the semaphore, which NuttX
  allows from an interrupt. A handle park (`nros_platform_wake_park_handles_us`)
  waits on the wake object's pipe, and an interrupt cannot `write`. So when
  the pipe exists, the signal queues the byte on the low-priority work queue
  (`work_queue(LPWORK, ...)`, callable from an interrupt). `wake_drop`
  cancels a queued byte (`work_cancel_sync`).
- The worker writes through the pipe's `struct file` (`file_get` when the pipe
  is made, `file_put` on drop), never through the descriptor. The first
  version wrote through the descriptor, and every handle park slept to its
  deadline: a NuttX descriptor names a file only in its own task's table, and
  the work queue's thread is another task. The test found this.
- Without `CONFIG_SCHED_LPWORK` there is no such queue. The post still wakes a
  semaphore park, but for a wake object that parks on handles the signal
  returns -1, and the first handle park prints one named line ("CONFIG_SCHED_LPWORK
  required for handle park"). The QEMU ARM board sets it.
- The board compiles `c/nuttx_isr_timer.c`: a periodic NuttX watchdog, whose
  function runs from the timer interrupt, calling the image's callback. It
  counts fires outside interrupt context (`up_interrupt_context`), and the test
  requires 0.

The test image `isr-wake-nuttx-qemu-arm` has two phases:
1. A guard condition triggered from the interrupt while the executor parks
   (its semaphore park).
2. Ten rounds of the wake object signalled from the interrupt while the main
   thread takes a handle park with no handles and a 1 s deadline. Each must
   end on the signal, within 10 ms of it.

Measured (`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_nuttx_qemu_arm`,
a 20 ms watchdog):
- The first bound, the 500 us share the other ports meet, failed 3 times in
  32 runs, under load and once after a rebuild. It was the bound, not the
  wake: no interrupt went unserved. NuttX's clock counts 1 ms ticks, and
  QEMU's `-icount shift=auto` doubles guest time per instruction mid-run.
  [Issue 2748](2748-nuttx-executor-isr-wake-late-under-load.md) has the data.
  NuttX is held to the 10 ms cap and the lower bounds; the image reads the
  ARM generic timer.
- After that: 5 of 5 quiet runs and 10 of 10 under saturation (load 18 to 90)
  green. Every run had 50 interrupts and 50 callbacks, and 10 of 10 handle
  parks were woken by the signal. Latency read 39 to 1485 us, scaling with
  the icount shift.
- Mutation, the port refusing: 624 interrupts were served by 50 callbacks,
  and every trigger was refused. Red.
- Mutation, the worker writing through the descriptor (the first version):
  the semaphore phase stayed green, while 9 of 10 handle parks slept their
  full 1 s. Red. So each phase catches its own defect.
