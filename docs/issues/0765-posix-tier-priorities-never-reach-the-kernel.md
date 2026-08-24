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

## Decided and implemented 2026-08-24 — apply, or say why not

Direction taken (maintainer): borrow play_launch's art — use `CAP_SYS_NICE`
where it is available, and WARN where it is not. Option (3), retiring the knob,
is off the table; the eleven pins become real.

### What was in the way, and it was not the privilege

`nros-platform-posix`'s `task_init` ALREADY sets `SCHED_FIFO` at the declared
priority — on the attribute with `PTHREAD_EXPLICIT_SCHED`, then again on the
running thread. Two things defeated it:

1. **The refusal was swallowed.** `(void) pthread_setschedparam(...)`. On a host
   without the privilege every call returns `EPERM` and nothing said so.
2. **The native board never goes through that path.** `nros-board-linux` spawns
   tiers with `std::thread::scope`, so `task_init` — and its attribute — is
   never involved for a native tier. The machinery existed and could not reach
   the case it was written for.

### Implemented

* `nros_posix_apply_current_priority(name, priority)` — the sibling of
  `nros_nuttx_apply_current_priority` and `freertos_apply_tier_priority`: one
  implementation per port, one marker, a refusal REPORTED. RAW SCHED_FIFO
  values (issue 0623's vocabulary); `0` means undeclared.
* Called at tier entry by `run_one_tier` AND `run_boot_tier`. The boot tier runs
  on the calling thread, which makes it the easiest to forget — and forgetting
  it is not hypothetical: the same omission was a live defect on FreeRTOS and
  cost issue 0636 a measurement round on NuttX.
* `EPERM` prints ONCE per process, with the remedy and the reason it must be
  re-applied:

```
[warn] nros: SCHED_FIFO is REFUSED for this process (EPERM), so every tier's
declared priority is INERT and the kernel runs them all at the default policy.
       Grant the capability to the binary that runs the tiers:
           sudo setcap cap_sys_nice+ep <executable>
       Re-run it after EVERY rebuild: a file capability is bound to the file's
       CONTENTS, so replacing the binary drops it. Or raise RLIMIT_RTPRIO.
```

* The board's standing note was CORRECTED rather than left. It said priority and
  core were both "advisory (not applied natively)". That is now false for
  priority, and a stale note claiming a declaration is inert while it is being
  honoured is worse than the silence it replaced.

### Measured

Native realtime entry against a live router, unprivileged host:

```
nros: NOTE — posix tier `core` is advisory (not applied natively); priority IS
      applied where the process may request SCHED_FIFO
[warn] nros: SCHED_FIFO is REFUSED for this process (EPERM) ...
nros: tier priority FAILED tier=`low`  prio=10 rc=1 — tier runs at inherited priority
nros: tier priority FAILED tier=`high` prio=80 rc=1 — tier runs at inherited priority
```

Both tiers report, boot tier included. `realtime_tiers` unchanged: 17 rows ran,
4 skipped.

One defect found in this very diagnostic, on the way. The markers were invisible
under a plain pipe and appeared only under `stdbuf -o0`: a tier's spin loop
never returns, and stdout to a pipe is block-buffered, so the line sat in a
buffer flushed at exit — i.e. never. Every e2e harness reads through a pipe, so
the diagnostic would have been unreadable exactly where it is needed. Fixed with
`fflush`. **A diagnostic nobody can read is not a diagnostic**, which is the
failure this whole issue is about, reproduced inside its own fix.

### NOT verified here, and it needs a privileged step

The REFUSAL path is verified end-to-end. The SUCCESS path is not: this host has
`ulimit -r` = 0 with a hard limit of 0, so `SCHED_FIFO` cannot be requested
without `CAP_SYS_NICE`, and granting it needs root — which this agent does not
do. What remains is one command by someone who can run it:

```
sudo setcap cap_sys_nice+ep <the built native_entry>
```

then re-run the entry and expect `tier priority set tier=...` in place of
`FAILED`. Until that is done, "priority IS applied where permitted" rests on the
`pthread_setschedparam` contract and on the same code path working on NuttX, not
on a measurement of the applied case.

## Success path VERIFIED 2026-08-24 — and it immediately creates a POSIX band problem

The privileged step was run (`setcap cap_sys_nice+ep` on the built
`native_entry`), so the half that rested on the pthread contract is now
measured.

**Our own markers:**

```
nros: tier priority set tier=`low`  prio=10
nros: tier priority set tier=`high` prio=80
```

**The kernel's own view, which is the one that counts** — play_launch's
verification recipe, `ps -eLo tid,cls,rtprio,comm`, taken while the entry ran:

```
2227128  FF     10 native_entry      <- boot tier `low`, on the calling thread
2227133  FF     80 nros-tier-high    <- spawned tier
2227129  TS      -  native_entry
2227131  TS      -  native_entry
```

`FF` is SCHED_FIFO and `rtprio` matches each declared value exactly. Both arms
are proven, including the boot tier — the one that runs on the calling thread
and was the easiest to leave out.

The capability is bound to the binary's CONTENTS, so the very next fixture
rebuild dropped it and the unprivileged path returned. That is the behaviour the
warning text describes, observed rather than assumed, and it means an
unprivileged `just ci` keeps working with inert priorities and a loud line —
which is the intended arrangement, not a gap.

### The finding: POSIX now has RFC-0079's inversion, and cannot fix it

Read the `TS` rows again. With tier priorities real, the tiers run SCHED_FIFO at
10 and 80 while every other thread in the process — including zenoh-pico's read
and lease tasks — stays on SCHED_OTHER. **A SCHED_FIFO thread outranks every
SCHED_OTHER thread unconditionally**, so both tiers now preempt the transport
they publish over.

That is issue 0623's inversion, arriving on POSIX the moment priorities stopped
being advisory. It was not reachable before, because nothing was applied.

And POSIX cannot currently answer it: `zpico_set_task_config` DISCARDS the
priority on Linux/macOS (the `#else` arm this issue opened with), so the
transport cannot be raised to meet the tiers even by an operator who wants it
to. A POSIX `[board.priority_plan]` therefore needs BOTH halves — a transport
band that can be set, and a pool below it — and only the second exists.

So the status is: the knob is real and honest, and the port is not finished.
RFC-0079's POSIX row stays open with a sharper question than it started with —
not "can priorities apply here" (yes, measured) but "what reserves the transport
when the tiers are FIFO and the transport is not".
