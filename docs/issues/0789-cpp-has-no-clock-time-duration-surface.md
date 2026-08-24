---
id: 789
title: "nros-cpp ships no clock, time or duration surface — a ported rclcpp node
  cannot stamp a message header; and ROS time exists in Rust but not in C"
status: open
type: bug
area: api
related: [rfc-0036, rfc-0073, phase-379, phase-209, issue-0788]
---

## Problem

Two halves of one shape, found by phase 379's `timer` stage.

**C++ has no clock surface at all.** `packages/api/nros-cpp/include/nros/` holds
`timer.hpp` and nothing else — no `clock.hpp`, no `time.hpp`, no `duration.hpp`.
Meanwhile:

* C has 27 `nros_clock_*` / `nros_time_*` symbols (`nros_clock_get_now_ns`,
  `nros_time_add`, `nros_time_sub`, `nros_time_compare`, …).
* Rust has `Clock`, `Time`, `Duration` and `TimerDuration`, with the full
  conversion set.

So `node->get_clock()->now()` — how nearly every ported rclcpp publisher stamps a
header — has nothing to call. Phase 209 exists to make a normal ROS 2 C++ node
land here by swapping build glue rather than rewriting source; this is a source
rewrite for any node that timestamps anything.

**ROS time runs the other way.** Our Rust `Clock` can be driven by a simulator's
`/clock` (`set_ros_time_override`, `clear_ros_time_override`,
`is_ros_time_override_active`, `get_ros_time_override`). Our C has none of them.
A C image cannot be run against a simulator or a bag while a Rust one can.

Neither half is a divergence from ROS 2. Both are our own languages disagreeing
about what exists — the same class as issue 0788, one level up from naming.

## Related gaps found alongside

* `Time::to_ros_msg` — converting a timestamp into `builtin_interfaces/Time` for
  a message header. Only Rust has a `Time` at all, and even it has only
  `to_nanos`, so the caller fills the message field by hand.
* Timer accessors C lacks and Rust has: `is_ready`, `time_until_next_call`,
  `time_since_last_call`, and any cancel predicate whatsoever.
* Runtime period change, which **no** language has: a node whose rate is a
  parameter must destroy and recreate its timer.

## Why it matters

A timestamp is not an optional part of a ROS message. `std_msgs/Header` carries
one, every sensor message embeds a header, and the receiver's ability to reason
about latency or ordering depends on it. An API that can publish a message but
cannot fill its `stamp` is not a client library a sensor node can use.

The ROS-time half matters differently: simulation and bag replay are how a node
is tested before it reaches hardware, and today that is available in one of our
three languages.

## Evidence

* `ls packages/api/nros-cpp/include/nros/` — `timer.hpp`, and no sibling.
* `grep -c 'nros_clock_\|nros_time_' packages/api/nros-c/include/nros/nros_generated.h`
  — 27.
* `scripts/api-parity.py --topic timer`, and the `gap` rows in
  `docs/reference/api-parity-ledger/timer.json` (`cpp:Clock::now`, `cpp:Time`,
  `cpp:Duration::Duration`, `cpp:Node::get_clock`, `c:enable_ros_time_override`,
  `rust:Time::to_ros_msg`).
* The types stage recorded `cpp:Clock` and `cpp:Duration` as gaps first and
  deferred the argument here — see `docs/reference/api-parity-ledger/types.json`.

## Direction

Not decided here; phase 379 W3 owns the coverage work. The pieces are separable
and the first is much the largest:

* A C++ `Clock`/`Time`/`Duration` over the C surface that already exists. The
  values are there; what is missing is the C++ face. RFC-0073 defines the
  platform clock contract the C layer already implements.
* `Node::get_clock()` and `Node::now()`, which is what ported source actually
  calls.
* ROS time in C, or a written decision that C images are not simulatable — but
  written down, because today the absence is silent.
