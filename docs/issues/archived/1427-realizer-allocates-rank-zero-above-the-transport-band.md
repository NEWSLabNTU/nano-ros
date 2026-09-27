---
id: 1427
title: "The realizer maps dense rank 0 to Zephyr priority 0, above the
  transport band the image's Kconfig implies - the board priority plan RFC-0079
  declares is read by two scripts and not by nros-orchestration-ir"
status: resolved
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-21
related: [issue-0623, issue-0852, issue-1426, issue-1508, phase-459, rfc-0079, rfc-0052]
resolved_in: phase-459 W4 (+ the residue below)
---

## Resolution

Fixed by **phase-459 W4** (`d37ff02af`, `54ef74564`, 2026-09-24), which is the
change this issue's "What would fix it" asked for, and completed on 2026-09-27
with an end-to-end acceptance, a refusal where the plan cannot be satisfied,
and two stale statements this issue named.

### Re-measured before touching anything, at 4d439a115

Three numbers, all three against current `main` and the first also against the
commit this issue verified (`783cdfa14`):

1. **`rank_to_priority` for dense rank 0 on Zephyr.** At `783cdfa14` the
   function was `rank_to_priority(rank, rank_count, caps)` and returned `pos`
   under `low_number_is_high`, i.e. **0** — the report was right. At
   `4d439a115` it takes a `PriorityPlan` and returns **5**, the most urgent
   address in the island's `pool.app` `[5, 14]`. The next rank is **6**.
2. **The transport band on a 15-preemptive-priority image.** Band 200 ->
   `band_to_posix` 10 -> k_thread `15 - 10 - 1` = **4**; the island's lease band
   255 -> posix 14 -> k_thread **0**, so `reserved.transport = [0, 4]` and
   `pool.app = [5, 14]`. With both bands at Kconfig's 200 the band is `[4, 4]`
   and the pool is unchanged.
3. **Does `priority_plan` reach the realizer?** It does now, and the issue's
   `grep` is no longer empty: 15 hits under `packages/core`, the load-bearing
   one being `derive.rs:91` -> `realize_rtos(&ranked, &input, &caps,
   &priority_plan)`. `packages/core/nros-orchestration-ir/src/priority_plan.rs`
   is the module; `zephyr_plan` is the arithmetic and
   `scripts/lib/priority_plan.py:resolve_zephyr_plan` remains its checker.

### The acceptance, end to end

`derived_tiers_bake::the_derived_table_lands_below_the_transport_band`
(`packages/cli/nros-cli-core/tests/`) bakes the phase-459 W0 fixture down the
real road — launch file -> `nros-launch-resolve` -> callback groups ->
`derive_tiers_from_contracts` — and asserts the resulting
`[tiers.*.zephyr] priority` rows: **30 Hz pair 5, 10 Hz pair 6**, inside the
pool the island's `.config` resolves and strictly greater than the transport
band's least urgent thread (k_thread 4; Zephyr counts down, so "below the band"
is a larger number). No `priority` degradation is recorded.

That file's header used to say the numbers were deliberately NOT asserted,
because pinning 0 and 1 would read as if 0 were intended. The reason was good
and is now spent, so the header says what is asserted instead.

**Mutation check.** `derive.rs` reverted to the pre-W4 plan
(`PriorityPlan::whole_range(caps.n_priorities, caps.low_number_is_high)`):
the test fails with `{mrm_emergency_stop_operator: 0, stop_mode_operator: 0,
mrm_comfortable_stop_operator: 1, mrm_handler: 1}` — this issue's report,
reproduced end to end by its own acceptance test.

### When the plan cannot be satisfied: COMPRESS, or REFUSE — never clamp

W4 clamped BOTH under-capacity cases onto `least_urgent_app_priority()`, and
for an EMPTY pool that is not a pool address at all. Measured on the pre-0852
band (`READ_PRIORITY=16`, `transport = [0, 14]`, `app = [15, 14]`): both
derived tiers came out at k_thread **14** — the transport's own priority — with
a stderr `Degradation` as the only trace. A derived tier had reached a reserved
band, which is what RFC-0079 §6 reserves for an AUTHORED tier that names the
band, and it was the 0623 inversion produced by the mechanism built to prevent
it.

