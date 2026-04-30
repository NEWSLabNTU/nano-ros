# nros-rmw-dds

Native DDS RMW backend for nano-ros, built on the
[`dust_dds`](../dust-dds) Rust DDS implementation. **Agent-free** — the
node speaks RTPS directly with peer DDS participants (CycloneDDS,
FastDDS, dust-dds, etc.).

## Role

Implements the [`nros-rmw`](../../core/nros-rmw) trait surface
(`Rmw`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`,
`ServiceClientTrait`) over `dust_dds`. Two operating modes selected at
compile time via the `std` / `nostd-runtime` feature pair.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | Feature gating + public re-exports. |
| `src/session.rs` | Domain participant lifecycle. |
| `src/publisher.rs` / `src/subscriber.rs` | Topic data path. |
| `src/service.rs` | DDS request/reply mapping. |
| `src/runtime.rs` | `nostd-runtime` cooperative single-task runtime adapter for `DdsRuntime`. |
| `src/transport.rs` / `src/transport_nros.rs` | Transport wiring (POSIX sockets via `dust_dds` stock UDP transport, or the nros-platform UDP path). |
| `src/raw_type.rs` | Type-erased serializer/deserializer plumbing. |
| `src/waker_cell.rs` | Async waker bridge for `DdsServiceClient` / `DdsSubscriber`. |

## Modes

| Mode | Cargo features | Use case |
|------|---------------|----------|
| `std` | `std` (forwards `dust_dds/std` + `dust_dds/rtps_udp_transport`) | POSIX. Uses dust-dds's stock threaded UDP transport. |
| `nostd-runtime` | `nostd-runtime` | Cooperative single-task runtime. Transport via `nros-platform-api::PlatformUdp`. Used by every embedded RTOS path (FreeRTOS / NuttX / ThreadX / Zephyr / bare-metal). |

Both modes can be active simultaneously (Phase 95 dual-feature
refactor). Mutually exclusive with `nros-rmw-zenoh` and
`nros-rmw-xrce`.

## When to use

- ROS 2 interop with native DDS RMW implementations (no agent).
- Industrial / safety contexts where DDS is mandated.
- Multicast SPDP discovery is acceptable on the network.

## See also

- [Custom RMW Backend porting guide](../../../book/src/porting/custom-rmw.md)
- [dust-dds Rust DDS](../dust-dds) (vendored fork)
- [Phase 71 — DDS infrastructure block](../../../docs/roadmap/archived/phase-71-dust-dds-platform-agnostic.md)
- [Phase 97 — DDS per-platform examples + cross-platform E2E](../../../docs/roadmap/phase-97-dds-per-platform-examples.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/dds/nros-rmw-dds>
