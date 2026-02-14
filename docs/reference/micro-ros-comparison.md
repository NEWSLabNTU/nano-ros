# Micro-ROS vs nros: Architecture Comparison & Lessons Learned

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

### nros Architecture

```
┌─────────────────────────┐     ┌──────────────────────┐
│    Microcontroller      │     │    Desktop/SBC       │
│                         │     │                      │
│  ┌───────────────────┐  │     │  ┌────────────────┐  │
│  │  User Application │  │     │  │   ROS 2 Node   │  │
│  ├───────────────────┤  │     │  └───────┬────────┘  │
│  │    nros       │  │     │          │           │
│  ├───────────────────┤  │     │    ┌─────▼─────┐     │
│  │  transport (zenoh)  │  │     │    │ rmw_zenoh │     │
│  ├───────────────────┤  │     │    └─────┬─────┘     │
│  │    zenoh-pico     │◄─┼─────┼──────────┘           │
│  ├───────────────────┤  │     │                      │
│  │ Transport (smoltcp│  │     │  ┌────────────────┐  │
│  │  / posix / UDP)   │  │     │  │    zenohd      │  │
│  └───────────────────┘  │     │  │   (router)     │  │
└─────────────────────────┘     └──────────────────────┘
```

**Key: Direct peer communication via Zenoh.** No agent process needed. The MCU participates directly in the Zenoh network, which rmw_zenoh also uses. zenohd is a lightweight router, not a protocol translator.

## Detailed Comparison

### 1. Middleware Protocol

| Aspect            | micro-ROS                    | nros                     |
|-------------------|------------------------------|------------------------------|
| **Protocol**      | DDS-XRCE (binary, compact)   | Zenoh (native)               |
| **Bridge needed** | Yes (micro-ROS Agent)        | No (direct zenoh-pico)       |
| **Discovery**     | Agent-mediated (centralized) | Zenoh router (peer-capable)  |
| **Latency**       | Agent adds hop + translation | Direct peer communication    |
| **Complexity**    | 6+ software layers           | 3-4 layers                   |
| **Transport**     | Serial, UDP, TCP, custom     | UDP (posix, smoltcp, zephyr) |

**Lesson:** nros's agent-less design is simpler to deploy and has lower latency. However, micro-ROS's serial transport is valuable for MCUs without networking — worth considering for nros.

### 2. CDR Serialization

| Aspect            | Micro-CDR                    | nros serdes           |
|-------------------|------------------------------|---------------------------|
| **Language**      | C99                          | Rust                      |
| **Size**          | ~615 lines core              | Rust proc-macro generated |
| **Buffer model**  | Static user-provided         | Static user-provided      |
| **Dynamic alloc** | None                         | None                      |
| **Endianness**    | Both, per-operation override | Both                      |
| **Fragmentation** | Callback-based multi-buffer  | Not supported             |
| **Alignment**     | XCDR-compliant               | XCDR-compliant            |
| **Quality level** | ROS 2 REP-2004 Level 1       | Not certified             |

**Lessons:**
- **Fragmentation callbacks**: Micro-CDR's `on_full_buffer` callback lets serialization span multiple small buffers. This is useful for DMA-based networking with ring buffers. nros could benefit from similar streaming serialization for large messages.
- **Per-operation endianness override**: Micro-CDR allows overriding endianness per serialize call. nros currently handles this at the buffer level, which is sufficient for ROS 2 (always little-endian).

### 3. Memory Management

| Aspect               | micro-ROS                                       | nros                        |
|----------------------|-------------------------------------------------|---------------------------------|
| **Strategy**         | Static pools, compile-time sized                | Static buffers, Rust ownership  |
| **Pool system**      | Linked-list free/allocated pools                | No pool (stack/static alloc)    |
| **Dynamic fallback** | Optional (`ALLOW_DYNAMIC_ALLOCATIONS`)          | Via `alloc` feature flag        |
| **Limits**           | CMake-time: `MAX_NODES`, `MAX_PUBLISHERS`, etc. | Rust generics / const generics  |
| **History buffers**  | `RMW_UXRCE_MAX_HISTORY` input buffer slots      | Per-subscriber buffer           |
| **Configurability**  | colcon.meta / CMake / Kconfig                   | Cargo features / const generics |

