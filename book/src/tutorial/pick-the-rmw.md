# Choosing an RMW

<!-- type: tutorial · level: 1 -->

Every nano-ros binary picks its RMW (ROS Middleware) backend at **build
time**. There are three: **Cyclone DDS**, **Zenoh** (zenoh-pico), and
**XRCE-DDS**. This page helps you pick one and explains how each backend
connects your device to a ROS 2 network. The mechanics of changing your
pick are in [Switching RMW Backends](../communicate/switching-rmw.md).

## Start with Cyclone DDS

For getting started, use **Cyclone DDS**. It is the one backend that
needs no daemon, router, or agent: a nano-ros node publishes with
nothing else running, and a stock ROS 2 node using `rmw_cyclonedds_cpp`
sees it directly — same RTPS wire protocol, same discovery, no
translation layer in between.

The reason the other two are not the starting point: nano-ros ships no
bridge process of its own. The zenoh router **is** ROS 2's own
`rmw_zenohd`, so the zenoh backend requires a ROS 2 installation just to start the
router; the XRCE backend requires a running Micro-XRCE-DDS Agent.
Cyclone DDS requires neither.

Current maturity: pub/sub and services interoperate with stock
`rmw_cyclonedds_cpp`. Which status events and other features it wires is
listed in the [Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md);
the remaining known limitations are in
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
the router binary. See [ROS 2 Interop](talk-to-ros2.md)
for the full three-terminal walkthrough.

## XRCE-DDS — smallest footprint, serial transport

Choose the **XRCE-DDS** backend when:

- RAM is the constraint: ~3 KB of client RAM, and the backend allocates
  only when a session or entity is created — never per message. (It is
  not heap-free: each allocation goes through `nros_platform_alloc`, and
  their number follows the entities you declare.)
- You need a **serial (UART)** transport — the only backend that works
  on an MCU with no networking hardware. See
  [Serial Transport](../communicate/serial-transport.md).
- You are joining an existing micro-ROS / DDS deployment: the agent
  bridges to any DDS-based RMW.

The MCU cannot participate without the **Micro-XRCE-DDS Agent**
(`MicroXRCEAgent`) running on a host; if the agent stops, the device
loses all connectivity.

## What each backend supports

The [Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md) is the
one capability table in this book. It is generated from the backend
sources, so it is current: services, status events, graph introspection,
zero-copy loans and the rest, per backend. In short, all three backends
do publish/subscribe, service servers and clients, and status events;
they differ in the less common features (for example, XRCE-DDS has no
graph introspection, and only Zenoh has the zero-copy loan API). The
version of each backend's upstream library is listed on
[Support Status](../reference/support-status.md).

## How each backend works

The three backends differ less in what they can do than in the shape of
the network around your device: what else has to run, who handles
discovery, and what happens when a process goes away.

### Zenoh

