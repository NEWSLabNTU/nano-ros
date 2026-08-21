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

**Those rates are not trustworthy at these sample sizes, and the corrected
reading is that the fixes are NOT MEASURABLY BETTER YET.** The final
arrangement was run in three batches on the same binaries: 10/10, then 6/10,
then 7/12 — cumulative 23/32 (72%) against a 6-run baseline of 3/6 (50%). The
batch-to-batch spread is wider than the effect being measured, and an unrelated
Zephyr build shared the host throughout. Anyone quoting "16/20" from an earlier
draft of this section is quoting the two good batches.

That is the trap this issue's own history records: two earlier partial fixes
went 1/5 -> 3/5 -> 4/6 and were read as progress toward a fix that never
arrived. A defensible before/after needs ~30 runs per arm on an idle host, and
has not been done.

**Both landed changes stand on their own merits, not on the rate.** `tiers[0]`
meaning "most urgent" on some kernels and "least urgent" on others, with the
session owner picked by index, is indefensible however the cell behaves;
`PTHREAD_EXPLICIT_SCHED` is what POSIX requires for a scheduling attribute to
apply at all. Three unit tests pin the first.

### What remains — and the diagnosis is now precise

The two orderings fail SYMMETRICALLY, which is the finding:

* **owner applies its priority BEFORE spawning** — it prints its own marker,
  then the console stops dead before the spawn line. `high` never exists.
* **owner applies AFTER spawning** — `high` runs, self-applies 110, sets its
  sporadic budget and registers; the console then stops before the owner's
  `apply_tier_priority`. `low` never prints.

Either way the OWNER is starved, once at its inherited priority and once at its
declared one. That points at what it is starved BY: with `high` at 110 spinning
(and a 5000/10000 µs sporadic budget that keeps it runnable), plus the zenoh
read task already spawned during session open, the owner's 100 is not above the
traffic. This is issue 0623 — a tier priority and a transport priority quoted in
different units landing in one scheduler — reached from the tier side. Fixing
0636 probably requires ruling on that relationship rather than reordering two
calls.

A read-back diagnostic in the POSIX port (compare `pthread_getschedparam`
against what was asked) was tried and REMOVED: it reported `high` at prio=1
policy=1 on a run where that task demonstrably ran at 110, so it is measuring
something other than the thread's effective priority on this NuttX config.

Other candidates, none tested:

* the C arm (`nuttx_run_tiers.c`) still takes its own boot tier and has not been
  moved onto `boot_tier_index`, so the two language arms now disagree about
  which tier owns the session;
* whether `PTHREAD_EXPLICIT_SCHED` is honoured by this NuttX config at all — the
  attribute path reports nothing, so a silent decline is indistinguishable from
  success (the marker is the only evidence, which is what #579 was for);
* host load: the whole series ran against a competing build, and this cell is
  QEMU.

## Measured 2026-08-21 — 67/67, including under saturating load

The section above says a defensible reading "needs ~30 runs per arm on an idle
host, and has not been done". Done now, on the current tree (which contains both
`17666723d` and `845637eff`), 48-core host:

| arm | runs | pass | 1-min load (mean / peak) |
| --- | --- | --- | --- |
| ambient (host busy with unrelated work) | 20 | 20 | 3.05 / 4.36 |
| ambient, second batch | 20 | 20 | 2.87 / 3.23 |
| + 8 spinners | 15 | 15 | ~13 |
| + 64 spinners (48 cores — oversubscribed) | 12 | 12 | 68.2 peak |
| **total** | **67** | **67** | |

The load arms are the point, not padding. Every failing run in this issue's
history shared the host with a build, and CLAUDE.md records that full-sweep QEMU
lanes flake under load — so a quiet-host pass would not have ruled the
starvation out, only masked it. At 1.4x oversubscription the cell still passes
12/12.

Against the last recorded rate of 23/32 (72%), `P(67/67 | p=0.72) = 2.8e-10`.
Even against a hypothetical 95%, `P = 0.032`. **The Rust arm's starvation is not
reproducible on this tree**; the three landed fixes converged, and the "NOT
converged" reading was small-n against a competing Zephyr build, exactly as that
section warned about itself.

