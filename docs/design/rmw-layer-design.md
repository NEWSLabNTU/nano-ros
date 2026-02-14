# RMW Layer Design: Middleware Abstraction for nros

## Problem Statement

### 1. Link packages are zenoh-specific

`nano-ros-link-smoltcp` depends on `nano-ros-transport-zenoh-sys` (features: `bare-metal`, `link-tcp`). It implements zenoh-pico's platform TCP symbols (`_z_open_tcp`, `_z_close_tcp`, etc.) — functions defined by zenoh-pico's C API. If nros supported a non-zenoh middleware, this link crate would be useless because those symbols are zenoh-pico specific.

The naming `nano-ros-link-*` implies a transport-agnostic "link layer" concept, but these crates are tightly coupled to zenoh-pico's C platform interface.

### 2. "transport-zenoh" naming collision

In Zenoh's own architecture, "transport" refers to the protocol layer (TCP, UDP, serial). Our `nano-ros-transport-zenoh` is more like an **RMW (Robot Middleware)** layer — it provides session management, pub/sub, and services. Calling it "transport" creates confusion with Zenoh's internal transport concept.

### 3. No middleware abstraction boundary

Currently, `nano-ros-transport/src/shim.rs` is the only middleware backend. The traits in `traits.rs` (Transport, Session, Publisher, Subscriber, etc.) define the right abstraction, but:
- `TopicInfo::to_key()` encodes rmw_zenoh keyexpr format (`<domain>/<topic>/<type>/TypeHashNotSupported`) directly in the trait module
- `ServiceInfo` and `ActionInfo` have the same zenoh-specific keyexpr logic
- The `shim` module is conditionally compiled but there's no mechanism for alternative backends

### 4. `nano-ros-` prefix is too long

The `nano-ros-` prefix (9 chars) creates unwieldy crate names, especially for the zenoh plumbing crates. `nano-ros-transport-zenoh-sys` is already 28 characters. Adding middleware-specific qualifiers makes it worse.

### 5. Platform crates mix zpico symbols and user API

Each platform crate (e.g., `nano-ros-platform-qemu`) serves a dual role:
1. **zpico system symbols** (~727 lines) — `z_malloc`, `z_clock_now`, libc stubs — no nros dependency
2. **User-facing API** (~587 lines) — `Publisher<M>`, `Subscription<M>`, `run_node()` — depends on `nros-core`

This coupling means the `zpico-` prefix doesn't fit the platform crates (they depend on nros-core), while the pure zpico symbols inside them have no nros dependency at all.

### 6. BSP crates are thin wrappers

The 4 Rust BSP crates are pure `pub use nano_ros_platform_*::*;` re-exports (8-11 lines each). They add no value — the platform crates ARE the user-facing API. These should be removed.

The Zephyr BSP (`nano-ros-bsp-zephyr`) is different — it's a 832-line C convenience library wrapping `zenoh_shim.h` for Zephyr C users. It has no nros-core dependency.

## Current Architecture

```
nano-ros-transport (core traits)
  └── shim.rs (zenoh backend, #[cfg(feature = "shim")])
        └── uses nano-ros-transport-zenoh (safe Rust wrapper)
              └── uses nano-ros-transport-zenoh-sys (C FFI + zenoh-pico)
                    ├── zenoh-pico (C library, compiled from submodule)
                    ├── zenoh_shim.c (C wrapper providing zenoh_shim_* API)
                    └── expects platform symbols at link time:
                          ├── _z_open_tcp, _z_read_tcp, etc. ← nano-ros-link-smoltcp
                          └── z_malloc, z_clock_now, etc.    ← nano-ros-platform-*

nano-ros-platform-qemu (MIXED: zpico symbols + user API)
  ├── z_malloc, z_clock_now, libc stubs      (zpico-only, 727 lines)
  ├── Publisher<M>, Subscription<M>, run_node() (nros-dependent, 587 lines)
  └── Lan9118 driver init, DWT timing          (hardware-specific)
```

## ROS 2 RMW Design (Reference)

ROS 2 uses a C function interface (`rmw.h`) as the middleware abstraction:

