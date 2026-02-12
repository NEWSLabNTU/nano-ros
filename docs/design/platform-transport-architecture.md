# Transport Protocols and Platform Backends

This document describes the transport protocols available in zenoh-pico, how nano-ros currently integrates with them, and the path to supporting additional protocols on RTOS and bare-metal platforms.

For the smoltcp+zenoh-pico integration design, see [smoltcp-zenoh-pico-integration.md](smoltcp-zenoh-pico-integration.md).

## Protocol Overview

Zenoh-pico supports 7 transport protocols, each controlled by a compile-time feature flag. nano-ros inherits these through the `zenoh-pico-shim-sys` crate.

| Protocol      | Locator Format          | Flow     | Reliable    | Multicast | Feature Flag                   | Default |
|---------------|-------------------------|----------|-------------|-----------|--------------------------------|---------|
| TCP           | `tcp/host:port`         | Stream   | Yes         | No        | `Z_FEATURE_LINK_TCP`           | ON      |
| UDP Unicast   | `udp/host:port`         | Datagram | No          | No        | `Z_FEATURE_LINK_UDP_UNICAST`   | ON      |
| UDP Multicast | `udp/group:port`        | Datagram | No          | Yes       | `Z_FEATURE_LINK_UDP_MULTICAST` | ON      |
| Serial        | `serial/dev#baudrate=N` | Stream   | Yes (CRC32) | No        | `Z_FEATURE_LINK_SERIAL`        | OFF     |
| Bluetooth     | `bt/name`               | Stream   | Yes         | No        | `Z_FEATURE_LINK_BLUETOOTH`     | OFF     |
| Raw Ethernet  | `reth/iface`            | Datagram | No          | Yes       | `Z_FEATURE_RAWETH_TRANSPORT`   | OFF     |
| WebSocket     | `ws/host:port`          | Stream   | Yes         | No        | `Z_FEATURE_LINK_WS`            | OFF     |

TLS (`tls/host:port`) is also available as an encrypted wrapper over TCP, requiring Mbed TLS 3.0+.

## Current nano-ros Integration

nano-ros uses **TCP only** for all current platforms. The zenoh-pico build in `zenoh-pico-shim-sys` enables:

```cmake
Z_FEATURE_LINK_TCP=1
Z_FEATURE_LINK_UDP_UNICAST=1
Z_FEATURE_LINK_UDP_MULTICAST=1
Z_FEATURE_LINK_SERIAL=1         # Enabled in build.rs but not exposed to Rust API
Z_FEATURE_INTEREST=1             # Required for cross-network routing through zenohd
Z_FEATURE_MATCHING=1
```

### Platform Backends in nano-ros

Three mutually exclusive backends, selected via Cargo features:

| Backend | Cargo Feature  | Threading       | Networking         | Platforms                |
|---------|----------------|-----------------|--------------------|--------------------------|
| POSIX   | `shim-posix`   | pthreads        | POSIX sockets      | Linux, macOS             |
| Zephyr  | `shim-zephyr`  | Zephyr threads  | Zephyr BSD sockets | Zephyr RTOS              |
| smoltcp | `shim-smoltcp` | Single-threaded | smoltcp polling    | Bare-metal ARM, ESP32-C3 |

### Locator Configuration

Locators are configured via `Context::from_env()` or `InitOptions`:

```rust
// Environment variables
// ZENOH_LOCATOR=tcp/192.168.1.1:7447
// ZENOH_MODE=client
let ctx = Context::from_env()?;

// Programmatic
let ctx = Context::new(InitOptions::new().locator("tcp/127.0.0.1:7447"))?;
```

The locator string is passed directly to zenoh-pico, which parses the protocol schema and dispatches to the appropriate link handler.

## zenoh-pico Link Architecture

zenoh-pico uses a three-tier abstraction:

```
┌──────────────────────────────────────────────────────────┐
│  Transport Layer                                         │
│  - Unicast: point-to-point, sequence numbers, defrag     │
│  - Multicast: group communication, lease-based           │
├──────────────────────────────────────────────────────────┤
│  Link Layer (_z_link_t)                                  │
│  - Protocol-specific open/close/read/write via fn ptrs   │
│  - Union of socket types (TCP, UDP, serial, raweth, ...) │
│  - Capabilities: stream/datagram, reliable/unreliable    │
├──────────────────────────────────────────────────────────┤
│  Platform Layer (_z_sys_net_socket_t)                     │
│  - Per-platform socket representation                    │
│  - Per-platform I/O implementations                      │
│  - Threading, memory, timing primitives                  │
└──────────────────────────────────────────────────────────┘
```

Each link declares its capabilities via a bitfield:
- **Transport**: unicast, multicast, or raweth
- **Flow**: stream (TCP, serial) or datagram (UDP)
- **Reliable**: yes (TCP, serial with CRC) or no (UDP)

The transport layer uses these to select sequencing and fragmentation strategies.

## Protocol Details

### TCP

Standard reliable stream transport. Used by default in nano-ros for all platforms.

- Requires a full TCP/IP stack (POSIX sockets, lwIP, or smoltcp)
- `TCP_NODELAY` enabled when `Z_FEATURE_TCP_NODELAY=1`
- `SO_KEEPALIVE` for dead connection detection
- Linger timeout: `Z_TRANSPORT_LEASE / 1000` seconds

### UDP

Lightweight datagram transport. Unicast for point-to-point, multicast for discovery (scouting).

- Lower latency than TCP (no handshake, no retransmits)
- No connection state
- Multicast enables peer discovery without a router (`ZENOH_MODE=peer`)
- Scouting uses UDP multicast by default

### Serial/UART

Point-to-point reliable transport over a serial link. No IP stack required.

**Frame format** (COBS-encoded):

```
+---+---+------+--------------+----------+---+
| O | H |  Len |   Payload    |   CRC32  | 0 |
+---+---+------+--------------+----------+---+
  1   1    2       N bytes         4        1    (bytes)

O = COBS overhead byte
H = Header flags (Init=0x01, Ack=0x02, Reset=0x04)
Len = Payload length (little-endian)
CRC32 = Error detection checksum
0 = End-of-packet marker
```

- MTU: 1500 bytes; max frame on wire: 1516 bytes (with COBS overhead)
- COBS encoding guarantees no 0x00 bytes in the frame body, using 0x00 as delimiter
- CRC32 provides error detection (no retransmission — the zenoh transport layer handles reliability)

**Connection handshake**:
1. Client sends `INIT` flag
2. Server responds `INIT | ACK`
3. On error: server sends `RESET`, client waits 250ms and retries

**Two initialization APIs**:
- Pin-based (embedded): `_z_open_serial_from_pins(sock, tx_pin, rx_pin, baudrate)`
- Device-based (OS): `_z_open_serial_from_dev(sock, "/dev/ttyUSB0", baudrate)`

### Raw Ethernet

Layer 2 direct MAC frame transmission. No IP stack required.

```
Standard frame:
+------+------+---------+--------+-------------+
| DMAC | SMAC | EthType | Length |   Payload   |
+------+------+---------+--------+-------------+
   6      6       2        2       up to 1500     (bytes)

VLAN-tagged frame:
+------+------+------+-----+---------+--------+-------------+
| DMAC | SMAC | 8100 | Tag | EthType | Length |   Payload   |
+------+------+------+-----+---------+--------+-------------+
   6      6      2      2       2        2       up to 1496    (bytes)
```

- Default EtherType: `0x72e0` (vendor-specific)
- Supports VLAN tagging for network segmentation
- MAC address whitelisting for filtering
- Topic-to-MAC mapping: routes zenoh key expressions to specific MAC destinations
- Currently implemented only on Linux (AF_PACKET raw sockets)

### Bluetooth

Serial Port Profile (SPP) over Bluetooth Classic. Stream-based, point-to-point.

- Master or slave mode
- Currently implemented on Arduino ESP32 (`BluetoothSerial.h`)
- Not available on most RTOS platforms

## zenoh-pico Platform Backends

zenoh-pico ships backends for 11+ platforms. Each implements a platform abstraction layer providing sockets, threading, memory, and timing primitives.

