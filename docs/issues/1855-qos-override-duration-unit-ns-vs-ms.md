---
id: 1855
title: "`qos_overrides.*` duration policies: the resolver writes NANOSECONDS (rclcpp's unit), the lowering reads MILLISECONDS, so a contract deadline or lease is baked 1e6 times too long or refused"
status: open
type: bug
area: codegen
related: [1854, 0303, rfc-0100]
---

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
`500000000` ms (about 5.8 days), and a `deadline:` of 4.3 s or more overflows
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