```
rclcpp / rclpy (user API)
  └── rcl (common logic: name resolution, parameters, actions)
        └── rmw (C function interface: ~60 functions)
              ├── rmw_fastrtps_cpp (FastDDS implementation)
              ├── rmw_cyclonedds_cpp (CycloneDDS implementation)
              └── rmw_zenoh_cpp (Zenoh implementation)
```

Key RMW functions:
- `rmw_create_publisher()` / `rmw_publish()` / `rmw_destroy_publisher()`
- `rmw_create_subscription()` / `rmw_take()` / `rmw_destroy_subscription()`
- `rmw_create_service()` / `rmw_take_request()` / `rmw_send_response()`
- `rmw_create_client()` / `rmw_send_request()` / `rmw_take_response()`
- `rmw_wait()` — blocks until any entity has data

ROS 2 selects implementations at **runtime** via `dlopen()`. This is inappropriate for embedded — compile-time selection via feature flags and trait monomorphization is correct for `no_std`.

For a detailed analysis of rmw.h's limitations for embedded and what nros adopts vs avoids, see `docs/reference/rmw-h-analysis.md`.

## Design (Implemented)

### Naming Principles

1. **Core packages use `nros-` prefix** — short (5 chars), recognizable, used for the middleware-agnostic library stack
2. **`nros-rmw-zenoh`** is the RMW glue package — bridges `nros-rmw` traits with zenoh-pico
3. **Zenoh-pico plumbing uses `zpico-` prefix** — crates with NO nros dependency: the sys crate, smoltcp TCP provider, and pure system symbol crates. These are zenoh-pico's internal implementation details
4. **User-facing platform packages use `nros-` prefix** — they depend on nros-core and provide the high-level API (`Publisher<M>`, `run_node()`, etc.)

### Dependency Chain

The target dependency chain flows top-down:

```
Platform API (nros-qemu, nros-esp32, ...)
  → nros-core (RosMessage, Serialize, Deserialize)
    → nros-rmw (RMW traits: Session, Publisher, Subscriber)
      → nros-rmw-zenoh (zenoh RMW implementation)
        → zpico-sys (C FFI + zenoh-pico library)
```

With link-time symbol resolution for embedded platforms:

```
nros-qemu (user-facing, composes everything)
  ├── nros-core                    (message types, traits)
  ├── nros-rmw                     (RMW trait interface)
  │     └── nros-rmw-zenoh         (zenoh RMW impl: shim, keyexpr, liveliness)
  │           └── zpico-sys        (C FFI, zenoh-pico library)
  ├── zpico-platform-qemu          (link-time: z_malloc, z_clock_now, libc stubs)
  ├── zpico-smoltcp                (link-time: _z_open_tcp, _z_read_tcp)
  └── lan9118-smoltcp              (hardware driver)
```

### Crate Rename & Split Table

