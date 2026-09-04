# Phase 425 — ROS time is a type, not a behaviour: give the timer API the official semantics

**Status (2026-09-05).** W1 in progress. Opened by an owner decision on the
`cpp:Node::create_wall_timer` ledger row: the answer to "do we adopt rclcpp's
name" is yes, and the reason given widens the work past a rename — bag replay
and simulation need ROS *time*, not just the word `wall`.

## The situation

We ship the whole vocabulary of ROS time and none of its behaviour.

`ClockType::RosTime` exists (`packages/core/nros-core/src/clock.rs:65`,
documented "Simulation time (can be paused/scaled)"), with a process-global
override behind it (`ROS_TIME_OVERRIDE_NANOS`, `:76`). The C surface exposes
`nros_set_ros_time_override` (`packages/api/nros-c/src/clock.rs:382`),
`nros_clock_time_started`, `nros_is_enabled_ros_time_override`. The C++ face has
`nros::Clock::ros_time_is_active()` and `started()`
(`packages/api/nros-cpp/include/nros/clock.hpp`), and a node carries a clock —
`Node::get_clock()` at `node.hpp:271`. Issue 0789 landed all of it on
2026-08-25.

Three things are missing, and together they mean no user can get ROS-time
behaviour today:

1. **Nothing installs a ROS time.** No `/clock` subscriber exists and none could:
   `packages/interfaces/` ships `rcl-interfaces`, `lifecycle-msgs` and
   `diagnostic-msgs`, and no `rosgraph_msgs`. `use_sim_time` appears in this repo
   only as an arbitrary parameter name in the parameters example and its
   round-trip test — nothing reads it. The override is reachable only by a
   program calling the setter itself.
2. **No timer consults a clock.** Every timer accumulates the executor's spin
   delta (`arena.rs:1717`, `entry.elapsed_us += delta_us`), and that delta comes
   from the platform monotonic counter. A paused simulator does not pause a
   timer; a bag replayed at 0.5x does not halve its rate; a jump backwards is
   invisible.
3. **The name for the clock-taking form is occupied.** rclcpp reserves bare
   `create_timer` for the overloads that TAKE a `Clock::SharedPtr`, and spells
   the steady one `create_wall_timer`. Ours is backwards: `Node::create_timer`
   (`node.hpp:548`) is a steady timer under rclcpp's clock-taking name. A ported
   node calling `create_timer` gets, silently, the one thing it did not ask for.

## Why it is worth building, not just documenting

The two use cases the owner named are the ones that cannot be worked around:

* **Bag replay.** `ros2 bag play --clock` publishes `/clock`; a node whose timers
  ignore it runs at wall rate against data arriving at replay rate. Every
  rate-dependent computation (control loops, filters, watchdogs) is then measuring
  a clock nobody else in the system is on.
* **Simulation.** A simulator that pauses expects its subscribers to pause. A
  nano-ros node in the loop keeps firing, and the divergence is silent — the
  failure looks like a bug in the algorithm.

Neither is an embedded-only concern, but neither is excluded by the constraints
either: ROS time costs one subscription, one global `i64`, and a per-timer
branch. Nothing here needs an allocator or `std`.

## Work items

### W1 — free the name (C++)

Rename, in one commit, with **no deprecated alias** (an alias keeps the
ours-only ledger row alive forever):

| from | at | to |
| --- | --- | --- |
| `Node::create_timer` | `node.hpp:548` | `Node::create_wall_timer` |
| `nros::create_timer` (`NROS_CPP_STD` free fn) | `std_compat.hpp:61` | `nros::create_wall_timer` |
| `ComponentNode::create_timer` (member-bound) | `component_node.hpp:389` | `ComponentNode::create_wall_timer` |
| `ComponentNode::create_timer` (callback+ctx) | `component_node.hpp:406` | `ComponentNode::create_wall_timer` |
| `NROS_CREATE_TIMER` | `component_node.hpp:834` | `NROS_CREATE_WALL_TIMER` |

`create_timer_oneshot`, `create_timer_in`, `ComponentNode::create_timer_in` and
`bind_timer` KEEP their `timer` stem — recorded here so the stem split is a
decision and not an oversight. `create_timer_oneshot` is the name rclrs 0.7.0
independently adopted; `_in` is our callback-group suffix, shared with
`create_publisher_in` / `create_subscription_in`.

Half these sites are invisible to `just check api-parity` — `component_node.hpp`
is not reachable from `nros.hpp`, and `std_compat.hpp` is a no-op without
`NROS_CPP_STD`. Sweep with `rg -w create_timer packages/api/nros-cpp/
examples/**/cpp/` and put the command in the commit message.

**Acceptance.** `rg -w create_timer` over `packages/api/nros-cpp/` and
`examples/` returns only the `_oneshot` / `_in` / `bind_` family; `just check
api-parity` green; `just check cpp` green.

### W2 — `rosgraph_msgs/msg/Clock`

Add `packages/interfaces/rosgraph-msgs` on the `diagnostic-msgs` pattern
(`package.xml` + committed `generated/humble/`, `nros-` prefixed crate, constant
`version = "0.0.0"`), plus its private regeneration recipe.

**Acceptance.** A `no_std` core crate can subscribe `rosgraph_msgs/msg/Clock`;
the leaf-lockfile invariant holds (`just check leaf-lockfiles`).

### W3 — the time source

A `TimeSource` that subscribes `/clock` with rclcpp's `ClockQoS` (best effort,
keep-last 1, volatile — `cpp:ClockQoS::ClockQoS` is already a `gap` row) and
installs each sample through the existing override. Driven by the
`use_sim_time` parameter, which becomes a REAL node parameter with rclcpp's
meaning rather than a name examples happen to use.

Off by default and compile-time excludable: an image that will never see a
simulator should not carry the subscription.

**Acceptance.** With `use_sim_time:=true` and `ros2 bag play --clock`, a node's
`get_clock()->now()` tracks bag time; with no publisher on `/clock`,
`started()` is false and nothing blocks.

### W4 — timers on a clock

`create_timer(clock, period, cb)` in C++ (the name W1 frees), and the
equivalent selection on the C and Rust surfaces. A ROS-time timer accumulates
the ROS-clock delta rather than the spin delta: zero while the simulator is
paused, scaled with replay rate.

Jump handling is part of the semantics, not an extra: a backwards jump resets
`elapsed_us` rather than stalling the timer for the length of the jump. rclcpp
does this through jump callbacks (`c:clock_add_jump_callback`, currently
`declined`); we need the behaviour, and whether the callback surface follows is
a W5 question, not a W4 one.

**Acceptance.** A test that installs a ROS time by hand — no simulator needed —
and asserts: paused ROS time fires no ROS-time timer while a wall timer on the
same executor keeps firing; 2x ROS time doubles the rate; a backwards jump
produces one period, not a stall.

### W5 — ledger, docs, book

Re-verdict what W1–W4 change (`cpp:Node::create_wall_timer` and
`cpp:create_wall_timer` from `rename` to landed; `cpp:Node::create_timer` and
`cpp:create_timer` lose their ours-only halves; `rust:Node::get_clock` gap;
`cpp:ClockQoS::ClockQoS` gap; the `IntoTimerOptions` family's `declined` rows
get re-read against whatever W4 lands). Book page for sim time. RFC for the
time-source design if W3's shape outgrows this doc.

## Sequencing

W1 first and alone: it is the breaking change, it is mechanical, and it frees
the name W4 needs. W2 and W3 are the runtime; W4 is meaningless without them —
a clock-taking `create_timer` whose ROS clock nothing ever drives is the same
lie in a new place. W5 last, because it records what actually landed.
