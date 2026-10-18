---
id: 1975
title: "Four small rclcpp accessors Jazzy added are missing: Logger::get_effective_level, Publisher::is_durability_transient_local, Timer::get_clock, NodeOptions::clock_type"
status: open
type: enhancement
area: api
severity: low
found: 2026-10-19
related: [rfc-0036]
---

## What

Comparing our C++ API against Jazzy's rclcpp (`just api-parity --edition jazzy`)
leaves four theirs-only members that are plain accessors over state we
already hold:

| upstream | ours today |
| --- | --- |
| `rclcpp::Logger::get_effective_level()` | `Logger::set_level` exists (adopt-bounded); the read side is missing |
| `rclcpp::Publisher::is_durability_transient_local()` | the publisher's QoS is known at creation |
| `rclcpp::TimerBase::get_clock()` | `create_timer` binds a `Clock` (phase-425 W4) |
| `rclcpp::NodeOptions::clock_type()` | clocks exist; NodeOptions does not carry the choice |

## Ruling: ADOPT

None needs an allocator or exceptions; each is a getter (or, for
`clock_type`, a builder field) over data already stored.

## Work

Add the four, then delete their `"editions": ["jazzy"]` `gap` rows from the
api-parity ledger.