| Current                                      | New                             | nros deps?          | Role                                                      |
|----------------------------------------------|---------------------------------|---------------------|-----------------------------------------------------------|
| **Core (middleware-agnostic)**               |                                 |                     |                                                           |
| `nros`                                   | `nros`                          | —                   | Unified re-export crate                                   |
| `nros-core`                              | `nros-core`                     | —                   | Core types, traits                                        |
| `nros-serdes`                            | `nros-serdes`                   | —                   | CDR serialization                                         |
| `nros-macros`                            | `nros-macros`                   | —                   | `#[derive(RosMessage)]` proc macros                       |
| `nros-params`                            | `nros-params`                   | —                   | Parameter server                                          |
| `nano-ros-transport`                         | **`nros-rmw`**                  | —                   | RMW abstraction traits                                    |
| `nros-node`                              | `nros-node`                     | —                   | High-level node API (desktop)                             |
| `nros-c`                                 | `nros-c`                        | —                   | C API (rclc-style)                                        |
| **RMW zenoh glue**                           |                                 |                     |                                                           |
| `nano-ros-transport-zenoh` + `shim.rs`       | **`nros-rmw-zenoh`**            | nros-rmw            | Maps zpico-sys to nros-rmw traits                         |
| **Zenoh-pico internals (NO nros deps)**      |                                 |                     |                                                           |
| `nano-ros-transport-zenoh-sys`               | **`zpico-sys`**                 | none                | FFI + C shim + zenoh-pico submodule                       |
| `nano-ros-link-smoltcp`                      | **`zpico-smoltcp`**             | none                | TCP via smoltcp (`_z_open_tcp` etc.)                      |
| split from `nano-ros-platform-qemu`          | **`zpico-platform-qemu`**       | none                | System symbols for QEMU (727 lines)                       |
| split from `nano-ros-platform-esp32`         | **`zpico-platform-esp32`**      | none                | System symbols for ESP32                                  |
| split from `nano-ros-platform-esp32-qemu`    | **`zpico-platform-esp32-qemu`** | none                | System symbols for ESP32 QEMU                             |
| split from `nano-ros-platform-stm32f4`       | **`zpico-platform-stm32f4`**    | none                | System symbols for STM32F4                                |
| `nano-ros-bsp-zephyr`                        | **`zpico-zephyr`**              | none                | Zephyr C integration (wraps zenoh_shim.h)                 |
| **User-facing platform API (nros deps)**     |                                 |                     |                                                           |
| `nano-ros-platform-qemu` (user API portion)  | **`nros-qemu`**                 | nros-core, nros-rmw | QEMU user API: `Publisher<M>`, `run_node()`               |
| `nano-ros-platform-esp32` (user API portion) | **`nros-esp32`**                | nros-core, nros-rmw | ESP32 WiFi user API                                       |
| `nano-ros-platform-esp32-qemu` (user API)    | **`nros-esp32-qemu`**           | nros-core, nros-rmw | ESP32 QEMU user API                                       |
| `nano-ros-platform-stm32f4` (user API)       | **`nros-stm32f4`**              | nros-core, nros-rmw | STM32F4 user API                                          |
| **Removed**                                  |                                 |                     |                                                           |
| `nano-ros-bsp-qemu`                          | REMOVED                         | —                   | Thin wrapper (11 lines)                                   |
| `nano-ros-bsp-esp32`                         | REMOVED                         | —                   | Thin wrapper (11 lines)                                   |
| `nano-ros-bsp-esp32-qemu`                    | REMOVED                         | —                   | Thin wrapper (11 lines)                                   |
| `nano-ros-bsp-stm32f4`                       | REMOVED                         | —                   | Thin wrapper (8 lines)                                    |
| **Verification**                             |                                 |                     |                                                           |
| `nano-ros-ghost-types`                       | `nros-ghost-types`              | none                | Ghost model types (shared between tests and Verus proofs) |
| `nano-ros-verification`                      | `nros-verification`             | nros-*              | Verus deductive proofs (excluded from workspace)          |
| **Drivers (unchanged)**                      |                                 |                     |                                                           |
| `lan9118-smoltcp`                            | `lan9118-smoltcp`               | none                | LAN9118 Ethernet driver                                   |
| `openeth-smoltcp`                            | `openeth-smoltcp`               | none                | OpenCores Ethernet driver                                 |

### Directory Layout

```
packages/
  core/                              # Core nros packages (middleware-agnostic)
    nros/                            #   Unified re-export
    nros-core/                       #   Core types, traits, lifecycle
    nros-serdes/                     #   CDR serialization
    nros-macros/                     #   Proc macros
    nros-params/                     #   Parameter server
    nros-rmw/                        #   RMW abstraction traits
    nros-node/                       #   High-level node API (desktop)
    nros-c/                          #   C API
  zpico/                             # Zenoh-pico internals (NO nros deps)
    zpico-sys/                       #   FFI + C shim + zenoh-pico submodule
    zpico-smoltcp/                   #   TCP via smoltcp for zenoh-pico
    zpico-platform-qemu/             #   z_malloc, z_clock_now, libc stubs for QEMU
    zpico-platform-esp32/            #   Same for ESP32 WiFi
    zpico-platform-esp32-qemu/       #   Same for ESP32 QEMU
    zpico-platform-stm32f4/          #   Same for STM32F4
    zpico-zephyr/                    #   Zephyr C convenience library
    nros-rmw-zenoh/                  #   RMW glue (bridges zpico ↔ nros-rmw)
  boards/                            # User-facing platform packages (nros deps)
    nros-qemu/                       #   Publisher<M>, run_node(), Config for QEMU
    nros-esp32/                      #   Same for ESP32 WiFi
    nros-esp32-qemu/                 #   Same for ESP32 QEMU
    nros-stm32f4/                    #   Same for STM32F4
  drivers/                           # Hardware drivers (middleware-agnostic)
    lan9118-smoltcp/
    openeth-smoltcp/
  interfaces/                        # Generated ROS 2 types
    rcl-interfaces/
  testing/                           # Test infrastructure
    nros-tests/
  verification/                      # Formal verification
    nros-ghost-types/                #   Ghost model types (workspace member)
    nros-verification/               #   Verus proofs (excluded from workspace)
  codegen/                           # Message binding generator
```