Split into three outcomes (`rtos_realizer::Allocation`):

* `Exact` — an address of the rank's own.
* `Compressed` — a rank past a non-empty pool shares its least urgent address;
  degradation recorded, naming the pool. **Compression is right here**: every
  number is still a pool address, only the ORDER weakens, and a FreeRTOS
  `pool.app` is three wide, so refusing would make the derivation unusable on
  the boards it exists for.
* `Refused` — an empty pool yields **no derived tier for that node**, with a
  degradation naming the band that ate the pool and the knob to widen. There is
  no legal address, so the alternatives are to fabricate one or to decline.

The refusal is a refusal to ALLOCATE, not a build abort: it reaches all three
derivation roads through the degradation record they already print (bake stderr
+ `PlanSchedWarning`, codegen-entry stderr, `nros::main!` expansion stderr), and
it leaves the image on its authored configuration rather than replacing an
author's schedule with a fabricated one. `least_urgent_app_priority` returns
`Option<i64>` so the fabrication is unavailable by construction rather than by
discipline. RFC-0079 §5 is amended with this two-case rule.

### The boot-time report agrees

FreeRTOS's `report_tiers_above_transport` (issue 0623) is the tree's only place
where a tier priority and a transport priority meet at run time in one
vocabulary. `the_freertos_pool_stays_below_the_boot_report_floor` reads the
floor from `FreertosScheduling::default()` (`zenoh_read`/`lease`/`poll` = 4) and
asserts every address `pool.app` `[1, 3]` can hand out is strictly below it — so
a derived FreeRTOS image can never warn about itself at boot. Mutation check:
`zenoh_read_priority: 4` -> `3` fails it with `can allocate 3, which meets the
transport floor 3`. The floor is READ, not restated: a core crate cannot depend
on a board crate, and a missing pattern fails rather than passing vacuously.

Zephyr — where this issue was measured — has no such boot report, and adding
one needs a Zephyr build to verify. Filed as **issue 1508**, option 2.

### Tests

| test | what it pins |
| --- | --- |
| `rtos_realizer::priority_plan_allocates_inside_the_application_pool` | island plan -> 5, 6 (W4) |
| `rtos_realizer::an_empty_application_pool_derives_no_tier_rather_than_a_reserved_one` | the refusal, and that the tier table is empty too |
| `rtos_realizer::a_realized_priority_is_always_a_pool_address` | the CLASS, over 10 plans: in `pool.app`, on no reserved band |
| `rtos_realizer::priority_plan_clamps_a_rank_past_the_pool_and_records_it` | compression stays compression (W4) |
| `priority_plan::priority_plan_reports_the_stale_band_as_an_empty_pool` | `least_urgent_app_priority() == None`, and `app.hi` IS the transport's |
| `priority_plan::the_freertos_pool_stays_below_the_boot_report_floor` | the boot report agrees |
| `derived_tiers_bake::the_derived_table_lands_below_the_transport_band` | the bake road, end to end |

### The two stale statements this issue named

