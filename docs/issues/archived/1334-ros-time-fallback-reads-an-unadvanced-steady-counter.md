---
id: 1334
title: "`ClockType::RosTime` with no `/clock` falls back to a steady counter
  NOTHING advances, so a `TimerClockSource::Ros` timer standing alone never
  fires — and the C surface falls back somewhere else"
status: resolved
resolved_in: "this branch — `RosTime` with no override IS `SystemTime`, one expression; the sweep took the steady clock with it"
type: bug
area: core, api
related: [phase-425, phase-430, phase-359, issue-1321]
---

> **Resolved.** `ClockType::RosTime` with no override and `ClockType::SystemTime`
> now evaluate the SAME expression — `Clock::wall_now()`
> (`packages/core/nros-core/src/clock.rs`) — so the Rust surface agrees with the
> C one (`get_ros_time_ns` → `get_system_time_ns`), with rclcpp
> (`ClockType::ROS_TIME` under `use_sim_time` false reads the system clock), and
> with the five doc sites across Rust/C/C++ that already promised it. The
> alternative — refuse or report — was declined: it contradicts phase-425's
> recorded choice and RFC-0089's "a node built for simulation still runs
> standalone", and it would be a compile-and-differ against rclcpp for a state
> that is NOT a misconfiguration (`use_sim_time` false is every standalone
> node). The state that IS a misconfiguration — `use_sim_time` TRUE and no
> `/clock` ever — already has its one-shot report (phase-430 W1,
> `time_source::SILENCE_WARN_US`), whose text ("ROS-time timers are running on
> SYSTEM time meanwhile") this fix makes TRUE; no second spelling was added.
>
> **The sweep found one more reader and it was the same defect:**
> `ClockType::SteadyTime` read the same unadvanced counter on every flavour,
> while the C surface answered `NROS_CLOCK_STEADY_TIME` with
> `nros_platform_clock_ns`. Two surfaces, one image, different answers, and the
> Rust one not a clock — 1334 exactly, one clock over. It now reads the port
> too (`Clock::steady_now()`, `platform_steady_clock()`), which overturns half
> of phase-359 W10's "`SteadyTime` is the counter on every flavour,
> deliberately … advanced by its owner": the owner never arrived. The counter
> survives as the fallback for a build with NO port, which is the only build
> where `update_steady_time`'s documented RTIC contract has a caller.
>
> **Issue 1321's reasoning survives, with one of its two premises replaced.**
> `TimerClockSource::remaining_is_wall_time` still answers `false` for `Ros`
> UNCONDITIONALLY. Its doc used to give two reasons and the first was this
> defect ("that fallback reads an in-image steady counter, so the remainder is
> a constant there too"), which is now false. The rule stands on the other two,
> both about the rule rather than one image's state: property 1 of phase-430
> (a ROS-time timer's wake source is a `/clock` MESSAGE, and a rule that
> flipped with whether a publisher was running would flip at the first sample,
> which is exactly when the remainder stops being wall microseconds), and the
> fact that the cost is LATENCY bounded by the caller's own spin budget. What
> the fix DOES change is that the latency is now observable: before, a
> standalone `Ros` timer never fired at all, so contributing nothing to the
> park cost nothing. Recorded at the site and in phase-430's row 1.

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

## What the fix measured

`Clock::now()`'s `SystemTime` arm and its `RosTime` no-override arm are now one
call, not two expressions that happen to agree — which is the whole mechanism of
the defect: phase-359 W10 moved `SystemTime` onto the platform port and left the
copy in the `RosTime` arm behind, reading a counter whose only writers are this
file's own tests.

Both surfaces, before and after, on a port-linked image:

| Surface | Before | After |
| --- | --- | --- |
| Rust `Clock::ros_time().now()`, no override | `0` (the counter, forever) | the port's wall clock |
| C `nros_clock_get_now(NROS_CLOCK_ROS_TIME)`, no override | the port's wall clock | unchanged |
| Rust `Clock::steady().now()` | `0` (the same counter) | the port's monotonic clock |
| C `nros_clock_get_now(NROS_CLOCK_STEADY_TIME)` | `nros_platform_clock_ns` | unchanged |

Negative controls, all five FAILING on the pre-fix tree with the fix reverted
and the tests kept:

| Test | Pre-fix |
| --- | --- |
| `nros-core` `ros_time_with_no_override_reads_the_port_wall_clock` | `Time { sec: 0, nanosec: 1234 }` vs `Time { sec: 1000000000, nanosec: 0 }` |
| `nros-core` `an_override_still_outranks_the_wall_clock` | `Time { sec: 0, nanosec: 0 }` vs `Time { sec: 1000000000, nanosec: 0 }` (the cleared-override half) |
| `nros-core` `platform_port_outranks_the_counter_for_the_steady_clock` | `1234` vs `42000000000` |
| `nros-c` `ros_time_agrees_with_the_rust_surface` | "C said 1789229098745062043 ns, Rust said 0 ns" |
| `nros-c` `every_clock_type_agrees_with_the_rust_surface` | "steady time disagrees by 2885106976858332 ns" |

The one test that passes BOTH before and after is deliberate and says so in its
own doc comment: `nros-node`'s
`ros_timer_fires_on_its_period_with_no_clock_source_installed`. That lane links
no platform port, so the wall clock IS the counter there and the two code paths
are the same static — which is the precise statement of the fix rather than a
hole in it. It is the end-to-end guard that the TIMER path follows this clock at
all, in the shape a standalone image ships; the controls live where a port makes
the two separable.

Two things the measurement corrected on the way:

* `test_system_clock_returns_nonzero` asserted `Clock::system().now().sec > 0`
  under `all(std, not(platform-clock))` — a build with no port, where
  `Clock::system()` reads the counter and the assertion is false. It had been
  false since phase-359 W10 and nobody saw it, because that cfg is in NO lane
  (the one nros-core feature lane is `std,platform-clock`; the default lane has
  no `std`). Replaced by `without_a_port_every_clock_is_the_counter`, which
  asserts what is true in that shape and is reachable from the default lane.
* The clock tests write three process-global statics and ran in parallel
  threads with no lock. `ClockGuard` (a `no_std` spin lock, because `std` here
  is a feature) now orders them and restores the defaults on drop.
