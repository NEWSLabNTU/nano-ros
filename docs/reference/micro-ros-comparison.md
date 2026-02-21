# Micro-ROS vs nano-ros: Architecture Comparison

## Repositories Studied

All cloned to `external/`:

| Repository                | Description                                  |
|---------------------------|----------------------------------------------|
| `micro_ros_setup`         | Setup tool and build orchestration           |
| `micro-ros-rclc`          | C client library with deterministic executor |
| `rmw_microxrcedds`        | RMW implementation over XRCE-DDS             |
| `Micro-XRCE-DDS-Client`   | DDS-XRCE client (eProsima)                   |
| `Micro-CDR`               | CDR serialization library (eProsima)         |
| `micro-ROS-Agent`         | Bridge between micro-ROS and ROS 2 DDS       |
| `micro-ROS-demos`         | Example applications                         |
| `freertos_apps`           | FreeRTOS application examples                |
| `micro_ros_zephyr_module` | Zephyr RTOS integration                      |
| `micro_ros_arduino`       | Arduino integration                          |

## Architecture Overview

### micro-ROS Architecture

```
┌─────────────────────────┐     ┌──────────────────────┐
│    Microcontroller      │     │    Desktop/SBC       │
│                         │     │                      │
│  ┌───────────────────┐  │     │  ┌────────────────┐  │
│  │  User Application │  │     │  │   ROS 2 Node   │  │
│  ├───────────────────┤  │     │  └───────┬────────┘  │
│  │      rclc         │  │     │          │           │
│  ├───────────────────┤  │     │    ┌─────▼─────┐     │
│  │      rcl          │  │     │    │  rmw_dds  │     │
│  ├───────────────────┤  │     │    └─────┬─────┘     │
│  │ rmw_microxrcedds  │  │     │          │           │
│  ├───────────────────┤  │     │    ┌─────▼─────┐     │
│  │ XRCE-DDS Client   │  │     │    │    DDS    │     │
│  ├───────────────────┤  │     │    └─────┬─────┘     │
│  │    Micro-CDR      │  │     │          │           │
│  ├───────────────────┤  │     │  ┌───────▼────────┐  │
│  │ Transport (serial │◄─┼─────┼──│  micro-ROS     │  │
│  │  / UDP / custom)  │  │     │  │    Agent       │  │
│  └───────────────────┘  │     │  └────────────────┘  │
└─────────────────────────┘     └──────────────────────┘
```

**Key: Agent-based architecture.** The MCU runs a lightweight XRCE-DDS client that communicates with an Agent process on a more powerful machine. The Agent bridges to the full DDS network.

### nano-ros Architecture (Multi-RMW)

nano-ros supports two RMW backends — selectable at compile time via Cargo
features or Zephyr Kconfig. Both use the same user-facing API (`Executor`,
`Node`, `add_subscription()`, `spin_blocking()`).

**Zenoh backend** (agent-less, direct peer):

```
┌─────────────────────────┐     ┌──────────────────────┐
│    MCU / Linux           │     │    Desktop/SBC       │
│                         │     │                      │
│  ┌───────────────────┐  │     │  ┌────────────────┐  │
│  │  User Application │  │     │  │   ROS 2 Node   │  │
│  ├───────────────────┤  │     │  └───────┬────────┘  │
│  │  nros (Executor)  │  │     │          │           │
│  ├───────────────────┤  │     │    ┌─────▼─────┐     │
│  │  nros-rmw-zenoh   │  │     │    │ rmw_zenoh │     │
│  ├───────────────────┤  │     │    └─────┬─────┘     │
│  │   zenoh-pico      │◄─┼─────┼──────────┘           │
│  ├───────────────────┤  │     │                      │
│  │ Transport (smoltcp│  │     │  ┌────────────────┐  │
│  │  / posix / zephyr)│  │     │  │    zenohd      │  │
│  └───────────────────┘  │     │  │   (router)     │  │
└─────────────────────────┘     └──────────────────────┘
```

**XRCE-DDS backend** (agent-based, like micro-ROS):

