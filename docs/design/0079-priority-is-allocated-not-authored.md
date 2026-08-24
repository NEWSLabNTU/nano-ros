---
rfc: 0079
title: "Scheduling priority is ALLOCATED from a declared address plan — the user states a timing contract, never a kernel number"
status: Draft
since: 2026-08
last-reviewed: 2026-08
implements-tracked-by: [issue-0506, issue-0623, issue-0736]
amends: [rfc-0016, rfc-0052]
supersedes: []
superseded-by: null
---

# RFC-0079 — Priority is allocated, not authored

## The problem, counted

`system.toml` asks the user to write a raw kernel priority per platform. Across
the five bringups that declare tiers today:

| | count |
| --- | --- |
| tiers declared | **11** |
| raw platform priority pins | **38** |
| `spin_period_us` authored | **11 of 11 tiers (100 %)** |
| `deadline_us` authored | 4 |
| `period_us` authored | 2 |
| `budget_us` authored | 2 |
| `deadline_policy` authored | **0** |
| `stack_bytes` authored | **0** |

Two readings, and both are damning.

**One logical tier costs 3.5 kernel numbers.** To write them correctly the
author must know each kernel's range, each kernel's DIRECTION (Zephyr counts
down, FreeRTOS and NuttX count up), and where every system task already sits.
The last is written down nowhere. It had to be measured: FreeRTOS ships a
transport band at 4/4/4; NuttX's zenoh read/lease threads inherit whichever
thread opened the session; until issue 0736 NuttX could not state a transport
priority at all, because `zpico_set_task_config` discarded it.

**The field authored every time is the one that should not exist, and the
fields that should drive everything are mostly absent.** `spin_period_us` — an
executor implementation detail — is on 100 % of tiers. The actual real-time
contract (`period`, `deadline`) is on a minority. The user is writing the
mechanism and omitting the intent.

### This is a DHCP/static-IP collision

One address space; static assignments authored by hand; addresses already held
by infrastructure; and **no reservation protocol**. Every failure this RFC
tracks is that collision:

* **issue 0623** — a tier written as `5` sat ABOVE a transport band reading
  `16`, because the two were quoted in different units into one scheduler. A
  static IP on the router's address.
* **issue 0736** — tiers at NuttX FIFO 110/100 against transport threads at the
  inherited 100, with no way to state otherwise. A pool with no reservations
  declared.
* **issue 0506** — "transport above application tiers is the right default but
  has no budget." The question is unanswerable while the address plan is
  implicit.

## The design

### 1. The user declares a timing contract

The whole taught vocabulary:

```toml
[tiers.control]
profile  = "realtime_periodic"
period   = "10ms"
deadline = "10ms"        # optional — defaults to `period`
on_miss  = "report"      # optional
```

No kernel numbers. No platform sections. No direction to know.

### 2. Profiles are presets over an expressive core

Named for the SCHEDULING SHAPE, never an application role — a preset called
`control` would bake in a scenario, and ROS spans manipulation, AMRs, drones,
perception rigs and recorders.

| profile | shape |
| --- | --- |
| `realtime_periodic` | hard deadline, periodic, bandwidth-bounded |
| `responsive` | event-driven, latency-sensitive, sporadic |
| `throughput` | bulk work, preemptible |
| `background` | opportunistic, may starve |

This is deliberately the shape ROS 2 QoS already has, and which this codebase
already implements (`QosSettings::services_default()`): a small set of named
profiles over a fully expressive struct, profiles taught first. Users arrive
understanding it.

### 3. ORDER is derived from the contract, not stated

Priority order is **deadline-monotonic**: shorter deadline ⇒ more urgent. Two
tiers at 10 ms and 100 ms order themselves. Rules, in order:

1. Explicit relative constraints (`above = "telemetry"`) — a partial order.
2. Deadline-monotonic over declared `deadline`.
3. Period-monotonic where `deadline` is absent (implicit-deadline task).
4. Ties break by **declaration order**, stated because a nondeterministic
   assignment is a fixture-flake generator.

