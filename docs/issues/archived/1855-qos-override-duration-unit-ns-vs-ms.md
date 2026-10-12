---
id: 1855
title: "`qos_overrides.*` duration policies: the resolver writes NANOSECONDS (rclcpp's unit), the lowering reads MILLISECONDS, so a contract deadline or lease is baked 1e6 times too long or refused"
status: resolved
resolved_in: "issue 1855 PR (qos_override::lower reads nanoseconds, bakes milliseconds)"
type: bug
area: codegen
related: [1854, 0303, rfc-0100]
---

## Resolution

The lowering (`nros_orchestration_ir::qos_override::lower`, shared by the CLI's
C/C++ entry emitters and `nros::main!`) reads `deadline`, `lifespan` and
`liveliness_lease_duration` in NANOSECONDS, rclcpp's unit and the unit
play_launch's resolver derives a contract's `deadline:` / `lease_duration:`
into, and bakes milliseconds:

- the conversion is `nros_rmw::duration_to_qos_ms`, the runtime's own boundary
  rule: 0 stays "no check", a sub-millisecond remainder ceils, a value at or
  past the u32 infinite sentinel is refused;
- rclcpp's spelling of an infinite duration (`i64::MAX` nanoseconds) maps to
  `DURATION_INFINITE_MS`;
- a NONZERO value under 1 ms is refused, not rounded: it is below the wire's
  resolution and is exactly what a pre-1855 millisecond spelling looks like.
  Negative and non-integer values are refused too. The diagnostic says
  NANOSECONDS and gives 500 ms as 500000000.

The in-tree millisecond spellings moved to nanoseconds
(`tests/fixtures/qos_agreement/launch/agrees.launch.xml`, the lowering's and
the macro's unit tests, the C++ emit test). The play_launch resolver is
unchanged: it was already right.

Tests: `duration_policies_read_nanoseconds_and_bake_milliseconds` (a 500 ms
lease bakes 500, a 4.3 s deadline bakes 4300) and
`an_unrepresentable_duration_is_refused` in `qos_override.rs`, and
`a_contract_lease_and_deadline_bake_in_milliseconds` in
`packages/cli/nros-cli-core/tests/contract_qos_reaches_the_entry.rs`, which
takes a contract with `lease_duration: 500ms` and `deadline: 4.3s` through
the pinned resolver and the C++ entry emitter.

## Problem

Two producers, two units, one parameter name.

- play_launch's resolver (`ros-launch-resolve` `model_builder.rs`, "Phase 74
  -- the contract's QoS reaches the running node") derives a contract's
  `deadline:` and `lease_duration:` into
  `qos_overrides.<topic>.<role>.{deadline,liveliness_lease_duration}` as
  `Int((ms * 1e6) as i64)` -- NANOSECONDS, which is rclcpp's unit for these
  parameters (`QosOverridingOptions`). Its own test pins it:
  `qos_overrides./safety/scan.subscription.liveliness_lease_duration: 100000000`
  for a 100 ms lease.
- nano-ros's lowering (`nros_orchestration_ir::qos_override::lower`) reads
  `deadline`, `lifespan` and `liveliness_lease_duration` as MILLISECONDS
  (`ms("a duration in milliseconds")`, a `u32`), and the runtime folds the
  value as milliseconds (`nros_cpp_qos_override_t.value`).

So a contract `lease_duration: 500ms` reaches the baked table as
500,000,000 ms (about 5.8 days), and a `deadline:` of 4.3 s or more overflows
the `u32` and the build is refused with a `BadValue` naming a value the author
never wrote. In-tree launch files that write the parameter by hand
(`tests/fixtures/qos_agreement/launch/agrees.launch.xml`, `deadline = 100`)
use milliseconds, so the two producers cannot both be right.

Found while fixing issue 1854: with the rclcpp-shape table now baked, a
contract duration policy would reach a C++ image with this unit error. The
1854 tests use `liveliness:` (unitless) on purpose.

## Direction

Pick ONE unit for the parameter spelling. rclcpp's is nanoseconds, and a
launch file shared with a stock rclcpp node means nanoseconds there, so the
lowering should read nanoseconds and convert (with the in-tree millisecond
spellings migrated), or the resolver should stop deriving durations for
nano-ros models. Either way the lowering should refuse a value that is not
representable rather than wrap or overflow.