What that does NOT license: the fixes' own justifications never rested on the
rate (`tiers[0]` meaning opposite things on different kernels is indefensible
regardless), and 67 runs on ONE host is not a claim about CI hardware.

Reproduce with `scripts/dev/measure-tier-priority-cell.sh <runs> [spinners]`.
It greps the cell's own `2/2 tiers ACCEPT` line rather than the aggregate test
result — `sched_dims_applied` is one test over many cells, so a SKIPPED nuttx
cell would otherwise read as a pass (the absorbing-verdict trap, issue 0445),
which is how a rate can be collected from runs that never ran the thing.

## Confirmed by inspection: the C arm still has the pre-fix arrangement

The candidate this issue lists as untested — "the C arm (`nuttx_run_tiers.c`)
still takes its own boot tier" — is real, and the contract makes it precise.
`nros-cpp/include/nros/main.hpp:481` documents what the emitter produces:

> `tiers` must be a non-null array of `n_tiers` `NativeTierSpec` entries
> **sorted highest-priority-first** (the codegen emitter produces them in that
> order)

and `nuttx_run_tiers.c:536` takes:

```c
const nros_tier_spec_t* boot = &tiers[0];
```

So on the C/C++ arm the session owner is the MOST urgent tier — the arrangement
`17666723d` removed from the Rust arm because it starves every peer under
SCHED_FIFO on a uniprocessor guest. The two language arms on one board now
disagree about which tier owns the session.

Not measured here: the matrix has only `sched(TierPriority, NuttxArm, Rust,
Runtime)`, so no cell asserts tier markers on the C/C++ arm at all. The
`nuttx cpp SporadicBudget` cell does exercise `nuttx_run_tiers.c`, so the path
runs — nothing checks this property of it.

### Fixed 2026-08-21

The C arm now chooses the least urgent tier too, and the two arms agree again.

Not by copying `boot_tier_index` into C — that is the cross-language rule
duplication this repo keeps paying for — but by using the ordering the emitter
already guarantees. `nros/main.hpp` documents the array as sorted
highest-priority-first, and NuttX is bigger-is-more-urgent, so the least urgent
tier is the LAST element. That also keeps the remaining tiers CONTIGUOUS, which
is what lets the existing chain-spawn walk them unchanged; picking an interior
index would have required rebuilding the array.

Relying on an ordering guaranteed elsewhere is exactly the kind of assumption
that rots quietly, so it is CHECKED: the loop verifies the table is
non-increasing and, if it is not, says so and falls back to index 0 — the
behaviour before this change. The alternative failure is silent starvation
seconds later on one platform, which is what this issue spent its history
chasing.

Verified as far as the tree allows, which is not far: `just nuttx
build-fixtures` RC=0, `[nuttx cpp SporadicBudget] ACCEPT` (the one cell that
exercises `nuttx_run_tiers.c`), `realtime_tiers_e2e` green, and the Rust cell
still 10/10. **Nothing asserts the starvation property on the C arm** — the
matrix has no `sched(TierPriority, NuttxArm, Cpp, …)` cell, so this change is
justified by the same reasoning that justified `17666723d` on the Rust side, not
by a measurement of its own. A C/C++ TierPriority cell is the missing coverage.

### FreeRTOS arm fixed 2026-08-21 — the last kernel that had it

`17666723d` wired `boot_tier_index` on `nros-board-nuttx` and
`nros-board-linux`, and recorded that "`freertos`, `zephyr`, `threadx` still
take `tiers[0]` (the last two already get the right tier from the sort)". That
parenthesis is right and it does not cover FreeRTOS:

| kernel | direction | `tiers[0]` after the descending sort |
| --- | --- | --- |
| Zephyr, ThreadX | smaller is more urgent | LEAST urgent — correct owner already |
| NuttX, POSIX, **FreeRTOS** | bigger is more urgent | MOST urgent — the starving arrangement |

