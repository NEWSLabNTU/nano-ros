---
id: 2021
title: "The per-tier monitor-table emit has no C-ABI board left to reach it"
status: open
type: tech-debt
area: [codegen]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108]
---

## Measured

The entry packs can emit a monitor-row table per tier (each tier installs its
own nodes' rows on its own executor). That shape needs a family whose tier
runner gives each tier its own executor.

After phase-509 P10, every family with a C-ABI tier runner has a level runner
instead: Zephyr, Linux, FreeRTOS and NuttX. ThreadX has no C-ABI tier runner.
The level shape installs one table (`typed_emit_levels_install_one_monitor_table`).

The two tests that pinned the per-tier shape
(`typed_emit_tiers_slice_monitor_rows_per_tier`, C and C++) were removed in
#2553. No board was left to emit it, so the emitter branch that renders
per-tier tables is now unreachable from any board key.

## Closes when

The per-tier monitor branch is removed from both packs, along with any lowering
field that exists only to feed it. Alternatively, a family that keeps
per-tier executors is named, with a test.
