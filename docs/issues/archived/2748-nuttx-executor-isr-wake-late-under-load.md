---
id: 2748
title: "The NuttX interrupt-wake test held a 500 us latency bound its clock could not resolve and QEMU's icount scales"
status: resolved
type: bug
severity: low
area: [testing, nuttx, platform]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "fix(nuttx, issue 2706): an interrupt wakes a parked NuttX executor and a handle park, tested from a watchdog"
related: [2706]
---

## What happened

Issue 2706's NuttX interrupt-wake test (`isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_nuttx_qemu_arm`)
first held the NuttX image to the bound the FreeRTOS, bare-metal and ThreadX
images meet: 95 % of the 50 guard callbacks within 500 us of their interrupt,
and every one under 10 ms. Before that bound changed:

- 1 of 12 runs failed right after a rebuild, with its output not kept.
- Under a concurrent full gate (host load 33 to 36, 80 cores), 2 of 20 runs
  failed:
  - run 1: 51 interrupts, 50 callbacks; only 21 of 50 within 500 us. From
    sample 26 on, the samples read 1000 to 5000 us, mostly a steady 3000,
    with one of 17000.
  - run 4: 50 interrupts, 50 callbacks; one sample of 15000 us, over the cap.

No run lost a wake: every interrupt was served, and the refusal count was
0. The other 18 runs at that load were identical to quiet ones.

## Cause

The bound, not the wake path. Two things made it unmeasurable on this image:

1. **The clock.** `clock_gettime` on this NuttX configuration counts whole
   ticks (`CONFIG_USEC_PER_TICK=1000`). A sample read 0 when the callback ran
   in its interrupt's tick and 1000 when it crossed a tick boundary, so the
   500 us bound tested tick alignment. The image now reads the ARM generic
   timer (CNTVCT over CNTFRQ), through the board's
   `nros_board_nuttx_isr_timer_now_ns`.
2. **QEMU's `-icount shift=auto`.** With the fine clock, the latency within
   ONE quiet run doubles in steps: 39, 78, 159, 317, then about 645 us. The
   wake path is a fixed run of instructions; QEMU raises the shift mid-run,
   and guest time per instruction is 2^shift ns. The networked QEMU
   launchers must keep `shift=auto` (`qemu.rs` asserts it). So on this image
   a microsecond bound measures QEMU's shift. The handle park, which never
   enters the executor, reads about 520 us the same way.

Under saturation (160 spinning shells, load about 96), the guest's timer
itself fired off schedule: one run's handle parks lasted 3 to 39 ms against
a 20 ms watchdog. That is the emulator starved by the host, the class
`isr_wake_rtos.rs` already records for FreeRTOS.

## Resolution

The bound is per-platform. This is BECAUSE OF icount, and is recorded in the
test as `Latency::CapOnly`, so nobody "tightens" it back:

- The NuttX image is held to the cap only: every callback under 10 ms, half
  the 20 ms period.
- The FreeRTOS, bare-metal, Zephyr and ThreadX images keep the 500 us share
  bound, which they meet.
- Every image gains lower bounds:
  - no callback observed before its interrupt's stamp (latencies are printed
    and parsed signed);
  - no more callbacks than interrupts;
  - on NuttX, no handle park woken with no interrupt during it.
- The NuttX test prints every sample of both phases before it asserts
  anything.

The teeth do not rest on the bound:
- the port refusing is red on the refused and served counts (624 interrupts
  for 50 callbacks);
- a handle park whose pipe byte never arrives is red on 9 of 10 parks
  sleeping to their 1 s deadline.

Measured after the change: 5 of 5 quiet runs and 10 of 10 runs under
saturation (load 18 to 90) green, latency 39 to 1485 us.

If a microsecond bound is wanted on NuttX again, it needs a clock that does
not scale with the icount shift. The guest's instruction count is one.