| Platform        | TCP | UDP | Serial     | BT | Raw Eth | TLS | Threading     | IP Stack     |
|-----------------|-----|-----|------------|----|---------|-----|---------------|--------------|
| Unix/Linux      | Y   | Y   | Y (device) | N  | **Y**   | Y   | pthread       | POSIX        |
| Windows         | Y   | Y   | N          | N  | N       | N   | Win32         | Winsock2     |
| Zephyr          | Y   | Y   | Y (device) | N  | N       | N   | Zephyr kernel | Zephyr BSD   |
| ESP-IDF         | Y   | Y   | Y (device) | Y  | N       | N   | FreeRTOS      | lwIP         |
| Arduino ESP32   | Y   | Y   | Y          | Y  | N       | N   | FreeRTOS      | lwIP         |
| FreeRTOS + lwIP | Y   | Y   | N          | N  | N       | N   | FreeRTOS      | lwIP         |
| FreeRTOS + TCP  | Y   | Y   | N          | N  | N       | N   | FreeRTOS      | FreeRTOS+TCP |
| RPi Pico        | Y   | Y   | N          | N  | N       | N   | pico-sdk      | lwIP/CYW43   |
| ARM Mbed        | Y   | Y   | ?          | ?  | N       | N   | Mbed OS       | lwIP         |
| ThreadX STM32   | Y   | Y   | ?          | ?  | N       | N   | ThreadX       | NetX         |
| Emscripten      | Y   | Y   | N          | N  | N       | N   | N/A           | WebSocket    |

### Runtime Protocol Availability: POSIX (Linux/macOS)

The POSIX backend uses zenoh-pico's Unix platform layer. All protocol code is built-in — no BSP needed.

| Protocol      | Status         | Implementation                          | Notes                                                                                           |
|---------------|----------------|-----------------------------------------|-------------------------------------------------------------------------------------------------|
| TCP           | **Works**      | POSIX sockets, `getaddrinfo()`          | Full client + server. `TCP_NODELAY`, `SO_KEEPALIVE`                                             |
| UDP Unicast   | **Works**      | POSIX sockets, `sendto()`/`recvfrom()`  | Client send works. Listen/server has a `@TODO` — returns error                                  |
| UDP Multicast | **Works**      | `IP_ADD_MEMBERSHIP` / `IPV6_JOIN_GROUP` | Full. Uses `getifaddrs()` for interface selection                                               |
| Serial        | **Works**      | `termios.h` API                         | Device-based (`/dev/ttyUSB0`). Pin-based returns error (N/A on desktop). Baudrates: 9600–921600 |
| Raw Ethernet  | **Linux only** | `AF_PACKET` raw sockets                 | macOS/BSD: compile-time `#error`. Requires root or `CAP_NET_RAW`                                |
| TLS           | **Works**      | mbedTLS (v2.x/v3.x)                     | Full client + server + mTLS. Requires mbedTLS library                                           |
| Bluetooth     | **No**         | `#error "not supported yet"`            | Compile-time error if enabled                                                                   |
| WebSocket     | **No**         | Missing entirely                        | Only Emscripten has WS. No Unix implementation                                                  |

**What nano-ros gets on `platform-posix` without any BSP:**
- TCP, UDP multicast, Serial, TLS — all via zenoh-pico built-in code
- UDP unicast send works; UDP unicast listen is unimplemented upstream
- Raw Ethernet on Linux only (requires `CAP_NET_RAW` or root)
- Protocol selected at runtime via locator string

**Gaps that would need a BSP or upstream contribution:**
- Raw Ethernet on macOS/BSD (would need BPF implementation)
- Bluetooth (would need BlueZ integration on Linux)
- WebSocket (would need a WS library)

### Runtime Protocol Availability: Zephyr

The Zephyr backend uses zenoh-pico's Zephyr platform layer. Uses Zephyr BSD sockets for IP and Zephyr UART driver for serial.

| Protocol        | Status      | Implementation               | Notes                                                                                   |
|-----------------|-------------|------------------------------|-----------------------------------------------------------------------------------------|
| TCP             | **Works**   | Zephyr BSD sockets           | Full client + server. `SO_RCVTIMEO` logged as "consistently fails" but continues        |
| UDP Unicast     | **Partial** | `sendto()` only              | Client send works. Listen/server: `@TODO` — returns error                               |
| UDP Multicast   | **Works**   | Zephyr `net_if` APIs         | Full. Uses `net_if_ipv4_maddr_add()`/`join()`. Always uses default interface            |
| Serial (device) | **Works**   | Zephyr UART driver           | `device_get_binding()` + `uart_poll_in()`/`uart_poll_out()`. 8N1, configurable baudrate |
| Serial (pin)    | **No**      | `@TODO`                      | Returns error. Would need GPIO/UART init per board                                      |
| Serial (listen) | **No**      | `@TODO`                      | Returns error. Client mode only                                                         |
| Raw Ethernet    | **No**      | `#error "not supported yet"` | Compile-time error. Zephyr has `AF_PACKET` but zenoh-pico doesn't use it                |
| Bluetooth       | **No**      | `#error "not supported yet"` | Compile-time error. Zephyr has a full BLE stack but zenoh-pico doesn't use it           |
| TLS             | **No**      | Not implemented              | Zephyr has mbedTLS support but no zenoh-pico integration                                |
| WebSocket       | **No**      | Not implemented              |                                                                                         |

**Required Zephyr Kconfig for TCP + UDP:**
```
CONFIG_NETWORKING=y
CONFIG_NET_TCP=y
CONFIG_NET_UDP=y
CONFIG_NET_SOCKETS=y
CONFIG_NET_SOCKETS_POSIX_NAMES=y
CONFIG_NET_IPV4=y
CONFIG_POSIX_API=y
```

**Additional Kconfig for UDP multicast:**
```
CONFIG_NET_UDP_MULTICAST=y
CONFIG_NET_IPV4_IGMP=y
CONFIG_NET_IF_MCAST_IPV4_ADDR_COUNT=4
```

**Additional Kconfig for Serial:**
```
CONFIG_SERIAL=y
```
Plus a devicetree UART entry (e.g., `&uart0 { status = "okay"; };`).

**What nano-ros gets on `platform-zephyr` without additional BSP work:**
- TCP (full), UDP multicast (full), Serial device-based (client mode)
- Protocol selected at runtime via locator string

**Gaps that would need a BSP or upstream contribution:**

| Gap                    | Effort | Approach                                                                                                                                                                      |
|------------------------|--------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Raw Ethernet           | Medium | Zephyr has `AF_PACKET` raw sockets. Implement `_z_open_raweth()` etc. using Zephyr's raw socket API. Could be a nano-ros BSP extension or an upstream zenoh-pico contribution |
| Serial pin-based       | Low    | Board-specific GPIO→UART init in BSP, then delegate to existing device-based serial code                                                                                      |
| Serial listen (server) | Low    | Implement `_z_listen_serial_from_dev()` — mostly mirrors the open path                                                                                                        |
| Bluetooth (BLE)        | High   | Zephyr has full BLE stack. Would need SPP or custom GATT service. Likely upstream contribution                                                                                |
| TLS                    | Medium | Zephyr bundles mbedTLS. Could port Unix TLS code to use Zephyr's mbedTLS config                                                                                               |
| UDP unicast listen     | Low    | Upstream `@TODO` — standard `bind()` + `recvfrom()`                                                                                                                           |

## Feature Model

### Design Constraints

1. **zenoh is the only transport today.** The architecture supports multiple transports, but only zenoh (via zenoh-pico) is implemented. All platform and link layer details below are zenoh-specific.
2. **zenoh-pico is not modified.** Protocol availability on each platform is fixed by what zenoh-pico already implements.
3. On OS platforms (POSIX, Zephyr), zenoh-pico's built-in backend provides the protocol implementations. nano-ros cannot add protocols without modifying zenoh-pico.
4. On bare-metal, nano-ros provides the platform layer. Platform and link crates provide protocol backends that zenoh-pico calls.

### What the Platform Determines

The platform choice fixes the set of protocols that are *possible* at runtime. The user selects among them via the locator string.

**`platform-posix`** — zenoh-pico's Unix backend handles all networking:

| Runtime Protocols                        | Via                                                   |
|------------------------------------------|-------------------------------------------------------|
| TCP, UDP Multicast, Serial (device), TLS | zenoh-pico built-in (POSIX sockets, termios, mbedTLS) |
| Raw Ethernet (Linux only)                | zenoh-pico built-in (AF_PACKET)                       |

No BSP needed. No backend features needed. All available protocols are compiled in and selected at runtime via locator string.

**`platform-zephyr`** — zenoh-pico's Zephyr backend handles all networking:

| Runtime Protocols                   | Via                                                   |
|-------------------------------------|-------------------------------------------------------|
| TCP, UDP Multicast, Serial (device) | zenoh-pico built-in (Zephyr BSD sockets, UART driver) |

No backend features needed. BSP role is limited to board configuration (device tree, Kconfig). Cannot add raweth or BLE without modifying zenoh-pico.

**`platform-bare-metal`** — nano-ros provides the platform layer:

| Runtime Protocols | Requires                                                  |
|-------------------|-----------------------------------------------------------|
| TCP, UDP          | `link-smoltcp` crate (platform provides Ethernet driver) |
| Serial            | `link-serial` crate (platform provides UART driver)      |
| Raw Ethernet      | `link-raweth` crate (platform provides frame-level Ethernet driver) |

