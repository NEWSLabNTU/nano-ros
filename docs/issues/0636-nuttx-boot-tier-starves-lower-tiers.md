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

## Measured 2026-08-21 — the coverage landed, and it says the C/C++ arm is NOT fixed

Both gaps this issue named are now closed as CELLS:

* `sched(TierPriority, NuttxArm, Cpp, Runtime)` — added by `91b623e57`.
* `cell(FreertosMps2, Rust, Zenoh, RealtimeTiers, Workspace, Runtime)` — "issue
  0636 gap 2 … This cell measures it."

Verified they RUN rather than exist, which is the distinction this issue exists
to enforce. In the ROS distrobox (the only environment here with both QEMU and a
router):

* **FreeRTOS Rust** — runs. `realtime_tiers` reports 16 ran / 9 skipped and
  `freertos/rust` is not among the skips. Gap 2 is genuinely covered.
* **NuttX C++ TierPriority** — runs, and **fails**:

```
TierPriority/nuttx/cpp: [nuttx cpp TierPriority] 1 of 2 declaring tiers produced
NEITHER `nros: tier priority set tier=` NOR `nros: tier priority FAILED tier=`
with their own declared priority: `low` prio=100 — accepted and dropped
```

**Rate: 8 pass / 4 fail over 12 consecutive runs (67 %).**

That is this issue's original signature — the `low` tier printing nothing —
one language over, and it is not a flake: 4 failures in 12 is the same order as
the Rust arm's history (1/5 → 3/5 → 4/6 → 23/32) before its fixes converged.

### What that means for the "Fixed 2026-08-21" section above

That section is candid that it could not measure itself: "this change is
justified by the same reasoning that justified `17666723d`, not by a measurement
of its own. A C/C++ TierPriority cell is the missing coverage." The cell arrived,
and the reasoning did not hold. Choosing the least urgent tier as session owner
was necessary on the C arm and is not sufficient.

So the Rust and C/C++ arms are now in different states, which is worth saying
plainly because the issue currently reads as though both are done:

| arm | state |
| --- | --- |
| NuttX Rust | 67/67, including 1.4x oversubscription |
| NuttX C/C++ | **8/12** |

### Note on reading the failure

The cell's message only appears with `--success-output immediate` on a PASSING
run; on a failing run it is in the normal output. Worth knowing before
concluding from a green summary that the cell did nothing — `sched_dims_applied`
is one test over many cells, the absorbing-verdict shape issue 0445 describes.

### Not diagnosed here

Why `low` still misses its marker on the C arm when the boot-tier choice is
fixed. The Rust arm needed THREE changes to converge (spawn-attribute priority,
the post-spawn yield, and the boot-tier choice); the C arm has had one of the
three. The other two are the obvious next candidates, and neither has been
checked on this arm.


## Diagnosed and fixed 2026-08-21 — the boot tier's marker sat behind its own spawn

The section above was right that "choosing the least urgent tier as session
owner was necessary and is not sufficient", and right to leave the cause open.
The cause is an ORDERING bug in `nuttx_run_tiers.c`, not a scheduling-policy
one, and it is visible from the emitted tier table.

The NuttX projection of the realtime-cpp bringup has TWO tiers, not three —
`mid` declares no `[tiers.mid.nuttx]`, so it is dropped:

```c
{ "high", …, 110LL, …, &__nros_entry_setup_tier_0, … },
{ "low",  …, 100LL, …, &__nros_entry_setup_tier_1, … },
… NuttxBoard::run_tiers(…, __nros_tiers, 2u);
```

With `boot_idx = n_tiers - 1`, **`low` IS the boot thread.** It is not a
spawned tier that failed to start — it is the caller. And the boot path ran in
this order:

1. boot's node setup (declares),
2. `nuttx_spawn_next_tier(...)` → creates `high` at `SCHED_FIFO` 110 with
   `PTHREAD_EXPLICIT_SCHED`,
3. `nros_nuttx_apply_current_priority(boot->name, …)` — the #579 marker.

Step 2 creates a thread that outranks the caller, which is still at the default
`app_main` priority. On this uniprocessor guest `high` preempts the boot thread
the instant `pthread_create` returns, so step 3 runs only once `high` first
blocks in `spin_once`. When that lands after the cell's deadline, `low` has
printed NEITHER marker and the cell reports it "accepted and dropped" — the
exact observed message. It is intermittent because *when* `high` first blocks
depends on the zenoh handshake and the transport threads' interleaving, which
is why it reads as a flake and is not one.

