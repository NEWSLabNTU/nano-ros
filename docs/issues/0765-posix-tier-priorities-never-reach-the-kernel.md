---
id: 765
title: "`[tiers.*.posix] priority` is advisory — 11 pins that never reach the kernel, and the privilege argument that kept them there is solved in play_launch"
status: open
type: limitation
area: boards, orchestration
related: [rfc-0079, issue-0506, issue-0623]
---

## What is true today

Eleven `[tiers.<name>.posix] priority` pins exist across the bringups. None of
them reaches the Linux scheduler. `nros-board-linux` says so at boot, once,
whenever a tier declares one:

```
nros: NOTE — posix tier priority/core are advisory (not applied natively);
scheduling is the executor's SchedContext
```

The board calls no `sched_setscheduler` and no `sched_setaffinity`. The numbers
feed exactly two things: the executor's own `SchedContext` ordering, and
`boot_tier_index`. Symmetrically, `zpico_set_task_config` DISCARDS the priority
on Linux/macOS — the `#else` arm that also held NuttX until issue 0736.

So on POSIX there is no kernel address space for RFC-0079 §4 to describe, and a
`[board.priority_plan]` written in kernel priorities would be fiction. That is
why POSIX is the one port the collision report calls `UNPLANNED` for a reason
other than "nobody got to it".

## The stated reason, and why it no longer holds

The code gives one:

> strict ordering needs `SCHED_FIFO` + privileges

True as far as it goes — `RLIMIT_RTPRIO` is 0 for a normal user. It has been
read as "so POSIX cannot have real priorities", and that conclusion is wrong,
because **the sibling repo in this very tree already does it without root.**

`play_launch` (`packages/cli/third-party/play_launch`) applies `SCHED_FIFO`/
`SCHED_RR`, RT priority and CPU affinity to every thread of every process it
launches, unprivileged:

```
play_launch  (uncapped, links ROS, runs as you)
    │  ApplySched{pid, tier}   — pipe IPC, once per process at spawn
    ▼
play_launch_rt_helper  (ROS-free, holds CAP_SYS_NICE only)
    → sched_setscheduler / sched_setaffinity on every thread of that pid
```

A file capability on a small ROS-free helper, granted by `play_launch setcap`,
re-granted after every rebuild because a capability is bound to the binary's
contents. Nothing runs as root. Their guide is explicit: *"No `sudo` anywhere.
Nothing runs as root."*

So the privilege problem is solved, in-tree, by a component nano-ros already
vendors.

## Why nano-ros's case is NOT identical

Worth stating before anyone copies the helper wholesale:

* **play_launch schedules PROCESSES it spawns; nano-ros schedules THREADS it
  owns inside one process.** The helper's `ApplySched{pid, tier}` shape assumes
  a supervisor outside the target. nano-ros's tiers are `std::thread`s in the
  same image, so the equivalent is `pthread_setschedparam` on itself — which
  needs the capability on the nano-ros process, not on a helper it talks to.
* **nano-ros has in-process system tasks and play_launch does not.**
  zenoh-pico's read and lease threads are pthreads in the nano-ros process. So
  unlike the Linux case play_launch models — where the transport is somebody
  else's process — nano-ros on POSIX has exactly the reserved-band problem
  RFC-0079 describes for the RTOSes. A POSIX plan therefore needs
  `reserved.transport`, and today nothing can be said about it because nothing
  is applied on either side.

## What this blocks

RFC-0079 §4 for `posix`: 11 of the 19 remaining `UNPLANNED` pins.
`check-tier-priority-plan` cannot judge them, and `nros ws model-dims` cannot
show a user where their tiers landed, because they land nowhere.

## Not decided — this is the discussion, not the answer

1. **Adopt a capability helper** (play_launch's shape, adapted from
   process-scheduling to self-scheduling), and make POSIX tiers real.
2. **Require `CAP_SYS_NICE` on the nano-ros process** and degrade loudly
   without it — simpler, but pushes a deployment requirement onto every user
   of the native lane, including tests.
3. **Keep POSIX advisory and say so louder** — retire `[tiers.*.posix]
   priority` as a user-facing knob entirely, since eleven numbers that do
   nothing are worse than none. The executor's `SchedContext` ordering would
   become the only POSIX story, and RFC-0079's contract (`profile`, `period`,
   `deadline`) would drive it.

(3) deserves more weight than it looks: RFC-0079 already argues the user should
not be writing kernel numbers, and on POSIX they are writing numbers that reach
no kernel at all.
