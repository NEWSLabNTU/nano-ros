# RMW Backends: Zenoh, XRCE-DDS, DDS, Cyclone DDS

nano-ros supports four RMW (ROS Middleware) backends for connecting
embedded devices to a ROS 2 network. Each backend targets different
deployment scenarios and resource constraints. Each Node picks its
backend at build time; **one binary can link multiple backends and
bridge between them** — see
[Cross-backend Bridges](./cross-backend-bridges.md) for the
multi-RMW pattern.

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

## DDS (rmw-dds)

The DDS backend uses [dust-dds](https://github.com/s2e-systems/dust-dds), a pure-Rust DDS / RTPS implementation with `no_std + alloc` support and OMG-certified RTPS interoperability.

**How it works:**

1. The MCU runs the dust-dds participant directly — no agent, no router.
2. SPDP / SEDP discovery flows over UDP multicast (`239.255.0.1:7400`+); peers find each other on the same domain via standard RTPS.
3. dust-dds creates DDS readers / writers per topic and exchanges samples via UDP unicast or multicast.
4. ROS 2 nodes using any DDS-based RMW (Cyclone DDS, FastDDS, dust-dds itself) interoperate directly — same wire protocol, same discovery.

**Key characteristics:**
- Brokerless peer-to-peer — no `zenohd`, no Agent process needed.
- Pure-Rust transport: dust-dds replaces zenoh-pico's C transport layer with a `NrosPlatformRuntime` adapter on every platform except `platform-posix`, which keeps using dust-dds's stock OS-thread transport.
- Heap required (`alloc`-only on `no_std` platforms). Boot-time allocation only on RTOS targets where the global allocator is opt-in (`feature = "platform-zephyr"` enables `k_malloc`-backed allocator in `nros-c` / `nros-cpp`).
- Transport options: UDP unicast + multicast.
- ROS 2 interop is end-to-end (no protocol translator) — DDS-on-MCU appears in the ROS 2 graph the same way a desktop ROS 2 node does.

## Cyclone DDS (rmw-cyclonedds)

> **Maturity status.** Cyclone DDS support is **pub/sub-only today.**
> Service create / recv / reply returns `NROS_RMW_RET_UNSUPPORTED`;
> status events (liveliness, deadline-miss, etc.) are not wired to
> Cyclone listeners yet. Wire-level interop with stock
> `rmw_cyclonedds_cpp` (Humble) works for topic publishing and
> subscribing. If your fleet needs RPC or lifecycle events over
> Cyclone, use Zenoh or dust-DDS instead until the gaps close. Full
> known-limitations list:
> [`docs/reference/cyclonedds-known-limitations.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/cyclonedds-known-limitations.md).

The Cyclone DDS backend uses [Eclipse Cyclone DDS](https://github.com/eclipse-cyclonedds/cyclonedds), the same DDS implementation that ROS 2 ships with via `rmw_cyclonedds_cpp`. Built as a **standalone C++ library** at `packages/dds/nros-rmw-cyclonedds/` that registers itself with the runtime through the C ABI vtable in `nros-rmw-cffi`.

**How it works:**

1. The application (or platform) calls `nros_rmw_cyclonedds_register()` once before `nros::init()` — this is automatic when the consumer's CMake build sets `-DNANO_ROS_RMW=cyclonedds`.
2. The runtime stores the vtable pointer; subsequent `nros::init()` calls dispatch through it for session, publisher, subscriber, and service operations.
3. `dds_create_domain` / `dds_create_participant` / `dds_create_topic` / `dds_create_writer` / `dds_create_reader` are invoked under the hood; Cyclone owns its own RX threads.
4. ROS 2 nodes using stock `rmw_cyclonedds_cpp` interoperate directly — same wire protocol, same discovery, no key rewriting (unlike `rmw_zenoh`'s `<domain>/<topic>/<type>/...` scheme).

**Key characteristics:**
- **Pure-C++ backend** (not a Cargo crate) — Autoware contributors can read and extend the wrapper using the same patterns as `actuation_module/include/common/dds/`.
- **ROS 2 Tier-1 wire compat** — pinned to Cyclone DDS tag `0.10.5` to match `ros-humble-cyclonedds` 0.10.5 + `ros-humble-rmw-cyclonedds-cpp` 1.3.4.
- Static `ddsi_config` via `dds_create_domain_with_rawconfig` skips the XML parser; embedded-friendly.
- Discovery via SPDP multicast or unicast peer list (mirrors Cyclone's standard config knobs).
- Heap required (Cyclone uses `malloc`); `BUILD_SHARED_LIBS=ON` produces `libddsc.so` for POSIX, static link for embedded.
- **No services / actions yet** — service create/recv/reply currently returns `NROS_RMW_RET_UNSUPPORTED`.
- **No status events yet** — `register_subscriber_event` / `register_publisher_event` / `assert_publisher_liveliness` slots are not wired to Cyclone listeners yet.

**Build:**
```bash
just cyclonedds setup       # build Cyclone DDS from third-party/dds/cyclonedds (tag 0.10.5)
just cyclonedds build-rmw   # build packages/dds/nros-rmw-cyclonedds
just cyclonedds test        # run the CTest harness
```

Each example picks its RMW via `-DNANO_ROS_RMW=cyclonedds` at
configure time; the root `CMakeLists.txt` add_subdirectory's
`packages/dds/nros-rmw-cyclonedds/` and links the resulting target
into `NanoRos::NanoRos`. No `build/install/` prefix, no
`find_package(NrosRmwCyclonedds)` deleted both.

**Known limitations.** Cyclone DDS currently has a 2× CDR roundtrip per message, deferred status-event wiring, pending service request-id correlation, and incomplete Cortex-A/R Zephyr board support. See `docs/reference/cyclonedds-known-limitations.md` for the full list. ARMv8-R toolchain prep (Cortex-A 64-bit FVP, Cortex-R52 hardware) is in `docs/reference/zephyr-armv8r-setup.md`.

## Comparison

| Aspect               | Zenoh (`rmw-zenoh`)            | XRCE-DDS (`rmw-xrce`)          | DDS (`rmw-dds`)                 | Cyclone DDS (`rmw-cyclonedds`) |
|-----------------------|--------------------------------|---------------------------------|---------------------------------|---------------------------------|
| **Client RAM**        | ~16 KB+ (heap required)        | ~3 KB (fully static)            | ~32 KB+ (heap required)         | ~32 KB+ (heap required)         |
| **Client Flash**      | ~100 KB+                       | ~75 KB                          | ~120 KB+                        | ~150 KB+ (`libddsc.so` ~1.4 MB on POSIX, sized down on embedded link) |
| **Bridge process**    | `zenohd` (generic router)      | Agent (protocol translator)     | None — RTPS multicast directly  | None — RTPS multicast directly  |
| **Peer-to-peer**      | Yes (no router needed)         | No (agent always required)      | Yes (RTPS native)               | Yes (RTPS native)               |
| **Discovery**         | Client participates            | Agent handles on behalf         | SPDP / SEDP on UDP multicast    | SPDP / SEDP on UDP multicast or static peer list |
| **Entity creation**   | Client creates directly        | Client requests, agent creates  | Client creates directly         | Client creates directly         |
| **Transport options** | TCP, UDP, TLS                  | UDP, serial, CAN FD             | UDP unicast + multicast (RTPS)  | UDP unicast + multicast (RTPS)  |
| **Heap allocation**   | Required (C-level)             | None                            | Required (Rust `alloc` crate)   | Required (Cyclone uses `malloc`) |
| **Implementation**    | Rust + zenoh-pico C            | Rust + Micro-XRCE-DDS-Client C  | Pure Rust (dust-dds)            | C++ wrapper over upstream Cyclone DDS C |
| **ROS 2 interop**     | Via `rmw_zenoh_cpp` + `zenohd` | Via Agent + any DDS RMW         | Direct (DDS↔DDS, any RMW)       | Direct against `rmw_cyclonedds_cpp` (same upstream version) |
| **Failure mode**      | Router crash = lose routing    | Agent crash = lose connectivity | Peer goes offline = its samples stop arriving | Peer goes offline = its samples stop arriving |
| **C source files**    | ~100+                          | 28                              | 0 — pure Rust                   | Upstream Cyclone (~600+ files, vendored unchanged via submodule) |

## Multi-backend binaries (bridges)

A single nano-ros binary can link **more than one RMW backend** and
forward traffic between them. The bridge pattern is useful for:

- **Translating between protocols** — a gateway node running zenoh
  ingress + DDS egress lets MCU fleets on zenoh-pico talk to an
  Autoware stack on Cyclone DDS without a separate translator.
- **Hard-real-time + best-effort split** — a high-priority Node on
  dust-DDS for control loops, a low-priority Node on Zenoh for
  telemetry, both in one process.
- **Bringing up an XRCE Agent**-free fleet — bridge XRCE devices
  to a Zenoh network so they look like first-class participants
  to stock ROS 2.

The pattern uses `Executor::open_with_rmw("<name>", ...)` to pin the
primary session and `node_builder("name").rmw("<other>").build()` to
open additional sessions on other backends. Both backends must be
in the binary's link line (Cargo manifest deps + `register()` call
each).

A worked example lives at
[`examples/bridges/native-rust-zenoh-to-dds/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/bridges/native-rust-zenoh-to-dds);
the time-triggered variant under
[`examples/native/rust/bridge/tt-zenoh-to-xrce/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/native/rust/bridge/tt-zenoh-to-xrce)
shows the same pattern under an ARINC-653-style cyclic schedule.

Full walkthrough: [Cross-backend Bridges](./cross-backend-bridges.md).

## Feature Selection

### Cargo.toml (Rust)/129 reshape — RMW selection is driven by **manifest deps**,
not by features on `nros`. Add `nros` with the platform feature plus
exactly one `nros-rmw-<x>` shim dep:

```toml
# Zenoh backend
[dependencies]
nros = { path = "<...>/packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix"] }
nros-rmw-zenoh = { path = "<...>/packages/zpico/nros-rmw-zenoh",
                   features = ["std", "platform-posix", "ros-humble"] }

# XRCE-DDS backend
[dependencies]
nros = { path = "<...>/packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix"] }
nros-rmw-xrce-cffi = { path = "<...>/packages/xrce/nros-rmw-xrce-cffi",
                       features = ["std"] }