The Zenoh backend uses [zenoh-pico](https://github.com/eclipse-zenoh/zenoh-pico),
a lightweight C client for the Zenoh protocol. The MCU takes part in the
Zenoh network directly; there is no protocol translation layer.

1. The MCU runs zenoh-pico in **client mode** and connects to a zenoh
   router (`rmw_zenohd`, ROS 2's own) over TCP, UDP or TLS.
2. zenoh-pico creates publishers and subscribers directly on the Zenoh
   network.
3. ROS 2 nodes running `rmw_zenoh_cpp` connect to the same router. The
   router forwards messages without interpreting them; it is not a
   protocol translator.
4. The device takes part in ROS 2 graph discovery through liveliness
   tokens, so `ros2 topic list` and `ros2 node list` see it.

Zenoh itself defines a **peer mode** in which two zenoh-pico devices talk
directly with no router. **nano-ros does not ship it**: zenoh-pico is
built without its multicast transport and scouting, because every
nano-ros deployment reaches its peers through a router or an agent and
those code paths cost flash on an MCU. Setting `NROS_SESSION_MODE=peer`
is therefore refused at session open, with a message saying so.

If the router stops, the device loses routing but keeps running.
Footprint: roughly 16 KB+ of RAM (heap required) and 100 KB+ of flash.

### XRCE-DDS

The XRCE-DDS backend uses
[Micro-XRCE-DDS-Client](https://github.com/eProsima/Micro-XRCE-DDS-Client),
the same client library micro-ROS uses. It follows an **agent-based
model**: a small client on the MCU delegates entity creation to an agent
process.

1. The MCU runs the XRCE-DDS client and connects to an **Agent** process
   over UDP or serial (UART, HDLC framing).
2. The client asks the Agent to create entities ("a publisher on topic X
   with type Y").
3. The Agent creates full DDS entities on the client's behalf and bridges
   data between the XRCE protocol and the DDS data space.
4. ROS 2 nodes using any DDS-based RMW (Fast DDS, Cyclone DDS)
   communicate through the Agent.

The Agent is mandatory and handles discovery; the client does none of
its own. If the Agent stops, the device loses all connectivity.

Heap use on the MCU is small and bounded: the backend allocates when a
session or entity is created, never per message. Each
`XRCE_MAX_SUBSCRIBERS` slot is heap, so size it to the subscriptions you
declare. Footprint: roughly 3 KB of client RAM and 75 KB of flash. The
upstream client also defines a CAN FD transport, but nano-ros builds do
not enable it.

### Cyclone DDS

The Cyclone DDS backend uses
[Eclipse Cyclone DDS](https://github.com/eclipse-cyclonedds/cyclonedds),
the same DDS implementation ROS 2 uses through `rmw_cyclonedds_cpp`. It is
a C++ library that registers itself with the nano-ros runtime through the
RMW C ABI.

1. Selecting `cyclonedds` for the image links the backend and registers
   it before the executor opens; you do not call a register function
   yourself.
2. The executor's session, publisher, subscriber and service operations
   dispatch to Cyclone, which creates the DDS domain, participant,
   topics, writers and readers. Cyclone runs its own receive threads.
3. ROS 2 nodes using stock `rmw_cyclonedds_cpp` interoperate directly:
   same wire protocol, same discovery, and no key rewriting (unlike
   `rmw_zenoh`'s `<domain>/<topic>/<type>/…` key expressions).

Discovery is SPDP / SEDP over UDP multicast, or a static unicast peer
list. The domain is configured from a built-in configuration rather than
an XML file, so no XML parser is linked. Cyclone allocates with
`malloc`, so a heap is required: plan for roughly 32 KB+ of RAM and
150 KB+ of flash on an embedded link (the POSIX shared library,
`libddsc.so`, is about 1.4 MB). When a peer goes offline, only its
samples stop arriving; there is no central process whose loss takes the
network down.

**Getting the Cyclone library.** Selecting `cyclonedds` for an image is
all you have to do; the build provides Cyclone itself. It uses a prebuilt
install if you point at one (`-DCMAKE_PREFIX_PATH=<install>` or
`-DCycloneDDS_DIR=`); otherwise it builds Cyclone from source, by default
from the pinned sources that ship with nano-ros, or from your own copy
with `-DCYCLONEDDS_SOURCE_DIR=<path>`. A plain `nros build` therefore
needs no separate Cyclone step. The `idlc` code generator is a host tool,
found on `PATH` (for example from a ROS 2 install) or through
`-DIDLC_EXECUTABLE=`.

**Zephyr on Cortex-A / Cortex-R** needs the `aarch64-zephyr-elf`
toolchain in your Zephyr SDK install; toolchain setup for these targets
is in
[`docs/reference/zephyr-armv8r-setup.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/reference/zephyr-armv8r-setup.md).

The footprint figures in this section are order-of-magnitude planning
numbers, not measurements; they vary with features, message set and
optimisation level. Size your own image with `arm-none-eabi-size`; the
RAM knobs are listed in the
[Static Pool Inventory](../reference/static-pool-inventory.md).

## Cyclone DDS network configuration per platform

Cyclone DDS speaks RTPS over UDP multicast and unicast. On every RTOS the
network stack needs IGMP enabled, enough network buffers for the SPDP /
SEDP discovery burst, and a way to join a multicast group. The settings
per platform:

### Zephyr

```kconfig
# IGMP for SPDP multicast discovery (239.255.0.1:7400+).
CONFIG_NET_IPV4_IGMP=y

# Multicast group slots — RTPS uses up to 4 builtin groups per
# participant (SPDP + SEDP pubs / subs / topics).
CONFIG_NET_IF_MCAST_IPV4_ADDR_COUNT=4

# Net-buffer pools — defaults of 14 / 36 are too small for the SEDP
# burst when more than two service / action entities exist on each
# participant. Symptom of undersizing: runtime starvation as the
# discovery exchange backs up.
CONFIG_NET_PKT_RX_COUNT=256
CONFIG_NET_PKT_TX_COUNT=128
CONFIG_NET_BUF_RX_COUNT=512
CONFIG_NET_BUF_TX_COUNT=256
CONFIG_NET_BUF_DATA_SIZE=512

# Heap — Cyclone DDS (malloc) + heapless mailboxes.
CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=4194304
CONFIG_HEAP_MEM_POOL_SIZE=524288
```

On `qemu_cortex_a9`, also use a board overlay to enlarge the GEM
driver's RX / TX descriptor rings (default 32), so a brief pause in
draining does not immediately drop frames:

```dts
&gem0 {
    promiscuous-mode;
    rx-buffer-descriptors = <128>;
    tx-buffer-descriptors = <64>;
};
```

On `native_sim`, Zephyr's offloaded-sockets driver does not forward
`IP_ADD_MEMBERSHIP` to the host kernel, so multicast discovery does not
work there. The `native_sim` configuration discovers peers by unicast to
`127.0.0.1` instead (see
[Troubleshooting](../operate/troubleshooting.md#cyclone-dds-two-native_sim-nodes-never-discover-each-other));
use `qemu_cortex_a9` to exercise multicast DDS on Zephyr.

### FreeRTOS + lwIP

```c
// FreeRTOSConfig.h / lwipopts.h
#define LWIP_IGMP            1   // SPDP multicast
#define LWIP_SO_RCVTIMEO     1   // Cyclone DDS recv timeouts
#define LWIP_BROADCAST       1
#define IP_REASSEMBLY        1   // RTPS DATA_FRAG fragments
#define MEMP_NUM_NETBUF      32  // discovery burst headroom
```

### NuttX

```kconfig
CONFIG_NET_IGMP=y
CONFIG_NET_BROADCAST=y
CONFIG_NET_UDP_NRECVS=4
CONFIG_NET_RECV_TIMEO=y
```

### ThreadX + NetX Duo

`tx_user.h` / `nx_user.h`:

```c
#define NX_ENABLE_IGMPV2          // IGMP v2 for RTPS multicast
// NetX BSD layer init must call `bsd_initialize` early — required
// for `setsockopt(SO_RCVTIMEO)` support.
```

### Bare-metal smoltcp

```rust
// Bridge config in the board crate.
let mut config = smoltcp::iface::Config::new(...);
config.multicast_groups = vec![Ipv4Address::new(239, 255, 0, 1)];
// MulticastConfig::Strict in smoltcp 0.x; the bridge must expose
// a `join_multicast_group` API.
```

### POSIX

No configuration is needed: the kernel handles IGMP and `setsockopt`.
Make sure the loopback interface is up (it is by default).

## Decision summary

- **Default / getting started / DDS fleet** → Cyclone DDS. Nothing to
  run beside your node; direct `rmw_cyclonedds_cpp` interop. Needs
  ~32 KB RAM and a network stack with IGMP.
- **`rmw_zenoh_cpp` fleet, small MCU, or TCP-only network** → Zenoh.
  Needs the router from your ROS 2 install
  (`ros2 run rmw_zenoh_cpp rmw_zenohd`).
- **Tiny MCU, serial link, or micro-ROS deployment** → XRCE-DDS.
  Smallest footprint and no per-message allocation, but the agent is
  mandatory.

Any of the three handles standard publish/subscribe and services,
integration with ROS 2 desktop nodes, and development under QEMU.

You do not have to pick exactly one per system: a single binary can
link **multiple backends** and bridge between them, for example zenoh
devices to a Cyclone DDS stack, or XRCE devices to a Zenoh network — see
[Cross-backend Bridges](../communicate/bridges.md).

Once you have picked, [Switching RMW Backends](../communicate/switching-rmw.md)
shows the exact edits per build system.

## Design background

- [RFC-0031: RMW selection and lowering](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md)
- [RFC-0075: zenoh router provenance and the unstable seam](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