### 4. The PORT declares an address plan

Authored in the board crate by whoever ports the RTOS — never by the user:

```toml
[board.priority_plan]
direction = "bigger-is-urgent"
range     = [1, 255]
reserved.transport = [200, 210]   # rx, lease, flush
reserved.driver    = [211, 220]
reserved.foreign   = [230, 255]   # lwIP tcpip_thread, NuttX work queues, idle
pool.app           = [10, 190]
```

`reserved.foreign` records tasks nano-ros does not create. They are FACTS about
the port; a wrong one is a porting bug, and must be verifiable rather than
asserted.

### 4.1 A band is STATIC or DERIVED, and Zephyr forces the second kind

Three ports state a literal read off the port — FreeRTOS 4, NuttX 100, ThreadX
14. Zephyr cannot, and that is a property of the port rather than of anyone's
diligence.

Zephyr's tiers are RAW `k_thread` priorities passed to `k_thread_create`. Its
transport is not: zenoh-pico's Zephyr platform builds its read and lease tasks
with `pthread_create`, so the priority travels a chain before it lands anywhere
a tier can be compared against —

```
CONFIG_NROS_ZENOH_{READ,LEASE}_PRIORITY   (Kconfig, default 16)
  → ZPICO_{READ,LEASE}_TASK_PRIORITY      band 0..31
  → POSIX   lo + (span·n·2 + 31)/62,  hi = CONFIG_NUM_PREEMPT_PRIORITIES − 1
  → k_thread   NUM_PREEMPT − posix − 1   (SCHED_RR, `POSIX_TO_ZEPHYR_PRIORITY`)
```

— and BOTH ends are per-image Kconfig: the band itself, and
`NUM_PREEMPT_PRIORITIES`, which is per-board. A literal
`reserved.transport = [7, 7]` would be true for exactly one image and quietly
wrong for the next. **That is this RFC's own defect, one level up**: a number
written once, in a place that cannot see what it depends on.

So a plan declares one of two kinds of band:

* **STATIC** — `reserved.transport = [4, 4]`, plus the source it was read from.
  Checkable by `check-tier-priority-plan` with nothing but the repo.
* **DERIVED** — no numbers at all. The descriptor names what the band depends
  on and who resolves it:

```toml
[board.priority_plan]
tier_key  = "zephyr"
direction = "smaller-is-urgent"
derived   = "zephyr"
resolver  = "scripts/lib/priority_plan.py:resolve_zephyr_plan"
inputs    = ["CONFIG_NUM_PREEMPT_PRIORITIES", "CONFIG_NROS_ZENOH_READ_PRIORITY", …]
```

Two rules make the derived kind honest rather than an excuse:

1. **DEFERRED is not UNPLANNED.** The static checker reports a derived port's
   pins as deferred and names the resolver and the command that finishes the
   job. "Checked elsewhere, here is where" and "nobody checks this" are
   different states and must read differently — collapsing them is how the
   unchecked pins got to 21 in the first place.
2. **Unapplied is not a band.** If the image's Kconfig gates
   (`CONFIG_POSIX_PRIORITY_SCHEDULING`, `CONFIG_PREEMPT_ENABLED`) are off, the
   priority is never applied, the tasks INHERIT their creator, and there is no
   band to reserve. The resolver returns that as its own verdict rather than a
   number — the same rule as "absent is not a budget", and the state NuttX was
   in before issue 0736.

`check-tier-priority-plan-image.py <build>/zephyr/.config` resolves and judges.
Against the `ws-rs-realtime-entry` image
(`NUM_PREEMPT_PRIORITIES=15`, both gates on):

```
read   band  16 -> posix   7 -> k_thread   7
reserved.transport = [7, 7]   pool.app = [8, 14]   range = (-16, 14)
```

