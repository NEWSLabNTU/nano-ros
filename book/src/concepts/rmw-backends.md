# RMW Backends: Zenoh vs XRCE-DDS

nano-ros supports two RMW (ROS Middleware) backends for connecting embedded devices to a ROS 2 network. Each backend targets different deployment scenarios and resource constraints. Only one backend can be active at compile time.

## Zenoh (rmw-zenoh)

The Zenoh backend uses [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico), a lightweight C client for the Zenoh protocol. The MCU participates directly in the Zenoh network -- there is no protocol translation layer.

**How it works:**

1. The MCU runs zenoh-pico in **client mode**, connecting to a `zenohd` router process over TCP, UDP, or TLS.
2. zenoh-pico creates publishers and subscribers directly on the Zenoh network.
3. ROS 2 nodes running `rmw_zenoh_cpp` connect to the same `zenohd` router, enabling transparent interop.
4. In **peer mode**, two zenoh-pico devices can communicate directly without any router.

**Key characteristics:**
- Peer-to-peer capable (no mandatory bridge process)
- `zenohd` is a generic router, not a protocol translator -- it forwards messages without interpreting them
- If `zenohd` crashes, peers in client mode lose routing but the MCU continues running
- Full ROS 2 graph discovery via liveliness tokens
- Transport options: TCP, UDP, TLS (via `zpico-smoltcp` or platform sockets)

## XRCE-DDS (rmw-xrce)

The XRCE-DDS backend uses [Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client), the same client library used by micro-ROS. It follows an **agent-based model** where a lightweight client on the MCU delegates entity creation to an agent process.

**How it works:**

1. The MCU runs the XRCE-DDS client, connecting to an **Agent** process over UDP or serial (UART).
2. The client sends requests to the Agent: "create a publisher on topic X with type Y."
3. The Agent creates full DDS entities on behalf of the client and bridges data between the XRCE protocol and the DDS data space.
4. ROS 2 nodes using any DDS-based RMW (FastDDS, Cyclone DDS) communicate through the Agent.

**Key characteristics:**
- Agent is **mandatory** -- the MCU cannot participate in the network without it
- If the Agent crashes, the MCU loses all connectivity
- Fully static memory allocation on the MCU (no heap required)
- Client-side discovery is not supported; the Agent handles it
- Transport options: UDP, serial (HDLC framing), CAN FD

## Comparison

| Aspect               | Zenoh (`rmw-zenoh`)            | XRCE-DDS (`rmw-xrce`)          |
|-----------------------|--------------------------------|---------------------------------|
| **Client RAM**        | ~16 KB+ (heap required)        | ~3 KB (fully static)            |
| **Client Flash**      | ~100 KB+                       | ~75 KB                          |
| **Bridge process**    | `zenohd` (generic router)      | Agent (protocol translator)     |
| **Peer-to-peer**      | Yes (no router needed)         | No (agent always required)      |
| **Discovery**         | Client participates            | Agent handles on behalf         |
| **Entity creation**   | Client creates directly        | Client requests, agent creates  |
| **Transport options** | TCP, UDP, TLS                  | UDP, serial, CAN FD             |
| **Heap allocation**   | Required (C-level)             | None                            |
| **Platform symbols**  | ~55 (clock, malloc, sockets, RNG, ...) via `zpico-platform-shim` | ~1 (`clock_gettime`) via `xrce-platform-shim` |
| **ROS 2 interop**     | Via `rmw_zenoh_cpp` + `zenohd` | Via Agent + any DDS RMW         |
| **Failure mode**      | Router crash = lose routing    | Agent crash = lose connectivity |
| **C source files**    | ~100+                          | 28                              |

## Feature Selection

### Cargo.toml (Rust)

Select exactly one RMW backend in your dependency features:

```toml
# Zenoh backend
[dependencies]
nros = { features = ["rmw-zenoh", "platform-bare-metal"] }

# XRCE-DDS backend
[dependencies]
nros = { features = ["rmw-xrce", "platform-bare-metal"] }
```

### Kconfig (Zephyr)

```kconfig
# Zenoh backend
CONFIG_NROS_RMW_ZENOH=y

# XRCE-DDS backend
CONFIG_NROS_RMW_XRCE=y
```

Enabling both simultaneously produces a `compile_error!()`.

## When to Use Which

**Choose Zenoh when:**
- You need ROS 2 interop with `rmw_zenoh_cpp` (the recommended ROS 2 middleware for Jazzy+)
- You want peer-to-peer communication without any bridge process
- Your MCU has at least ~16 KB of heap and ~100 KB of flash
- You are using TCP or UDP networking
- You want simpler deployment (zenohd is a single binary with no configuration required)

**Choose XRCE-DDS when:**
- Your MCU has very limited RAM (under 8 KB available for middleware)
- You need serial (UART) transport -- useful for MCUs without networking hardware
- You are integrating with an existing micro-ROS or DDS deployment
- You want zero heap allocation on the MCU side
- You need CAN FD transport

**Either works well for:**
- Standard pub/sub and service patterns
- Integration with ROS 2 desktop nodes
- QEMU-based development and testing
