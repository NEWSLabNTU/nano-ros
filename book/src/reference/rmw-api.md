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

Concrete implementations of the RMW trait surface. Read the source — and
each crate's `README.md` — for a worked example before writing your own.
The rustdoc of a backend crate replicates the trait surface and adds
little extra; the source tree is what's worth reading.

| Backend | Source | Notes |
|---------|--------|-------|
| zenoh-pico (default) | [`packages/zpico/nros-rmw-zenoh`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/zpico/nros-rmw-zenoh) | Reference shim. C transport via zenoh-pico; lending via `z_bytes_from_static_buf`. |
| XRCE-DDS | [`packages/xrce/nros-rmw-xrce`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/xrce/nros-rmw-xrce) | Mutually exclusive with zenoh; lending via `uxr_prepare_output_stream`. |
| dust-DDS (Rust DDS) | [`packages/dds/nros-rmw-dds`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/dds/nros-rmw-dds) | `std` + `nostd-runtime` variants (POSIX threading + cooperative single-task). |
| PX4 uORB | [`packages/px4/nros-rmw-uorb`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/px4/nros-rmw-uorb) | Phase 90 — typed-trampoline registry, ROS-name → uORB-topic map. |

The zenoh shim is the canonical reference port — every other backend
follows the same trait-implementation pattern.

## Writing a custom backend

- Conceptual guide: [Custom RMW Backend](../porting/custom-rmw.md) — full
  Rust + C walkthrough, covers the lending traits and arena lifecycle.
- C vtable porters: see the [rmw-cffi Doxygen reference](../api/rmw-cffi/index.html)
  for per-field return-value, threading, and blocking conventions.