and it finds four real violations — `tiers.high.zephyr = 5` outranks the
transport in every bringup that has it, undeclared, exactly as every other port
did before its plan landed.

**The pins are not moved yet, deliberately.** `realtime_tiers`' `zephyr/rust`
row fails for an unrelated, pre-existing reason ("low-tier /telem never reached
5 deliveries — the low tier was not scheduled"), so the one cell that could
validate a move cannot show green either way. Moving them would be a change
justified by reasoning rather than measurement, which this issue's siblings have
already cost enough to make the rule obvious.

Worth recording that the obvious hypothesis was TESTED and REFUTED: `high` (5)
outranking the transport (7) which outranks `low` (10) looks like an exact
explanation for "low was not scheduled", so `high` was moved to 9 — inside the
pool, below the transport — the image rebuilt, and the row fails identically.
Whatever starves that tier, it is not this.

### 5. The realizer allocates

Ordinal sequence → concrete numbers inside `pool.app`, in the port's direction,
spread with headroom, never overlapping a reservation. The result lands in the
SystemModel build artifact (already a build artifact — RFC-0063) and is
inspectable with `nros ws model-dims`. Insufficient distinct slots is a build
error naming the pool, not a silent squeeze.

### 6. Crossing into a reserved band requires naming it

The legitimate rare case — a hard safety loop that must preempt networking:

```toml
[tiers.safety]
profile = "realtime_periodic"
period  = "1ms"
above   = "transport"          # names a SYSTEM band, deliberately
```

The realizer honours it and reports the consequence, or refuses. This makes
0623's rule structural: **both orderings stay legitimate, and neither can be
chosen by accident**, because the only way to get one is to name it.

## The primitive reduction

Real-time scheduling has a canonical triple — (C, T, D): execution time,
period, deadline. Every tier field today is one of those, derived from them, or
a leak.

**Authored:** `profile`, `period`, `deadline` (defaults to `period`), `on_miss`.

**Derived, never authored:**

| today | derived from |
| --- | --- |
| `budget_us` | Σ WCET of the tier's callbacks (RFC-0078) + headroom |
| `spin_period_us` | the tier's fastest release |
| `class` / `sched_class` | which of the above are present |
| `priority` (×5 platforms) | §3 order + §5 allocation |
| `preempt_threshold`, `time_slice_us` | the realizer (ThreadX) |

**Expert overrides — checked, outside the taught path:** an explicit priority
pin, `core`, `stack_bytes`.

Six timing knobs become two numbers.

### Why `budget_us` in particular must go

RFC-0078 establishes that an execution-time bound **belongs to a context, not
to code**, and is declared per measurement profile. `budget_us` is a SECOND
declaration of the same physical quantity — hand-written, in a different file,
with nothing reconciling the two.

Issue 0736 is that divergence: `budget_us = 5000` against callbacks measuring
2000–4000 µs. Neither number was measured. The tier dispatched on 3 of 1250
spins.

Under this RFC the field does not exist. A budget comes from a declared WCET or
there is no budget — and **absent means fixed-priority with no bandwidth
server**, not a guessed allowance. RFC-0078's "absent is not zero" becomes
"absent is not a budget". Issue 0736 could not have been written.

### Naming defects fixed while here

* **Two different "periods".** `period_us` (release interval) and
  `spin_period_us` (executor poll rate) sit side by side meaning unrelated
  things, and their interaction IS issue 0736's surface — a 1 ms spin against a
  10 ms timer. Deriving the spin rate removes the field and the confusion at
  once.
* **"class" means three things in one file format**: a tier's scheduling class,
  a component's C++ `class_name`, and `sched_class` on the resolved table.
  Measured: 2 tier `class` against 75 component `class` in the same files.
* **`deadline_policy` → `on_miss`**, and it is authored 0 times today, so
  renaming costs nothing.
* **Units in the name.** `_us` forces microsecond integers; `period = "10ms"`
  is a duration. Folded in here rather than deferred, because migrating the
  field names twice is worse than once.

## Amendment and retirement plan

This RFC does not delete its predecessors; it narrows them.

**RFC-0016 (Stable) — "normalized 0–31 priority".** Its normalization is
RETIRED as a user-facing concept. The 0–31 scale was the defect issue 0623
measured: a tier written `5` against a transport reading `16` was above it, not
below. `to_freertos_priority` survives only as a conversion for callers still
supplying normalized values (already true in the code today — nothing in the
FreeRTOS default path calls it). RFC-0016's platform capability tables stay
Stable and are the input to §4's address plans. **Action:** amend RFC-0016 with
a `superseded-in-part-by: rfc-0079` note on the priority-model section only.

**RFC-0052 (Draft) — the SystemModel→RTOS mapper.** This RFC is the realizer
half that RFC-0052 §"the agnostic core" already calls for: it specifies a
*"priorityless* ordered/segmented structure" plus a per-platform realizer. That
is precisely §3 and §5. The implementation went the other way — raw
per-platform tables in the user's file — which pushed the realizer's job onto
the user. **Action:** amend RFC-0052's realization section to name RFC-0079 as
the realizer's priority-assignment rule; no text of it is retired.

**RFC-0078 (Draft) — WCET per profile.** Unchanged and load-bearing: it is the
source `budget` derives from. This RFC adds the consumer.

**`ros-launch-manifest-sched`.** The shared agnostic core (ranking, feasibility,
clock segmentation) is unaffected — it is already priorityless. Only the
nano-ros realizer changes. `rt_priority_band` remains the Linux realizer's,
which nano-ros does not use.

## Migration

38 pins exist. They are not deleted:

1. A pin becomes a **static lease** — still honoured, now CHECKED against the
   plan. A pin inside a reserved band, or colliding with another pin, is a
   BUILD ERROR. Today it silently wins, and you learn about it as a 96 %
   dispatch loss three sessions later.
2. `spin_period_us` (11 of 11) becomes derived; an authored value is accepted
   as an override and warns.
3. `deadline_policy` (0) and `stack_bytes` (0) rename freely — nobody writes
   them. `stack_bytes` is advisory anyway: issue 0667 established it is a FLOOR
   the port raises, never a size the caller can get right.
4. `budget_us` (2) is the only hard break. Both sites are in this repo's own
   bringups.

### The collision report, run

`scripts/dev/priority-collision-report.py` evaluates all 38 pins against the
system bands that can be CITED from code today. **Four of 38 are clean.**

| verdict | at first run | now |
| --- | --- | --- |
| `UNPLANNED` — the port declares no band at all | 21 | **0** |
| `PREEMPTS` — more urgent than a system band | 8 | **0** |
| `COLLIDES` — lands exactly ON one | 4 | **0** |
| below bands — correct by the plan | 5 | **27** |

Four of five ports are allocated and enforced:

| port | plan | pins | reserved | pool |
| --- | --- | --- | --- | --- |
| FreeRTOS | static | 9 | `transport 4..4` | `1..3` |
| NuttX | static | 8 | `transport 100..100` (INHERITED) | `1..99` |
| ThreadX | static | 2 | `transport 14..14` | `15..31` |
| Zephyr | DERIVED (§4.1) | 8 | resolved per image — `7..7` here | `8..14` |
| POSIX | static | 11 | `transport 90..99` (`sched_get_priority_*` range) | `1..89` |

Zephyr's four `tiers.high.zephyr = 5` violations are closed: moved to 9, inside
the resolved pool and below the transport, in all four bringups.
`check-tier-priority-plan-image` reports 8 of 8 clean against the realtime
image's own `.config`.

Measured with every realtime fixture rebuilt — the fullest coverage this work
has had: **`realtime_tiers` 17 rows ran, 4 skipped, 1 failed.** The three Zephyr
rows (rust, c, cpp) all pass, as do all three FreeRTOS, NuttX-arm c and cpp,
ThreadX and the three native rows. The four skips want tooling this host lacks
(`native/cpp-rclcpp` needs ROS, `nuttx-riscv/*` need `qemu-system-riscv32`); the
one failure is issue 0736's `nuttx-arm/rust`, which is unrelated to allocation
and known flaky.

**Both defect columns are closed on the two ports that can describe
themselves.** FreeRTOS and NuttX declare `[board.priority_plan]`;
`check-tier-priority-plan` fails a pin that lands on a reserved band, and fails
one that outranks a band without saying so. All 12 offending pins were moved
into their port's `pool.app` rather than grandfathered:

| | before | after |
| --- | --- | --- |
| FreeRTOS (pool 1–3, transport 4) | high 5, mid 3, low 2 | high 3, mid 2, low 1 |
| NuttX (pool 1–99, transport 100) | high 110, low 100 | high 99, low 98 |

Order is preserved throughout, and every tier now sits below the link it
publishes over. Measured after the move: all four `TierPriority` arms report
every tier ACCEPT (`freertos cpp` 3/3, `freertos c` 2/2, `nuttx cpp` 2/2,
`nuttx rust` 2/2), and `realtime_tiers` is 16 of 17 rows — the one red is issue
0736's, unmoved at 49 vs 25 inside its usual range. That is now the THIRD
independent check that 0736's publish failures are not about priority ordering.

`above = "<band>"` is what keeps the rule honest rather than merely strict. It
is declared on the TIER, not the platform table, because it states something
about the system rather than about one kernel's numbering:

```toml
[tiers.safety]
above = "transport"
```

With it, the pin is accepted and the consequence is PRINTED — "this tier can
outrun the link it publishes over, and inbound traffic waits on it". Without
it, the error names both remedies. Verified in both directions: the declaration
turns an error into a reported choice, and removing it turns it back.

The specific findings answer the question the migration section was guessing at.
Static pins are not a rare escape hatch; they are the only mechanism, and they
are mostly wrong or unverifiable:

* **`tiers.low.nuttx = 100` collided in every bringup that has it (4 of 4).**
  100 is the app_main default the zenoh read/lease threads inherit, so the boot
  tier and the transport shared one priority and round-robinned against each
  other. Nobody wrote that down; it fell out of two defaults meeting. FIXED —
  moved to 99, which keeps the tier order and vacates the band. The
  `[nuttx cpp|rust TierPriority]` cells confirm 2/2 tiers ACCEPT at the new
  value, and issue 0736's cell is unmoved (68 vs 30, inside its usual range),
  consistent with the separate finding there that transport PRIORITY is not
  what makes its publishes fail.
* **`tiers.high.freertos = 5` preempts the transport band in 4 bringups.** This
  is exactly what `report_tiers_above_transport` warns about at boot — "tier
  `high` at 5 >= 4 — this tier PREEMPTS transport I/O". The diagnostic exists to
  make that a CHOICE. The count says the choice was never made: it is 4 for 4,
  in every bringup that targets FreeRTOS.
* **21 pins are on ports that declare no band**, so no tool can say whether
  they are right. Two of those ports cannot even express one:
  `zpico_set_task_config` discards priority on Linux/macOS, and did so on NuttX
  until issue 0736.

The 8 `PREEMPTS` and 4 `COLLIDES` are what a checked static lease rejects. That
is the migration cost, and it is concentrated: all in this repo's own bringups.

> **Correction, made while implementing §4.** The first run of this report also
> counted `app` (FreeRTOS priority 3) as a reserved band, giving a fifth
> COLLIDES for `tiers.mid.freertos = 3`. That was wrong: `app_priority` is the
> priority `app_task` is CREATED at, and `run_tiers` immediately replaces it
> with the boot tier's own. A starting value is not a standing occupant, so 3
> belongs to the pool. Recorded rather than silently fixed — requiring a cited
> source per band is what made it checkable, and the first thing that citation
> caught was my own number.

The report also finds **no ambiguous ordering** — no two tiers in one bringup
share a value on one platform — so deadline-monotonic derivation has a total
order to reproduce on every existing bringup, and the declaration-order
tiebreak is not load-bearing yet.

## Port status, and what actually blocks the rest

Three ports declare a plan. The remaining two are blocked on different things,
and the difference matters more than the count:

| port | tiers written in | transport actually at | plan |
| --- | --- | --- | --- |
| FreeRTOS | raw | raw 4 (`FreertosScheduling::default`) | declared |
| NuttX | raw | raw 100 (INHERITED, nobody chose it) | declared |
| ThreadX | raw | raw 14 (`Z_TASK_PRIORITY`) | declared |
| Zephyr | raw `k_thread` | pthread, band 0–31 → `SCHED_RR` | **blocked** |
| POSIX | advisory — never applied | discarded (privilege) | **n/a** |

**ThreadX was not blocked, and a first reading of this said it was.** The claim
was that its transport sits on the normalised band while its tiers are raw —
issue 0623's split, unfixed. The band exists (`_z_task_threadx_priority`
inverts it, because ThreadX counts 0 as most urgent), but it is only reached by
a caller of `zpico_set_task_config`, and nothing calls that on ThreadX. So
`_z_task_init` takes its `attr == NULL` path and every zenoh task gets the
compile-time `Z_TASK_PRIORITY` — a RAW ThreadX priority, in the tiers' own
units. Describable immediately, and now described.