So FreeRTOS was the one kernel left holding the defect, in BOTH arms —
`freertos_run_tiers.c:397` and `entry.rs`'s "finally run the highest-priority
tier (tiers[0]) on this task". Both now take the least urgent tier.

Mechanics as for the NuttX C arm, and for the same reason: these arms
CHAIN-spawn, handing each tier a `rest` SLICE, so skipping an interior index
would change that protocol. The least urgent tier is the LAST element of a
descending table, which keeps the remainder contiguous. That is why they do not
call `boot_tier_index` — same rule, different mechanics — and why the ordering
is CHECKED rather than assumed, with a loud line and a fall back to index 0 if
the table is not non-increasing.

Verified: `just freertos build-fixtures` RC=0; the `realtime_tiers_e2e`
freertos rows (C and C++, the multi-tier path this changes) RAN and passed;
`entry_e2e` freertos cells 4 ran, 0 failed.

**Not covered:** the matrix has `RealtimeTiers` cells for FreertosMps2 C and Cpp
only, so the Rust multi-tier arm is COMPILED by the lane and not RUN by any
cell. Same gap as the NuttX C arm above, one language over.

### Both coverage gaps closed 2026-08-21

This issue's two "not covered" notes are gone.

**NuttX C/C++ `TierPriority`** — see the section above. Closing it turned up
that the C arm never printed the marker at all.

**FreeRTOS Rust `RealtimeTiers`** — `Mps2An385Freertos::run_tiers` →
`run_tiers_entry` was exported, reachable from the `nros::main!` macro, and
called by NOTHING: every FreeRTOS realtime fixture was C or C++, and the one
Rust FreeRTOS entry (`workspaces/rust`) is single-tier `run_entry`. So this
issue's own FreeRTOS fix landed on that arm by reasoning. It now has a cell,
and the path works:

```
[INFO] Control::register on a tier admitting group `ctrl`
[INFO] Telem::register on a tier admitting group `telem`
[INFO] on_ctrl:  first publish OK (tier `high` is dispatching)
[INFO] on_telem: first publish OK (tier `low`  is dispatching)
Multi-tier setup complete — entering boot-tier spin loop.
```

`realtime_tiers_e2e`: 17 rows ran, 0 failed.

Three things the build taught that reading would not have:

* **The board link flags must live at the WORKSPACE `.cargo/config.toml`, not
  the leaf's.** The lane runs `cd <workspace>; cargo build -p <entry>`, so a
  leaf config is never on cargo's path from the CWD upward. The failure is not
  loud: the build SUCCEEDS and emits a 10 KB image with a zero vector table,
  which QEMU meets as `Lockup: can't escalate 3 to HardFault` with every
  register zero.
* **The locator is the slirp gateway `192.0.3.1`, not the `10.0.2.2` the pubsub
  Rust FreeRTOS entry uses.** The realtime lane boots QEMU with
  `net=192.0.3.0/24,host=192.0.3.1`; the pubsub address answers nothing.
* **The proof has to be order-independent.** `wait_for_output_pattern` CONSUMES
  the stream, and the boot tier here is `low` (100 ms) while `high` is 10 ms —
  so `high` publishes FIRST. A sequential wait on `low` then `high` ate `high`'s
  line and reported a tier that had dispatched as one that never did. The new
  `Proof::SerialDispatch` accumulates and only waits for what it has not seen;
  the assertion is "every tier dispatched", which says nothing about order.

`SerialDispatch` rather than the C/C++ cells' `SerialTicks`: the Rust realtime
nodes deliberately print no per-tick line (issue 0572 — a 10 ms tier would swamp
the console), so their marker is the first-publish one. That is the right anchor
for THIS issue anyway: the defect it keeps finding is a tier that is never
scheduled, and "this tier dispatched" is exactly that property.

One thing the run surfaced and did not fix: the 0623 transport-band diagnostic
fires on these priorities (`tier `high` at 5 >= transport floor 4 — this tier
PREEMPTS transport I/O`). The numbers are copied from `realtime-cpp`, so its
cells sit in the same arrangement. The diagnostic exists to make that a choice;
nobody has made it.

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