Platform crates provide system primitives (memory, clock, RNG). Link crates provide network symbol implementations. Both directly implement zenoh-pico's standard platform symbols — no custom intermediate FFI.

### Feature Structure

Three orthogonal feature axes control the build:

```
Transport (select one middleware protocol):
├── zenoh                  # zenoh-pico middleware (only transport today)
│                            Implies: platform-posix + alloc (convenience default)
│                            Future alternatives: dds, mqtt, ...
│
Platform (mutually exclusive, compile-time):
├── platform-posix         # zenoh-pico Unix backend
│                            zenoh-pico provides ALL symbols (system + network)
│                            No additional crates needed
│
├── platform-zephyr        # zenoh-pico Zephyr backend
│                            zenoh-pico provides ALL symbols (system + network)
│                            BSP: board-level device tree + Kconfig only
│
└── platform-bare-metal    # nano-ros provides symbol implementations
    │
    ├── Platform crate (one per hardware target):
    │   ├── platform-qemu      # QEMU MPS2-AN385: LAN9118, DWT clock
    │   ├── platform-esp32     # ESP32-C3: WiFi/OpenETH, CCOUNT clock
    │   └── platform-stm32f4  # STM32F4: EthernetDMA, SysTick clock
    │   Implements: z_malloc, z_clock_now, z_random_u32, z_sleep_ms,
    │               threading stubs, socket helpers (_z_socket_close, etc.)
    │
    └── Link crate (one per protocol, composable):
        ├── link-smoltcp  # TCP/UDP via smoltcp IP stack
        │   Implements: _z_open_tcp, _z_send_tcp, _z_read_tcp, etc.
        │   Activates: zenoh-pico-shim-sys/link-tcp
        │
        ├── link-serial   # UART serial link
        │   Implements: _z_open_serial_from_pins, _z_read_serial_internal, etc.
        │   Activates: zenoh-pico-shim-sys/link-serial
        │
        └── link-raweth   # Layer 2 raw Ethernet frames
            Implements: _z_open_raweth, _z_send_raweth, _z_receive_raweth, etc.
            Activates: zenoh-pico-shim-sys/link-raweth
```

### Transport Features

The **transport** axis selects the middleware protocol that carries ROS 2 messages. Currently only zenoh is implemented.

| Feature | Middleware | Crate Dependency | Status |
|---------|-----------|-----------------|--------|
| `zenoh` | zenoh-pico | `zenoh-pico-shim` | Implemented |

The `zenoh` feature is a convenience alias for `platform-posix` + `alloc` in the top-level `nano-ros` crate. For bare-metal, users don't use the `zenoh` feature — they depend on `zenoh-pico-shim-sys` (via platform + link crates) directly.

If a second transport were added (e.g., DDS via Micro-XRCE-DDS, or MQTT), it would follow the same pattern:

```toml
# nano-ros/Cargo.toml (hypothetical)
[features]
zenoh = ["nano-ros-node/zenoh", "nano-ros-transport/zenoh"]
dds   = ["nano-ros-node/dds", "nano-ros-transport/dds"]
mqtt  = ["nano-ros-node/mqtt", "nano-ros-transport/mqtt"]
```

Each transport feature would gate a different backend in `nano-ros-transport`, selecting the middleware-specific session, publisher, and subscriber implementations. The `nano-ros-node` API (`Node`, `Publisher<M>`, `Subscription<M>`) stays transport-agnostic — transport selection only affects which concrete types back the trait objects.

Transport and platform features are orthogonal: `zenoh` works with `platform-posix`, `platform-zephyr`, or `platform-bare-metal`. A future `dds` transport would similarly work across platforms that provide the required system primitives.

### Compile-Time Protocol Enablement

Protocol availability requires compile-time enablement at two levels, depending on the platform.

**POSIX**: No compile-time configuration needed by the user. `zenoh-pico-shim-sys/build.rs` passes all relevant `Z_FEATURE_LINK_*` flags to CMake. All protocols zenoh-pico supports on Unix are compiled in.

**Zephyr**: Two compile-time layers must agree:

1. **Zephyr Kconfig** — enables the kernel subsystem:
   ```
   CONFIG_NET_TCP=y              # Required for tcp/... locators
   CONFIG_NET_UDP=y              # Required for udp/... locators
   CONFIG_NET_UDP_MULTICAST=y    # Required for scouting
   CONFIG_SERIAL=y               # Required for serial/... locators
   ```

2. **zenoh-pico feature flags** — enables the protocol code:
   ```
   Z_FEATURE_LINK_TCP=1
   Z_FEATURE_LINK_UDP_UNICAST=1
   Z_FEATURE_LINK_UDP_MULTICAST=1
   Z_FEATURE_LINK_SERIAL=1
   ```

Both must be set for a locator string to work at runtime. If Kconfig enables TCP but zenoh-pico's `Z_FEATURE_LINK_TCP=0`, or vice versa, the protocol is unavailable.

On Zephyr, zenoh-pico is built by Zephyr's CMake system (not by nano-ros's `build.rs`). The zenoh-pico feature flags are controlled via Zephyr's Kconfig overlay:

```
# prj.conf (Zephyr project)
CONFIG_ZENOH_PICO_LINK_TCP=y
CONFIG_ZENOH_PICO_LINK_UDP_UNICAST=y
CONFIG_ZENOH_PICO_LINK_UDP_MULTICAST=y
CONFIG_ZENOH_PICO_LINK_SERIAL=y
```

The `bsp-zephyr` crate provides default Kconfig fragments that enable the common protocols. Board-specific overlays can adjust these.

**Bare-metal**: `zenoh-pico-shim-sys[bare-metal]` enables all `Z_FEATURE_LINK_*` flags unconditionally. LTO strips unused protocol code. The platform config header (`zenoh_generic_config.h`) can override individual flags if needed.

### Runtime Protocol Selection

On all platforms, the locator string selects the protocol at runtime:

```rust
let ctx = Context::new(InitOptions::new().locator("tcp/192.168.1.1:7447"))?;
let ctx = Context::new(InitOptions::new().locator("serial//dev/ttyUSB0#baudrate=115200"))?;
```

If the user passes a locator for a protocol that was not compiled in, zenoh-pico fails at session open with an error.

### Usage Examples

```toml
# Desktop (Linux) — zenoh transport, all link protocols available
# "zenoh" = platform-posix + alloc (convenience alias)
# User selects link protocol at runtime: ZENOH_LOCATOR=tcp/127.0.0.1:7447
nano-ros = { features = ["zenoh"] }

# Zephyr — TCP, UDP Multicast, Serial (device) available
# User selects at runtime via locator
# Board prj.conf must enable matching Kconfig options
nano-ros = { features = ["platform-zephyr"] }

# Bare-metal QEMU with Ethernet — TCP via smoltcp
[dependencies]
nano-ros-platform-qemu = { path = "..." }
nano-ros-link-smoltcp = { path = "..." }

# Bare-metal serial only — no IP stack, no Ethernet driver needed
[dependencies]
nano-ros-platform-qemu = { path = "..." }
nano-ros-link-serial = { path = "..." }

# Bare-metal with TCP + Serial — two transports
[dependencies]
nano-ros-platform-qemu = { path = "..." }
nano-ros-link-smoltcp = { path = "..." }
nano-ros-link-serial = { path = "..." }
```

### How Bare-Metal Provides Protocol Support

On bare-metal, zenoh-pico is compiled from source by `zenoh-pico-shim-sys`. It calls platform functions (`z_malloc`, `_z_open_tcp`, etc.) that must be provided externally. Unlike POSIX and Zephyr where zenoh-pico provides its own implementations, bare-metal implementations come from nano-ros crates.

**Key principle:** Platform and link crates implement zenoh-pico's own symbols directly using `#[unsafe(no_mangle)] extern "C"`. No custom intermediate FFI symbols (no `smoltcp_socket_open`, no `nros_net_tcp_open`). The same symbols that zenoh-pico defines in its headers and implements for POSIX/Zephyr are implemented by our Rust crates for bare-metal.

```
zenoh-pico C code
  calls z_malloc(), z_clock_now(), _z_open_tcp(), _z_send_tcp(), ...
    ↓ (link-time symbol resolution)
Rust #[unsafe(no_mangle)] extern "C" functions
  system symbols: implemented by platform crate (platform-qemu)
  network symbols: implemented by link crate (link-smoltcp)
    ↓
platform crate → hardware (clock, heap, RNG)
link crate → smoltcp → Ethernet driver → hardware
```

Compared to POSIX/Zephyr where zenoh-pico's own C files implement these symbols using OS APIs (`socket()`, `connect()`, `pthread_create()`, etc.), on bare-metal our Rust crates provide the implementations directly.