# DDS backend (dust-dds, pure Rust)
[dependencies]
nros = { path = "<...>/packages/core/nros",
         default-features = false,
         features = ["std", "alloc", "rmw-cffi", "platform-posix"] }
nros-rmw-dds = { path = "<...>/packages/dds/nros-rmw-dds",
                 default-features = false,
                 features = ["std", "alloc", "platform-posix"] }

# Cyclone DDS backend — Rust runtime sees the generic rmw-cffi C-ABI
# vtable; actual Cyclone wiring lives C++-side under
# packages/dds/nros-rmw-cyclonedds/ and is selected at CMake
# configure time via -DNANO_ROS_RMW=cyclonedds. The Rust
# manifest only carries the `rmw-cffi` feature; no Rust shim dep.
[dependencies]
nros = { path = "<...>/packages/core/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix"] }
```

Each example also calls `<backend>::register()` from `main()` before
`Executor::open` — this drags the rlib's CGU into the binary so the
linkme distributed-slice walker finds the backend. C/C++ builds rely
on the CMake-emitted strong stub from `nano_ros_link_rmw(... RMW <x>)`
instead.

For C++ consumers, the CMake option is the canonical way:

```bash
cmake -S . -B build -DNANO_ROS_RMW=cyclonedds  # zenoh / xrce / dds / cyclonedds
```

### Kconfig (Zephyr)

```kconfig
# Zenoh backend
CONFIG_NROS_RMW_ZENOH=y

