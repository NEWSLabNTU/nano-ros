# nros-rmw-xrce

XRCE-DDS RMW backend for nano-ros. Talks to a `MicroXRCEAgent` (or any
DDS-XRCE compliant agent), which proxies traffic into a full DDS
domain.

## Role

Implements the [`nros-rmw`](../../core/nros-rmw) trait surface
(`Rmw`, `Session`, `Publisher`, `Subscriber`, `ServiceServerTrait`,
`ServiceClientTrait`, plus the lending traits) over the eProsima
Micro-XRCE-DDS C client (vendored under `packages/xrce/xrce-sys/`).
Mutually exclusive with `nros-rmw-zenoh` at compile time.

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | Public re-exports + `XrceRmw` factory. |
| `src/naming.rs` | ROS topic ↔ XRCE entity-key mapping. |
| `src/config.rs` | Agent locator + session-key config. |
| `src/platform_udp.rs` | UDP transport bound to `nros-platform-api::PlatformUdp`. |
| `src/platform_serial.rs` | Serial transport (UART / virtio-console). |

## When to use

- DDS topology required (interop with FastDDS, CycloneDDS, RTI Connext)
  but agent indirection is acceptable.
- The board can run the small XRCE client (~16 KB heap) but cannot host
  a full DDS RTPS stack.
- Existing PX4 / ROS 2 infrastructure already uses an XRCE Agent.

For native DDS without the agent, see [`nros-rmw-dds`](../../dds/nros-rmw-dds).

## Lending (zero-copy)

`PublishLoan` is implemented via `uxr_prepare_output_stream` so the
publisher writes directly into the reliable output stream's buffer.

## See also

- [Custom RMW Backend porting guide](../../../book/src/porting/custom-rmw.md)
- [XRCE-DDS analysis](../../../docs/reference/xrce-dds-analysis.md)
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/xrce/nros-rmw-xrce>
