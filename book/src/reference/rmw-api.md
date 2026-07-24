# RMW API

The RMW (ROS middleware) vtable is the porting boundary between
nano-ros and a concrete pub/sub transport (zenoh-pico, XRCE-DDS,
Cyclone DDS, uORB, …). RMW is **internal** — user
applications use the [Rust](rust-api.md) / [C](c-api.md) /
[C++](cpp-api.md) APIs, not the vtable directly.

## Canonical reference

The C vtable `nros_rmw_vtable_t` is the source of truth. Every
function pointer's brief, parameter docs, ownership rules
(buffer-borrowed vs caller-owned), blocking / non-blocking
classification, return-code conventions, and lifetime contract for
loaned slots live in the Doxygen output.

| Surface | Link |
|---|---|
| **rmw-cffi Doxygen** (canonical) | [HTML](../api/rmw-cffi/index.html) · [header](https://github.com/NEWSLabNTU/nano-ros/blob/main/packages/core/nros-rmw-abi/include/nros/rmw_vtable.h) |

To regenerate locally:

```bash
just doc-rmw-cffi   # produces target/doxygen/rmw-cffi/
```

This page does **not** duplicate the interface specification — read
the Doxygen for that.

## Reference implementations

Concrete backends. Each crate's `README.md` walks the
implementation; the source is the worked example to copy.

| Backend | Source | Notes |
|---|---|---|
| zenoh-pico | [`packages/zpico/nros-rmw-zenoh`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/zpico/nros-rmw-zenoh) | Default. C transport via zenoh-pico. Native zero-copy publish via `z_bytes_from_static_buf`. |
| micro-XRCE-DDS-Client | [`packages/xrce/nros-rmw-xrce`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/xrce/nros-rmw-xrce) | C-only shim; agent-based. |
| Cyclone DDS | [`packages/dds/nros-rmw-cyclonedds`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/dds/nros-rmw-cyclonedds) | C++ shim; standalone CMake project. |
| PX4 uORB | [`packages/px4/nros-rmw-uorb`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/px4/nros-rmw-uorb) | Typed-trampoline registry over PX4 uORB. |

The zenoh-pico shim is the canonical reference port.

## Writing a custom backend

- Conceptual walkthrough: [Custom RMW Backend](../porting/custom-rmw.md).
- Coming from upstream `rmw.h`?
  → [RMW API: Differences from upstream `rmw.h`](../design/rmw-vs-upstream.md).
- Coverage status vs upstream `rmw.h`:
  [`docs/research/rmw-c-abi-coverage.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/research/rmw-c-abi-coverage.md).
