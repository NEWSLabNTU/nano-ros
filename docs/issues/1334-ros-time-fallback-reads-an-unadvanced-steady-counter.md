---
id: 1334
title: "`ClockType::RosTime` with no `/clock` falls back to a steady counter
  NOTHING advances, so a `TimerClockSource::Ros` timer standing alone never
  fires — and the C surface falls back somewhere else"
status: open
type: bug
area: core, api
related: [phase-425, phase-430, issue-1321]
---

## Problem

Two surfaces answer "what is ROS time when no `/clock` source is installed?"
and they give different answers, and the Rust one is not a clock.

`nros_core::Clock::now()` for `ClockType::RosTime`
(`packages/core/nros-core/src/clock.rs:298`–`305`):

```rust
ClockType::RosTime => {
    let override_nanos = atomic_time::get_ros_override();
    if override_nanos >= 0 {
        Time::from_nanos(override_nanos)
    } else {
        let nanos = atomic_time::get_steady();   // <- NOT the wall clock
        Time::from_nanos(nanos)
    }
}
```

`atomic_time::get_steady()` reads `STEADY_TIME_NANOS`, which the type's own
doc calls a counter "advanced by its owner" — and **nothing in the tree owns
it**. Measured:

```
$ grep -rn "update_steady_time\|add_steady\|advance_steady" packages examples \
    --include='*.rs' --include='*.c' --include='*.hpp' --include='*.h'
packages/core/nros-core/src/clock.rs:98    (the setter itself)
packages/core/nros-core/src/clock.rs:146   (the setter itself, 32-bit arm)
packages/core/nros-core/src/clock.rs:355   (pub fn update_steady_time)
packages/core/nros-core/src/clock.rs:365   (pub fn update_steady_time_ms)
packages/core/nros-core/src/clock.rs:445   (its own unit test)
packages/core/nros-core/src/clock.rs:449   (its own unit test)
```

Zero callers outside `nros-core`'s own tests. So in any shipped image the
counter sits at 0 and `Clock::ros_time().now()` returns the epoch, forever.

The C surface does NOT do this. `get_ros_time_ns` (`packages/api/nros-c/src/
clock.rs:157`) falls back to `get_system_time_ns()` — the wall clock — so
`nros_clock_get_now(NROS_CLOCK_ROS_TIME)` and a Rust `Clock::ros_time().now()`
disagree on the same image in the same state.

## What it costs

A `TimerClockSource::Ros` timer with no `/clock` publisher **never fires**.
`timer_try_process` (`executor/arena.rs`) advances such a timer by
`now_ns - last_clock_ns` on `Clock::ros_time()`; with the counter frozen that
step is 0 on every poll, so `elapsed_us` never reaches `period_us`.

That is the opposite of what three places promise:

* `TimerClockSource::Ros`'s own doc — "system time when none is (the same
  fallback `rclcpp::Clock` has, so a node built for simulation still runs
  standalone)" (`packages/core/nros-node/src/timer.rs:164`–`167`);
* `Executor::register_timer_on_clock`'s doc — "With no `/clock` source
  installed, a `Ros` timer reads system time and behaves like a wall timer
  with NTP steps";
* phase-430's delta row 15, which verdicts property 3 NO LONGER APPLIES on
  exactly that fallback.

So the decision recorded in row 15 was made, written down three times, and
not implemented: the code takes neither the documented arm (wall) nor the
refused one (do not fire) — it takes a third that reads as "do not fire" by
accident.

## How it was found

Issue 1321 (the park bound). Deciding what a `Ros` timer may contribute to a
WALL park bound needs the answer to "when no `/clock` is attached, is its
remaining time wall microseconds?" — the documented fallback says yes, the
code says the question does not arise because nothing ever moves. 1321's fix
does not depend on which way this is resolved (a `Ros` timer contributes
nothing either way, see its "Ros" arm), but the argument had to be measured
rather than read, and this is what the measurement found.

## Shape of a fix (not done here)

Make `ClockType::RosTime`'s no-override arm read the same thing
`ClockType::SystemTime` does — `platform_wall_clock()`, then the counter —
which is one line and makes the Rust surface agree with the C one and with
all three docs. The alternative (keep the counter and give it an owner) means
naming who advances it on every port, which is a platform-clock duplicate
nobody asked for.

*Acceptance:* a unit test registering a `TimerClockSource::Ros` timer with NO
override installed and asserting it fires at its period; and one asserting
`Clock::ros_time().now()` equals `Clock::system().now()` to within a
tolerance when no override is set, which is what both docs claim today.