```
┌─────────────────────────┐     ┌──────────────────────┐
│    MCU / Linux           │     │    Desktop/SBC       │
│                         │     │                      │
│  ┌───────────────────┐  │     │  ┌────────────────┐  │
│  │  User Application │  │     │  │   ROS 2 Node   │  │
│  ├───────────────────┤  │     │  └───────┬────────┘  │
│  │  nros (Executor)  │  │     │          │           │
│  ├───────────────────┤  │     │    ┌─────▼─────┐     │
│  │  nros-rmw-xrce    │  │     │    │  rmw_dds  │     │
│  ├───────────────────┤  │     │    └─────┬─────┘     │
│  │ XRCE-DDS Client   │  │     │          │           │
│  ├───────────────────┤  │     │  ┌───────▼────────┐  │
│  │ Transport (UDP /  │◄─┼─────┼──│  XRCE Agent    │  │
│  │  serial / custom) │  │     │  └────────────────┘  │
│  └───────────────────┘  │     │                      │
└─────────────────────────┘     └──────────────────────┘
```

**Key differences from micro-ROS:**
- **Zenoh backend** has no agent — direct peer communication via zenohd (router,
  not translator). Lower latency, simpler deployment.
- **XRCE backend** uses the same eProsima XRCE-DDS Client library as micro-ROS,
  but with nano-ros's Rust API instead of rclc/rcl/rmw stack (3 layers vs 6+).
- User code is **identical** regardless of backend — switch via one feature flag.

## Detailed Comparison

### 1. Middleware Protocol

| Aspect            | micro-ROS                    | nano-ros (zenoh)             | nano-ros (XRCE)                      |
|-------------------|------------------------------|------------------------------|--------------------------------------|
| **Protocol**      | DDS-XRCE                     | Zenoh (native)               | DDS-XRCE                             |
| **Bridge needed** | Yes (micro-ROS Agent)        | No (direct zenoh-pico)       | Yes (XRCE Agent)                     |
| **Discovery**     | Agent-mediated               | Zenoh router (peer-capable)  | Agent-mediated                       |
| **Latency**       | Agent adds hop + translation | Direct peer communication    | Agent adds hop + translation         |
| **Complexity**    | 6+ software layers           | 3 layers                     | 3 layers                             |
| **Transport**     | Serial, UDP, TCP, custom     | UDP (posix, smoltcp, zephyr) | UDP, serial (posix, smoltcp, zephyr) |

nano-ros's zenoh backend is agent-less and lower latency. The XRCE backend
provides parity with micro-ROS's transport model when an agent is available.

### 2. CDR Serialization

| Aspect             | Micro-CDR                    | nano-ros (nros-serdes)                        |
|--------------------|------------------------------|-----------------------------------------------|
| **Language**       | C99                          | Rust (proc-macro generated)                   |
| **Size**           | ~615 lines core              | ~500 lines + derive macro                     |
| **Buffer model**   | Static user-provided         | Static user-provided                          |
| **Dynamic alloc**  | None                         | None                                          |
| **Endianness**     | Both, per-operation override | Both                                          |
| **Large messages** | Callback-based multi-buffer  | Configurable fragmentation/reassembly buffers |
| **Alignment**      | XCDR-compliant               | XCDR-compliant                                |
| **Quality level**  | ROS 2 REP-2004 Level 1       | Not certified (Kani + Verus verified)         |

nano-ros handles large messages via configurable transport-level buffer sizes
(`ZPICO_FRAG_MAX_SIZE` up to 64KB, `XRCE_TRANSPORT_MTU` up to 4KB) rather
than CDR-level fragmentation callbacks. Buffer overflow is detected and
reported via `TransportError::MessageTooLarge`.

### 3. Memory Management

| Aspect               | micro-ROS                                       | nano-ros                                         |
|----------------------|-------------------------------------------------|--------------------------------------------------|
| **Strategy**         | Static pools, compile-time sized                | Arena-based callbacks, static buffers            |
| **Pool system**      | Linked-list free/allocated pools                | Arena allocator (`MaybeUninit` byte array)       |
| **Dynamic fallback** | Optional (`ALLOW_DYNAMIC_ALLOCATIONS`)          | Via `alloc` feature flag                         |
| **Limits**           | CMake-time: `MAX_NODES`, `MAX_PUBLISHERS`, etc. | Const generics: `Executor<S, MAX_CBS, CB_ARENA>` |
| **History buffers**  | `RMW_UXRCE_MAX_HISTORY` input buffer slots      | Per-subscriber configurable buffer               |
| **Configurability**  | colcon.meta / CMake / Kconfig                   | Const generics, env vars, Kconfig (Zephyr)       |