**Summary of responsibilities:**

| Component                         | Provided By                                   | What It Does                                                    |
|-----------------------------------|-----------------------------------------------|-----------------------------------------------------------------|
| Protocol code (link/transport)    | zenoh-pico (compiled by zenoh-pico-shim-sys)  | Framing, sequencing, locator parsing                            |
| Simplified C API (`zenoh_shim_*`) | `zenoh-pico-shim-sys` (`c/shim/zenoh_shim.c`) | Session/publisher/subscriber management wrapper over zenoh-pico |
| Platform type definitions         | `zenoh-pico-shim-sys` (C header)              | `_z_sys_net_socket_t`, `z_clock_t`, etc.                        |
| System symbol implementations     | Platform crate (Rust `#[no_mangle]`)          | `z_malloc`, `z_clock_now`, `z_sleep_ms`, threading stubs        |
| Network symbol implementations    | Link crate (Rust `#[no_mangle]`)         | `_z_open_tcp`, `_z_send_tcp`, `_z_close_tcp`, serial/raweth ops |
| Hardware driver                   | Platform crate                                | Direct hardware access (Ethernet MAC, UART, clock)              |

### Migration Path

The existing `shim-*` features remain as aliases during migration:

```toml
# Backward compatibility aliases
shim-posix   = ["platform-posix"]
shim-zephyr  = ["platform-zephyr"]
shim-smoltcp = ["platform-bare-metal"]
```

The `zenoh` convenience feature continues to imply `platform-posix` for desktop use. For bare-metal, users migrate from `nano-ros-bsp-qemu` to separate `nano-ros-platform-qemu` + `nano-ros-link-smoltcp` dependencies.

## Protocol Integration Details

### Serial Transport

**On POSIX**: Already works. zenoh-pico's Unix backend implements serial via termios. Pass `serial//dev/ttyUSB0#baudrate=115200` as the locator. No nano-ros changes needed.

**On Zephyr**: Already works. zenoh-pico's Zephyr backend implements device-based serial via `uart_poll_in()`/`uart_poll_out()`. Requires device tree UART entry. Pass `serial/uart0#baudrate=115200` as the locator.

**On bare-metal** (new, requires `link-serial`):

BSP implements the C functions that zenoh-pico's serial link calls:

```c
// Pin-based (MCUs with GPIO control)
z_result_t _z_open_serial_from_pins(_z_sys_net_socket_t *sock,
                                     uint32_t txpin, uint32_t rxpin,
                                     uint32_t baudrate);

// Device-based (named peripherals)
z_result_t _z_open_serial_from_dev(_z_sys_net_socket_t *sock,
                                    char *device, uint32_t baudrate);

z_result_t _z_read_serial(...);
z_result_t _z_send_serial(...);
z_result_t _z_close_serial(...);
```

zenoh-pico handles the COBS framing, CRC32, and connection handshake. The BSP only provides raw byte I/O.

**Bare-metal architecture** (serial, no IP stack):

```
┌──────────────────┐  UART  ┌──────────────────┐  TCP  ┌────────┐
│  MCU (bare-metal) │───────│  Host / Gateway   │──────│ zenohd │
│  nano-ros         │serial │  zenohd (serial   │      │(network│
│  zenoh-pico       │://    │  + tcp listener)  │      │  mesh) │
│  BSP UART driver  │       │                   │      │        │
└──────────────────┘       └──────────────────┘      └────────┘
```

### UDP Transport

**On POSIX**: Already works. Pass `udp/host:port` as the locator.

**On Zephyr**: UDP multicast works (scouting, group communication). UDP unicast listen is an upstream `@TODO`.

**On bare-metal** (requires `link-smoltcp`, new work):