RFC-0079 §4.1's chain was corrected by W4. `packages/boards/zephyr/
nros-board.toml` was not, and still carried "Kconfig, default 16", the private
0-31 band and `zpico_posix_set_priority`'s `lo + (span*n*2 + 31)/62` — the
descriptor the RFC quotes, restating the numbers its own neighbouring paragraph
explains cannot be restated. It no longer restates the chain at all: it points
at RFC-0079 §4.1 and names the two live spellings (Python resolver, Rust
`zephyr_plan`) that check each other. Prose cannot join that pair.

### What is NOT fixed, and is now issue 1508

A bake passes `PriorityPlan::for_target("zephyr")` — the Kconfig DEFAULTS
projection — because no bake road has a `.config`. Measured: an image with
`CONFIG_NUM_PREEMPT_PRIORITIES=32` reserves `[7, 7]` and owns `[8, 31]`, and an
image with `CONFIG_NROS_ZENOH_READ_PRIORITY=100` reserves `[4, 9]` and owns
`[10, 14]` — against which a baked 5 is once again ABOVE the transport. No
in-tree image is in either state (every Zephyr image runs the default 15, none
lowers a band), which is why this issue's acceptance passes; and nothing would
report it if one appeared, because `check-tier-priority-plan-image.py` judges
AUTHORED pins and a derived tier is authored nowhere.

## The allocation (verified at 783cdfa14)

`rank_to_priority` (`packages/core/nros-orchestration-ir/src/rtos_realizer.rs:336-346`):
dense rank 0 (most urgent) becomes priority `pos` when
`caps.low_number_is_high`, and `sched_caps_for("zephyr")` (`:140-200`) is
32 priorities, low-number-is-high. So the most urgent derived tier lands at
Zephyr preemptive 0, the next at 1. `grep priority_plan packages/core` is
empty: the realizer takes `SchedCaps` (count and direction) and nothing else.

## The transport, on the same image

The zenoh read and lease tasks are pthreads at
`CONFIG_NROS_ZENOH_READ_PRIORITY` (`zephyr/Kconfig:461-469`, default 200 on
a 0..255 band since phase-364 W5), mapped by
`packages/platform/nros-platform-zephyr/src/platform.c:501-503` as
`lo + band * (hi - lo) / 255` against `CONFIG_NUM_PREEMPT_PRIORITIES`. On
the Autoware Safety Island (15 preemptive priorities) that is POSIX 10, Zephyr
preemptive 4. A derived control tier at 0 outranks the transport that
delivers its inputs; a derived telemetry tier at 1 does too. That is issue
0623's inversion, produced by the mechanism built after 0623 to derive
priorities correctly.

## The plan exists and is not the realizer's input

RFC-0079 section 4 gives every port an address plan.
`packages/boards/zephyr/nros-board.toml:50-62` declares Zephyr's as DERIVED,
with `resolver = "scripts/lib/priority_plan.py:resolve_zephyr_plan"` and the
Kconfig inputs it depends on; `scripts/check-tier-priority-plan-image.py`
resolves it from a `.config` and judges authored pins against it, with a
selftest for the stale pre-0852 band. Both readers are Python and both judge
AUTHORED tiers. The derived path, which RFC-0079 says should be the normal
one, never consults the plan.

RFC-0079 section 4.1 itself still writes the chain as
`CONFIG_NROS_ZENOH_{READ,LEASE}_PRIORITY (default 16) -> band 0..31`; the
Kconfig has been 0..255 with default 200 since phase-364 W5, and the script
already knows it (`check-tier-priority-plan-image.py:334`). The RFC's worked
example (`read band 16 -> posix 7 -> k_thread 7`) is for the old band.

## What would fix it

phase-459 W4. `realize_rtos` takes a `PriorityPlan` beside `SchedCaps`: a
STATIC plan's pool from the descriptor, the DERIVED Zephyr plan resolved from
the image's `.config` in Rust by the same arithmetic as the script (the
script becomes the checker of the Rust result). Rank 0 maps to the most
urgent priority inside `pool.app`; ranks past the pool are clamped with a
recorded `Degradation`. On the island's `.config` the pool is `[5, 14]`, so
30 Hz derives to 5 and 10 Hz to 6. POSIX keeps RFC-0079's "half-solved"
status and allocates in the executor's ordering space.

## Acceptance

The phase-459 W0 fixture's Zephyr bake yields priorities inside the resolved
pool and `check-tier-priority-plan-image.py` on its `.config` reports zero
violations; the negative control pins `CONFIG_NROS_ZENOH_READ_PRIORITY=16`
and the resolver reports the stale band. RFC-0079 section 4.1 corrected in
the same change.