**Lessons:**
- **Compile-time entity limits**: micro-ROS's approach of configuring max entity counts (nodes, publishers, subscribers) via build system is practical for embedded. nros currently doesn't enforce such limits, relying on Rust's ownership model. Could add optional compile-time bounds for safety-critical systems.
- **Memory pool pattern**: micro-ROS's `rmw_uxrce_mempool_t` (linked-list of pre-allocated items) is a proven pattern for deterministic allocation. nros could adopt this for C API resource management.

### 4. Executor Model

| Aspect            | rclc Executor                              | nros Executor                    |
|-------------------|--------------------------------------------|--------------------------------------|
| **Language**      | C                                          | Rust (with C API)                    |
| **Handle types**  | Sub, Timer, Service, Client, Action, Guard | Sub, Timer, Service, Guard           |
| **Max handles**   | User-specified at init                     | `NANO_ROS_EXECUTOR_MAX_HANDLES` (16) |
| **Trigger modes** | any, all, always, one, custom              | any (implicit)                       |
| **Semantics**     | RCLCPP + LET (Logical Execution Time)      | RCLCPP + LET                         |
| **Periodic spin** | `spin_period()`, `spin_one_period()`       | `spin_once()`, `spin()`              |
| **Invocation**    | `ON_NEW_DATA`, `ALWAYS`                    | `ON_NEW_DATA`, `ALWAYS`              |

**Lessons:**
- **Trigger conditions**: rclc's custom trigger functions (execute only when ALL handles ready, or specific handle ready) are useful for sensor fusion. nros could add this.
- **Action handles**: rclc's executor natively supports action client/server handles. nros already has action support but should ensure executor integration matches.
- **`spin_period()`**: rclc has explicit periodic spin with precise timing. nros has `spin_once()` but could add `spin_period()` for fixed-rate control loops.

### 5. Zephyr Integration

| Aspect                | micro-ROS Zephyr Module        | nros BSP Zephyr       |
|-----------------------|--------------------------------|---------------------------|
| **Integration**       | Zephyr module (module.yml)     | Zephyr module + C BSP     |
| **Build system**      | CMake + Makefile + colcon      | CMake + Cargo             |
| **Transport**         | Serial (UART), Serial-USB, UDP | UDP (Zephyr sockets)      |
| **Configuration**     | Kconfig menus                  | Kconfig + Cargo features  |
| **Thread model**      | Single-threaded, cooperative   | Single-threaded           |
| **Stack requirement** | 25KB main stack                | TBD                       |
| **Ring buffers**      | 2KB per direction (serial)     | Not applicable (UDP only) |

**Lessons:**
- **Serial transport**: micro-ROS's UART transport with ring buffers and interrupt-driven RX is well-tested and widely used. nros only supports UDP currently. Adding serial transport would greatly expand MCU support (many boards lack Ethernet/WiFi).
- **USB-CDC transport**: micro-ROS supports USB CDC-ACM with DTR handshaking. Very useful for development.
- **Kconfig integration**: micro-ROS's Kconfig menus for entity limits, transport selection, and tuning are excellent UX. nros could improve its Kconfig integration.
- **WiFi configuration**: micro-ROS's Zephyr module has Kconfig entries for WiFi SSID/password. Practical touch.

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

**nros (Rust, ~30 lines for publisher):**
```rust
let context = Context::from_env()?;
let mut executor = context.create_basic_executor();
let mut node = executor.create_node("talker".namespace("/demo"))?;
let publisher = node.create_publisher::<Int32>("chatter")?;
executor.spin(|| { publisher.publish(&msg); });
```

**nros (C API, rclc-compatible, ~80 lines):**
```c
nano_ros_support_t support;
nano_ros_support_init(&support, &context);

nros_node_t node;
nros_node_init(&node, &support, "my_node", "/ns");

nano_ros_publisher_t publisher;
nano_ros_publisher_init(&publisher, &node, "topic", "std_msgs/msg/Int32", ...);
```

**Lessons:**
- nros's Rust API is significantly more ergonomic (type-safe generics, RAII, builder pattern).
- nros's C API already mirrors rclc conventions, which is good for migration.
- micro-ROS's `RCCHECK()` / `RCSOFTCHECK()` macros for error handling are practical. nros C examples could adopt similar patterns.

### 7. Message Generation