The current smoltcp bridge (`SmoltcpZenohBridge`) only implements TCP sockets. Adding UDP requires:
- UDP socket slots in the bridge's socket table
- Handling for `sendto`/`recvfrom` semantics (vs TCP's `send`/`recv`)
- Multicast group join support in smoltcp (supported by smoltcp, not yet wired)

UDP multicast on bare-metal would enable `ZENOH_MODE=peer` — direct peer-to-peer discovery without a zenohd router, valuable for isolated embedded networks.

### Raw Ethernet Transport

**On POSIX (Linux)**: Already works. Pass `reth/eth0#ethtype=72e0` as the locator. Requires root or `CAP_NET_RAW`.

**On Zephyr**: Not available. zenoh-pico's Zephyr backend has `#error "not supported yet"`. Would need upstream contribution.

**On bare-metal** (new, requires `link-raweth`):

BSP implements frame-level send/receive:

```c
z_result_t _z_open_raweth(_z_sys_net_socket_t *sock, const char *interface);
z_result_t _z_send_raweth(_z_sys_net_socket_t *sock, const uint8_t *buf, size_t len);
z_result_t _z_receive_raweth(_z_sys_net_socket_t *sock, uint8_t *buf, size_t len,
                              _z_sys_net_endpoint_t *addr, const _zp_raweth_whitelist_t *wl);
z_result_t _z_close_raweth(_z_sys_net_socket_t *sock);
```

Existing BSP Ethernet drivers (LAN9118, OpenCores OpenETH) already operate at the frame level — smoltcp wraps them. The raweth backend exposes this API directly to zenoh-pico, bypassing the IP stack.

**Architecture** (raw Ethernet, no IP stack):

```
┌──────────────────────────────────────────┐
│  Application                             │
│  nano-ros node                           │
├──────────────────────────────────────────┤
│  zenoh-pico (raweth transport)           │
│  - MAC addressing, EtherType filtering   │
│  - Optional VLAN tagging                 │
├──────────────────────────────────────────┤
│  BSP Ethernet Driver (link-raweth)    │
│  - Raw frame TX/RX                       │
│  - No IP stack needed                    │
├──────────────────────────────────────────┤
│  Hardware (LAN9118, OpenETH, STM32 MAC)  │
└──────────────────────────────────────────┘
```

**Limitations**:
- No routing (Layer 2 only — same physical network segment)
- Requires a zenohd instance on the same LAN with raweth support, or a zenoh peer also using raweth
- VLAN configuration needed for network segmentation
- Topic-to-MAC mapping must be configured statically

### Bluetooth Transport

**On all platforms**: Not available in zenoh-pico's Unix or Zephyr backends (`#error`). Only Arduino ESP32 and ESP-IDF have implementations.

**Not recommended for nano-ros** unless targeting Arduino ESP32 specifically. BLE bandwidth (~1 Mbps) and latency are poor for real-time ROS 2 traffic. Serial over UART is simpler and faster for short-range wired connections.

## Platform / Link Matrix

How each hardware target maps to platform and link crates:

### Bare-metal Platforms

| Platform Crate               | Hardware                            | Compatible Links        | Notes                                                       |
|------------------------------|-------------------------------------|-------------------------|-------------------------------------------------------------|
| `platform-qemu` (ARM)        | LAN9118 Ethernet, QEMU virtual UART | smoltcp, serial, raweth | LAN9118 driver works at frame level                         |
| `platform-esp32` (WiFi)      | WiFi (smoltcp), ESP32 UART          | smoltcp, serial         | No raweth (WiFi is IP-only)                                 |
| `platform-esp32-qemu`        | OpenETH Ethernet, QEMU virtual UART | smoltcp, serial, raweth | OpenETH driver works at frame level                         |
| `platform-stm32f4`           | STM32 MAC + PHY, STM32 USART        | smoltcp, serial, raweth | Most boards have Ethernet and USART                         |
| **New: `platform-rpi-pico`** | RP2040 UART, CYW43 WiFi             | serial                  | Could also use zenoh-pico's RPi Pico backend (new platform) |

### OS Platforms

Protocols are handled entirely by zenoh-pico's built-in backends. No link crate needed.

| Platform          | Protocols Available at Runtime              | Role                                          |
|-------------------|---------------------------------------------|-----------------------------------------------|
| `platform-zephyr` | TCP, UDP Multicast, Serial (device)         | Device tree, Kconfig, CMake build integration |
| `platform-posix`  | TCP, UDP, Serial, Raw Ethernet (Linux), TLS | N/A — OS handles everything                   |

### Potential New Platforms

Some targets don't fit neatly into the current three platforms. These would reuse zenoh-pico's existing backends rather than nano-ros's custom bare-metal layer:

| Target              | zenoh-pico Backend | Protocols            | Notes                                                            |
|---------------------|--------------------|----------------------|------------------------------------------------------------------|
| FreeRTOS + lwIP     | `freertos/lwip`    | TCP, UDP             | New `platform-freertos-lwip` feature, reuse zenoh-pico's backend |
| ESP-IDF             | `espidf`           | TCP, UDP, Serial, BT | New `platform-espidf` feature, reuse zenoh-pico's backend        |
| RPi Pico (pico-sdk) | `rpi_pico`         | TCP, UDP             | New `platform-rpi-pico` feature, reuse zenoh-pico's backend      |

These are alternatives to `platform-bare-metal` — they use zenoh-pico's built-in RTOS backends instead of nano-ros's custom smoltcp layer. The trade-off: more protocols available out of the box (zenoh-pico already implements them), but depends on the RTOS's networking stack.

## Platform / Link Crate Architecture

### Design Principle

zenoh-pico defines a standard platform API: C function symbols that every platform must implement. On POSIX, zenoh-pico's `src/system/unix/` provides implementations using POSIX sockets and pthreads. On Zephyr, `src/system/zephyr/` provides implementations using Zephyr BSD sockets and kernel threads.

On bare-metal, **nano-ros provides these same symbols** split across two crate types:

- **Platform crates** implement system symbols (memory, clock, RNG, sleep, threading)
- **Link crates** implement network symbols (TCP, serial, raw Ethernet operations)

Both crate types implement zenoh-pico's own symbols directly via `#[unsafe(no_mangle)] extern "C"`. No C shim translation layer, no custom intermediate symbols.

### Crate Structure

```
zenoh-pico-shim-sys[bare-metal]
├── build.rs: compiles zenoh-pico C library from source
│   └── Generates config header from Cargo features (link-tcp, link-serial, etc.)
├── Platform type definitions header (zenoh_bare_metal_platform.h)
│   └── Defines: _z_sys_net_socket_t, _z_sys_net_endpoint_t, z_clock_t, z_time_t
├── Generated config header (from build.rs, replaces hardcoded zenoh_generic_config.h)
│   └── Sets: Z_FEATURE_LINK_TCP, Z_FEATURE_LINK_SERIAL, etc. based on Cargo features
│   └── Always sets: Z_FEATURE_MULTI_THREAD=0, Z_FEATURE_UNICAST_TRANSPORT=1
├── zenoh_shim C API (c/shim/zenoh_shim.c)
│   └── Simplified wrapper: zenoh_shim_init, zenoh_shim_open, zenoh_shim_declare_publisher,
│       zenoh_shim_declare_subscriber, zenoh_shim_put, zenoh_shim_spin_once, etc.
│       These are nano-ros's OWN symbols (not zenoh-pico symbols) providing a simpler
│       C API over zenoh-pico's complex session/publisher/subscriber management.
├── Does NOT implement any zenoh-pico platform symbols (z_malloc, _z_open_tcp, etc.)
└── Platform symbols resolved at link time from platform + link crates

platform-qemu (Rust crate, hardware-specific)
├── Implements system symbols: z_malloc, z_clock_now, z_random_u32, z_sleep_ms, ...
├── Implements threading stubs: _z_task_*, _z_mutex_*, _z_condvar_* (no-ops)
├── Implements socket helpers: _z_socket_close, _z_socket_wait_event,
│   _z_socket_accept, _z_socket_set_non_blocking
│   (These are transport-independent on bare-metal: _z_socket_close just resets
│    the handle struct, _z_socket_wait_event just polls, others are no-ops.
│    The protocol-specific close _z_close_tcp/_z_close_serial is a separate
│    code path called by zenoh-pico's link layer via function pointers.)
├── Provides: libc stubs (strlen, memcpy, strtoul, ...)
├── Provides: hardware init (Ethernet driver, DWT cycle counter, semihosting)
├── Provides: run_node() / Node API
└── Depends on link crate via Cargo (for network polling in z_sleep_ms)

link-smoltcp (Rust crate, protocol-specific)
├── Implements TCP symbols: _z_open_tcp, _z_send_tcp, _z_read_tcp, _z_close_tcp, ...
├── Implements endpoint symbols: _z_create_endpoint_tcp, _z_free_endpoint_tcp
├── Depends on zenoh-pico-shim-sys = { features = ["link-tcp"] }
├── Contains: SmoltcpBridge (socket table, RX/TX buffers, poll logic)
├── Calls z_clock_now() via extern "C" declaration (link-time resolution)
└── Registers poll callback with platform for network-stack driving
```

### zenoh-pico Symbol Inventory

zenoh-pico declares these symbols in its headers. Every platform must provide implementations.

#### System Symbols

Declared in `zenoh-pico/include/zenoh-pico/system/common/platform.h`.

**Memory management** (always required):

| Symbol      | Signature                                 |
|-------------|-------------------------------------------|
| `z_malloc`  | `void *z_malloc(size_t size)`             |
| `z_realloc` | `void *z_realloc(void *ptr, size_t size)` |
| `z_free`    | `void z_free(void *ptr)`                  |

**Random number generation** (always required):

| Symbol          | Signature                                   |
|-----------------|---------------------------------------------|
| `z_random_u8`   | `uint8_t z_random_u8(void)`                 |
| `z_random_u16`  | `uint16_t z_random_u16(void)`               |
| `z_random_u32`  | `uint32_t z_random_u32(void)`               |
| `z_random_u64`  | `uint64_t z_random_u64(void)`               |
| `z_random_fill` | `void z_random_fill(void *buf, size_t len)` |

**Clock** (always required):

| Symbol               | Signature                                                           |
|----------------------|---------------------------------------------------------------------|
| `z_clock_now`        | `z_clock_t z_clock_now(void)`                                       |
| `z_clock_elapsed_us` | `unsigned long z_clock_elapsed_us(z_clock_t *time)`                 |
| `z_clock_elapsed_ms` | `unsigned long z_clock_elapsed_ms(z_clock_t *time)`                 |
| `z_clock_elapsed_s`  | `unsigned long z_clock_elapsed_s(z_clock_t *time)`                  |
| `z_clock_advance_us` | `void z_clock_advance_us(z_clock_t *clock, unsigned long duration)` |
| `z_clock_advance_ms` | `void z_clock_advance_ms(z_clock_t *clock, unsigned long duration)` |
| `z_clock_advance_s`  | `void z_clock_advance_s(z_clock_t *clock, unsigned long duration)`  |

**Time** (always required):

| Symbol                    | Signature                                                              |
|---------------------------|------------------------------------------------------------------------|
| `z_time_now`              | `z_time_t z_time_now(void)`                                            |
| `z_time_now_as_str`       | `const char *z_time_now_as_str(char *const buf, unsigned long buflen)` |
| `z_time_elapsed_us`       | `unsigned long z_time_elapsed_us(z_time_t *time)`                      |
| `z_time_elapsed_ms`       | `unsigned long z_time_elapsed_ms(z_time_t *time)`                      |
| `z_time_elapsed_s`        | `unsigned long z_time_elapsed_s(z_time_t *time)`                       |
| `_z_get_time_since_epoch` | `z_result_t _z_get_time_since_epoch(_z_time_since_epoch *t)`           |

**Sleep** (always required):

| Symbol       | Signature                            |
|--------------|--------------------------------------|
| `z_sleep_us` | `z_result_t z_sleep_us(size_t time)` |
| `z_sleep_ms` | `z_result_t z_sleep_ms(size_t time)` |
| `z_sleep_s`  | `z_result_t z_sleep_s(size_t time)`  |

**Socket helpers** (always required):

On POSIX and Zephyr, these live in `network.c` because they use OS socket APIs (`close()`, `select()`). On bare-metal, they are **transport-independent** — provided by the platform crate:
- `_z_socket_close`: just resets `{ _handle = -1; _connected = false; }`. Does NOT call `_z_close_tcp()` — that's a separate code path called by zenoh-pico's link layer via function pointers.
- `_z_socket_wait_event`: calls the network poll callback once. Transport-agnostic.
- `_z_socket_accept`: returns error (bare-metal is client-only).
- `_z_socket_set_non_blocking`: no-op (bare-metal is always non-blocking).

This allows multiple link crates to be linked without duplicate symbol conflicts.

| Symbol                       | Signature                                                                              |
|------------------------------|----------------------------------------------------------------------------------------|
| `_z_socket_set_non_blocking` | `z_result_t _z_socket_set_non_blocking(const _z_sys_net_socket_t *sock)`               |
| `_z_socket_accept`           | `z_result_t _z_socket_accept(const _z_sys_net_socket_t *in, _z_sys_net_socket_t *out)` |
| `_z_socket_close`            | `void _z_socket_close(_z_sys_net_socket_t *sock)`                                      |
| `_z_socket_wait_event`       | `z_result_t _z_socket_wait_event(void *peers, _z_mutex_rec_t *mutex)`                  |

**Threading** (guarded by `Z_FEATURE_MULTI_THREAD == 1`, stubs when 0):

| Symbol                  | Signature                                                                                        |
|-------------------------|--------------------------------------------------------------------------------------------------|
| `_z_task_init`          | `z_result_t _z_task_init(_z_task_t *task, z_task_attr_t *attr, void *(*fun)(void *), void *arg)` |
| `_z_task_join`          | `z_result_t _z_task_join(_z_task_t *task)`                                                       |
| `_z_task_detach`        | `z_result_t _z_task_detach(_z_task_t *task)`                                                     |
| `_z_task_cancel`        | `z_result_t _z_task_cancel(_z_task_t *task)`                                                     |
| `_z_task_exit`          | `void _z_task_exit(void)`                                                                        |
| `_z_task_free`          | `void _z_task_free(_z_task_t **task)`                                                            |
| `_z_mutex_init`         | `z_result_t _z_mutex_init(_z_mutex_t *m)`                                                        |
| `_z_mutex_drop`         | `z_result_t _z_mutex_drop(_z_mutex_t *m)`                                                        |
| `_z_mutex_lock`         | `z_result_t _z_mutex_lock(_z_mutex_t *m)`                                                        |
| `_z_mutex_try_lock`     | `z_result_t _z_mutex_try_lock(_z_mutex_t *m)`                                                    |
| `_z_mutex_unlock`       | `z_result_t _z_mutex_unlock(_z_mutex_t *m)`                                                      |
| `_z_mutex_rec_init`     | `z_result_t _z_mutex_rec_init(_z_mutex_rec_t *m)`                                                |
| `_z_mutex_rec_drop`     | `z_result_t _z_mutex_rec_drop(_z_mutex_rec_t *m)`                                                |
| `_z_mutex_rec_lock`     | `z_result_t _z_mutex_rec_lock(_z_mutex_rec_t *m)`                                                |
| `_z_mutex_rec_try_lock` | `z_result_t _z_mutex_rec_try_lock(_z_mutex_rec_t *m)`                                            |
| `_z_mutex_rec_unlock`   | `z_result_t _z_mutex_rec_unlock(_z_mutex_rec_t *m)`                                              |
| `_z_condvar_init`       | `z_result_t _z_condvar_init(_z_condvar_t *cv)`                                                   |
| `_z_condvar_drop`       | `z_result_t _z_condvar_drop(_z_condvar_t *cv)`                                                   |
| `_z_condvar_signal`     | `z_result_t _z_condvar_signal(_z_condvar_t *cv)`                                                 |
| `_z_condvar_signal_all` | `z_result_t _z_condvar_signal_all(_z_condvar_t *cv)`                                             |
| `_z_condvar_wait`       | `z_result_t _z_condvar_wait(_z_condvar_t *cv, _z_mutex_t *m)`                                    |
| `_z_condvar_wait_until` | `z_result_t _z_condvar_wait_until(_z_condvar_t *cv, _z_mutex_t *m, const z_clock_t *abstime)`    |

#### Network Symbols — TCP

Declared in `zenoh-pico/include/zenoh-pico/system/link/tcp.h`. Guarded by `Z_FEATURE_LINK_TCP == 1`.

| Symbol                   | Signature                                                                                              |
|--------------------------|--------------------------------------------------------------------------------------------------------|
| `_z_create_endpoint_tcp` | `z_result_t _z_create_endpoint_tcp(_z_sys_net_endpoint_t *ep, const char *s_addr, const char *s_port)` |
| `_z_free_endpoint_tcp`   | `void _z_free_endpoint_tcp(_z_sys_net_endpoint_t *ep)`                                                 |
| `_z_open_tcp`            | `z_result_t _z_open_tcp(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout)`    |
| `_z_listen_tcp`          | `z_result_t _z_listen_tcp(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep)`                 |
| `_z_close_tcp`           | `void _z_close_tcp(_z_sys_net_socket_t *sock)`                                                         |
| `_z_read_tcp`            | `size_t _z_read_tcp(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len)`                         |
| `_z_read_exact_tcp`      | `size_t _z_read_exact_tcp(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len)`                   |
| `_z_send_tcp`            | `size_t _z_send_tcp(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len)`                   |

#### Network Symbols — UDP

Declared in `zenoh-pico/include/zenoh-pico/system/link/udp.h`. Guarded by `Z_FEATURE_LINK_UDP_UNICAST` / `Z_FEATURE_LINK_UDP_MULTICAST`.

| Symbol                        | Signature                                                                                                                                                    |
|-------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `_z_create_endpoint_udp`      | `z_result_t _z_create_endpoint_udp(_z_sys_net_endpoint_t *ep, const char *s_addr, const char *s_port)`                                                       |
| `_z_free_endpoint_udp`        | `void _z_free_endpoint_udp(_z_sys_net_endpoint_t *ep)`                                                                                                       |
| `_z_open_udp_unicast`         | `z_result_t _z_open_udp_unicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout)`                                                  |
| `_z_listen_udp_unicast`       | `z_result_t _z_listen_udp_unicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout)`                                                |
| `_z_close_udp_unicast`        | `void _z_close_udp_unicast(_z_sys_net_socket_t *sock)`                                                                                                       |
| `_z_read_udp_unicast`         | `size_t _z_read_udp_unicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len)`                                                                       |
| `_z_read_exact_udp_unicast`   | `size_t _z_read_exact_udp_unicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len)`                                                                 |
| `_z_send_udp_unicast`         | `size_t _z_send_udp_unicast(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len, const _z_sys_net_endpoint_t rep)`                                |
| `_z_open_udp_multicast`       | `z_result_t _z_open_udp_multicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, _z_sys_net_endpoint_t *lep, uint32_t tout, const char *iface)` |
| `_z_listen_udp_multicast`     | `z_result_t _z_listen_udp_multicast(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, uint32_t tout, const char *iface, const char *join)`         |
| `_z_close_udp_multicast`      | `void _z_close_udp_multicast(_z_sys_net_socket_t *sr, _z_sys_net_socket_t *ss, const _z_sys_net_endpoint_t rep, const _z_sys_net_endpoint_t lep)`            |
| `_z_read_udp_multicast`       | `size_t _z_read_udp_multicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len, const _z_sys_net_endpoint_t lep, _z_slice_t *ep)`                    |
| `_z_read_exact_udp_multicast` | `size_t _z_read_exact_udp_multicast(const _z_sys_net_socket_t sock, uint8_t *ptr, size_t len, const _z_sys_net_endpoint_t lep, _z_slice_t *ep)`              |
| `_z_send_udp_multicast`       | `size_t _z_send_udp_multicast(const _z_sys_net_socket_t sock, const uint8_t *ptr, size_t len, const _z_sys_net_endpoint_t rep)`                              |

#### Network Symbols — Serial

Declared in `zenoh-pico/include/zenoh-pico/system/link/serial.h`. Guarded by `Z_FEATURE_LINK_SERIAL == 1`.

| Symbol                       | Signature                                                                                                             |
|------------------------------|-----------------------------------------------------------------------------------------------------------------------|
| `_z_open_serial_from_pins`   | `z_result_t _z_open_serial_from_pins(_z_sys_net_socket_t *sock, uint32_t txpin, uint32_t rxpin, uint32_t baudrate)`   |
| `_z_open_serial_from_dev`    | `z_result_t _z_open_serial_from_dev(_z_sys_net_socket_t *sock, char *dev, uint32_t baudrate)`                         |
| `_z_listen_serial_from_pins` | `z_result_t _z_listen_serial_from_pins(_z_sys_net_socket_t *sock, uint32_t txpin, uint32_t rxpin, uint32_t baudrate)` |
| `_z_listen_serial_from_dev`  | `z_result_t _z_listen_serial_from_dev(_z_sys_net_socket_t *sock, char *dev, uint32_t baudrate)`                       |
| `_z_close_serial`            | `void _z_close_serial(_z_sys_net_socket_t *sock)`                                                                     |
| `_z_read_serial_internal`    | `size_t _z_read_serial_internal(const _z_sys_net_socket_t sock, uint8_t *header, uint8_t *ptr, size_t len)`           |
| `_z_send_serial_internal`    | `size_t _z_send_serial_internal(const _z_sys_net_socket_t sock, uint8_t header, const uint8_t *ptr, size_t len)`      |

#### Network Symbols — Raw Ethernet

Declared in `zenoh-pico/include/zenoh-pico/system/link/raweth.h`. Guarded by `Z_FEATURE_RAWETH_TRANSPORT == 1`.

| Symbol              | Signature                                                                                                                                          |
|---------------------|----------------------------------------------------------------------------------------------------------------------------------------------------|
| `_z_open_raweth`    | `z_result_t _z_open_raweth(_z_sys_net_socket_t *sock, const char *interface)`                                                                      |
| `_z_close_raweth`   | `z_result_t _z_close_raweth(_z_sys_net_socket_t *sock)`                                                                                            |
| `_z_send_raweth`    | `size_t _z_send_raweth(const _z_sys_net_socket_t *sock, const void *buff, size_t buff_len)`                                                        |
| `_z_receive_raweth` | `size_t _z_receive_raweth(const _z_sys_net_socket_t *sock, void *buff, size_t buff_len, _z_slice_t *addr, const _zp_raweth_whitelist_array_t *wl)` |
| `_z_raweth_ntohs`   | `uint16_t _z_raweth_ntohs(uint16_t val)`                                                                                                           |
| `_z_raweth_htons`   | `uint16_t _z_raweth_htons(uint16_t val)`                                                                                                           |

#### Platform Type Definitions

Defined in the platform header (`zenoh_bare_metal_platform.h`), included by zenoh-pico during compilation.

| Type                    | Definition                                    | Purpose                                                 |
|-------------------------|-----------------------------------------------|---------------------------------------------------------|
| `z_clock_t`             | `uint64_t`                                    | Monotonic clock (milliseconds)                          |
| `z_time_t`              | `uint64_t`                                    | System time (milliseconds, same as clock on bare-metal) |
| `_z_sys_net_socket_t`   | `struct { int8_t _handle; bool _connected; }` | Socket handle + connection state                        |
| `_z_sys_net_endpoint_t` | `struct { uint8_t _ip[4]; uint16_t _port; }`  | IPv4 address + port                                     |
| `_z_task_t`             | `void *` (stub)                               | Thread handle (unused, `Z_FEATURE_MULTI_THREAD=0`)      |
| `z_task_attr_t`         | `void *` (stub)                               | Thread attributes (unused)                              |
| `_z_mutex_t`            | `void *` (stub)                               | Mutex (no-op on single-threaded)                        |
| `_z_mutex_rec_t`        | `void *` (stub)                               | Recursive mutex (no-op)                                 |
| `_z_condvar_t`          | `void *` (stub)                               | Condition variable (no-op)                              |

### Implementation Matrix

Who provides each symbol set, comparing POSIX/Zephyr (zenoh-pico self-contained) with bare-metal (nano-ros crates):

| Symbol Category                  | POSIX                        | Zephyr                         | Bare-metal                         |
|----------------------------------|------------------------------|--------------------------------|------------------------------------|
| Memory (`z_malloc`, ...)         | zenoh-pico `unix/system.c`   | zenoh-pico `zephyr/system.c`   | **platform-qemu** (Rust)           |
| Clock (`z_clock_now`, ...)       | zenoh-pico `unix/system.c`   | zenoh-pico `zephyr/system.c`   | **platform-qemu** (Rust)           |
| RNG (`z_random_*`)               | zenoh-pico `unix/system.c`   | zenoh-pico `zephyr/system.c`   | **platform-qemu** (Rust)           |
| Sleep (`z_sleep_*`)              | zenoh-pico `unix/system.c`   | zenoh-pico `zephyr/system.c`   | **platform-qemu** (Rust)           |
| Threading stubs                  | zenoh-pico `unix/system.c`   | zenoh-pico `zephyr/system.c`   | **platform-qemu** (Rust)           |
| Socket helpers (`_z_socket_*`)   | zenoh-pico `unix/network.c`  | zenoh-pico `zephyr/network.c`  | **platform crate** (Rust)          |
| TCP (`_z_open_tcp`, ...)         | zenoh-pico `unix/network.c`  | zenoh-pico `zephyr/network.c`  | **link-smoltcp** (Rust)       |
| UDP (`_z_open_udp_*`, ...)       | zenoh-pico `unix/network.c`  | zenoh-pico `zephyr/network.c`  | **link-smoltcp** (Rust)       |
| Serial (`_z_open_serial_*`, ...) | zenoh-pico `unix/network.c`  | zenoh-pico `zephyr/network.c`  | **link-serial** (Rust)        |
| Raw Ethernet                     | zenoh-pico `unix/network.c`  | N/A                            | **link-raweth** (Rust)        |
| Platform type header             | zenoh-pico `platform/unix.h` | zenoh-pico `platform/zephyr.h` | **zenoh-pico-shim-sys** (C header) |

### Cross-Dependency Resolution

Two cross-dependencies exist between platform and transport:

1. **Transport needs platform clock**: `_z_open_tcp()` uses `z_clock_now()` for connection timeouts. `_z_read_tcp()` and `_z_send_tcp()` use it for I/O timeouts.

2. **Platform needs transport poll**: `z_sleep_ms()` must poll the network stack to avoid missing packets during busy-wait. `_z_socket_wait_event()` must drive the network to make progress.

Both are resolved using **zenoh-pico's own symbols** — no custom FFI:

**Transport → Platform** (clock): Transport-smoltcp declares `z_clock_now` as an extern and calls it. At link time, the symbol resolves to platform-qemu's implementation.

```rust
// link-smoltcp/src/tcp.rs
extern "C" { fn z_clock_now() -> u64; }

#[unsafe(no_mangle)]
extern "C" fn _z_open_tcp(sock: *mut SysNetSocket, rep: SysNetEndpoint, tout: u32) -> i8 {
    // ... socket setup ...
    let start = unsafe { z_clock_now() };  // zenoh-pico symbol, from platform
    loop {
        do_poll();
        if is_connected(handle) { break; }
        if unsafe { z_clock_now() } - start > tout as u64 { return -1; }
    }
    0
}
```

**Platform → Transport** (poll): Platform-qemu depends on link-smoltcp via Cargo. The link crate exposes a Rust callback registration API (not an FFI symbol). Platform registers a poll function during init that has access to the hardware-specific Device, Interface, and SocketSet.

```rust
// platform-qemu/src/node.rs
unsafe fn network_poll() {
    let eth = &mut *(GLOBAL_DEVICE as *mut Lan9118);
    transport_smoltcp::SmoltcpBridge::poll(&mut *GLOBAL_IFACE, eth, &mut *GLOBAL_SOCKETS);
    clock::advance_clock_ms(1);
}

// During init:
transport_smoltcp::set_poll_callback(network_poll);
```

```rust
// platform-qemu implements z_sleep_ms using the registered poll
#[unsafe(no_mangle)]
extern "C" fn z_sleep_ms(time_ms: usize) -> i8 {
    let start = clock::now_ms();
    while clock::now_ms() - start < time_ms as u64 {
        unsafe { network_poll(); }
    }
    0
}
```

The poll callback is a Rust `fn()` pointer stored in the link crate — not a `#[no_mangle]` FFI symbol. The cross-dependency uses only zenoh-pico symbols (`z_clock_now`) and internal Rust APIs (`SmoltcpBridge::poll`, `set_poll_callback`).

### Current vs New: QEMU Example

**Current** (monolithic `bsp-qemu`):

```
bsp-qemu/
├── bridge.rs       # SmoltcpBridge + ALL FFI: smoltcp_alloc, smoltcp_socket_open, ...
├── buffers.rs      # TCP socket buffers
├── clock.rs        # smoltcp_clock_now_ms FFI
├── libc_stubs.rs   # strlen, memcpy, strtoul, ...
├── publisher.rs    # Typed publisher wrapper
├── subscriber.rs   # Typed subscriber wrapper
├── config.rs       # Network configuration
├── node.rs         # run_node(), smoltcp_network_poll callback
└── timing.rs       # DWT cycle counter

zenoh-pico-shim-sys/c/platform_smoltcp/
├── system.c        # C shim: z_malloc→smoltcp_alloc, z_clock_now→smoltcp_clock_now_ms
└── network.c       # C shim: _z_open_tcp→smoltcp_socket_open+smoltcp_socket_connect
```

Custom symbols: `smoltcp_alloc`, `smoltcp_realloc`, `smoltcp_free`, `smoltcp_clock_now_ms`,
`smoltcp_random_u32`, `smoltcp_poll`, `smoltcp_socket_open`, `smoltcp_socket_connect`,
`smoltcp_socket_close`, `smoltcp_socket_send`, `smoltcp_socket_recv`, etc. (20+ custom symbols)

**Proposed** (split platform + transport):

```
platform-qemu/
├── lib.rs          # z_malloc, z_random_*, z_clock_*, z_sleep_*, z_time_*
│                     _z_task_* stubs, _z_mutex_* stubs, _z_condvar_* stubs
│                     _z_socket_close, _z_socket_wait_event,
│                     _z_socket_accept, _z_socket_set_non_blocking
├── libc_stubs.rs   # strlen, memcpy, strtoul, ...
├── config.rs       # Network configuration
├── node.rs         # run_node(), poll callback registration
└── timing.rs       # DWT cycle counter

link-smoltcp/
├── lib.rs          # _z_create_endpoint_tcp, _z_free_endpoint_tcp
│                     _z_open_tcp, _z_listen_tcp, _z_close_tcp
│                     _z_read_tcp, _z_read_exact_tcp, _z_send_tcp
├── bridge.rs       # SmoltcpBridge: socket table, RX/TX buffers, poll()
└── poll.rs         # Poll callback slot (Rust fn pointer, not FFI)

zenoh-pico-shim-sys[bare-metal]/
├── build.rs generates config header from Cargo features (no C shim files)
├── Platform type header (zenoh_bare_metal_platform.h)
├── zenoh_shim C API (zenoh_shim.c — nano-ros's simplified wrapper)
└── Compiles zenoh-pico C library with feature-gated Z_FEATURE_LINK_* flags
```

Custom symbols: **none**. All FFI symbols are zenoh-pico's standard platform API.

### Multi-Link Composition

Multiple link crates can be linked for a single bare-metal target. This section explains how conflicts are avoided.

**Protocol symbol isolation.** Each link crate implements a disjoint set of zenoh-pico symbols:

| Transport Crate   | Symbols Provided                                                                                                                                                                          |
|-------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| link-smoltcp | `_z_open_tcp`, `_z_close_tcp`, `_z_send_tcp`, `_z_read_tcp`, `_z_read_exact_tcp`, `_z_listen_tcp`, `_z_create_endpoint_tcp`, `_z_free_endpoint_tcp` (+ UDP variants)                      |
| link-serial  | `_z_open_serial_from_pins`, `_z_open_serial_from_dev`, `_z_listen_serial_from_pins`, `_z_listen_serial_from_dev`, `_z_close_serial`, `_z_read_serial_internal`, `_z_send_serial_internal` |
| link-raweth  | `_z_open_raweth`, `_z_close_raweth`, `_z_send_raweth`, `_z_receive_raweth`, `_z_raweth_ntohs`, `_z_raweth_htons`                                                                          |

No overlap — each protocol has unique symbol names in zenoh-pico's API.

**Socket helpers stay in platform.** `_z_socket_close`, `_z_socket_wait_event`, `_z_socket_accept`, and `_z_socket_set_non_blocking` are provided by the platform crate. On bare-metal these are transport-independent:

- `_z_socket_close` resets the handle struct. It does **not** call `_z_close_tcp` — that's a separate code path called by zenoh-pico's link layer via function pointers.
- `_z_socket_wait_event` drives the network poll callback. Transport-agnostic.
- `_z_socket_accept` returns error (client-only).
- `_z_socket_set_non_blocking` is a no-op.

Since only the platform crate provides these symbols, there is no duplication.

**Feature flag coordination via Cargo.** zenoh-pico guards all protocol code behind `Z_FEATURE_LINK_*` compile-time flags. If a flag is disabled, zenoh-pico never compiles code calling those symbols — no linker error for missing implementations.

`zenoh-pico-shim-sys` exposes Cargo features that control these flags:

```toml
# zenoh-pico-shim-sys/Cargo.toml
[features]
bare-metal = []
link-tcp = []           # sets Z_FEATURE_LINK_TCP=1
link-udp-unicast = []   # sets Z_FEATURE_LINK_UDP_UNICAST=1
link-udp-multicast = [] # sets Z_FEATURE_LINK_UDP_MULTICAST=1
link-serial = []        # sets Z_FEATURE_LINK_SERIAL=1
link-raweth = []        # sets Z_FEATURE_RAWETH_TRANSPORT=1
```

Link crates activate the features they implement:

```toml
# link-smoltcp/Cargo.toml
[dependencies]
zenoh-pico-shim-sys = { features = ["bare-metal", "link-tcp"] }

# link-serial/Cargo.toml
[dependencies]
zenoh-pico-shim-sys = { features = ["bare-metal", "link-serial"] }
```

Cargo's feature unification merges them. When both are linked:

```toml
# User's Cargo.toml
[dependencies]
nano-ros-platform-qemu = { path = "..." }
nano-ros-link-smoltcp = { path = "..." }
nano-ros-link-serial = { path = "..." }
# → zenoh-pico-shim-sys gets: bare-metal + link-tcp + link-serial
# → zenoh-pico compiled with Z_FEATURE_LINK_TCP=1, Z_FEATURE_LINK_SERIAL=1
# → Both symbol sets required and provided
```

When only one transport is linked:

```toml
[dependencies]
nano-ros-platform-qemu = { path = "..." }
nano-ros-link-serial = { path = "..." }
# → zenoh-pico-shim-sys gets: bare-metal + link-serial
# → zenoh-pico compiled with Z_FEATURE_LINK_TCP=0, Z_FEATURE_LINK_SERIAL=1
# → Only serial symbols required and provided
# → No TCP code compiled, no missing symbols
```

**Build.rs implementation.** `zenoh-pico-shim-sys/build.rs` reads Cargo features and generates the config header:

```rust
// build.rs (simplified)
let tcp = cfg!(feature = "link-tcp") as u8;
let serial = cfg!(feature = "link-serial") as u8;
write!(config, "#define Z_FEATURE_LINK_TCP {tcp}\n");
write!(config, "#define Z_FEATURE_LINK_SERIAL {serial}\n");
```

This replaces the current hardcoded `zenoh_generic_config.h` with a generated one.

### Custom Platform for Unsupported Hardware

A user with custom hardware (e.g., W5500 SPI Ethernet) writes:

1. **Platform crate**: Implements system symbols (`z_malloc`, `z_clock_now`, etc.) for their hardware. Can use an existing platform as a template.

2. **Transport selection**: Uses `link-smoltcp` if their chip has a smoltcp driver, or writes a custom transport implementing the relevant `_z_open_*` / `_z_send_*` / `_z_read_*` symbols.

3. **Dependencies**:
   ```toml
   [dependencies]
   zenoh-pico-shim-sys = { features = ["bare-metal"] }
   nano-ros-link-smoltcp = { path = "..." }
   ```

The contract is zenoh-pico's own platform API — no nano-ros-specific FFI to learn.

## Recommended Priority

1. **Platform/transport split** — Extract platform-qemu and link-smoltcp from the monolithic bsp-qemu. Prerequisite for all other transport work.

2. **Serial on bare-metal** (`link-serial`) — Lowest effort, highest impact. Enables any MCU with a UART to participate in a zenoh network without an IP stack.

3. **Serial on POSIX/Zephyr** — Zero effort. Already works in zenoh-pico. Just needs documentation and testing with nano-ros locator configuration.

4. **UDP on bare-metal** — Extend link-smoltcp to support UDP sockets in the bridge. Enables peer mode (no router) on embedded.

5. **Raw Ethernet on bare-metal** (`link-raweth`) — Eliminates the IP stack for Ethernet-connected bare-metal devices. Existing driver crates already work at the frame level.

6. **New platform backends** (FreeRTOS, ESP-IDF, RPi Pico) — Reuse zenoh-pico's existing backends. Provides more protocols out of the box than the custom bare-metal layer.

7. **Bluetooth** — Niche. Only pursue if Arduino ESP32 BLE is a specific user requirement.

## Appendix: Current Cargo Feature Map

Current transport-related feature flags across nano-ros crates (before refactoring):

```
nano-ros (top-level)
├── zenoh          → nano-ros-node/zenoh → nano-ros-transport/zenoh
├── shim-posix     → nano-ros-node/shim-posix → ... → zenoh-pico-shim-sys/posix
├── shim-zephyr    → nano-ros-node/shim-zephyr → ... → zenoh-pico-shim-sys/zephyr
├── shim-smoltcp   → nano-ros-node/shim-smoltcp → ... → zenoh-pico-shim-sys/smoltcp
├── polling        → nano-ros-node/polling (manual poll loop)
└── rtic           → nano-ros-node/rtic (RTIC executor)

zenoh-pico-shim-sys
├── posix          → builds with zenoh-pico Unix backend (self-contained)
├── zephyr         → builds with zenoh-pico Zephyr backend (self-contained)
├── smoltcp        → builds zenoh-pico + custom C shim with smoltcp_* FFI symbols
├── system-zenohpico → uses pre-built library (ZENOH_PICO_DIR)
└── smoltcp-platform-rust → includes default smoltcp bridge implementation
```

The `shim-smoltcp` feature conflates platform (bare-metal) with backend (smoltcp). It bundles C shim files (`system.c`, `network.c`) that translate zenoh-pico symbols to custom `smoltcp_*` FFI, and the Rust bridge that implements those FFI symbols is in the BSP crate.

The proposed refactoring:
- Replaces `shim-smoltcp` with `bare-metal` (platform only, no transport)
- Removes C shim files — Rust crates implement zenoh-pico symbols directly
- Splits BSP into platform crate (system symbols) + link crate (network symbols)
- Enables `link-serial` and `link-raweth` as independent alternatives