### Architecture Diagram

```
User code
  │
  ▼
nros-qemu / nros-esp32 / ...     # User-facing platform API
  │  Publisher<M>, Subscription<M>, run_node(), Config
  │
  ├────────────────┐
  ▼                ▼
nros-core        nros-rmw                  # Core middleware-agnostic
  │ RosMessage     │ Session, Publisher,
  │ Serialize      │ Subscriber traits
  │ Deserialize    │
  │                ▼ (compile-time feature)
  │              nros-rmw-zenoh            # RMW glue
  │                │ Implements nros-rmw traits
  │                │ keyexpr, liveliness, attachment
  │                ▼
  │              zpico-sys                 # zenoh-pico C library + FFI
  │                │ Compiles zenoh-pico
  │                │ Expects symbols at link time
  │                │
  │         ┌──────┴──────┐
  │         ▼             ▼
  │     zpico-smoltcp   zpico-platform-*   # Link-time symbol providers
  │       TCP symbols     System symbols     (NO nros deps)
  │       _z_open_tcp     z_malloc
  │       _z_read_tcp     z_clock_now
  │       _z_send_tcp     z_random_u32
  │         │             strlen, memcpy
  │         ▼
  │       smoltcp                          # IP stack
  │         │
  ▼         ▼
lan9118   Hardware (allocator, clock, RNG)
```

### Platform Crate Split Detail

Each former platform crate was split into two:

**Example: QEMU**

```
nano-ros-platform-qemu (CURRENT: 1,314 lines, mixed)
  ├── memory.rs, clock.rs, random.rs, sleep.rs,     ─┐
  │   time.rs, threading.rs, socket.rs, libc_stubs.rs │ 727 lines
  │                                                   ▼
  │                                        zpico-platform-qemu (NEW)
  │                                          No nros deps
  │                                          Only: cortex-m, smoltcp, zpico-smoltcp
  │
  ├── node.rs, publisher.rs, subscriber.rs,  ─┐
  │   config.rs, error.rs, timing.rs, lib.rs  │ 587 lines
  │                                           ▼
  │                                nros-qemu (NEW)
  │                                  Deps: nros-core, nros-rmw, nros-rmw-zenoh
  │                                  Also: zpico-platform-qemu, zpico-smoltcp
  │                                         lan9118-smoltcp (link-time)
```

**What goes where:**

| Module                                 | zpico-platform-* | nros-* |
|----------------------------------------|------------------|--------|
| `memory.rs` (z_malloc, z_free)         | ✅               |        |
| `clock.rs` (z_clock_now)               | ✅               |        |
| `random.rs` (z_random_*)               | ✅               |        |
| `sleep.rs` (z_sleep_*)                 | ✅               |        |
| `time.rs` (z_time_*)                   | ✅               |        |
| `threading.rs` (mutex/task stubs)      | ✅               |        |
| `socket.rs` (socket stubs)             | ✅               |        |
| `libc_stubs.rs` (strlen, memcpy)       | ✅               |        |
| `config.rs` (Config, network settings) |                  | ✅     |
| `error.rs` (Error enum)                |                  | ✅     |
| `timing.rs` (DWT cycle counter)        |                  | ✅     |
| `node.rs` (Node, run_node, keyexpr)    |                  | ✅     |
| `publisher.rs` (Publisher\<M\>)        |                  | ✅     |
| `subscriber.rs` (Subscription\<M\>)    |                  | ✅     |
| `lib.rs` (re-exports, prelude)         |                  | ✅     |

