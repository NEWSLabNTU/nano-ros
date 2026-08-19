# Choosing an RMW

Every nano-ros binary picks its RMW (ROS Middleware) backend at **build
time**. There are three: **Cyclone DDS**, **Zenoh** (zenoh-pico), and
**XRCE-DDS**. This page helps you pick one; the mechanics of changing
your pick live in [Switching RMW Backends](./rmw-switching.md), and the
per-backend deep dive (wire behavior, per-platform network configuration)
is in [RMW Backends](./rmw-backends.md).

## Start with Cyclone DDS

For getting started, use **Cyclone DDS**. It is the one backend that
needs no daemon, router, or agent: a nano-ros node publishes with
nothing else running, and a stock ROS 2 node using `rmw_cyclonedds_cpp`
sees it directly — same RTPS wire protocol, same discovery, no
translation layer in between.

The reason the other two are not the starting point: nano-ros ships no
bridge process of its own. The zenoh router **is** ROS 2's own
`rmw_zenohd` (see
[RFC-0075](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)),
so the zenoh backend requires a ROS 2 installation just to start the
router; the XRCE backend requires a running Micro-XRCE-DDS Agent.
Cyclone DDS requires neither.

Current maturity: pub/sub and services interoperate with stock
`rmw_cyclonedds_cpp`; status events (liveliness, deadline-miss) are not
wired to Cyclone listeners yet. The full list is in
[`docs/reference/cyclonedds-known-limitations.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/cyclonedds-known-limitations.md).

## Zenoh — ROS 2 interop and small embedded targets

Choose the **Zenoh** backend (built on zenoh-pico) when:

- You are integrating with a ROS 2 fleet running `rmw_zenoh_cpp`.
- Your target is a small MCU: ~16 KB of heap and ~100 KB of flash are
  enough, versus ~32 KB+ RAM for Cyclone DDS.
- Your network cannot do UDP multicast (RTPS discovery needs IGMP;
  zenoh-pico in client mode runs over plain TCP).

The zenoh backend needs a router, and the router comes from your ROS 2
installation — it is the same `rmw_zenohd` that ROS 2 zenoh nodes use:

```bash
ros2 run rmw_zenoh_cpp rmw_zenohd
```

That invocation works on any host with `rmw_zenoh_cpp` installed,
regardless of where ROS 2 lives — never hard-code an install path to
the router binary. See [ROS 2 Interop](../getting-started/ros2-interop.md)
for the full three-terminal walkthrough.

## XRCE-DDS — smallest footprint, serial transport

Choose the **XRCE-DDS** backend when:

- RAM is the constraint: ~3 KB of client RAM, fully static allocation,
  no heap required on the MCU.
- You need a **serial (UART)** transport — the only backend that works
  on an MCU with no networking hardware. See
  [Serial Transport](./serial-transport.md).
- You are joining an existing micro-ROS / DDS deployment: the agent
  bridges to any DDS-based RMW.

The MCU cannot participate without the **Micro-XRCE-DDS Agent**
(`MicroXRCEAgent`) running on a host; if the agent stops, the device
loses all connectivity.

## Capability matrix

| Aspect               | Cyclone DDS (`cyclonedds`)     | Zenoh (`zenoh`)                | XRCE-DDS (`xrce`)               |
|----------------------|--------------------------------|--------------------------------|---------------------------------|
| **Client RAM**       | ~32 KB+ (heap required)        | ~16 KB+ (heap required)        | ~3 KB (fully static)            |
| **Client Flash**     | ~150 KB+                       | ~100 KB+                       | ~75 KB                          |
| **Bridge process**   | None — RTPS multicast directly | `rmw_zenohd` router (from your ROS 2 install) | Agent (protocol translator, mandatory) |
| **Peer-to-peer**     | Yes (RTPS native)              | Not as shipped — zenoh-pico's multicast/scouting is compiled out | No (agent always required)      |
| **Discovery**        | SPDP / SEDP on UDP multicast or static peer list | Client participates (liveliness tokens) | Agent handles on behalf         |
| **Entity creation**  | Client creates directly        | Client creates directly        | Client requests, agent creates  |
| **Transports**       | UDP unicast + multicast (RTPS) | TCP, UDP, TLS                  | UDP, serial (HDLC framing)      |
| **Heap allocation**  | Required (Cyclone uses `malloc`) | Required (C-level)           | None                            |
| **Implementation**   | C++ wrapper over upstream Cyclone DDS C | Rust + zenoh-pico C   | Rust + Micro-XRCE-DDS-Client C  |
| **ROS 2 interop**    | Direct against `rmw_cyclonedds_cpp` (same upstream version) | Via `rmw_zenoh_cpp` + its router | Via agent + any DDS RMW    |
| **Failure mode**     | Peer goes offline = its samples stop arriving | Router crash = lose routing | Agent crash = lose connectivity |

Notes on two rows: zenoh's router-free peer mode exists upstream but
nano-ros compiles zenoh-pico without the multicast transport and
scouting, so every deployment reaches its peers through the router;
and the upstream Micro-XRCE-DDS client defines a CAN FD transport,
but nano-ros builds do not enable it.

## Decision summary

- **Default / getting started / DDS fleet** → Cyclone DDS. Nothing to
  run beside your node; direct `rmw_cyclonedds_cpp` interop. Needs
  ~32 KB RAM and a network stack with IGMP.
- **`rmw_zenoh_cpp` fleet, small MCU, or TCP-only network** → Zenoh.
  Needs the router from your ROS 2 install
  (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
- **Tiny MCU, serial link, or micro-ROS deployment** → XRCE-DDS.
  Smallest footprint, zero heap, but the agent is mandatory.

You do not have to pick exactly one per system: a single binary can
link **multiple backends** and bridge between them — see
[Cross-backend Bridges](./cross-backend-bridges.md).

Once you have picked, [Switching RMW Backends](./rmw-switching.md)
shows the exact edits per build system.
