# Use QoS, status events and discovery

<!-- type: how-to · level: 2 -->

**Needs first:** [Talk to ROS 2](../../tutorial/talk-to-ros2.md)

Use status events when a node must react to what its middleware sees: a
publisher it depends on going silent, a periodic topic missing its deadline,
samples lost to a slow consumer, or the first subscriber appearing.

The events are the ones ROS 2 defines, with the
[QoS](https://docs.ros.org/en/humble/Concepts/Intermediate/About-Quality-of-Service-Settings.html)
policies behind them. Two things differ in nano-ros. A callback you register
runs from inside `spin_once`, on the executor thread, like a message
callback; there is no waitset to poll. And an incompatible QoS pair is not an
event: it fails at `create_publisher` or `create_subscription` with
`NROS_RMW_RET_INCOMPATIBLE_QOS`, so you see it at start-up.

| Event | Side | Fires when |
| --- | --- | --- |
| `LivelinessChanged` | subscription | a matched publisher starts or stops asserting liveliness |
| `RequestedDeadlineMissed` | subscription | no sample arrived within the deadline |
| `MessageLost` | subscription | the backend dropped samples |
| `LivelinessLost` | publisher | this publisher missed its own liveliness deadline |
| `OfferedDeadlineMissed` | publisher | this publisher fell behind its offered rate |
| `PublicationMatched`, `SubscriptionMatched` | either | an endpoint on the other side appears or leaves |
| `PublisherIncompatibleType`, `SubscriptionIncompatibleType` | either | the same topic name appears with another type |

## Before you start

- A node package whose subscription or publisher you want to watch.
- Its backend. Not every backend generates every event; the
  [Per-RMW feature matrix](../../reference/rmw-feature-matrix.md) lists
  which ones each backend implements.

## Steps

1. Ask the backend first, if the node must run on more than one. A
   registration the backend cannot serve answers `Unsupported`:

   ```rust
   if sub.supports_event(EventKind::RequestedDeadlineMissed) {
       sub.on_requested_deadline_missed(Duration::from_millis(15), on_late)?;
   } else {
       // fall back to a timeout of your own
   }
   ```

2. Register a subscription callback. The callback runs from `spin_once`,
   counts as one callback against the executor's per-spin limit, and never
   runs after the subscription is finalized.

<!-- tabs -->

### Rust

```rust
let mut sub = node.create_polling_subscription::<SensorReading>("/sensor")?;
sub.on_liveliness_changed(|status| {
    if status.alive_count == 0 {
        log::warn!("sensor publisher went silent");
    }
})?;
sub.on_message_lost(|status| {
    log::warn!("dropped {} samples", status.total_count_change);
})?;
```

### C

```c
static void on_liveliness_changed(rcl_subscription_t *sub,
                                  rclc_liveliness_changed_status_t status,
                                  void *user_context) {
    if (status.alive_count == 0) {
        trigger_failover();
    }
}

/* After rclc_executor_add_subscription(): */
rcl_ret_t rc = rclc_subscription_set_liveliness_changed_callback(
    &sub, on_liveliness_changed, NULL);
```

The C setters return `RCL_RET_OK`, or `RCL_RET_UNSUPPORTED` when the backend
does not generate the event.

### C++

```cpp
static void on_liveliness_changed(void* storage,
                                  nros_cpp_liveliness_changed_status_t status,
                                  void* user_context) {
    if (status.alive_count == 0) trigger_failover();
}

// On an rclcpp::PollingSubscription:
sub.on_liveliness_changed(on_liveliness_changed, nullptr);
sub.on_requested_deadline_missed(15 /* ms */, on_deadline_missed, nullptr);
```

The C++ setters take a function pointer and a `user_context`, not a lambda,
and the subscription setters exist on `PollingSubscription` only.
`ErrorCode::Unsupported` means the backend does not generate the event.

<!-- /tabs -->

3. Start publishing once someone listens. A matched callback on the
   publisher reports the subscribers it matches; publisher-side events are
   noticed when the publisher publishes or asserts liveliness, so a
   publisher that is not publishing yet asserts liveliness while it waits:

   ```rust
   pub_.on_matched(|s| {
       if s.current_count > 0 { LISTENER_PRESENT.store(true, Ordering::Release); }
   })?;
   while !LISTENER_PRESENT.load(Ordering::Acquire) {
       let _ = pub_.assert_liveliness();
       executor.spin_once(Duration::from_millis(50));
   }
   ```

   When the subscriber goes away, `current_count_change` is `-1`.

4. Watch a periodic topic's deadline. Set the deadline to the period plus
   the jitter you accept; for a 100 Hz topic, 15 ms:

   ```rust
   let mut sub = node.create_polling_subscription::<SensorReading>("/imu")?;
   sub.on_requested_deadline_missed(Duration::from_millis(15), |status| {
       if status.total_count_change > 0 {
           log::error!("IMU late: {} missed deadlines", status.total_count_change);
       }
   })?;
   ```

   On zenoh the argument is the deadline unless the subscription's QoS
   states one. On Cyclone DDS the deadline comes from the QoS profile and the
   argument is ignored.

## What each backend generates

- **zenoh** generates the five core events and both matched and
  incompatible-type pairs. Liveliness is per publisher (`alive_count` is 0
  or 1), and there is no `LivelinessChanged` on bare metal. Matched counts
  come from the graph cache, as in `rmw_zenoh_cpp`.
- **Cyclone DDS** generates the same set by polling: the runtime asks Cyclone
  on the entity's own data path. A publisher that stops publishing stops
  being asked, so its events arrive with its next publish or
  `assert_liveliness`.
- **XRCE-DDS** generates the two deadline events only, by a clock check on
  the client. The agent's DDS entity keeps an infinite deadline, so a DDS
  peer matching on deadline sees none.

A subscription with any status callback is asked on every spin, not only
when data arrives, so a publisher leaving is noticed at the next spin after
the backend sees it. A subscription with no status callback costs an idle
executor nothing.

## Example

A subscription that fails over when its publisher goes silent and counts
late samples, on any backend:

```rust
use core::time::Duration;

let mut sub = node.create_polling_subscription::<VehicleAttitude>("/vehicle_attitude")?;
sub.on_liveliness_changed(|status| {
    if status.alive_count == 0 {
        request_minimum_risk_manoeuvre();
    } else if status.alive_count_change > 0 {
        clear_failover();
    }
})?;
if sub.supports_event(EventKind::RequestedDeadlineMissed) {
    sub.on_requested_deadline_missed(Duration::from_millis(15), |status| {
        LATE.fetch_add(status.total_count_change as u32, Ordering::Relaxed);
    })?;
}
```

## If it doesn't work

- **A C subscription setter returns `RCL_RET_NOT_INIT`.** The subscription
  is not in an executor yet. Register after
  `rclc_executor_add_subscription`.
- **A C or C++ setter returns `RCLC_RET_FULL` or `ErrorCode::Full`.** The
  process-wide table holds 16 registrations across every C and C++ entity.
  One publisher with all four of its kinds and one subscription with all
  five take nine.
- **A registration answers `Unsupported`.** The backend does not generate
  that event; see the list above. Use `supports_event` and a fallback.
- **A publisher-side event never fires.** The publisher is not publishing.
  Call `assert_liveliness` from its loop.

## See also

- [Per-RMW feature matrix](../../reference/rmw-feature-matrix.md): the
  generated list of events and QoS policies per backend.
- [Talk to ROS 2](../../tutorial/talk-to-ros2.md): QoS on the wire
  between nano-ros and ROS 2.
- [The execution model](../../background/execution-model.md#what-one-spin-does): the per-spin
  callback limit that event callbacks share.
- [How nano-ros differs from ROS 2](../../background/differences-from-ros2.md).
