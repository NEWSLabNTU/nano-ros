---
id: 1508
title: "A bake allocates derived priorities from Zephyr's Kconfig DEFAULTS
  projection, not from the image's own plan - so an image that moves
  CONFIG_NUM_PREEMPT_PRIORITIES or lowers a transport band gets a table above
  its own transport, and no lane judges a derived tier"
status: open
type: bug
area: [codegen, zephyr, scheduling]
severity: medium
found: 2026-09-27
related: [issue-1427, issue-0623, issue-0506, issue-0852, rfc-0079, phase-459]
---

## What issue 1427 left

phase-459 W4 made `realize_rtos` allocate out of a `PriorityPlan`
(RFC-0079), which fixed the allocation issue 1427 measured. The plan a BAKE
passes is `PriorityPlan::for_target(target_rtos)`
(`packages/core/nros-orchestration-ir/src/derive.rs:91`), and for Zephyr that
is the **Kconfig DEFAULTS projection**, documented as such: Zephyr's own
`CONFIG_NUM_PREEMPT_PRIORITIES=15` / `CONFIG_NUM_COOP_PRIORITIES=16` with ONE
transport task at the platform default band 200.

The image's real plan is a function of four per-image Kconfig values, which is
the whole reason RFC-0079 §4.1 calls Zephyr's band DERIVED.
`PriorityPlan::from_zephyr_dotconfig` resolves it and is called by **tests
only** - nothing on a bake road has a `.config` in hand, because `nros sync`
runs before Zephyr's cmake generates one.

## Measured, 2026-09-27, at 4d439a115 (island `.config`, `input_two` ranking)

| plan | `reserved.transport` | `pool.app` | rank 0 |
| --- | --- | --- | --- |
| defaults projection (what a bake uses) | `[4, 4]` | `[5, 14]` | 5 |
| island image, bands 200/255 | `[0, 4]` | `[5, 14]` | 5 |
| Kconfig defaults, bands 200/200 | `[4, 4]` | `[5, 14]` | 5 |
| **image with `CONFIG_NUM_PREEMPT_PRIORITIES=32`** | `[7, 7]` | **`[8, 31]`** | (bake still says 5) |
| **image with `CONFIG_NROS_ZENOH_READ_PRIORITY=100`** | `[4, 9]` | **`[10, 14]`** | (bake still says 5) |

The first three agree, which is why 1427's acceptance passes and why every
in-tree Zephyr image is correct today: nothing in the tree overrides
`CONFIG_NUM_PREEMPT_PRIORITIES` (Zephyr's default 15 everywhere) and no image
lowers a transport band below 200.

The last two are the defect. A board whose SoC defconfig sets a different
preemptive count, or an image that lowers a transport priority, gets a derived
table allocated from `[5, 14]` while its transport sits at k_thread 7 or in
`[4, 9]` - **the derived tier is more urgent than the transport that feeds
it**, which is issue 0623's inversion and exactly what 1427 was about, arrived
at through a different door.

## And nothing would catch it

`scripts/check-tier-priority-plan-image.py` DOES resolve the real plan from a
built image's `.config`, and the Zephyr fixture lane runs it
(`just/zephyr-ci.just:456`). It judges `scan_pins()` - **AUTHORED**
`[tiers.<name>.zephyr] priority` rows in `system.toml` files. A derived tier is
not authored anywhere: it is written into the resolved SystemModel, which is a
build artifact that `check-no-tracked-models` forbids committing. So the one
checker that can see an image's real band never sees the numbers this issue is
about.

## Candidate fixes, none of them free

1. **Judge derived tiers in the image checker.** The C/C++ generated entry TU
   carries the table as `nros_native_tier_spec_t` `.priority` literals, in the
   same build tree as the `.config` - so the pair the checker needs is on disk
   together. A RUST image's table is baked by the `nros::main!` proc-macro and
   appears in no parseable artifact, so this covers part of the matrix and must
   REPORT the part it cannot see rather than passing (issue 0196's rule).
2. **Give Zephyr a boot-time report.** FreeRTOS has
   `report_tiers_above_transport` (issue 0623); Zephyr has no equivalent, and it
   is the one place where both numbers exist in one vocabulary for ANY image,
   whatever its Kconfig. `nros_board_zephyr_run_tiers_ns` holds the tier array
   (RAW k_thread) and the platform can map a band through
   `nros_zephyr_native_priority` + `NUM_PREEMPT - posix - 1`. Needs a Zephyr
   build to verify, which is why it was not done under 1427.
3. **Get a `.config` to the derivation.** Not possible where the derivation runs
   today; it would mean re-deriving at cmake configure time, after Kconfig.

(1) and (2) are detectors and are complementary: (1) fails a build, (2) tells
an operator about an image nobody re-baked.

## Acceptance

An image whose Kconfig implies a pool other than `[5, 14]` is REPORTED - by a
lane, by its own boot output, or both - rather than silently carrying a derived
table from the defaults projection. Whichever detector lands, it must say what
it could not examine.