**Zephyr is genuinely blocked**, and not on willingness. Its tiers are native
`k_thread` priorities passed straight to `k_thread_create` (negatives =
cooperative), while zenoh-pico's Zephyr platform creates its tasks with
`pthread_create` and `zpico_posix_set_priority` maps a normalised 0–31 across
`CONFIG_NUM_PREEMPT_PRIORITIES` under `SCHED_RR`. Those are two scales in two
namespaces, and stating a band in either without the POSIX→native conversion
would re-author issue 0623 inside the mechanism built to prevent it. The
conversion is Zephyr's, not ours, so the plan needs it read out of Zephyr's
POSIX layer rather than guessed.

**POSIX is half-solved as of 2026-08-24, and the remaining half is sharper.**
Tier priorities now apply — `setcap cap_sys_nice+ep`, then `ps -eLo
tid,cls,rtprio` shows `FF 10` and `FF 80` for the two tiers, boot tier included
— or print a line naming the missing capability. But that immediately creates
this RFC's own inversion here: a SCHED_FIFO tier outranks every SCHED_OTHER
thread unconditionally, and zenoh-pico's read/lease tasks stay on SCHED_OTHER
because `zpico_set_task_config` DISCARDS priority on Linux/macOS. So POSIX has a
pool and no way to reserve a band — the transport cannot be raised to meet the
tiers even deliberately. The question is no longer "can priorities apply here"
but "what reserves the transport when the tiers are FIFO and the transport is
not".