| Aspect                    | micro-ROS                             | nros                               |
|---------------------------|---------------------------------------|----------------------------------------|
| **Tool**                  | rosidl + colcon (standard ROS 2)      | `cargo nano-ros generate`              |
| **Input**                 | .msg/.srv/.action files via ament     | package.xml → resolve from ROS 2       |
| **Output**                | C structs + CDR serialize/deserialize | Rust structs + `#[derive(RosMessage)]` |
| **Build integration**     | colcon workspace build                | Cargo build tool                       |
| **Dependency resolution** | ament_cmake transitive deps           | Custom resolver from package.xml       |
| **Offline support**       | Requires ROS 2 workspace              | Caches resolved interfaces             |

**Lessons:**
- micro-ROS relies on the full colcon/ament toolchain for message generation, which requires a ROS 2 installation. nros's `cargo nano-ros generate` is more self-contained.
- micro-ROS's pre-compiled library approach (Arduino) ships pre-built message types. nros could consider similar pre-built packages for common message sets.

### 8. Platform Support

| Platform                  | micro-ROS         | nros      |
|---------------------------|-------------------|---------------|
| **Linux (desktop)**       | Yes               | Yes           |
| **FreeRTOS**              | Yes (primary)     | No            |
| **Zephyr**                | Yes (module)      | Yes (BSP)     |
| **NuttX**                 | Yes               | No            |
| **Arduino**               | Yes (precompiled) | No            |
| **ESP-IDF**               | Yes               | No            |
| **QEMU bare-metal**       | No                | Yes           |
| **Cortex-M (bare-metal)** | Partial           | Yes (smoltcp) |
| **STM32F4**               | Via RTOS          | Yes (BSP)     |
| **RTIC**                  | No                | Yes           |

**Lessons:**
- micro-ROS has broader RTOS support (FreeRTOS, NuttX, ESP-IDF) due to its C codebase and RTOS-friendly threading model.
- nros has unique bare-metal support (QEMU, RTIC) that micro-ROS lacks.
- Arduino support gives micro-ROS massive reach in the hobbyist/education market.

## Key Takeaways for nros

### Things We Do Better

1. **No agent requirement** — simpler deployment, lower latency, fewer moving parts.
2. **Rust safety** — memory safety without runtime overhead, no use-after-free, no buffer overflows.
3. **Type-safe API** — generics eliminate runtime type mismatches.
4. **Bare-metal first** — true `no_std` with QEMU testing, not just RTOS wrappers.
5. **Self-contained tooling** — `cargo nano-ros` doesn't require a ROS 2 workspace to run.
6. **Simpler stack** — 3-4 layers vs micro-ROS's 6+ layers (app → rclc → rcl → rmw → XRCE-DDS → Micro-CDR → transport).

### Things to Learn from micro-ROS

1. **Serial transport** — UART/USB-CDC transport enables MCUs without networking. Priority addition.
2. **Entity limit configuration** — Compile-time bounds on nodes, publishers, subscribers for safety-critical certification.
3. **Trigger conditions** — Custom executor triggers (all-ready, one-ready, custom predicate) for sensor fusion.
4. **`spin_period()`** — Explicit periodic spin for fixed-rate control loops.
5. **Buffer fragmentation** — Callback-based multi-buffer CDR serialization for large messages on small MTUs.
6. **Lifecycle nodes** — State machine (unconfigured → inactive → active → finalized) for managed nodes.
7. **Quality level certification** — REP-2004 compliance documentation for production readiness.
8. **Pre-built message packages** — Ship common message types pre-compiled for quick starts.
9. **WiFi/network Kconfig** — Zephyr integration with WiFi SSID/password in menuconfig.
10. **Delivery control** — `uxrDeliveryControl` (max_samples, max_elapsed_time, max_bytes_per_second) for bandwidth management.

### Architectural Decisions Validated

1. **Zenoh over DDS-XRCE**: Eliminates agent complexity, direct interop with rmw_zenoh nodes.
2. **Rust over C**: Dramatically less boilerplate, memory safety without runtime cost.
3. **`#[derive(RosMessage)]`**: Generates CDR serialization at compile time vs runtime type support.
4. **rclc-compatible C API**: Enables C/C++ users to migrate from micro-ROS without major rewrites.

## Priority Items for Implementation

Based on this analysis, the highest-impact items to adopt from micro-ROS:

1. **Serial/UART transport** — Opens up dozens of MCU platforms without networking hardware.
2. **Lifecycle node support** — Standard ROS 2 managed node pattern, needed for production.
3. **Executor trigger conditions** — Low effort, high value for deterministic systems.
4. **`spin_period()`** — Small API addition for fixed-rate loops.
5. **Entity limit bounds** — Optional compile-time safety for certification.
