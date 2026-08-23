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
[priority_plan]
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

| verdict | pins |
| --- | --- |
| `UNPLANNED` — the port declares no band at all | **21** |
| `PREEMPTS` — more urgent than a system band | **8** |
| `COLLIDES` — lands exactly ON one | **5** |
| below bands — correct by the plan | **4** |

The specific findings answer the question the migration section was guessing at.
Static pins are not a rare escape hatch; they are the only mechanism, and they
are mostly wrong or unverifiable:

* **`tiers.low.nuttx = 100` collides in every bringup that has it (4 of 4).**
  100 is the app_main default the zenoh read/lease threads inherit, so the boot
  tier and the transport share one priority and round-robin against each other.
  Nobody wrote that down; it falls out of two defaults meeting.
* **`tiers.high.freertos = 5` preempts the transport band in 4 bringups.** This
  is exactly what `report_tiers_above_transport` warns about at boot — "tier
  `high` at 5 >= 4 — this tier PREEMPTS transport I/O". The diagnostic exists to
  make that a CHOICE. The count says the choice was never made: it is 4 for 4,
  in every bringup that targets FreeRTOS.
* **`tiers.mid.freertos = 3` lands on the `app` band**, sharing a priority with
  the application task itself.
* **21 pins are on ports that declare no band**, so no tool can say whether
  they are right. Two of those ports cannot even express one:
  `zpico_set_task_config` discards priority on Linux/macOS, and did so on NuttX
  until issue 0736.

The 8 `PREEMPTS` and 5 `COLLIDES` are the ones a checked static lease would
reject on day one. That is the migration cost, and it is concentrated: 4 tiers
in 5 bringups, all in this repo.

The report also finds **no ambiguous ordering** — no two tiers in one bringup
share a value on one platform — so deadline-monotonic derivation has a total
order to reproduce on every existing bringup, and the declaration-order
tiebreak is not load-bearing yet.

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
