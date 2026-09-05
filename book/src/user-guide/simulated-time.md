# Simulated Time

A node in a simulation or replaying a bag should run on the simulator's clock,
not the wall. nano-ros implements ROS 2's answer to that: `rosgraph_msgs/msg/Clock`
on `/clock`, the `use_sim_time` parameter, and timers that can be told which
clock advances them.

## The distinction the names carry

rclcpp draws it in the method name, and so do we:

| you write | it advances with | affected by `/clock` |
|---|---|---|
| `create_wall_timer(period, cb)` | the platform's monotonic clock | no |
| `create_timer(clock, period, cb)` | whatever `clock` is | yes, if it is a ROS-time clock |

A wall timer is the right choice for a watchdog, a transport keep-alive, or
anything whose job is to notice that real time has passed — a paused simulator
must not pause those. Everything that consumes simulated data wants the other
one.

If you are porting from rclcpp, the spelling you already write is the spelling
that works. If you are reading older nano-ros code, note that `create_timer`
used to mean the wall timer here; it does not any more, and there is no alias.

## Reading the time

A node's clock is ROS time, as in rclcpp:

```cpp
nros::Time now = node.get_clock()->now();
```

With no `/clock` source installed, a ROS-time clock reads system time. That is
the same fallback `rclcpp::Clock` has, and it is what makes a node written for
simulation still run standalone.

Two predicates report which situation you are in:

```cpp
node.get_clock()->ros_time_is_active();  // is a /clock source driving this?
node.get_clock()->started();             // has a first sample arrived?
```

## Turning it on

### With the parameter, as in ROS 2

Declare `use_sim_time` and the runtime attaches the source itself — from a
launch file, a params YAML, or `ros2 param set`:

```yaml
talker:
  ros__parameters:
    use_sim_time: true
```

`use_sim_time` is reserved: nothing reads its value, the runtime acts on it.
Set it false and `/clock` samples stop being installed.

The Rust crate needs the `sim-time` feature, because the subscription costs an
entity slot and an RX buffer:

```toml
nros-node = { version = "0.5", features = ["sim-time"] }
```

### Explicitly

```rust
node.install_ros_time_source()?;          // subscribes /clock
node.install_ros_time_source_on("/sim/clock")?;  // a remapped clock
```

QoS is `ClockQoS` — best effort, keep-last 1, volatile. A late subscriber wants
the next sample, not a replay of the simulation's history.

## Writing a timer that follows it

### C++

```cpp
nros::Timer t;
NROS_TRY(node.create_timer(t, *node.get_clock(), 100, on_tick));
```

### Rust

```rust
executor.register_timer_on_clock(
    TimerDuration::from_millis(100),
    TimerClockSource::Ros,
    || { /* … */ },
)?;
```

`TimerClockSource::Steady` is the default everywhere and stays free: it consumes
the spin delta the executor has already measured. `Ros` and `System` read their
clock on every poll of the timer, which on a target is a platform call — that is
why the choice is a separate entry point rather than a defaulted argument.

## What happens at the edges

**Paused.** `/clock` stops advancing, so the delta is zero and a ROS-time timer
does not fire. Wall timers on the same executor keep their cadence.

**Replayed at 0.5x or 2x.** The timer tracks the rate, because it advances by
the clock's own delta rather than by elapsed real time.

**A jump backwards** — a bag looping, a simulator reset — restarts the period.
The alternative would be a timer that stays silent for the length of the jump.

**A jump forwards** is not special-cased. It lands as a backlog, and
`TimerOverrunPolicy` decides: `Skip` (the default) coalesces it into one
activation and counts the rest, `CatchUp` replays every missed period.

**Unsubscribing does not reset the clock.** A node that stops listening keeps
the last simulated time rather than snapping back to the wall, which every
ROS-time timer would otherwise absorb as a jump backwards. Clearing is
deliberate: `Clock::clear_ros_time_override()`.

## Limits worth knowing before you rely on this

**One simulated clock per image.** The override is process-global, which is the
model `nros_core::Clock` has always documented. Two nodes in one image cannot be
on different simulated times.

**No jump callbacks.** `rclcpp::Clock::create_jump_callback` and the
`rcl_jump_threshold_t` machinery have no counterpart. The behaviour a jump
produces is fixed (above); you cannot subscribe to the event.

**No `wait_until_started` / `sleep_until`.** RFC-0021 forbids a blocking helper
that does not drive the executor. Poll `started()` from your spin loop.

**Timers only.** A ROS-time clock changes what `now()` returns and what a
ROS-time timer does. It does not retime message delivery, QoS deadlines, or
transport lease timers — those remain on the monotonic clock, which is what you
want for a link that has to stay up while the simulation is paused.

**The age monitor is on a different clock from your stamps.** RFC-0052's
`max-age-runtime` rule compares a message's `header.stamp` against an EPOCH
clock (µs since the UNIX epoch, installed by the board via
`Executor::set_epoch_clock`). If you stamp with `node.get_clock()->now()` while
a simulator is driving ROS time, the two disagree and the reported ages are
meaningless — usually enormous. Either stamp from the epoch clock, or leave the
age rule off for that topic while running under simulation.