Both enforce compile-time entity limits. nano-ros uses Rust const generics
(`MAX_CBS`, `CB_ARENA`) for the executor and env vars (`ZPICO_MAX_PUBLISHERS`,
`XRCE_MAX_SUBSCRIBERS`, etc.) for transport-level limits. The Zephyr module
exposes these via Kconfig menus.

### 4. Executor Model

| Aspect            | rclc Executor                              | nano-ros Executor                            |
|-------------------|--------------------------------------------|----------------------------------------------|
| **Language**      | C                                          | Rust (with C API)                            |
| **Handle types**  | Sub, Timer, Service, Client, Action, Guard | Sub, Timer, Service, Client, Action          |
| **Max handles**   | User-specified at init                     | Const generic `MAX_CBS` (default 4)          |
| **Callback arena**| N/A (function pointers only)               | `CB_ARENA` bytes for closures (default 4096) |
| **Trigger modes** | any, all, always, one, custom              | any (implicit)                               |
| **Semantics**     | RCLCPP + LET (Logical Execution Time)      | RCLCPP + LET                                 |
| **Spin methods**  | `spin_period()`, `spin_one_period()`       | `spin_once()`, `spin_blocking()`, `spin_period()`, `spin_one_period()` |
| **Invocation**    | `ON_NEW_DATA`, `ALWAYS`                    | `ON_NEW_DATA`, `ALWAYS`                      |

nano-ros's executor supports arena-based closure storage (capturing state
without heap allocation) and provides both `no_std` (`spin_once`,
`spin_one_period`) and `std` (`spin_blocking`, `spin_period`) spin methods.
Trigger conditions (all-ready, custom predicate) are not yet implemented.

### 5. Zephyr Integration

| Aspect                 | micro-ROS Zephyr Module          | nano-ros Zephyr Module            |
|------------------------|----------------------------------|-----------------------------------|
| **Integration**        | Zephyr module (module.yml)       | Zephyr module (module.yml)        |
| **Build system**       | CMake + Makefile + colcon        | CMake + Cargo (Corrosion)         |
| **RMW backends**       | XRCE-DDS only                    | Zenoh + XRCE-DDS (Kconfig choice) |
| **API languages**      | C only (rclc)                    | Rust + C (Kconfig choice)         |
| **Transport**          | Serial, Serial-USB, UDP          | UDP (Zephyr sockets)              |
| **Configuration**      | Kconfig menus                    | Kconfig menus + Cargo env vars    |
| **Thread model**       | Single-threaded, cooperative     | Single-threaded                   |
| **Message generation** | colcon + rosidl (requires ROS 2) | CMake macro (bundled interfaces)  |

nano-ros's Zephyr module offers dual-backend (zenoh/XRCE) and dual-API
(Rust/C) selection via Kconfig, plus self-contained message generation that
does not require a ROS 2 installation. micro-ROS has broader transport
options (serial/USB-CDC).

### 6. API Ergonomics

**micro-ROS (C, ~125 lines for publisher + parameter server):**
```c
rcl_allocator_t allocator = rcl_get_default_allocator();
rclc_support_t support;
rclc_support_init(&support, 0, NULL, &allocator);

rcl_node_t node;
rclc_node_init_default(&node, "my_node", "", &support);

rcl_publisher_t publisher;
rclc_publisher_init_default(&publisher, &node,
    ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "topic");

rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
rclc_executor_init(&executor, &support.context, 2, &allocator);
rclc_executor_add_timer(&executor, &timer);
rclc_executor_spin(&executor);
```

**nano-ros Rust — callback-based listener (~10 lines):**
```rust
let config = ExecutorConfig::from_env().node_name("listener");
let mut executor = Executor::<_, 4, 4096>::open(&config)?;
executor.add_subscription::<Int32, _>("/chatter", |msg, _info| {
    println!("Received: {}", msg.data);
})?;
executor.spin_blocking(SpinOptions::default())?;
```

