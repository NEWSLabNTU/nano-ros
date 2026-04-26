# RMW API

The RMW (ROS middleware) trait surface lives in the [`nros_rmw`](../api/rust/nros_rmw/index.html)
crate. Backend implementations:

- [`nros_rmw_zenoh`](../api/rust/nros_rmw_zenoh/index.html) — zenoh-pico
- `nros_rmw_xrce` (XRCE-DDS) — see source

## Trait surface

- [`Rmw`](../api/rust/nros_rmw/trait.Rmw.html) — backend lifecycle
- [`PublisherTrait`](../api/rust/nros_rmw/trait.PublisherTrait.html) /
  [`SubscriberTrait`](../api/rust/nros_rmw/trait.SubscriberTrait.html)
- [`ServiceServerTrait`](../api/rust/nros_rmw/trait.ServiceServerTrait.html) /
  [`ServiceClientTrait`](../api/rust/nros_rmw/trait.ServiceClientTrait.html)
- [`RmwSession`](../api/rust/nros_rmw/trait.RmwSession.html) — `drive_io`
- [`TopicInfo`](../api/rust/nros_rmw/struct.TopicInfo.html) /
  [`ServiceInfo`](../api/rust/nros_rmw/struct.ServiceInfo.html) — backend-agnostic descriptors

## Writing a custom backend

Conceptual guide: [Custom RMW Backend](../porting/custom-rmw.md).
Trait reference: link above. The zenoh shim under
[`nros_rmw_zenoh::shim`](../api/rust/nros_rmw_zenoh/shim/index.html) is the
canonical worked example.