`high` was never starving `low` of the CPU, and `low` was never missing: the
tier was configured correctly and could not say so, which is precisely the
failure mode #579's marker rule exists to expose. My earlier
"Fixed 2026-08-21" claim was reasoning without a measurement, and the cell that
this issue asked for is what refuted it.

### The fix

Apply the boot tier's declared dims (priority + sporadic + affinity, the block
carrying the marker) BEFORE the spawn. Nothing in that block needs the children
to exist, and at that point no other tier thread exists to be starved by a
self-demotion, so the thread still owns the CPU. #144's ordering is untouched —
boot's DECLARES already ran above it.

The Rust arm does the MIRROR of this and is green for the mirror reason: it
spawns every tier from the boot thread in a loop, so it must keep its inherited
priority ACROSS that loop or it never gets the CPU back to finish spawning.
This arm chain-spawns — exactly one create here, and tier N brings up tier
N+1 — so there is no later spawn to protect. Same rule ("where the owner
applies its own priority is load-bearing"), opposite half, because the spawn
topologies differ.

### Measured — A/B on one host, same fixture, clean builds both sides

`workspace-cpp-nuttx-realtime` rebuilt from `rm -rf` on each side (an
incremental rebuild does NOT pick up `nuttx_run_tiers.c`, which is how a
previous verification on this issue went vacuous):

| build | `[nuttx cpp TierPriority]` |
| --- | --- |
| baseline (HEAD) | **10 pass / 2 fail in 12** — `low` prio=100 dropped, both times |
| with the fix | **30 pass / 0 fail in 30** |

The baseline reproduces the reported defect on a second host (they measured
8/12; 10/12 here), so this is the same defect and not a local artifact. Under
the baseline failure rate the fixed run is ~0.4 % likely by chance.

No regression on the arms sharing the file: `[nuttx cpp SporadicBudget] ACCEPT`
still holds, and `realtime_tiers_e2e` runs 17 rows / 0 failed with `nuttx-arm/c`
and `nuttx-arm/cpp` both among the rows that ran.

### Arm table, updated

| arm | state |
| --- | --- |
| NuttX Rust | 67/67, including 1.4x oversubscription |
| NuttX C/C++ | **30/30** (was 8/12) |

### Still open — the FreeRTOS C boot tier adopts no priority at all

Found while sweeping the sibling chain-spawn arms, NOT fixed here because
nothing measures it yet. `freertos_run_tiers.c`'s boot path applies the boot
tier's core pin (`freertos_apply_core_pin(NULL, …)`, correctly placed before
the spawn) but never its PRIORITY: `nros_freertos_set_current_task_priority` is
called only from the Rust arm (`nros-board-freertos/src/entry.rs`). So the
FreeRTOS C boot tier keeps whatever priority `app_task` was created at, and its
declared `[tiers.*.freertos] priority` does not hold for it. Whether that
currently starves anything depends on where `app_task` sits relative to the
spawned tiers and the 0623 transport band — which is exactly the "both
orderings are legitimate; choosing by accident is not" case. Wants a
`TierPriority`/freertos/c cell before a fix, for the reason this issue just
demonstrated twice.

## Re-measured on `330c8abfe` (2026-08-21) — better, not converged: the marker MOVED

`330c8abfe` ("the boot tier's marker sat behind its own spawn — 10/12 → 30/30")
improves the cell here and does not converge it. On the current tree, NuttX
fixtures rebuilt, in the ROS distrobox:

**9 pass / 6 fail over 15 consecutive runs.** Every failure is
`TierPriority/nuttx/cpp`, and the tier it names has changed:

```
before 330c8abfe:  `low`  prio=100 — accepted and dropped
after  330c8abfe:  `high` prio=110 — accepted and dropped
log: nros: tier priority set tier=`low` prio=100
```

So the fix did what it says — `low` prints now — and the boot tier `high` has
taken its place as the one that never reports. One marker is still missing per
failing run; which one moved.

That the missing tier swapped rather than disappeared is the useful part: it
argues the residue is not a printing bug in either tier's path but the ordering
between them, which is what this issue has been about from the start.

The 30/30 in that commit does not reproduce on this host. Both numbers can be
honest — this issue's own history records the rate moving with host load, and
CLAUDE.md records QEMU lanes flaking under load — but 6 failures in 15 is not a
tail, and the arms remain in different states: Rust 67/67 including 1.4x
oversubscription, C/C++ 9/15.

### Correcting my own previous entry

The section above this one reported "8 pass / 4 fail" and attributed it to the
C++ TierPriority cell. The count was real; the attribution was reached by
grepping run output, and that method is wrong here in two ways I then hit in
both directions:

