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