**nano-ros Rust — manual publisher (~12 lines):**
```rust
let config = ExecutorConfig::from_env().node_name("talker");
let mut executor = Executor::<_, 4, 4096>::open(&config)?;
let mut node = executor.create_node("talker")?;
let publisher = node.create_publisher::<Int32>("/chatter")?;

loop {
    publisher.publish(&Int32 { data: count })?;
    executor.spin_once(10);
    std::thread::sleep(Duration::from_secs(1));
}
```

**nano-ros C API (~20 lines):**
```c
nros_support_t support;
nros_support_init(&support, "tcp/127.0.0.1:7447", domain_id);

nros_node_t node;
nros_node_init(&node, &support, "c_talker", "/");

nros_publisher_t publisher;
nros_publisher_init(&publisher, &node, &type_support, "/topic");

nros_executor_t executor;
nros_executor_init(&executor, &support, max_handles);
nros_executor_add_timer(&executor, &timer);
nros_executor_spin_period(&executor, period_ns);
```

nano-ros's Rust API is significantly more ergonomic (type-safe generics,
closures, builder pattern, RAII). The C API mirrors rclc conventions for easy
migration.

### 7. Message Generation

| Aspect                    | micro-ROS                             | nano-ros                                   |
|---------------------------|---------------------------------------|--------------------------------------------|
| **Tool**                  | rosidl + colcon (standard ROS 2)      | `cargo nano-ros generate`                  |
| **Input**                 | .msg/.srv/.action files via ament     | package.xml + .msg/.srv/.action files      |
| **Output (Rust)**         | N/A                                   | Rust structs + `#[derive(RosMessage)]`     |
| **Output (C)**            | C structs + CDR serialize/deserialize | C structs + CDR via `nros-codegen`         |
| **Build integration**     | colcon workspace build                | Cargo build tool / CMake macro             |
| **Dependency resolution** | ament_cmake transitive deps           | ament index, then bundled fallback         |
| **Offline support**       | Requires ROS 2 workspace              | Yes (bundled std_msgs, builtin_interfaces) |

nano-ros's `cargo nano-ros generate` works without a ROS 2 installation by
shipping bundled .msg/.srv files for standard packages. The Zephyr module uses
`nros_generate_interfaces()` CMake macro for C codegen.

### 8. Platform Support

| Platform                  | micro-ROS         | nano-ros                      |
|---------------------------|-------------------|-------------------------------|
| **Linux (desktop/RPi)**   | Yes               | Yes (platform-posix)          |
| **FreeRTOS**              | Yes (primary)     | No                            |
| **Zephyr**                | Yes (module)      | Yes (module, zenoh + XRCE)    |
| **NuttX**                 | Yes               | No                            |
| **Arduino**               | Yes (precompiled) | No                            |
| **ESP-IDF**               | Yes               | No                            |
| **ESP32 (bare-metal)**    | Via ESP-IDF       | Yes (BSP + QEMU)              |
| **QEMU ARM (bare-metal)** | No                | Yes (MPS2-AN385)              |
| **Cortex-M (bare-metal)** | Partial           | Yes (smoltcp)                 |
| **STM32F4**               | Via RTOS          | Yes (BSP)                     |
| **RTIC**                  | No                | Yes                           |

**RMW backends per platform:**

| Platform       | micro-ROS | nano-ros                            |
|----------------|-----------|-------------------------------------|
| Linux          | XRCE-DDS  | Zenoh, XRCE-DDS                     |
| Zephyr         | XRCE-DDS  | Zenoh, XRCE-DDS                     |
| Bare-metal ARM | N/A       | Zenoh (smoltcp), XRCE-DDS (smoltcp) |
| ESP32-C3       | XRCE-DDS  | Zenoh (WiFi)                        |

micro-ROS has broader RTOS coverage (FreeRTOS, NuttX, ESP-IDF, Arduino).
nano-ros has unique bare-metal support and dual-backend flexibility per
platform.

### 9. Formal Verification

| Aspect               | micro-ROS        | nano-ros                                                           |
|----------------------|------------------|--------------------------------------------------------------------|
| **Bounded checking** | None             | Kani (98 harnesses)                                                |
| **Deductive proofs** | None             | Verus (92 proofs)                                                  |
| **UB detection**     | None             | Miri (on no_std crates)                                            |
| **Coverage**         | N/A              | CDR, scheduling, time, parameters, executor, buffer state machines |
| **Quality level**    | REP-2004 Level 1 | Not certified (verification-based)                                 |

