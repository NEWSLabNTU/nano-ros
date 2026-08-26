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

## Narrowing, 2026-08-26 — the attr is NOT where the value is lost

Two things are now established without privileges, and together they eliminate
the hypothesis this issue proposed.

### 1. The attribute carries the requested priority, through the copy

`zpico_set_task_config`'s exact POSIX sequence, replayed standalone — `memset`,
`pthread_attr_init`, then `zpico_posix_fifo_set_priority`'s three setters, then
the session's `s->read_task_attr = g_default_read_task_attr` struct assignment:

```
FIFO range: [1, 99]
after init                   policy=OTHER inherit=INHERIT prio=0
setters rc: policy=0 inherit=0 param=0
after set_priority(90)       policy=FIFO inherit=EXPLICIT prio=90
after struct copy            policy=FIFO inherit=EXPLICIT prio=90
```

All three setters return 0, and the byte-copy of a glibc `pthread_attr_t`
preserves policy, inherit-mode and param. This needs no capability: setting
fields on an attribute object never does, which is why it is checkable on a host
where the real path cannot run at all.

So neither the `memset`-before-`pthread_attr_init` nor the struct assignment
loses the value. (The copy does duplicate glibc's internal extension pointer,
which is a latent double-free hazard on `pthread_attr_destroy` — worth its own
look, but not this bug.)

### 2. The forwarding chain does not drop it either

Read at the pinned zenoh-pico commit:

```
zpico_open:   read_opts = s->read_task_configured ? &s->read_task_opts : NULL
zp_start_read_task(api.c:2152)      -> _zp_start_read_task(.., opt.task_attributes)
_zp_start_read_task(session.c:450)  -> _zp_unicast_start_read_task(&zn->_tp, attr, task)
_zp_unicast_start_read_task(read.c) -> _z_task_init(task, attr, ..)
_z_task_init(unix/system.c:131)     -> pthread_create(task, attr, fun, arg)
```

Nothing nulls, rewrites or re-inits the attribute along it.

### 3. Therefore the stated hypothesis is out

The Notes say priority 1 "is exactly what `zpico_posix_rt_permitted()` puts on
the calling thread while probing, which is what an inheriting spawn would pick
up". A spawn inherits only when the attr is NULL or carries
`PTHREAD_INHERIT_SCHED`. Section 1 shows it carries `PTHREAD_EXPLICIT_SCHED`,
and section 2 shows it arrives.

**Unless `read_task_configured` is false** — then `read_opts` is NULL, the
attribute never reaches `pthread_create`, and the thread inherits exactly as
described. That is the one branch left standing, and it is what the next
instrumented run should print first: `g_default_read_task_configured` and
whether `read_opts` is NULL at the `zp_start_read_task` call.

Two other candidates eliminated while here: the weak `zpico_set_task_config`
stub is FreeRTOS-only (`nros-board-freertos/c/freertos_c_entry.c`, not linked on
Linux — the phase-386 weak-body class does not apply), and `nros-board-linux`
passes a literal `TRANSPORT_BAND_FLOOR = 90`, so nothing normalises it on the
way in.

### Why it was not confirmed here

This host cannot run the path at all: `RLIMIT_RTPRIO` is 0 soft / 0 hard and
there is no `CAP_SYS_NICE`, so `zpico_posix_rt_permitted()` returns 0 and
`zpico_posix_fifo_set_priority` early-returns. Confirming needs
`setcap cap_sys_nice+ep` on the entry binary, which needs root.

### Correction to this issue's Notes

`TEMP-0079-DIAG` is **not in the tree** — `git grep` finds neither it nor
`NROS_TMP_TRANSPORT_PRIO`, the runtime override the four measured arms varied.
Whoever picks this up cannot re-run the table as written without restoring both.
