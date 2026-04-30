# RMW API

The RMW (ROS middleware) trait surface is the porting boundary between
nano-ros's high-level API and a concrete pub/sub transport (zenoh-pico,
XRCE-DDS, dust-DDS, uORB, …). RMW is **internal** — user applications use the
[Rust](rust-api.md) / [C](c-api.md) / [C++](cpp-api.md) APIs, not the RMW traits
directly.

## API reference

| Surface | Generator | Link |
|---------|-----------|------|
| Rust traits (`Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`, `ServiceClientTrait`, lending traits) | rustdoc | [**`nros_rmw`**](../api/rust/nros_rmw/index.html) |
| C vtable for porters (`nros_rmw_vtable_t`) | Doxygen | [**rmw-cffi**](../api/rmw-cffi/index.html) |
| C FFI shim crate (Rust side) | rustdoc | [`nros_rmw_cffi`](../api/rust/nros_rmw_cffi/index.html) |

Each trait method's `///` block in the rustdoc documents thread safety, buffer
ownership, blocking allowance, and (for lending traits) the slot lifecycle.

## Example backend implementations

Concrete implementations of the RMW trait surface — read these for a
worked example before writing your own:

| Backend | Crate | Notes |
|---------|-------|-------|
| zenoh-pico (default) | [`nros_rmw_zenoh`](../api/rust/nros_rmw_zenoh/index.html) | Reference shim. C transport via zenoh-pico; lending via `z_bytes_from_static_buf`. |
| XRCE-DDS | `nros_rmw_xrce` (source: `packages/xrce/nros-rmw-xrce/`) | Mutually exclusive with zenoh; lending via `uxr_prepare_output_stream`. |
| dust-DDS (Rust DDS) | `nros_rmw_dds` (source: `packages/dds/nros-rmw-dds/`) | std + nostd-runtime variants. |
| PX4 uORB | `nros_rmw_uorb` (source: `packages/px4/nros-rmw-uorb/`) | Phase 90 — typed-trampoline registry. |

The zenoh shim under
[`nros_rmw_zenoh::shim`](../api/rust/nros_rmw_zenoh/shim/index.html) is the
canonical reference port: every other backend follows the same trait
implementation pattern.

## Writing a custom backend

- Conceptual guide: [Custom RMW Backend](../porting/custom-rmw.md) — full
  Rust + C walkthrough, covers the lending traits and arena lifecycle.
- C vtable porters: see the [rmw-cffi Doxygen reference](../api/rmw-cffi/index.html)
  for per-field return-value, threading, and blocking conventions.
