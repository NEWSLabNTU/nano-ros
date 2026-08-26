---
id: 803
title: "POSIX transport band is announced but not applied — zenoh read/lease land on SCHED_FIFO 1, below every tier"
status: open
type: bug
area: platform
related: [issue-0765, issue-0623, issue-0506, rfc-0079]
---

## Problem

On POSIX, `nros-board-linux` states a reserved transport band of `90..99`
(RFC-0079, issue 0765) and prints it at boot:

    nros: transport tasks at SCHED_FIFO 90 (reserved band floor; most urgent tier is 80)

The kernel does not agree. With `cap_sys_nice+ep` granted, the zenoh read
and lease threads run on `SCHED_FIFO` at priority **1** — `sched_get_priority_min`,
the bottom of the range — while the application tiers correctly occupy 10 and 80.

So the band is inverted in exactly the way it was introduced to prevent: the
app preempts the link it publishes over. Issue 0623 for POSIX, one layer under
issue 0765's fix.

## Evidence

Four arms varying only `NROS_TMP_TRANSPORT_PRIO` (a temporary runtime override,
so one `setcap`'d binary measures every arm — a rebuild drops the file
capability). Entry confined to one CPU with `taskset`; three reps each; kernel
state read from `ps -L -o tid,cls,rtprio`.

| arm (requested) | main | transport ×2 | tier-high | ctrl msgs | telem msgs |
| --- | --- | --- | --- | --- | --- |
| 90 (the RFC floor) | FF 10 | **FF 1** | FF 80 | 1997–1998 | 198–199 |
| 40 | FF 10 | **FF 1** | FF 80 | 1997–1999 | 199 |
| 5 | FF 10 | **FF 1** | FF 80 | 1998–1999 | 197–199 |
| 0 (call skipped) | FF 10 | **TS** | FF 80 | 1998–1999 | 198–199 |

Two facts pin it down:

- **The requested value is discarded.** 90, 40 and 5 produce byte-identical
  kernel tables. The number never reaches the thread.
- **The code path is live.** Arm 0 skips `zpico_set_task_config` entirely and
  the threads stay `SCHED_OTHER`, so it is that call which puts them on
  `SCHED_FIFO` — at the floor's minimum rather than the requested priority.

`applied=2 eperm=0` in every arm: the capability is effective and both tiers'
`sched_setscheduler` calls succeed. This is not a privilege failure.

## Why it stayed hidden

The threads *are* `SCHED_FIFO`, so any check that asks "did the band apply?"
by looking at the POLICY says yes. Only the priority is wrong, and the boot
line reports the requested value rather than the achieved one — the process
tells you what it asked for, never what it got.

## Notes for the fix

`zpico_posix_fifo_set_priority` clamps into `[sched_get_priority_min,
sched_get_priority_max]` and sets policy, `PTHREAD_EXPLICIT_SCHED`, and the
param on the attr; `zpico_open` copies the attr into the session and
`zp_start_read_task` forwards it to `pthread_create`. Reading that chain does
not reveal where the value is lost — priority 1 is exactly what
`zpico_posix_rt_permitted()` puts on the *calling* thread while probing, which
is what an inheriting spawn would pick up. Instrumentation (`TEMP-0079-DIAG`)
is in the tree to report what the attribute actually holds and what policy the
creating thread has at spawn time; confirm the mechanism before fixing.

Whatever the mechanism, the fix must also make the boot line report the
**achieved** priority, not the requested one, so this class cannot recur
silently.

## Related measurement

The same sweep found no delivery difference between any arm, including the
`SCHED_OTHER` control — at ~2000 msgs / 20 s over loopback the tiers sleep
most of their period, so priority ordering never binds. Justifying the value
of the floor needs a saturating workload and a latency measure, not a message
count. That question is RFC-0079's open one and stays open.
