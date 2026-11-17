---
id: 2666
title: "Zephyr mps2/an385 XRCE idle executor is switched in about 3 times a timer period, against a bound of 2"
status: open
type: bug
severity: medium
area: [zephyr, xrce, executor, testing]
related: [phase-521, 2252, 2608]
found: 2026-11-17
---

**Home: [phase-521](../roadmap/archived/phase-521-rt-unified-wait.md)** (RFC-0113 D2).

## What

`zephyr_cortex_m_qemu::zephyr_cortex_m_idle_xrce_image_wakes_only_for_its_deadlines`
fails every run:

```
idle-wake: switch_ins=15 fires=5 ...
the executor's thread woke 15 times over 5 timer periods
```

The assertion is `switch_ins <= 2 * fires + 2`, which is 12 for 5 fires. So the
executor thread is switched in about 3 times a period, not the 1 a deadline
needs. Reported by another session, which measured:

- The same count at `1037efabd0`, before its own change. The red pre-dates that change.
- 16 with `NROS_EXECUTOR_MAX_CBS=4`. Callback-table sizing is not the cause.
- The zenoh twin, `…_idle_zenoh_image_…`, passes on the same board.

The cell was last green in the issue 2252 verification (`max_late_us=0`; the
switch-in count was not recorded there). Since `3eefb44f93` (2026-11-10)
the witness has been a workspace row that no lane built, so nothing ran it.

## What the count counts

The witness (`packages/testing/nros-tests/bins/idle-wake-zephyr-mps2/…/Witness.c`)
increments `switch_ins` in Zephyr's `sys_trace_thread_switched_in_user` hook
whenever the executor thread is the one switched in. That counts every
switch-in: a wake from a blocked wait, and also a resumption after the
thread was PREEMPTED while it was runnable. RFC-0113 D2.3 bounds the first.
The second is not a wake. So the 15 is the sum of two different things until
it is split.

## Where an XRCE wake comes from on Zephyr

XRCE has no thread of its own. On Zephyr its readiness is the zsock UDP
descriptor (`nros_platform_udp_readiness_handle`, WAITABLE under
`CONFIG_ZVFS_EVENTFD`). The executor parks in
`nros_platform_wake_park_handles_us` (`nros-platform-zephyr/src/platform.c`):
a `zsock_poll` over the wake object's eventfd plus that descriptor, with the next
deadline as the timeout. It then drains the session non-blockingly. The zenoh twin
waits on a semaphore, and zenoh's own read task does the socket work.
That is the structural difference between the passing and failing cells.

Candidates, none measured yet:

1. **Agent traffic.** A frame from the Agent (a reliable-stream HEARTBEAT, an
   ACKNACK) makes the socket readable. That is an RX wake that carries no data
   the sched task owns. D2.3 allows a wake only for owned data or a deadline.
2. **The drain's own send.** The non-blocking drain runs the uxr session, which
   can send (an ACKNACK, a heartbeat for the reliable output stream). A
   `zsock_sendto` hands the frame to the higher-priority net TX thread, which
   preempts the executor. That is one extra switch-in each time, and not a wake.
3. **A loop inside the park.** `zsock_poll` returning with only the eventfd ready
   and the semaphore already taken (the "stale" path) loops again. So does
   Zephyr's own `zvfs_poll` re-polling after a `k_poll` return that found no
   ready descriptor. Each pass is a switch-in.

## Plan

Measure before choosing. Instrument the witness, without changing the
executor, to record per switch-in during the window:

- the uptime;
- whether the previous switch-out left the thread BLOCKED or still READY
  (pending vs preempted, from the `switched_out` hook);
- the LAN9118 RX interrupt times, which the witness already stamps.

For each park, also record which source ended it (handle, eventfd, timeout).
Then:

- If the extra switch-ins are preemptions (2): the count is not a wake count.
  Fix the witness to count blocked→running transitions only, and state the
  measured split in RFC-0113's evidence table. Leave the bound alone.
- If they are wakes on Agent traffic that carries no owned data (1): this is a
  D2.3 violation in the XRCE wait. Fix it in the backend or the park, or the
  RFC must state XRCE's session protocol as a measured, named exception.
- If they are park loops (3): fix the park.

The leaf needs branch `fix-2608-zephyr-witness-host-probe` (issue 2608's
witness split) to build. This work is based on it until it merges.