# XRCE-DDS backend
CONFIG_NROS_RMW_XRCE=y

# DDS backend (dust-dds)
CONFIG_NROS_RMW_DDS=y

# Cyclone DDS backend (Cortex-A/R Zephyr targets)
CONFIG_NROS_RMW_CYCLONEDDS=y
```

Enabling more than one simultaneously produces a `compile_error!()`.

## DDS — per-platform configuration profile

The DDS backend speaks raw RTPS over UDP multicast and unicast. The
host networking stack on every RTOS needs IGMP enabled, adequate net-
buffer pool sizing for the SPDP / SEDP discovery burst, and a way to
join a multicast group. Concrete deltas per platform:

### Zephyr (`platform-zephyr`)

```kconfig
# IGMP for SPDP multicast discovery (239.255.0.1:7400+).
CONFIG_NET_IPV4_IGMP=y

# Multicast group slots — RTPS uses up to 4 builtin groups per
# participant (SPDP + SEDP pubs / subs / topics).
CONFIG_NET_IF_MCAST_IPV4_ADDR_COUNT=4

# Net-buffer pools — defaults of 14 / 36 are too small for the SEDP
# burst when more than two service / action entities exist on each
# participant. Symptom of undersizing: runtime starvation as the
# discovery exchange backs up. Cortex_a9 has 512 MB SRAM; the cost
# is trivial.
CONFIG_NET_PKT_RX_COUNT=256
CONFIG_NET_PKT_TX_COUNT=128
CONFIG_NET_BUF_RX_COUNT=512
CONFIG_NET_BUF_TX_COUNT=256
CONFIG_NET_BUF_DATA_SIZE=512

