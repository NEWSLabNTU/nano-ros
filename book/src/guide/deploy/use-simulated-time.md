# Use simulated time

<!-- type: how-to · level: 3 -->

**Needs first:** [Parameters and launch files](../../tutorial/grow-your-workspace.md#3-add-a-launch-file-when-you-need-one)

Use simulated time when a node runs against a simulator or a replayed bag, and
its timers should follow the simulation's clock instead of the wall.

nano-ros implements ROS 2's mechanism: `rosgraph_msgs/msg/Clock` on `/clock`,
the `use_sim_time` parameter, and timers that state which clock advances
them. As in rclcpp, the method name carries the choice:

| You write | It advances with | Follows `/clock` |
| --- | --- | --- |
| `create_wall_timer(period, cb)` | the platform's monotonic clock | no |
| `create_timer(clock, period, cb)` | the clock you pass | yes, for a ROS-time clock |

Keep a wall timer for a watchdog or a keep-alive, anything whose job is to
notice that real time passed; a paused simulator must not pause it. Older
nano-ros code used `create_timer` for the wall timer. It no longer means that,
and there is no alias.

## Before you start

- A node package whose timers should follow the simulation.
- A `/clock` source: a simulator, or `ros2 bag play --clock`.

## Steps

1. Enable the feature in a Rust node package. The `/clock` subscription costs
   an entity slot and a receive buffer, so it is opt-in:

   ```toml
   nros = { version = "*", default-features = false, features = [
       "alloc", "rmw-cffi", "macros", "sim-time",
   ] }
   ```

2. Turn it on with the parameter, as in ROS 2: from a launch file, a
   parameters file, or at run time. The runtime acts on `use_sim_time`
   itself; nothing in your code reads it:

   ```yaml
   talker:
     ros__parameters:
       use_sim_time: true
   ```

   An image built with parameter services declares `use_sim_time = false`
   for every node, as rclcpp does, so this works on a node whose author never
   mentioned simulated time:

   ```bash
   ros2 param set /talker use_sim_time true
   ```

   A declaration of your own still wins. To attach the source in code
   instead, call `node.install_ros_time_source()?`, or
   `node.install_ros_time_source_on("/sim/clock")?` for a remapped clock.

3. Write the timers that should follow it on the ROS clock:

<!-- tabs -->

### Rust

```rust
executor.register_timer_on_clock(
    TimerDuration::from_millis(100),
    TimerClockSource::Ros,
    || { /* … */ },
)?;
```

`TimerClockSource::Steady` is the default and costs nothing extra: it uses
the spin delta the executor already measured. `Ros` and `System` read their
clock on every poll, a platform call on a board, which is why they have their
own entry point.

### C++

```cpp
rclcpp::Timer t;
RCLCPP_TRY(node.create_timer(t, *node.get_clock(), 100, on_tick));
```

<!-- /tabs -->

4. Read the time. A node's clock is ROS time, and with no `/clock` source it
   reads system time, as `rclcpp::Clock` does, so the node still runs
   standalone:

   ```cpp
   rclcpp::Time now = node.get_clock()->now();
   node.get_clock()->ros_time_is_active();  // is a /clock source driving it?
   node.get_clock()->started();             // has a first sample arrived?
   ```

5. Start the source and run the node:

   ```bash
   ros2 bag play my_bag --clock
   ```

## Example

A talker that publishes every 100 ms of simulated time, switched on from its
parameters file:

```yaml
# config/talker.param.yaml
talker:
  ros__parameters:
    use_sim_time: true
```

```rust
executor.register_timer_on_clock(
    TimerDuration::from_millis(100),
    TimerClockSource::Ros,
    || { /* publish */ },
)?;
```

```bash
ros2 bag play my_bag --clock
```

## What happens at the edges

- **Paused:** the delta is zero, so a ROS-time timer does not fire. Wall
  timers on the same executor keep their cadence.
- **Replayed at 0.5× or 2×:** the timer tracks the rate, because it advances
  by the clock's own delta.
- **A jump backwards** (a bag looping, a simulator reset) restarts the
  period.
- **A jump forwards** lands as a backlog, and the timer overrun policy
  decides: `Skip` (the default) fires once, `CatchUp` replays every period.
- **Unsubscribing** keeps the last simulated time rather than snapping back
  to the wall. Clear it on purpose with `Clock::clear_ros_time_override()`.

## Where nano-ros differs from rclcpp

- **One simulated clock per image.** The override is process-wide; two
  nodes in one image cannot be on different simulated times.
- **No jump callbacks.** `create_jump_callback` and `rcl_jump_threshold_t`
  have no counterpart.
- **No `wait_until_started` or `sleep_until`.** Poll `started()` from your
  spin loop.
- **Timers only.** Message delivery, QoS deadlines and transport leases stay
  on the monotonic clock, so a link stays up while the simulation is paused.

## If it doesn't work

- **ROS-time timers never fire.** A never-connected `/clock` looks exactly
  like a paused one. Five seconds after the source attaches with no sample,
  the image logs one warning:

  ```text
  use_sim_time is true but no /clock sample has arrived in 5s: ROS-time timers are
  running on SYSTEM time meanwhile. Publish rosgraph_msgs/msg/Clock (ros2 bag play
  --clock), or set use_sim_time false.
  ```

  Publish `/clock`, or turn `use_sim_time` off. The warning comes once;
  `Executor::sim_time_silence_warnings()` answers the same question in code.
- **Message ages are enormous.** The `max-age-runtime` rule compares
  `header.stamp` with the epoch clock, not ROS time. Stamp from the epoch
  clock, or leave that rule off for the topic under simulation.

## See also

- [Pick a scheduling model](../realtime/pick-a-scheduling-model.md): the
  timer overrun policy.
- [ROS 2 clock and time](https://design.ros2.org/articles/clock_and_time.html):
  the upstream design.

## Design background

- [RFC-0021: blocking API rules](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0021-blocking-api-rules.md)
- [RFC-0052: system model and RTOS mapper (runtime monitors)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0052-system-model-rtos-mapper.md)
