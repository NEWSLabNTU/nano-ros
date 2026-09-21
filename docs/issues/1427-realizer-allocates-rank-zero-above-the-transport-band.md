---
id: 1427
title: "The realizer maps dense rank 0 to Zephyr priority 0, above the
  transport band the image's Kconfig implies - the board priority plan RFC-0079
  declares is read by two scripts and not by nros-orchestration-ir"
status: open
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-21
related: [issue-0623, issue-0852, issue-1426, phase-459, rfc-0079, rfc-0052]
---

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