The original framing, kept because it is what the port looked like before: The Linux board
never calls `sched_setscheduler` — it prints "posix tier priority/core are
advisory (not applied natively)" whenever a tier declares one — and
`zpico_set_task_config` discards the priority there for the same privilege
reason. So there is no kernel address space to collide in, and a plan
describing kernel priorities would be fiction. If POSIX gets one it must
describe the EXECUTOR's ordering space, which is a different object from the
other four and deserves deciding rather than assuming.

## Prior art that is not prior — `play_launch` already implements this

`play_launch` (vendored at `packages/cli/third-party/play_launch`) shipped the
same design for Linux, and it is further along than this RFC's sketch. Its
platform file:

```yaml
target: posix
mapper: rate_monotonic          # or deadline_monotonic, chain_aware, manual
resources:
  rt_priority_band: { min: 10, max: 40 }
  isolated_cpus: [0]
overrides:
  control_node: { priority: 20, core: 0 }
```

and its guide states the thesis in one line: *"You stop hand-writing every
priority number — you write the mapper and the exceptions."*

The correspondence is close enough that RFC-0079 should ADOPT rather than
parallel it:

| RFC-0079 | play_launch | note |
| --- | --- | --- |
| `pool.app` | `resources.rt_priority_band {min,max}` | same object |
| deadline-monotonic derivation | `mapper:` — a NAMED, chosen algorithm | theirs is better; see below |
| static pin, checked | `overrides:`, "overrides always beat derived" | same rule |
| `[board.priority_plan]` per port | platform file per `target:` | same split |
| `nros ws model-dims` | `check --explain` | same need |

