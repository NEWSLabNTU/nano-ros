---
id: 1975
title: "Four small rclcpp accessors Jazzy added are missing: Logger::get_effective_level, Publisher::is_durability_transient_local, Timer::get_clock, NodeOptions::clock_type"
status: resolved
type: enhancement
area: api
severity: low
found: 2026-10-19
resolved: 2026-10-20
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

## Resolution

All four landed on the C++ API (`packages/api/nros-cpp`); the four Jazzy `gap`
rows are gone from the api-parity ledger.

| upstream | ours | shape |
| --- | --- | --- |
| `Logger::get_effective_level()` | `rclcpp::Logger::get_effective_level()` | same; forwards to `get_level()`, which already answered the effective level |
| `PublisherBase::is_durability_transient_local()` | `rclcpp::Publisher<M>::is_durability_transient_local()` | same; reads the GRANTED durability off `get_actual_qos()`, as upstream does (Unknown, e.g. XRCE, reads `false`) |
| `TimerBase::get_clock()` | `rclcpp::Timer::get_clock()` and `timer_->get_clock()` on `TimerHandle` | ADOPT-BOUNDED: returns `rclcpp::detail::ClockValuePtr` (the `Clock` by value behind `->`/`*`) instead of `const Clock::SharedPtr&` -- no allocator to share one from, and the handle is two words |
| `NodeOptions::clock_type()` | getter + builder setter, a REAL field | same; the `Node(name, options)` constructors build the node clock from it, and `get_node_options()` returns the stored options instead of a constant. A non-clock type latches `ok() == false` (upstream throws) |

`Timer::get_clock` answers from the executor, not from a copy on the C++ side:
`Executor::timer_clock_source` (`nros-node`, reads `TimerHeader::clock_source`,
the field the dispatcher reads) -> `nros_c::nros_timer_clock_type` (inverse of
`nros_timer_clock_source`) -> `nros_cpp_timer_get_clock_type`. No C or Rust
user-facing counterpart was added.

Ledger: `cpp:Timer::get_clock` -> `divergence` / `adopt-bounded` (no edition
scope; Humble has no such member, and the same row answers its ours-only item).
The other three are `same` under Jazzy and carry Humble-scoped `extension` rows,
since Humble's rclcpp lacks them. `rust:Executor::timer_clock_source` ->
`extension`.

Held by `packages/api/nros-cpp/tests/compile/jazzy_small_accessors_runtime.cpp`
(COMPILE+LINK+RUN on `just check cpp`, stub backend -- whose
`publisher_get_actual_qos` now echoes the create profile so the
transient-local answer is checked against a known grant),
`timer_clock_source_reports_the_registered_clock` (`nros-node`) and
`timer_clock_type_inverts_timer_clock_source` (`nros-c`).