### Zephyr BSP Classification

The current `nano-ros-bsp-zephyr` is a **Zephyr-specific C convenience library** wrapping `zenoh_shim.h`. It:
- Adds Zephyr system integration (network interface polling, `k_timeout_t` conversion)
- Provides ROS 2 keyexpr formatting
- Wraps zenoh handle management with user-friendly API
- Has **zero nros-core dependency** (defines its own C structs)

Classification: **`zpico-zephyr`** — it's a zenoh-pico integration for Zephyr's C ecosystem, analogous to how `nros-rmw-zenoh` is the Rust integration. It sits under `packages/zpico/`.

### What `nros-rmw-zenoh` contains

This glue crate absorbs:
1. The current `nano-ros-transport-zenoh/src/lib.rs` (safe wrappers around `zpico-sys` FFI)
2. The current `nano-ros-transport/src/shim.rs` (trait implementations mapping zenoh to RMW traits)
3. Zenoh-specific keyexpr formatting (moved from `TopicInfo::to_key()` etc.)
4. RMW attachment serialization (sequence number, timestamp, GID)
5. Liveliness token management for ROS 2 discovery

### RMW Trait Changes in `nros-rmw`

Move zenoh-specific keyexpr formatting out of `TopicInfo`/`ServiceInfo`/`ActionInfo`:

```rust
// nros-rmw/src/traits.rs (middleware-agnostic)
pub struct TopicInfo<'a> {
    pub name: &'a str,
    pub type_name: &'a str,
    pub type_hash: &'a str,
    pub domain_id: u32,
}
// No to_key() or to_key_wildcard() here — those are zenoh-specific

// nros-rmw-zenoh/src/keyexpr.rs (zenoh backend)
pub fn topic_to_zenoh_keyexpr<const N: usize>(topic: &TopicInfo) -> heapless::String<N> {
    // Format: <domain_id>/<topic>/<type>/TypeHashNotSupported
}
```

Rename the top-level factory trait:

```rust
// nros-rmw/src/traits.rs

/// Factory for creating middleware sessions
pub trait Rmw {
    type Session: Session;
    fn open(config: &RmwConfig) -> Result<Self::Session, RmwError>;
}

/// Middleware session — manages connection to the middleware
pub trait Session {
    type Publisher: Publisher;
    type Subscriber: Subscriber;
    type ServiceServer: ServiceServer;
    type ServiceClient: ServiceClient;

    fn create_publisher(&mut self, topic: &TopicInfo, qos: &QosSettings)
        -> Result<Self::Publisher, RmwError>;
    fn create_subscriber(&mut self, topic: &TopicInfo, qos: &QosSettings)
        -> Result<Self::Subscriber, RmwError>;
    fn create_service_server(&mut self, service: &ServiceInfo, qos: &QosSettings)
        -> Result<Self::ServiceServer, RmwError>;
    fn create_service_client(&mut self, service: &ServiceInfo, qos: &QosSettings)
        -> Result<Self::ServiceClient, RmwError>;

    fn spin_once(&self, timeout_ms: u32) -> Result<(), RmwError>;
    fn is_open(&self) -> bool;
}

pub trait Publisher {
    fn publish_raw(&self, data: &[u8]) -> Result<(), RmwError>;
}

pub trait Subscriber {
    fn has_data(&self) -> bool;
    fn try_recv_raw<'a>(&self, buf: &'a mut [u8]) -> Result<Option<usize>, RmwError>;
}

pub trait ServiceServer {
    fn has_request(&self) -> bool;
    fn try_recv_request<'a>(&self, buf: &'a mut [u8])
        -> Result<Option<ServiceRequest<'a>>, RmwError>;
    fn send_reply(&self, sequence: i64, data: &[u8]) -> Result<(), RmwError>;
}

pub trait ServiceClient {
    fn call_raw(&self, request: &[u8], reply_buf: &mut [u8]) -> Result<usize, RmwError>;
}
```

## Complexity Assessment: Adding Alternative Middleware

### Adding XRCE-DDS (micro-ROS middleware)

