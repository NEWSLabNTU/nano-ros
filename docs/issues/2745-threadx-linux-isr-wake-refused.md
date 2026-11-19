---
id: 2745
title: "On the hosted ThreadX Linux port, `trigger_from_isr` refuses: no test triggers from its timer thread"
status: open
type: test-gap
severity: low
area: [platform, threadx]
found: 2026-11-20
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
