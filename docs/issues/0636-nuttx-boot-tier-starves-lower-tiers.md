---
id: 636
title: "The NuttX boot tier holds the highest declared priority and spins, so on
  a uniprocessor guest every lower tier is starved for seconds"
status: open
type: bug
area: boards, platform
related: [issue-0579, issue-0623, issue-0246, phase-358, phase-364, phase-359]
---

## Symptom

`sched_dims_applied_e2e`'s `TierPriority/nuttx/rust` cell fails intermittently —
measured **1 of 5 runs passing** on 2026-08-16, solo, on an unloaded host. The
assertion is #579's: every DECLARING tier must print its own priority marker.
`high` (the boot tier, 110) prints; `low` (spawned, 100) prints nothing at all.

The captured guest console on a failing run ends like this:

```
nros entry ready
nros: multi-tier run — 2 tier(s) over one session
nros: boot tier `high` (session owner) — groups ["ctrl"], … spin 1000 us, priority 110
[INFO] Control::register on a tier admitting group `ctrl`
[INFO] Telem::register on a tier admitting group `telem`
nros: spawning tier `low` — groups ["telem"], class None, spin 10000 us
nros: tier `high` declares a sporadic budget but is the session-owning boot tier — kept SCHED_FIFO
nros: tier priority set tier=`high` prio=110
nros: tier `high` entering spin — wake primitive available (16 byte(s))
```

There is no `spawn tier 'low' attempt … failed` line — the spawn SUCCEEDED.
`low` simply never reaches its first statement.

## Why this is not a flake

The guest runs for ~12 s in each of these runs. `low` is not losing a tight
race at startup; it is not scheduled AT ALL for twelve seconds. The cell's
`timeout_secs` is 90, so widening the window does not help — nothing is
pending, the tier is starved.

The mechanism is structural, not incidental:

- `arm-virt` is uniprocessor here, and every tier is `SCHED_FIFO`.
- The boot tier is the session owner. It holds the HIGHEST declared priority in
  the set (110 against `low`'s 100 — the author's intent, correctly applied).
- Under `SCHED_FIFO` a lower-priority peer runs only when the holder BLOCKS.
  The owner's `spin_once` does not reliably block, so the only gaps are
  whatever the shared zenoh-pico session happens to produce.

So "`low` printed its marker" is really "the owner's spin blocked at least once
in 12 s", which is a property of the transport's timing, not of the scheduler
policy the test means to assert.

## What was already tried (partial, landed)

Two fixes reduced the failure rate without removing it. Both are correct in
their own right and are worth keeping; neither addresses the starvation.

1. **The spawn attribute now carries the tier's declared priority**
   (`sys::spawn_tier`, `NROS_PLATFORM_PRIORITY_RAW`). The board passed
   `PRIORITY_INHERIT` and left the tier to self-apply at entry — so a tier was
   BORN at the spawning (boot) tier's priority and stayed there until it first
   ran, which is the interval in question. phase-364 W5 taught the POSIX port
   to honour `attr->priority`; this call site had not been moved onto it.
   Measured 1/5 → 3/5.

2. **The owner yields once per tier after spawning, before raising itself.**
   At that point it still holds its inherited priority, so it and the new tiers
   are peers and a yield runs each to its first blocking point. Measured 3/5 →
   4/6.

That the rate improves and does not converge is the evidence for the diagnosis:
these shorten the starvation window, and the window is unbounded.

## Options (none chosen)

1. **Print the marker where the priority is actually SET.** Now that the attr
   carries it, the create-time path in the port knows the value and runs on the
   spawner's thread — so the fail-loud evidence #579 asks for stops depending
   on the child being scheduled. Cheapest, and it makes the assertion measure
   the thing it names.
2. **The session owner must not be the highest-priority tier.** Zephyr already
   takes this answer (issue 0251: sort so `tiers[0]` is lowest-priority and
   never needs to outrank anything). It is a real semantic change on NuttX.
3. **Give the owner's spin a bounded blocking point** so lower tiers get a
   scheduled gap by construction rather than by transport luck.

Option 1 fixes the TEST honestly; options 2 and 3 fix the RUNTIME. They are not
alternatives to each other — a consumer whose low tier is starved for seconds
has a real problem whether or not a marker printed.

## Partly fixed 2026-08-20 — 50% -> 80%, NOT converged