### Three things to take

1. **`mapper:` is a named choice, not a fixed rule.** This RFC specified
   deadline-monotonic as THE derivation. play_launch offers
   `rate_monotonic`, `deadline_monotonic`, `chain_aware` and `manual`, and
   picks per deployment. That is the better shape for the same reason profiles
   are: rate-monotonic and deadline-monotonic are different answers to
   different systems, and a tree that hard-codes one has made a scenario
   assumption exactly like naming a profile `control`. RFC-0079 §3 should
   become `mapper = "deadline_monotonic"` with a stated DEFAULT, not a law.
2. **`chain_aware` is the mapper this codebase actually wants**, eventually —
   it ranks by position in a causal pipeline rather than local rate, which is
   what a sensor→filter→control corridor needs. RFC-0052 already names
   `ros-launch-manifest-sched` as the shared, platform-agnostic core both
   runtimes vendor; the mapper vocabulary should come from there rather than
   be re-invented per consumer.
3. **A node with no timing facts lands in a DEFAULT tier and is reported as
   such**, rather than being given a plausible number. That is the same rule
   RFC-0078 sets for WCET ("absent is not zero") and this RFC sets for budget
   ("absent is not a budget"), and it should be stated for priority too:
   absent is not a priority.

### One thing NOT to take, and it is the reason this RFC exists