* `sched_dims_applied` is ONE test over 23 cells, so a pass/fail tally counts
  the TEST, not the cell. Any cell failing produces the same red.
* My filter dropped lines containing `ACCEPT`, which removed the actual
  `N of 14 cell(s) FAILED` line and left an informational
  `[nuttx rust CorePin] FALLBACK` — leading me to report a CorePin regression
  that does not exist. `AcceptOrFallback` is that cell's declared shape:
  arm-virt is single-core with no `CONFIG_SMP`, so FALLBACK is a PASS there
  (#260 says so in the cell's own comment).

The reliable read is the panic body — `sched_dims: N of 14 cell(s) FAILED:`
followed by the cell name — not a grep over the run. Recorded because this is
the third time in this issue's history that a rate was collected from runs whose
verdict came from somewhere other than the cell under test.


## The residue was the TEST READER, not the seam — 2026-08-21

The re-measurement above is right that `330c8abfe` did not converge the cell,
and right that the useful signal is that the missing tier MOVED rather than
disappeared. The cause is one level out from where both of us were looking.

`sched_dims_applied_e2e` booted every QEMU cell with:

```rust
let l = q.wait_for_output_pattern(ex.stem, timeout);
q.kill();
```

`wait_for_output_pattern` returns as soon as the pattern appears, and the next
line kills QEMU. The stem is `"nros: tier priority"` — the prefix EVERY tier's
marker shares. So the reader returned at the FIRST tier to report and shot the
image; any later tier's line survived only if it happened to be in the same
buffer flush. **Whichever tier printed SECOND is the one that read as
"accepted and dropped".**

That is exactly the observed swap. Before `330c8abfe` the spawned `high`
printed first and `low` was cut; after it `low` prints first and `high` is cut.
The one-line log quoted in the section above says so directly — a whole boot
that produced a single line of output. Both hosts were measuring the reader,
and the rate moved with host load because buffer timing does.

### Measured

`wait_for_output_each(&[Vec<String>], timeout)` (new, `qemu.rs`) waits until
EVERY required marker has appeared, taking per-tier alternatives so a loud
failure still counts as "the tier reported". The cell derives its wait from its
own assert shape instead of from the stem — if the rule is "each declaring tier
reports", that is also the thing to wait for.

| build | `[nuttx cpp TierPriority]` |
| --- | --- |
| seam ordering reverted, wait fixed | **12 / 12** |
| seam ordering kept, wait fixed | **8 / 8** |

So the seam ordering in `330c8abfe` was **not** load-bearing for this cell, and
that commit's "10/12 → 30/30" credited it with a convergence it did not cause —
the 30/30 was a host whose buffering usually carried both lines. Corrected here
and in the code comment, which claimed the order was "the whole fix". The block
stays before the spawn: the preemption window it closes is real, and a dim
applied behind a spawn that outranks you is only correct by luck. It is
robustness, not the fix.

Same short read explains the freertos/cpp `mid` flake found while adding the
FreeRTOS arm below (2 of 8 → 8/8 with the wait repaired), and it was latent in
every multi-marker QEMU cell, so it is fixed in the shared helper rather than
per cell.

## FreeRTOS arm added 2026-08-21 — it printed no tier-priority marker at all

The "Still open" note above turned out to understate it. FreeRTOS emitted
NEITHER marker in EITHER language, so #579's "every declaring tier adopts its
priority or says why" was enforced on NuttX alone and this kernel was silently
exempt. With nothing printing, there was no cell that could be written, which is
why the boot task adopting NO priority at all had gone unnoticed:
`nros_freertos_set_current_task_priority` was called only from the Rust arm, so
a C or C++ boot tier kept whatever `app_task` was created at and its declared
`[tiers.*.freertos] priority` did not hold for it.

Added, mirroring the NuttX seam:

* `freertos_apply_tier_priority(name, priority)` — adopts on the CALLING task
  and announces, called from the spawn path AND the boot path, so the two
  cannot drift. Loud when a declared priority is `>= configMAX_PRIORITIES`,
  which `xTaskCreate` would otherwise clamp SILENTLY.
* The boot path adopts before its spawn, for the same window as NuttX.
* `freertos_announce_spawn_failure` — a downstream tier that never STARTS is
  the same silent drop. `freertos_tier_task` ignores the spawn's return by
  design (a failed child must not stop this tier spinning), so an out-of-heap
  `xTaskCreate` lost a whole tier without a word. It still continues; it no
  longer does so quietly. This is also what PROVED `mid` was healthy rather
  than unspawned.
* Cells `sched(TierPriority, FreertosMps2, Cpp|C, Runtime)`.
* `tier_priority_line` — the NuttX-named renderer became board-neutral, since
  both seams print the identical line and a second per-kernel spelling is how
  these two drifted to begin with.

Note the two bringups differ and the tier list follows the BRINGUP, not the
seam: `realtime-cpp` declares `high`/`mid`/`low` (5/3/2), `realtime-c` declares
only `high`/`low` (5/2) — there is no `[tiers.mid]` there. Asserting three tiers
on the C cell fails on a `mid` that was never emitted; that cost a debugging
round here, and the emitted `__nros_tiers[]` is the thing to read.

**Measured, 8 consecutive runs each, after the wait fix:**

| cell | result |
| --- | --- |
| `[freertos cpp TierPriority]` | **8/8** — 3/3 tiers ACCEPT |
| `[freertos c TierPriority]` | **8/8** — 2/2 tiers ACCEPT |
| `[nuttx cpp TierPriority]` | **8/8** — 2/2 tiers ACCEPT |

Both cells FAIL on the unfixed seam, so they are not vacuous: with
`freertos_run_tiers.c` reverted they report `boot produced no
'nros: tier priority set tier=' marker — the dim was silently dropped`.

## Re-measured 2026-08-22 on a clean tree — converged, and the sweep is complete

Fixtures rebuilt from scratch first (an earlier session left probe builds in
place, and a museum binary would have made any rate meaningless).

| measurement | result |
| --- | --- |
| `measure-tier-priority-cell.sh 12`, ambient (1-min load **12–18**) | **12 / 12** |
| every `TierPriority` cell, one `sched_dims_applied_e2e` run | nuttx rust 2/2, nuttx cpp 2/2, freertos cpp 3/3, freertos c 2/2 — all ACCEPT, 0 FALLBACK |

The load figure matters more than the count: every failing run in this issue's
history shared the host with a build, and these twelve ran at 12–18. With the
earlier 67/67 that is 79 consecutive passes across two hosts and three load
regimes.

**Option 2 is landed on every kernel**, which is what actually fixed this — the
session owner is no longer the most urgent tier anywhere:

| kernel | direction | how the boot tier is chosen |
| --- | --- | --- |
| NuttX | bigger wins | `boot_tier_index(BiggerIsMoreUrgent)` |
| Linux/POSIX | bigger wins | `boot_tier_index(BiggerIsMoreUrgent)` |
| FreeRTOS | bigger wins | `boot_tier_index(BiggerIsMoreUrgent)` |
| ThreadX | smaller wins | `tiers[0]` — least urgent because the sort is descending |
| Zephyr | smaller wins | `tiers[0]` — same (issue 0251 took this answer first) |

**Independent confirmation from issue 0736.** That investigation instrumented
the same NuttX image and counted the SPAWNED tier entering `spin_once` **1250
times in 45 s** while the boot tier spun 450. Before this issue's fixes the
spawned tier printed nothing in twelve seconds. So the tier is being scheduled,
measured from a different direction by someone not looking for it — which is
better evidence than this issue's own cell, since that cell only asserts a
marker printed.

(0736's remaining defect is one layer in and is NOT this: the spawned tier is
scheduled and then its timer is skipped by the executor's cooperative Sporadic
budget gate. Two different failures that both present as "the fast tier is
quiet"; the earlier guess here that they might be one defect was wrong.)

## The residue was a coincidence holding up the fix — now asserted

ThreadX and Zephyr take `tiers[0]`, and that is correct ONLY because both are
smaller-number-wins kernels and `resolve_tiers` sorts DESCENDING. Nothing tested
that sort. The three boards that compute their choice would follow a direction
change correctly; these two would silently boot the MOST urgent tier and
reintroduce this issue's starvation — on exactly the two kernels that have no
`TierPriority` cell.

Their chain-spawn walks `&tiers[1..]`, so making the index computed there is a
restructure rather than a one-line change. Pinning the property they depend on
is the cheaper half, and it was the half that was missing:
`resolve_tiers_returns_highest_priority_first` asserts the order and names both
boards and this issue in its failure message. Falsified — flipping the
comparator to ascending turns it red.

## What remains

Only **option 3** — "give the owner's spin a bounded blocking point so lower
tiers get a scheduled gap by construction rather than by transport luck". With
option 2 landed the owner is the least urgent tier, so it cannot starve the
tiers it spawned, and the starvation this issue was opened for is gone. Option 3
is now a robustness property rather than a fix: it would make the guarantee
structural instead of resting on the priority order being right.

Recommend closing on that basis and re-opening option 3 as its own item if a
consumer needs the stronger guarantee.