Three changes landed, each correct on its own; the cell is still not reliable
and this issue stays open. Measured on one host, with an unrelated Zephyr build
competing for CPU throughout, so treat the absolute rates as this machine's.

**1. The boot tier is CHOSEN, not `tiers[0]` — and this was never NuttX-only.**
`resolve_tiers` orders by RAW priority descending and deliberately does not
invert per kernel, so `tiers[0]` is the MOST urgent tier on bigger-number-wins
kernels (NuttX, FreeRTOS, POSIX) and the LEAST urgent on smaller-number-wins
ones (Zephyr, ThreadX). Its own doc says so. Zephyr was not "taking a different
answer" as this issue supposed — it gets the non-starving arrangement as a side
effect of the sort direction. Which tier owned the session depended on the
kernel's number direction, which nobody chose.

`nros_platform::boot_tier_index(tiers, direction)` now picks the least urgent
tier, with the direction supplied by the board. Ties keep index 0, so a table
whose tiers are all equal behaves exactly as before. Wired on `nros-board-nuttx`
and `nros-board-linux`; `freertos`, `zephyr`, `threadx` still take `tiers[0]`
(the last two already get the right tier from the sort, so for them the call is
about stating the invariant, not changing behaviour).

**2. The POSIX port sets the child's priority on the pthread ATTRIBUTE.** It
applied it after `pthread_create` and discarded the result. That leaves a window
where the child holds the SPAWNER's priority, and under SCHED_FIFO on a
uniprocessor an equal-priority peer never preempts — so a child that should have
outranked the owner ran only when the owner happened to block. Now
`PTHREAD_EXPLICIT_SCHED` + `setschedpolicy` + `setschedparam` before create, so
the task is BORN with it; the post-create call stays as a fallback for a kernel
that declines the attribute. Without `EXPLICIT_SCHED` POSIX says the child
inherits the creator's policy AND priority and the attribute is ignored — a
silent no-op, not an error.

**3. Where the owner applies its own priority is load-bearing, and both
neighbours were measured.** Before the spawn loop: 4 of 8, and the failing
console stopped dead after the owner's own marker, before it had spawned
anything — dropping to the least urgent priority while the tier topology does
not yet exist puts the owner below the transport and system tasks already
running, and it never gets the CPU back. After the spawn loop: the rate above.
The `yield_now()` run is deleted; it existed to hand the CPU to tiers the owner
was about to outrank, and under SCHED_FIFO a yield never lets a lower-priority
thread run at all.

### The series, in order

| arrangement | rate |
| --- | --- |
| baseline (as this issue left it) | 3/6 |
| + boot tier chosen | 6/8 |
| + owner applies its priority BEFORE spawning | 4/8 (worse) |
| + priority on the pthread attr, owner applies AFTER spawning | 16/20 |

**The last row's first ten runs were 10/10 and the next ten were 6/10.** A
ten-run batch is not enough to call this converged, and reporting the good batch
alone would have been the mistake this issue's own history warns about — two
earlier partial fixes were recorded as 1/5 -> 3/5 -> 4/6 for the same reason.

### What remains

The residual failure is still `high` (the spawned, more urgent tier) missing its
marker, on a console that otherwise looks healthy. Candidates, none tested:

* the C arm (`nuttx_run_tiers.c`) still takes its own boot tier and has not been
  moved onto `boot_tier_index`, so the two language arms now disagree about
  which tier owns the session;
* whether `PTHREAD_EXPLICIT_SCHED` is honoured by this NuttX config at all — the
  attribute path reports nothing, so a silent decline is indistinguishable from
  success (the marker is the only evidence, which is what #579 was for);
* host load: the whole series ran against a competing build, and this cell is
  QEMU.

## Relationship to 0623

Same family, one layer over: 0623 is a tier priority and a transport priority
quoted in different units landing in one scheduler. This is a tier priority and
a *spin loop* landing in one uniprocessor, where the ordering the author wrote
is applied exactly and the result is starvation. Both are "the ordering is
right and the outcome is wrong".

## Reproduce

```
bash scripts/build/workspace-fixtures-build.sh nuttx rust
for i in $(seq 5); do
  cargo nextest run -p nros-tests --test sched_dims_applied_e2e --retries 0
done
```

Read the `log:` block in the panic — a failing run's console stops at
`tier 'high' entering spin`.
