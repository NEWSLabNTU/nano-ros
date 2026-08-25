---
id: 789
title: "ROS time exists in Rust and not in C, so a C image cannot be driven by a
  simulator's /clock (the C++ clock surface half is fixed)"
status: resolved
type: bug
area: api
related: [rfc-0036, rfc-0073, phase-379, phase-209, issue-0788, issue-0799]
resolved_in: "2026-08-25"
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

## Fixed 2026-08-25 — the C++ half

`nros/duration.hpp`, `nros/time.hpp` and `nros/clock.hpp` added over the C
surface that already existed, plus `Node::get_clock()` / `Node::now()` and all
three in the `nros.hpp` umbrella. `node->now()` compiles. **19 ledger `gap` rows
closed** — the C++ lane's `same` count went 54 → 63.

Matched exactly where rclcpp's names are the point: `Duration::{max,
nanoseconds, seconds, from_seconds, from_nanoseconds}`, `Time::{max,
nanoseconds, seconds, get_clock_type}`, `Clock::{now, get_clock_type}`,
`Node::{get_clock, now}`, and the arithmetic/comparison operators.

Deliberately not matched, each already declined in the ledger: `to_chrono` (no
`<chrono>` freestanding), the `rmw_time_t` conversions (the RMW seam is not
user-facing), `make_shared`/`make_unique`/`sleep_*` (no allocator, RFC-0021),
and the jump callbacks. `get_clock()` returns `Clock*` into a node-owned member
rather than a `shared_ptr`, so `node->get_clock()->now()` keeps its spelling.

`Time::to_msg(MsgT&)` / `Duration::to_msg(MsgT&)` supply what
`rust:Time::to_ros_msg` was recorded as a gap for. rclcpp spells it as an
implicit conversion operator, which a header-only library cannot provide for a
per-user generated type.

**Two corrections to this issue's own text**, both found by building the thing:

* This issue said `nros_clock_get_now_ns` returns nanoseconds directly. It does
  not — it takes an out-pointer and returns `nros_ret_t`. The C++ face is
  infallible anyway (an invalid clock reads 0; `is_valid()` is the report) and
  says so in its doc comment.
* `Duration::to_msg` does NOT delegate to `nros_time_from_nanoseconds`, because
  that function is wrong for negative spans — **issue 0799**, found here.

`Clock::{started, ros_time_is_active}` stay gaps for a different reason than
before: ROS time is real only in Rust, so the predicate would be a constant. See
`c:enable_ros_time_override`.

## Fixed 2026-08-25 — the ROS-time half

Six C entry points, in `packages/api/nros-c/src/clock.rs`, spelled as rcl spells
them so a ported node keeps its source:

| C | rcl | Rust it mirrors |
| --- | --- | --- |
| `nros_enable_ros_time_override(clock)` | `rcl_enable_ros_time_override` | `Clock::set_ros_time_override(0)` when none is active |
| `nros_disable_ros_time_override(clock)` | `rcl_disable_ros_time_override` | `Clock::clear_ros_time_override` |
| `nros_is_enabled_ros_time_override(clock, bool*)` | `rcl_is_enabled_ros_time_override` | `Clock::is_ros_time_override_active` |
| `nros_set_ros_time_override(clock, int64_t)` | `rcl_set_ros_time_override` | `Clock::set_ros_time_override` |
| `nros_get_ros_time_override(clock, int64_t*)` | — | `Clock::get_ros_time_override` (the `Option` becomes `NROS_RET_NOT_FOUND`) |
| `nros_clock_time_started(clock)` | `rcl_clock_time_started` | — (rcl's own implementation: read the clock, report non-zero) |

They drive **`nros_core::Clock`'s global, not a second copy**, so a process whose
C and Rust nodes share an image cannot hold two answers to "what time is it".
`nros_clock_get_now` / `_get_now_ns` read the override on a
`NROS_CLOCK_ROS_TIME` clock and fall back to the system clock as before.

**One model difference from rcl, and it is Rust's model that wins.** rcl keeps an
ENABLED flag and a VALUE separately; ours keeps one negative-sentinel value,
because that is what Rust has had since the clock shipped and a second model in C
is the split this issue is about. So `nros_set_ros_time_override` also enables,
there is no state where a stored value is inert, and a NEGATIVE time is
`NROS_RET_INVALID_ARGUMENT` (it is the sentinel; ROS time is since the epoch).
The override is process-global rather than per-clock for the reason the Rust
ledger rows already give: with no allocator a clock cannot own a separately
shared time source.

`nros::Clock::ros_time_is_active()` and `nros::Clock::started()` land on the C++
side over the same switches — the two members left as gaps *because* C had none.
The setters stay C free functions rather than C++ members: they drive a global,
and hanging a global's setter off an instance would read as per-clock state.
`wait_until_started` remains declined (RFC-0021 forbids a blocking helper that
does not drive the executor); its predicate is now available to poll.

**Ledger:** `c:{enable,disable,is_enabled,set}_ros_time_override`,
`c:clock_time_started`, `cpp:Clock::started` and `cpp:Clock::ros_time_is_active`
were `gap` and are now `same` — the rows are deleted. Two new `extension` rows
(`c:get_ros_time_override`, `c:duration_from_nanoseconds`). The twelve time-jump
rows that gave "the simulation case needs ROS time, which C does not have either"
as half their reason were rewritten: the reason is now that nobody asked to be
TOLD the clock stepped, which is a different and still-true claim.

Found on the way: `nros_time_from_nanoseconds` encodes negative times wrong —
**issue 0799**, fixed with this.

