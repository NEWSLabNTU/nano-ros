---
id: 2021
title: "The per-tier monitor-table emit has no C-ABI board left to reach it"
status: resolved
type: tech-debt
area: [codegen]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [phase-509, rfc-0108]
---

## Measured

The entry packs could emit a monitor-row table per tier (each tier installs its
own nodes' rows on its own executor). That shape needs a family whose tier
runner gives each tier its own executor.

After phase-509 P10, every family with a C-ABI tier runner has a level runner
instead: Zephyr, Linux, FreeRTOS and NuttX. ThreadX has no C-ABI tier runner,
so a tiered ThreadX plan takes the sched-context shape. A metadata probe maps
the tiered shape to sched contexts too. The per-tier branch in `lower_tiers`
(`one_executor == false`) was therefore unreachable from any board key, and the
two tests that pinned it were removed in #2553.

## Resolution

Removed the per-tier shape; the level shape is unchanged in behaviour.

- `codegen/entry/lower.rs`: `lower_tiers` lost its `one_executor` parameter and
  the per-tier `monitor_table` call; tier 0 (the boot level) carries the one
  table for every level. `monitor_table` lost its `tier` argument. A tiered
  plan on a family with a tier runner but no level runner is now refused by
  name instead of silently taking the removed shape.
- `nros-entry-lower`: `MonitorTable::{tier, tier_name}` removed (they only told
  per-tier tables' symbols apart), and `LoweredEntry::monitor_tables:
  Vec<MonitorTable>` became `monitor_table: Option<MonitorTable>`, since an
  image holds at most one.
- C and C++ entry packs (`entry.*.jinja`, `monitor_install.jinja`): the
  per-table loop, the `_t<N>` symbol tag and the `tier[N]` banners are gone.
  The level image's statics are now `__nros_mon_rows` etc. instead of
  `__nros_mon_rows_t0`.

`typed_emit_levels_install_one_monitor_table` still pins the level shape (one
table, installed once, by tier 0's setup); the `c_native_tiers_monitors`
golden was regenerated and
`a_mode_tier_deadline_lowers_onto_each_executor_shape` updated for the symbol
rename.
