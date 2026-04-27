# RMW API

The RMW (ROS middleware) trait surface lives in the
[`nros_rmw`](../api/rust/nros_rmw/index.html) crate.

## API reference

| Surface | Generator | Link |
|---------|-----------|------|
| Rust traits (`Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`) | rustdoc | [`nros_rmw`](../api/rust/nros_rmw/index.html) |
| Zenoh-pico backend | rustdoc | [`nros_rmw_zenoh`](../api/rust/nros_rmw_zenoh/index.html) |
| C vtable for porters (`nros_rmw_vtable_t`) | Doxygen | [rmw-cffi reference](../api/rmw-cffi/index.html) |
| C FFI shim crate (Rust side) | rustdoc | [`nros_rmw_cffi`](../api/rust/nros_rmw_cffi/index.html) |

`nros_rmw_xrce` (XRCE-DDS) shares the same trait surface but is built
under a mutually exclusive feature; not currently published as
rustdoc — see the source under `packages/xrce/nros-rmw-xrce/`.

## Trait surface (rustdoc deep links)

- [`Session`](../api/rust/nros_rmw/trait.Session.html) — session
  lifecycle + `drive_io` contract
- [`Publisher`](../api/rust/nros_rmw/trait.Publisher.html) /
  [`Subscriber`](../api/rust/nros_rmw/trait.Subscriber.html) —
  topic data path
- [`ServiceServerTrait`](../api/rust/nros_rmw/trait.ServiceServerTrait.html) /
  [`ServiceClientTrait`](../api/rust/nros_rmw/trait.ServiceClientTrait.html)
- [`TopicInfo`](../api/rust/nros_rmw/struct.TopicInfo.html) /
  [`ServiceInfo`](../api/rust/nros_rmw/struct.ServiceInfo.html) —
  backend-agnostic descriptors

Each trait method's `///` block documents thread safety, buffer
ownership, and blocking allowance.

## Writing a custom backend

- Conceptual guide: [Custom RMW Backend](../porting/custom-rmw.md) —
  full Rust + C walkthrough.
- Worked example: the zenoh shim under
  [`nros_rmw_zenoh::shim`](../api/rust/nros_rmw_zenoh/shim/index.html).
- C vtable porters: see the
  [rmw-cffi Doxygen reference](../api/rmw-cffi/index.html) for
  per-field return-value, threading, and blocking conventions.
