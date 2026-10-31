# QoS, Status Events, and Discovery

This page covers runtime event behavior that application authors need
after choosing an RMW backend: liveliness, deadline misses, lost
messages, and the places where nano-ros deliberately differs from
standard ROS 2 waitset-style event handling.

Transport-level status events let an application observe conditions
the underlying middleware detects: a remote node going silent, a
publisher missing its rate, a subscriber dropping samples. nano-ros
exposes a Tier-1 subset useful for embedded RTOS deployments —
liveliness changes, deadline misses, and message loss.

This page describes the event surface, the dispatch model, and the
patterns that motivate having events at all on RTOS targets.

## The event kinds

Nine event kinds: the five Tier-1 kinds below, plus Iron's matched and
incompatible-type pairs ([below](#matched-and-incompatible-type-iron)).
Three Tier-1 kinds on subscribers, two on publishers.

### Subscriber events

| Kind | Fires when | Use |
|------|-----------|-----|
| `LivelinessChanged` | A tracked publisher's liveliness state changes (publisher started asserting / stopped) | Safety-island fail-over: trigger MRM when remote control node goes silent |
| `RequestedDeadlineMissed` | An expected sample didn't arrive within the configured deadline | Periodic-sensor pattern: 100 Hz topic with 15 ms deadline; fire alarm when late |
| `MessageLost` | The backend dropped one or more samples (typically: ring-buffer overflow, slow consumer) | Diagnostic + adaptive: log, drop more aggressively, coalesce |

### Publisher events

| Kind | Fires when | Use |
|------|-----------|-----|
| `LivelinessLost` | This publisher missed its own liveliness assertion deadline | Self-monitoring: detect own task starvation |
| `OfferedDeadlineMissed` | This publisher promised X Hz, fell behind | Self-monitoring: detect overrunning compute |

### Matched and incompatible type (Iron+)

Issue 1972. Both pairs have one kind per side.

| Kind | Side | Fires when | Payload |
|------|------|-----------|---------|
| `PublicationMatched` | publisher | a subscription starts or stops matching it | `MatchedStatus` |
| `SubscriptionMatched` | subscription | a publisher starts or stops matching it | `MatchedStatus` |
| `PublisherIncompatibleType` | publisher | a subscription on the same topic name with another type appears | `IncompatibleTypeStatus` |
| `SubscriptionIncompatibleType` | subscription | a publisher on the same topic name with another type appears | `IncompatibleTypeStatus` |

`MatchedStatus` is upstream's `rmw_matched_status_t` — `total_count`,
`total_count_change`, `current_count`, `current_count_change` (signed: an
endpoint leaving is `-1`). The typical use is "start publishing once somebody
listens":

```rust
pub_.on_matched(|s| {
    if s.current_count > 0 { LISTENER_PRESENT.store(true, Ordering::Release); }
})?;
// Publisher-side events are observed on the publisher's poll points, so a
// publisher that is not publishing yet gives it one:
while !LISTENER_PRESENT.load(Ordering::Acquire) {
    let _ = pub_.assert_liveliness();
    executor.spin_once(Duration::from_millis(50));
}
```

C: `rclc_publisher_set_matched_callback`,
`rclc_publisher_set_incompatible_type_callback`,
`rclc_subscription_set_matched_callback` and
`rclc_subscription_set_incompatible_type_callback` (the subscription must
be added to an executor first), payloads `rclc_matched_status_t` /
`rclc_incompatible_type_status_t`. C++: `Publisher::on_matched` /
`on_incompatible_type` and the same pair on `PollSubscription`, taking
rclcpp's `MatchedInfo` / `IncompatibleTypeInfo` through a function pointer
plus `user_context`. Like the five Tier-1 C/C++ setters below, they keep
the registration in one process-wide 16-slot table shared by C and C++.

## Registering callbacks

Same shape as message callbacks. Register at construction time;
callback fires from inside `spin_once` when the backend detects the
event.

### Rust

```rust
use core::time::Duration;

let mut sub = node.create_polling_subscription::<SensorReading>("/sensor")?;

// Liveliness — fires when the publisher comes / goes
sub.on_liveliness_changed(|status| {
    if status.alive_count == 0 {
        log::warn!("sensor publisher went silent");
        trigger_failover();
    }
})?;

// Deadline — fires when no sample within 15 ms
sub.on_requested_deadline_missed(Duration::from_millis(15), |status| {
    metric_inc(&LATE_SAMPLE_COUNT, status.total_count_change);
})?;

// Message lost — fires when backend drops a sample
sub.on_message_lost(|status| {
    log::warn!("dropped {} samples", status.total_count_change);
})?;
```

Async equivalents, which complete only while the executor spins elsewhere:

```rust
let status = sub.next_liveliness_change().await?;
let status = sub.next_deadline_miss().await?;
```

### C

```c
static void on_liveliness_changed(
        rcl_subscription_t *sub,
        rclc_liveliness_changed_status_t status,
        void *user_context) {
    if (status.alive_count == 0) {
        trigger_failover();
    }
}

rclc_subscription_set_liveliness_changed_callback(
    sub, on_liveliness_changed, NULL);
```

Same shape on the four other event kinds.

Every setter returns `RCL_RET_OK` once the backend holds the
registration, and `RCL_RET_UNSUPPORTED` when the backend does not generate
that event (XRCE-DDS and uORB carry none; zenoh-pico has no
`LivelinessChanged` on bare metal). That answer comes from the backend, never
from the C layer. The other returns:

- `RCL_RET_NOT_INIT` — for the three subscription setters, the subscription
  has not been added to an executor yet (`rclc_executor_add_subscription*`).
  The executor owns the subscriber, so before that there is nothing to
  register on. Register after the add.
- `RCLC_RET_FULL` — the process-wide event-slot table is full. It holds 16
  registrations across every C and C++ entity; one publisher with all four of
  its kinds plus one subscription with all five take nine.

`deadline_ms` on the two deadline setters reaches the backend: zenoh uses it
when the entity's QoS states no deadline of its own, Cyclone DDS takes the
deadline from the QoS profile and ignores the argument. Registering the same
kind again replaces the earlier callback, and a callback never runs after
`rcl_publisher_fini` / `rcl_subscription_fini` (or
`rclc_executor_remove_subscription`), even if the backend fires late.

### C++

```cpp
static void on_liveliness_changed(void* storage,
                                  nros_cpp_liveliness_changed_status_t status,
                                  void* user_context) {
    if (status.alive_count == 0) trigger_failover();
}

// On `rclcpp::PollingSubscription` / `rclcpp::Publisher`.
sub.on_liveliness_changed(on_liveliness_changed, nullptr);
sub.on_requested_deadline_missed(15 /* ms */, on_deadline_missed, nullptr);
```

C function pointers + `user_context` only; there are no lambda or
`std::function` overloads. The subscription-side setters exist on
`PollingSubscription` alone. The first callback argument is the entity's
storage, and it follows the object through a C++ move. Same answers as C:
`ErrorCode::Unsupported` where the backend does not generate the event,
`ErrorCode::Full` when the shared 16-slot table is full.

## How dispatch works

Events fire from inside `drive_io` on the executor thread, the same
way message callbacks do:

```text
spin_once(timeout)
 └─ session.drive_io(...)
       ├─ backend RX worker detects message → fires message callback
       ├─ backend RX worker detects liveliness change → fires liveliness callback
       ├─ backend timer expires (deadline) → fires deadline callback
       └─ return
```

Same execution context, same priority, same constraints as message
callbacks. The `max_callbacks_per_spin` knob (see
[RTOS Cooperation](rtos-cooperation.md)) covers events too — an
event callback counts as one against the cap.

This means events get the same scheduling treatment as messages.
`max_callbacks_per_spin = 1` will fire either one message OR one
event per `spin_once`, whichever the backend has ready first.

## Why a callback model, not a waitset

Upstream `rmw.h` exposes events via a waitset — register an
`rmw_event_t` handle in a waitset, call `rmw_wait`, then `rmw_take_event`
to pull payload. nano-ros doesn't have a waitset (see
[RMW vs upstream](../design/rmw-vs-upstream.md) Section 4).

Reusing the existing message-callback path avoids introducing the
waitset. Trade-off: applications can't bulk-poll for events. For the
Tier-1 set this isn't load-bearing — events are rare, callbacks
are cheap.

## Backend support is uneven

Not every backend can generate every event. Apps must handle
`Unsupported` errors when registering:

| Backend | `LivelinessChanged` / `Lost` | `DeadlineMissed` | `MessageLost` |
|---------|------------------------------|------------------|---------------|
| Cyclone DDS | ✅ Polled `dds_get_liveliness_changed_status` / `_liveliness_lost_status` | ✅ Polled `dds_get_{requested,offered}_deadline_missed_status` (deadline comes from the QoS profile, not the `on_*` argument) | ✅ Polled `dds_get_sample_lost_status` |
| XRCE-DDS | ✗ Not exposed (xrce-dds-client API limitation) | ✅ Client-side clock check against the last takeable arrival / last publish, fired from `drive_io` (issue 2108). The Agent's DDS entity keeps an infinite deadline — `uxrQoS_t` has no field for one — so a peer matching on deadline still sees infinite | ✗ Not exposed |
| zenoh-pico | ✅ Poll-based via zenoh tokens (alive_count ∈ {0,1}) | ✅ Shim-side clock check (`<P as PlatformClock>::clock_ms`) | ✅ Seq-gap detection from RMW attachment |
| uORB | ✗ No wire-level liveliness | ✗ No rate concept | ✗ Not wired: the C++ backend (`nros-rmw-uorb`) fills no event slot. The publish-counter `MessageLost` of the retired Rust uORB backend (phase-108) did not carry over |

Matched / incompatible type: **Cyclone DDS** (its own matched status;
builtin-topic type comparison) and **zenoh-pico** (the graph cache's
liveliness tokens, as `rmw_zenoh_cpp` does) carry both pairs; **XRCE-DDS**
and **uORB** answer `Unsupported`. On zenoh the counts are snapshots of the
graph cache, so `total_count` counts increases seen between polls, and an
image built with graph discovery off (`ZPICO_GRAPH_DISCOVERY=0`) refuses the
registration.

✅ = wired. ✗ = not wired (registration answers `Unsupported`). The
slot-level view is generated from the backend sources in the
[per-RMW feature matrix](../reference/rmw-feature-matrix.md).

Cyclone DDS carries the whole Tier-1 set, as zenoh-pico does, but
it does it by POLLING rather than by calling you back: the backend
fills the ABI's `*_take_event` slots instead of `*_event_init`, and
the runtime drains them from the entity's ordinary data path
(`has_data` / `take_serialized` on a subscription; `publish` /
`assert_liveliness` on a publisher). That is invisible from the
application API — you still register with `on_*` and are still called
back — but it has one consequence worth knowing: a publisher that
stops publishing stops polling, so a publisher-side event is observed
on the next publish. Apps should still call
`Subscriber::supports_event(kind)` first or design for graceful
fallback.

The subscription side has no such limit (issue 2060). Its statuses are also
polled from `has_data`, and a subscription with an arrival waker is normally
asked only after something arrives. Registering any status callback on a
subscription changes that: the executor then asks it on every spin, arrival or
not. So once the last publisher has gone, the matched count falls, the
liveliness count drops, and `RequestedDeadlineMissed` fires, on the next spin
after the backend notices. On zenoh that is within one liveliness query and one
graph walk (at most every 50 ms). The interop cell
`native-qos-event-c-zenoh-bidir` measures both falls against stock ROS 2. A
subscription with no status callback is still asked only after an arrival, so
it costs an idle executor nothing. On Cyclone DDS, whose `has_data` always
answers "maybe", a subscription with a status callback is dispatched on every
spin; when nothing is waiting, the take finds nothing and the callback does not
run.

The `Subscriber::supports_event(kind)` query lets applications check
support before registering:

```rust
if sub.supports_event(EventKind::RequestedDeadlineMissed) {
    sub.on_requested_deadline_missed(...)?;
} else {
    // fallback: app-side timeout monitoring
}
```

Apps that need cross-backend portability code defensively. Apps
pinned to one backend can call register-and-unwrap.

## What's deliberately skipped

One upstream event type is intentionally absent from the API (`*_MATCHED`
and `*_INCOMPATIBLE_TYPE` were listed here until issue 1972 adopted them):

### `*_QOS_INCOMPATIBLE`

Fires when publisher and subscriber QoS profiles can't be
reconciled. nano-ros surfaces this synchronously at `create_publisher`
/ `create_subscription` time as `NROS_RMW_RET_INCOMPATIBLE_QOS`. No
runtime event needed; the failure is visible at startup.

## Patterns

### Drone-bridge fail-over on liveliness loss

```rust
let mut sub = node.create_polling_subscription::<VehicleAttitude>("/vehicle_attitude")?;
sub.on_liveliness_changed(|status| {
    if status.alive_count == 0 {
        // PX4 commander.cpp went silent — trigger MRM
        request_minimum_risk_manoeuvre();
    } else if status.alive_count_change > 0 {
        // commander came back online; clear MRM if appropriate
        clear_failover();
    }
})?;
```

This pairs with the cross-backend bridge example pattern.

### 100 Hz sensor with deadline alarm

```rust
let mut sub = node.create_polling_subscription::<SensorReading>("/imu")?;
sub.on_requested_deadline_missed(
    Duration::from_millis(15),    // expected 100 Hz, allow 15 ms deadline
    |status| {
        if status.total_count_change > 0 {
            log::error!("IMU late: {} missed deadlines", status.total_count_change);
            // Optional: enter degraded-mode controller
        }
    },
)?;
```

### Slow-consumer logging

```rust
let mut sub = node.create_polling_subscription::<Pointcloud>("/lidar")?;
sub.on_message_lost(|status| {
    log::warn!("dropped {} pointcloud frames (total: {})",
               status.total_count_change, status.total_count);
    // Tighten downstream filter, reduce work, coalesce, etc.
})?;
```

## See also

- [RMW API: Differences from upstream `rmw.h`](../design/rmw-vs-upstream.md)
  Section 8 — the design comparison this page expands on.
- [RTOS Cooperation](rtos-cooperation.md) — `max_callbacks_per_spin`
  treatment of event callbacks.
- [Differences from ROS 2](ros2-comparison.md) — broader surface
  comparison.
