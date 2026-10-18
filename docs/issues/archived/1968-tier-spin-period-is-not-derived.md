---
id: 1968
title: "A tier's spin period is not derived: unset, the C/C++ tier runner spins with a 1 ms timeout"
status: resolved
type: enhancement
area: [codegen, zephyr]
severity: medium
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "branch fix/derived-spin-period"
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

## Resolution

`nros_orchestration_ir::spin_period::apply_derived_spin_periods`, called from
`codegen entry`'s `resolve_plan_sched_recorded` right after `resolve_tiers`,
over the periods `rtos_realizer::node_periods_us` reads (the fold that stamps
`RealizedNode::period_us`, so the two cannot disagree). A plan with no model
(the cmake registered-node road, hand-built plans) is left as resolved.

- unset -> the smallest member release (or the tier's own `period`);
  printed as `codegen entry: tier `<t>` spin period <us> us, derived from ...`
- authored, faster -> kept, printed as an override
- authored, slower -> `codegen entry: tier `<t>` authors spin_period ...
  slower than its fastest release` (build error)
- members all event-driven -> unchanged (no release to derive from)

On the safety island's tier (`mrm`: 10 Hz handler, 30 Hz and 10 Hz
operators) the derived value is 33,333 us; its authored `10ms` is kept as an
override until it is removed.

Tests: `spin_period::tests` (7, nros-orchestration-ir) and
`derived_tiers_entry` (`an_unset_spin_period_is_derived_from_the_fastest_member`,
`a_faster_authored_spin_period_is_kept`,
`a_slower_authored_spin_period_is_refused`, over the derived-tiers-cpp
fixture through `plan_from_model` -> `enrich_plan` -> `resolve_plan_sched` ->
`emit_typed`).

Not here: the Rust `nros::main!` tier road reads `spin_period` from
`system.toml` itself and still floors an unset one.
