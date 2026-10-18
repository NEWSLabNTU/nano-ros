---
id: 1968
title: "A tier's spin period is not derived: unset, the C/C++ tier runner spins with a 1 ms timeout"
status: open
type: enhancement
area: [codegen, zephyr]
severity: medium
found: 2026-10-19
related: [rfc-0079, 1426, 0736]
---

## What happens

RFC-0079's table derives `spin_period_us` from "the tier's fastest release"
and keeps an authored value only as an override. nano-ros derives none. The
entry lowers an unset `spin_period` as `0`
(`codegen/entry/lower.rs`: `tier.spin_period_us.unwrap_or(0)`), and the C/C++
tier runners floor that at `SPIN_PERIOD_FLOOR_MS` (1 ms): a tier of 10 Hz and
30 Hz nodes is handed a park timeout a thousand times a second.

Downstream, the Autoware safety island's `system.toml` states
`[tiers.mrm] spin_period = "10ms"` by hand for exactly this reason (its
comment: "nano-ros derives no spin period yet"). That is the mechanism field
the RFC measured on 11 of 11 tiers.

## Direction

- Derive each tier's spin period from its members' release periods (the
  realizer's per-node period, the same one the priority record carries) and
  the tier's own `period` if authored: the smallest of them.
- Unset: take the derived value. Authored and no slower: keep it as the
  override, and say so. Authored and slower than the fastest release: refuse
  the build, naming the member.
- The C/C++ tier runner reads the value through the existing
  `nros_tier_spec_t.spin_period_us`.
