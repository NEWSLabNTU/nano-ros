---
id: 2745
title: "On the hosted ThreadX Linux port, `trigger_from_isr` refuses: no test triggers from its timer thread"
status: resolved
type: test-gap
severity: low
area: [platform, threadx]
found: 2026-11-20
resolved: 2026-11-22
resolved_in: "the change that archives this issue"
related: [2706, 1750, phase-579]
---

## What

`GuardCondition::trigger_from_isr` reaches the port's
`nros_platform_wake_signal_from_isr`. On the bare-metal ThreadX ports that is
`tx_semaphore_ceiling_put`, tested from the RISC-V `virt` machine-timer
interrupt (issue 2706). The hosted Linux port (`threadx-linux`,
`NROS_THREADX_HOSTED_PORT` in `nros-platform-threadx`) still returns -1, so
`trigger_from_isr` returns `false` and the executor sees the trigger at its next
deadline instead of at once.

The reason is the port, not the call. Its "interrupt" is the port's timer
thread, a host thread that ThreadX treats as interrupt context. Whether a
semaphore put from there is safe depends on the port's context handling (issue
1750 is a host thread wedging the same port's kernel), and no test triggers
from that thread.

## Closing it

Write a `threadx-linux` image that triggers a guard condition from the port's
timer interrupt (a ThreadX application timer under `TX_TIMER_PROCESS_IN_ISR`, or
the port's tick hook), while the executor parks. Show every trigger served, and
the refusing port red. Then give the hosted arm the bare-metal body.

## Resolution (2026-11-22)

The hosted arm of `nros_platform_wake_signal_from_isr` now has the
bare-metal body (`tx_semaphore_ceiling_put`), behind the kernel-context guard
the port's other signals use. That guard admits the timer thread by identity
(`nros_threadx_in_kernel_context`) and still refuses a foreign host thread
(issue 1750).

**Test.** `isr-wake-threadx-linux` (row `threadx-linux-isr-wake`,
`tests/isr_wake_rtos.rs`) links with `--wrap=_tx_timer_interrupt`, so a hook
runs inside the port's real timer interrupt, after the port's own tick and
before the context restore, with no change to the vendored kernel. Every
fifth tick (50 ms) the hook signals the wake while the app thread parks on it
with a 1 s budget. The image opens no session, so it needs no router and no
veth bridge: it drives the wake object `GuardCondition::trigger_from_isr`
reaches, which the executor parks on. The hook also counts calls outside the
interrupt bracket (`_tx_thread_system_state == 0`), which must be none.

| arm | interrupts | wakes | refused | outside ISR |
| --- | --- | --- | --- | --- |
| refusing (main) | 250 | 0 | 250 | 0 |
| this change, 5 runs | 50 each | 50 each | 0 | 0 |

**Latency on a hosted port is the host scheduler's.** Measured at load 15 to
23 on 24 cores: median 250 to 350 us, tails of 3 to 15 ms. The 500 us bound
the bare-metal and QEMU images meet is not a property of this wake path, so
the test holds this image to `Latency::Hosted`: one wake per interrupt, none
early, each before the next interrupt (the period the image states on its
`ISR_WAKE_READY` line). The period is 50 ms rather than 20 ms because a 15 ms
tail came within 5 ms of the 20 ms bound.