**play_launch has a POOL and no RESERVATIONS.** `rt_priority_band` is a range
the operator picks to sit clear of everything else on a Linux box; nothing in
the platform file describes what else holds an address, because on Linux the
things that do are other people's processes.

On an RTOS the transport is OURS and lives in the same space as the tiers —
zenoh-pico's read, lease and flush tasks, the netif poll task. That is the
reservation concept, and it is the part of this design that has no analogue
upstream. It is also where every measured defect came from: 0623, 0736, and
the 4 collisions and 8 preemptions this RFC's own report counted.

Notably it applies to nano-ros on POSIX too, and NOT to play_launch on POSIX,
because nano-ros's zenoh threads are pthreads inside the nano-ros process while
play_launch schedules processes from outside. Same target, different problem
(issue 0765).

## Open questions

* **`reserved.foreign` verification.** A plan can claim lwIP's `tcpip_thread`
  sits at N. Nothing checks it. A runtime probe that enumerates threads and
  compares against the plan would close it — on ports that can enumerate.
* **Do profiles need a per-port override?** A `responsive` tier may want
  different realization on a 2-priority kernel than on a 255-priority one.
* **Headroom policy.** Spread allocations evenly across the pool, or pack them
  tight leaving room at the top? Even spread survives an inserted tier; packing
  leaves room for static leases above.
* **Bootstrapping.** With no WCET declared anywhere today (RFC-0078: nothing
  outside rlm's tests sets `exec_ms`), every tier derives no budget on day one
  and runs fixed-priority. That is better than unmeasured literals and makes
  the missing evidence visible — but it means the sporadic-server path goes
  dormant until WCET declarations land, and that is a real behavioural change.
