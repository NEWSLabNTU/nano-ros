---
id: 1970
title: "Every tier's executor is sized for the whole image, so a second tier costs a second whole-image executor"
status: resolved
type: enhancement
area: [memory, zephyr, codegen]
severity: medium
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "branch fix/per-tier-executor-sizing"
related: [rfc-0079, 1551, 1568, 1232, 0961, 1971]
---

## What happens

A tiered C/C++ entry reserves one executor block per tier,
`__nros_tier_executor_storage[n_tiers][NROS_CPP_EXECUTOR_STORAGE_SIZE]`, and
every block is the image-wide executor: the header, and an inline backing
carved at `ExecutorSizing::DEFAULT` -- the arena derived from EVERY entity the
image declares, the callback tables at the image's `MAX_CBS`, the node tables
at `MAX_NODES`, the parameter store at the image's `MAX_PARAMETERS`.

The Autoware safety island (S32K344, one tier `mrm` with three components)
measures that block at 47,296 B of DTCM (arena 25,724 B of it). Its next
step, the 30 Hz emergency operator on a thread of its own, is a second tier,
and the second tier costs a second 47,296 B block although it hosts one of the
three nodes. DTCM has 10,432 B left above the image's 8 KiB guard, so the
split cannot be built.

## Why

The tier runner's storage contract is one STRIDE (issue 1551): block `i` is
`storage + i * stride`, and the executor opened over it carves its backing
from the context's own inline `_opaque`, whose length is a property of the
TYPE (`ExecutorInlineStorage`), i.e. of the image. Nothing knows a tier's own
demand.

## Direction

- The sizing descriptor states each tier's own facts (`[[tier]]`: its nodes,
  their entity counts and callback slots, the subscriptions they register,
  their declared parameters), composed by the same entity inventory over the
  tier's members only.
- `nros-node` derives each tier's `ExecutorSizing` from those facts with the
  same arena arithmetic the image's uses, and a tiered image's contexts carry
  no inline backing.
- The generated entry gives each tier an exactly-sized backing static, the
  runner hands it to the tier's executor, and the library refuses a backing
  smaller than the tier's layout.

## Resolution

- **Descriptor.** `[[tier]]` rows (`nros_sizing_descriptor::Tier`): name,
  nodes, the topics of their subscriptions, their parameter-store slots, and
  a per-tier `[tier.image]` (node count, callback slots, the seven entity
  counts). Written by `nros ws sizing-descriptor --tier-record
  <nros_priority_plan.json>` (`NanoRosEntry.cmake` passes the record
  `codegen entry` writes beside the TU) only for a board whose runner has a
  per-tier twin (`CAbiRunners::run_tiers_backed`: Zephyr), and only when
  every tier composes (`sizing_descriptor::tier_rows`: the image's
  `EntityInventory::derive` over the tier's components, the image's
  `ParamDeclarations::sizing` over its nodes).
- **nros-node.** `build.rs` prices each row with the image's arena
  arithmetic (`arena_required`, factored out of the image's), its
  subscriptions picked by topic on both roads (descriptor rows and the
  `NROS_ENTITY_DECLARED_DEPTHS` carrier), into `config::TIER_SIZINGS`. All or
  nothing; skipped (warning) when a rung states `MAX_CBS` / `MAX_NODES` /
  the arena at a value other than the image's own derived one. A tiered
  image's `ExecutorInlineStorage` carries no backing (`INLINE_SIZING =
  None`); `rclc_executor_init`, `nros_cpp_init`, `nros_cpp_init_multi` and
  `nros_cpp_executor_open_over_session` refuse there instead of carving.
- **FFI.** `nros_cpp_init_tier`, `nros_cpp_executor_open_over_session_tier`
  (carve from the caller's backing at the named tier's sizing; refuse an
  unknown tier or a short/misaligned backing before a session opens),
  `nros_cpp_tier_backing_size`. `nros::sizes::TIER_BACKING_SIZE_0..7` ->
  `NROS_CPP_TIER_COUNT` / `NROS_CPP_TIER_BACKING_SIZE_<i>` in the per-build
  header.
- **Runner and entry.** `nros_board_zephyr_run_tiers_backed_in` (and the
  `ZephyrBoard::run_tiers` overload) take `nros_tier_backing_t backings[]`.
  Both entry packs emit `__nros_tier_backing_<i>` statics and call it when
  the header's `NROS_CPP_TIER_COUNT` equals the entry's tier count, the
  whole-image runner otherwise.

## Measured (the safety island, `just board-build`, S32K344)

| image | executor storage in DTCM | DTCM used |
| --- | --- | ---: |
| one tier `mrm` (as shipped) | context 2,664 + backing 44,632 = 47,296 (unchanged: the tier is the image) | 112,456 |
| two tiers, `estop` = the 30 Hz emergency operator, `mrm` = the rest | contexts 5,328 + `estop` 19,656 + `mrm` 36,728 = 61,712, + the tier thread object 272 | 127,384 |

The second tier costs **+14,928 B** of DTCM with per-tier sizing (a second
whole-image executor was +47,296 B). It still does not fit the island: 3,688 B
free against the 8 KiB guard, 4,504 B short. What is left is per-executor,
not per-member: the remap table (16 x 404 B = 6,464 B in every executor) and
the arena floor (8,192 B; the `estop` tier's own demand is about 7,200 B),
filed as issue 1971.

Tests: `nros-sizing-descriptor` (`the_tier_rows_survive_a_round_trip`),
`nros-cli-core` (`tier_row_tests`, three), `nros-node`
(`a_tier_is_carved_at_its_own_counts`,
`an_image_sized_as_one_executor_keeps_the_inline_backing`),
`nros-build-helpers` (`tier_backing_tests`, two), and the zephyr C/C++ tier
goldens.

Not here: FreeRTOS and NuttX tier runners keep one whole-image executor per
tier (no `run_tiers_backed`; the descriptor states no tier for them).