# Heap — dust-dds + heapless mailboxes.
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=4194304
CONFIG_HEAP_MEM_POOL_SIZE=524288
```

On `qemu_cortex_a9`, additionally use a board overlay to bump the
GEM driver's RX / TX descriptor ring (default 32) so a brief drainer
pause doesn't immediately spill:

```dts
&gem0 {
    promiscuous-mode;
    rx-buffer-descriptors = <128>;
    tx-buffer-descriptors = <64>;
};
```

`native_sim` is not yet supported — Zephyr's NSOS driver doesn't
forward `IP_ADD_MEMBERSHIP` to the host kernel
(no `IPPROTO_IP` case in `nsos_adapt_setsockopt`). Use
`qemu_cortex_a9` for DDS-on-Zephyr testing instead.

### FreeRTOS + lwIP (`platform-freertos`)

```c
// FreeRTOSConfig.h / lwipopts.h
#define LWIP_IGMP            1   // SPDP multicast
#define LWIP_SO_RCVTIMEO     1   // dust-dds recv timeouts
#define LWIP_BROADCAST       1
#define IP_REASSEMBLY        1   // RTPS DATA_FRAG fragments
#define MEMP_NUM_NETBUF      32  // discovery burst headroom
```

### NuttX (`platform-nuttx`)

```kconfig
CONFIG_NET_IGMP=y
CONFIG_NET_BROADCAST=y
CONFIG_NET_UDP_NRECVS=4
CONFIG_NET_RECV_TIMEO=y
```

### ThreadX + NetX Duo (`platform-threadx`)

`tx_user.h` / `nx_user.h`:

```c
#define NX_ENABLE_IGMPV2          // IGMP v2 for RTPS multicast
// NetX BSD layer init must call `bsd_initialize` early — required
// for `setsockopt(SO_RCVTIMEO)` support.
```

### Bare-metal smoltcp (`platform-mps2-an385`, `platform-stm32f4`,
`platform-esp32-qemu`)

```rust
// Bridge config in the board crate.
let mut config = smoltcp::iface::Config::new(...);
config.multicast_groups = vec![Ipv4Address::new(239, 255, 0, 1)];
// MulticastConfig::Strict in smoltcp 0.x; the bridge must expose
// a `join_multicast_group` API.
```

### POSIX (`platform-posix`)

No configuration needed — the kernel does IGMP and `setsockopt`
support natively. Just ensure the loopback interface is up
(default).

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

**Choose DDS when:**
- You want direct, brokerless ROS 2 interop with no `zenohd` and no Agent
- Your MCU has at least ~32 KB RAM (heap for dust-dds futures + RTPS state) and a network stack with IGMP
- You're integrating with a DDS-only ROS 2 deployment (Cyclone DDS, FastDDS) and want first-class interop without protocol translation
- Your tolerance for failure is "samples from peer X stop arriving" rather than "the whole router goes down"

**Any of the three works well for:**
- Standard pub/sub and service patterns
- Integration with ROS 2 desktop nodes
- QEMU-based development and testing