XRCE-DDS (DDS for eXtremely Resource Constrained Environments) is the middleware used by micro-ROS. It uses an agent-based model: a lightweight client library runs on the MCU and communicates with an agent process on a gateway host. The agent creates DDS entities on behalf of the client and bridges to the full DDS network.

See `docs/reference/xrce-dds-analysis.md` for the full source code analysis (API, build system, platform requirements, libc needs).

**New crates needed:**

```
packages/
  xrce/                              # XRCE-DDS plumbing (like zpico/)
    xrce-sys/                        #   Direct FFI to Micro-XRCE-DDS-Client C API
    │  ├── micro-xrce-dds-client/    #     Git submodule
    │  └── micro-cdr/                #     Git submodule (Micro-CDR v2.0.2)
    xrce-smoltcp/                    #   4 UDP transport callbacks via smoltcp
    xrce-platform-qemu/              #   clock_gettime() for QEMU (1 symbol)
    nros-rmw-xrce/                   #   RMW glue: maps XRCE-DDS to nros-rmw traits
```

**Architecture (with XRCE-DDS backend):**

```
nros-qemu (user-facing, composes everything)
  ├── nros-core                    (message types, traits)
  ├── nros-rmw                     (RMW trait interface)
  │     └── nros-rmw-xrce          (XRCE-DDS RMW impl: entity lifecycle, callbacks)
  │           └── xrce-sys          (direct C FFI, no shim — 28 source files)
  │                 ├── micro-xrce-dds-client/  (submodule)
  │                 └── micro-cdr/              (submodule)
  ├── xrce-platform-qemu           (link-time: clock_gettime only)
  ├── xrce-smoltcp                 (link-time: 4 UDP transport callbacks)
  └── lan9118-smoltcp              (hardware driver)
```

**Key differences from zenoh-pico (verified from source study):**

| Aspect | zpico-sys | xrce-sys |
|--------|-----------|----------|
| C shim layer | `zenoh_shim.c` (1200+ lines) | **None** — direct FFI binding |
| Submodules | 1 (zenoh-pico) | 2 (XRCE-DDS Client + Micro-CDR) |
| C source files to compile | ~100+ | **28** |
| Platform symbols | ~55 FFI exports | **1** (`clock_gettime`) |
| Transport bridge | TCP (8+ callbacks) | UDP (4 callbacks) |
| Client heap | Required (~16KB) | **None** (fully static) |
| Config #defines | 20+ | ~8 |
| libc stubs | 14 (strlen, snprintf, strtoul, ...) | 3 (`memcpy`, `memset`, `strlen`) |

**Protocol mapping:**

| nros-rmw trait | XRCE-DDS operation |
|---|---|
| `Rmw::open(config)` | set transport callbacks + init session + create streams + create participant |
| `Session::create_publisher` | create topic + publisher + datawriter (participant shared) |
| `Publisher::publish_raw` | `uxr_buffer_topic(data, len)` — pre-serialized CDR, no double-serialization |
| `Session::create_subscriber` | create datareader + `uxr_buffer_request_data(UNLIMITED)` |
| `Subscriber::try_recv_raw` | Read from `uxrOnTopicFunc` callback buffer |
| `Session::spin_once` | `uxr_run_session_time(timeout)` |
| `ServiceServer` | Replier pattern: `uxrOnRequestFunc` callback + `uxr_buffer_reply` |
| `ServiceClient::call_raw` | `uxr_buffer_request` + `uxr_run_session_until_data` for reply |

**Build approach:**
- `build.rs` generates `config.h` from Cargo features (same pattern as zpico-sys)
- `cc::Build` compiles 28 C files directly — no CMake needed
- Feature flags: `bare-metal` / `posix` (mutually exclusive)
- Target-specific flags: ARM (`-mcpu=cortex-m3 -mthumb`), RISC-V (`-march=rv32imc`)

