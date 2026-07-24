---
id: 265
title: "Multi-tier config on tierless targets (esp32/RTIC/embassy/bare-metal/orin-spe) fails with a missing-method compile error, not a clean diagnostic"
status: open
type: polish
severity: low
area: orchestration
related: [phase-302]
---

## Finding (implementation-completeness audit, 2026-07-25)

`derive_target_rtos` falls back to `"posix"` for deploys with no
run_tiers (nros-macros/src/main_macro.rs ~2278). A multi-tier system on
esp32 therefore validates against posix rules, then dies with either a
misleading `MissingRtosSpec { rtos: "posix" }` (no posix sub-table on an
esp32 deploy!) or a raw missing-method compile error
(`Esp32QemuEntry::run_tiers` not found).

## Fix

The macro + CLI bake should reject early: "target <deploy> does not
support multi-tier execution (no run_tiers); collapse to a single tier
or pick a tiered board." Phase-302 W4.