nano-ros uses Kani bounded model checking and Verus unbounded deductive proofs to
verify memory safety, protocol correctness, and algorithmic properties. This
provides stronger guarantees than testing alone, though it is not a formal
certification program like REP-2004.

### 10. Additional Features

| Feature                    | micro-ROS           | nano-ros                                     |
|----------------------------|---------------------|----------------------------------------------|
| **Lifecycle nodes**        | Yes (via rclc)      | Yes (`LifecyclePollingNode`)                 |
| **Parameter services**     | Yes (via rclc)      | Yes (6 standard services)                    |
| **Actions**                | Yes (via rclc)      | Yes (server + client)                        |
| **Safety protocol**        | No                  | Yes (CRC, sequence tracking)                 |
| **Zero-copy receive**      | No                  | Yes (unstable-zenoh-api feature)             |
| **C function table (FFI)** | No                  | Yes (nros-rmw-cffi for third-party backends) |
| **Serial transport**       | Yes (UART, USB-CDC) | Partial (XRCE serial on POSIX only)          |

## Key Takeaways

### nano-ros Advantages

1. **Multi-backend architecture** — choose zenoh (agent-less) or XRCE-DDS
   (agent-based) per deployment. Same application code.
2. **Agent-less option** — zenoh backend has no bridge process, simpler
   deployment, lower latency.
3. **Rust safety** — memory safety without runtime overhead, no use-after-free,
   no buffer overflows. Closures in executor arena.
4. **Type-safe API** — generics eliminate runtime type mismatches.
5. **Bare-metal first** — true `no_std` with QEMU testing, not just RTOS
   wrappers.
6. **Self-contained tooling** — `cargo nano-ros generate` works without a ROS 2
   installation (bundled interfaces).
7. **Simpler stack** — 3 layers (app → nano-ros → transport) vs micro-ROS's 6+
   layers (app → rclc → rcl → rmw → XRCE-DDS → Micro-CDR → transport).
8. **Formal verification** — 98 Kani harnesses + 92 Verus proofs + Miri.
9. **Safety protocol** — optional CRC + sequence tracking for E2E integrity.
10. **Lightweight Linux client** — on RPi/SBC with PREEMPT_RT, uses ~1-2 MB per
    node vs rclcpp's 20+ MB, enabling real-time ROS 2 participation.

### micro-ROS Advantages

1. **Broader RTOS support** — FreeRTOS, NuttX, ESP-IDF, Arduino (massive reach).
2. **Serial/USB-CDC transport** — mature UART support with ring buffers and
   interrupt-driven RX for MCUs without networking.
3. **REP-2004 certification** — Level 1 quality standard for production.
4. **Trigger conditions** — custom executor triggers (all-ready, one-ready,
   custom predicate) for sensor fusion.
5. **Delivery control** — `uxrDeliveryControl` (max_samples, max_elapsed_time,
   max_bytes_per_second) for bandwidth management.
6. **Mature ecosystem** — extensive documentation, tutorials, community support.

### Remaining Gaps for nano-ros

Items where micro-ROS still leads and nano-ros could improve:

1. **Serial transport on embedded** — XRCE serial transport exists for POSIX
   but not for bare-metal or Zephyr. Expanding this would enable MCUs without
   networking hardware.
2. **Trigger conditions** — custom executor triggers for deterministic
   sensor-fusion patterns.
3. **Delivery control** — bandwidth-aware message throttling.
4. **RTOS coverage** — FreeRTOS and NuttX support would broaden reach.
5. **REP-2004 certification** — formal quality-level compliance documentation.

### Architectural Decisions Validated

1. **Multi-RMW over single protocol**: Having both zenoh and XRCE-DDS backends
   covers both agent-less and agent-based deployment models.
2. **Rust over C**: Dramatically less boilerplate, memory safety, closure-based
   callbacks without heap allocation.
3. **`#[derive(RosMessage)]`**: Compile-time CDR generation vs runtime type
   support introspection.
4. **rclc-compatible C API**: Enables migration from micro-ROS without major
   rewrites.
5. **`Executor<S, MAX_CBS, CB_ARENA>`**: Const-generic arena sizing eliminates
   runtime allocation while remaining configurable.