**Challenges:**
- Agent is mandatory (operational complexity vs zenoh's optional router)
- DDS entity hierarchy (participant > publisher > datawriter) adds internal complexity to `nros-rmw-xrce` — but hidden behind `Session::create_publisher()`
- Subscribing requires explicit `uxr_buffer_request_data` (vs zenoh's automatic data flow)
- Entity ID management — must track allocated IDs to avoid collisions

See `docs/roadmap/phase-34-rmw-abstraction.md` for implementation plan (steps 34.4-34.8).

### Adding MQTT-SN

**New crates needed:**

```
packages/
  mqtt-pico/                         # MQTT-SN plumbing (like zpico/)
    mqtt-pico-sys/                   #   FFI to MQTT-SN C library
    mqtt-pico-smoltcp/               #   TCP for MQTT via smoltcp
    mqtt-pico-platform-qemu/         #   System symbols for MQTT library
    nros-rmw-mqtt/                   #   RMW glue: maps MQTT to nros-rmw traits
```

**Changes to existing code:**
- `nros-rmw` — Add `mqtt` feature: `mqtt = ["dep:nros-rmw-mqtt"]`
- `nros-node` and `nros-c` — Minimal changes (they use abstract traits)
- User-facing platform packages — New variants or feature flags

**Protocol mapping challenges:**

| ROS 2 Concept | Zenoh Mapping                     | MQTT-SN Mapping                                |
|---------------|-----------------------------------|------------------------------------------------|
| Topic pub/sub | zenoh put/subscribe on keyexpr    | MQTT publish/subscribe on topic                |
| Services      | zenoh queryable/query             | MQTT request/response (MQTT 5.0) or two topics |
| Actions       | Decomposed into topics + services | Same decomposition                             |
| Discovery     | Liveliness tokens                 | Retained messages or separate discovery topic  |
| QoS           | zenoh reliability/congestion      | MQTT QoS 0/1/2                                 |
| Type info     | Keyexpr encoding                  | Topic name encoding                            |

The hardest part is **services** — MQTT lacks native request/reply. Requires correlation IDs and response topics (MQTT 5.0) or paired topics.

**Estimated effort:** 3-5 weeks for pub/sub, +1-2 weeks for services.

### Adding native Zenoh (not zenoh-pico)

For Linux/desktop targets, using the full Zenoh Rust library instead of zenoh-pico:

**New crates:**
1. `nros-rmw-zenoh-native` — Uses `zenoh` crate directly (pure Rust, no FFI, no C shim, no zpico crates needed)

**Changes:**
- `nros-rmw` — Add `zenoh-native` feature flag
- No zpico crates involved at all

**Estimated effort:** 1-2 weeks. The zenoh Rust API maps directly to our traits.

## Reference Documents

- **RMW trait design inspiration**: `docs/reference/rmw-h-analysis.md` — Analysis of ROS 2's `rmw.h` for embedded, 6 limitations identified, what to adopt vs avoid
- **XRCE-DDS feasibility**: `docs/reference/xrce-dds-analysis.md` — XRCE-DDS API analysis, platform requirements comparison, trait mapping
- **Rename plan**: `docs/roadmap/phase-33-crate-rename.md` — Crate rename steps
- **RMW abstraction plan**: `docs/roadmap/phase-34-rmw-abstraction.md` — Abstract RMW traits + XRCE-DDS integration

## Summary

### Three naming tiers

| Tier                  | Prefix       | nros deps? | Examples                                            |
|-----------------------|--------------|------------|-----------------------------------------------------|
| Core library          | `nros-`      | —          | `nros-core`, `nros-rmw`, `nros-node`                |
| RMW glue              | `nros-rmw-*` | nros-rmw   | `nros-rmw-zenoh`                                    |
| Zenoh-pico plumbing   | `zpico-`     | none       | `zpico-sys`, `zpico-smoltcp`, `zpico-platform-qemu` |
| User-facing platforms | `nros-`      | nros-core  | `nros-qemu`, `nros-esp32`                           |

### Key architectural decisions

1. **Split platform crates** into pure zpico symbols (no nros deps → `zpico-platform-*`) and user API (nros deps → `nros-*`)
2. **Remove 4 Rust BSP wrappers** — they're just `pub use *` re-exports
3. **Reclassify Zephyr BSP** as `zpico-zephyr` — it's a zpico C integration, not an nros package
4. **Move keyexpr formatting** from generic `TopicInfo` into `nros-rmw-zenoh`
5. **Dependency chain** flows cleanly: `nros-qemu → nros-core → nros-rmw → nros-rmw-zenoh → zpico-sys`
