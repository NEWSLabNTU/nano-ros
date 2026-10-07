Every deprecated item is deleted, in one batch. Each was a forwarder to a
name that has existed for at least one release; code that still uses an old
name now fails to compile instead of warning. Replace as follows.

**C++** (`nros-cpp`):

| deleted | write instead |
| --- | --- |
| `nros::Node` | `rclcpp::Node` (the same type) |
| `nros::Expected<T>` | `nros::ResultOf<T>` |
| `nros::bind_timer<C, &C::m>(node, out, ms, self)` | `node.create_wall_timer<C, &C::m>(out, ms, self)` |
| `Executor::spin(duration_ms, poll_ms)` | `Executor::spin_for(duration_ms, poll_ms)` |
| `LifecycleNode::trigger(id)` | `LifecycleNode::trigger_transition(id)` |
| `PollClient::server_available()` | `PollClient::service_is_ready()` (returns `ResultOf<bool>`) |
| `PollService::try_recv_request(...)` | `PollService::take_request(...)` |
| `PollService::send_reply(...)` | `PollService::send_response(...)` |
| `PollSubscription::try_recv(...)` | `PollSubscription::take(...)` |
| `PollSubscription::try_recv_sized<N>(...)` | `take_sized<N>(...)` |
| `PollSubscription::try_recv_validated(...)` | `take_validated(...)` |
| `PollSubscription::try_recv_validated_sized<N>(...)` | `take_validated_sized<N>(...)` |
| `PollSubscription::try_recv_raw(...)` | `take_serialized(...)` |
| `PollSubscription::try_recv_raw_with_attachment(...)` | `take_serialized_with_attachment(...)` |
| `PollSubscription::try_recv_sequence(...)` | `take_sequence(...)` |
| `QoS::Liveliness` | `nros::LivelinessPolicy` |
| `QoS::LivelinessNone` / `Automatic` / `ManualByTopic` / `ManualByNode` | `nros::LivelinessNone` / … (namespace scope) |
| `QoS::reliability_raw()` / `durability_raw()` / `history_raw()` / `liveliness_raw()` | `QoS::reliability()` / `durability()` / `history()` / `liveliness()` (return the policy enums) |
| `QoS::deadline_ms()` / `lifespan_ms()` / `liveliness_lease_ms()` | `QoS::deadline()` / `lifespan()` / `liveliness_lease_duration()` (return `nros::Duration`) |
| `QoS::deadline_ms(ms)` / `lifespan_ms(ms)` / `liveliness_lease_ms(ms)` | the same setters taking `nros::Duration` |

**Rust**:

| deleted | write instead |
| --- | --- |
| `nros::LogCrateLogger`, `nros::LogCrateOnceFlag` | `nros::Logger` with the `nros_*!` macros; `nros::ThrottleState` |
| `NodeContext::create_node_with_options(opts)` | `create_node(opts)` |
| `NodeCtx::create_subscription_borrowed` | `create_subscription_viewable` |
| `Executor::halt()` | `Executor::cancel()` |
| `ActionServerHandle::cancel(...)` | `ActionServerHandle::canceled(...)` |
| `SchedClass::TimeTriggered` | a `SchedContext` with `tt_window_offset_us` / `tt_window_duration_us` |
| `BoardConfig::zenoh_locator()`, `ThreadxConfig::zenoh_locator()` | `locator()` |
| `Config::with_zenoh_locator(...)` on the five board crates | `with_locator(...)` |

**C ABI**: `nros_rmw_cffi_register(vtable)` is deleted; a custom RMW backend
calls `nros_rmw_cffi_register_named("<name>", vtable)`, which every in-tree
backend already did. The `NROS_DEPRECATED` / `NROS_DEPRECATED_MSG` macros in
`<nros/visibility.h>` are gone too; nothing used them.
