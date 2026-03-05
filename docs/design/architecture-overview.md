# Architecture Overview

nano-ros is a lightweight ROS 2 client library for embedded real-time systems. It runs on bare-metal, FreeRTOS, NuttX, ThreadX, and Zephyr — as well as Linux/POSIX — with full `no_std` support throughout the core stack.

This document presents the overall architecture: the layered crate structure, RMW abstraction, executor model, board crates, and how everything composes at compile time.

## High-Level Layer Diagram

```mermaid
block-beta
  columns 3

  block:app:3
    A["Application Code"]
  end

  space:3

  block:facade:3
    B["nros (façade crate)"]
  end

  space:3

  block:core:3
    C["nros-node"]
    D["nros-params"]
    E["nros-core"]
  end

  space:3

  block:rmw:3
    H["nros-rmw (traits)"]
  end

  space:3

  block:backends:3
    I["nros-rmw-zenoh"]
    J["nros-rmw-xrce"]
    K["nros-rmw-cffi"]
  end

  space:3

  block:transport:3
    L["zpico-sys"]
    M["xrce-sys"]
    N["C vtable"]
  end

  app --> facade
  facade --> core
  core --> rmw
  rmw --> backends
  backends --> transport
```

Applications depend on the `nros` façade crate, which re-exports everything and enforces compile-time mutual exclusivity of feature axes. The core library stack is middleware-agnostic. Only the RMW backend crates know about specific transport protocols.

## Crate Dependency Graph

```mermaid
graph TD
    subgraph "User Code"
        APP[Application / Example]
    end

    subgraph "Façade"
        NROS["nros<br/><i>re-exports + feature gates</i>"]
    end

    subgraph "Core Library Stack"
        NODE["nros-node<br/><i>Executor, Node, handles</i>"]
        PARAMS["nros-params<br/><i>ParameterServer</i>"]
        CORE["nros-core<br/><i>RosMessage, RosService, RosAction</i>"]
        SERDES["nros-serdes<br/><i>CdrWriter, CdrReader</i>"]
        MACROS["nros-macros<br/><i>#[derive(RosMessage)]</i>"]
        RMW["nros-rmw<br/><i>Session, Publisher, Subscriber traits</i>"]
        CFFI["nros-rmw-cffi<br/><i>C vtable adapter</i>"]
        C_API["nros-c<br/><i>C FFI (rclc-style)</i>"]
    end

    subgraph "Zenoh Backend"
        RMW_Z["nros-rmw-zenoh<br/><i>ZenohSession, keyexpr, liveliness</i>"]
        ZPICO["zpico-sys<br/><i>C shim + zenoh-pico</i>"]
        ZSMOL["zpico-smoltcp<br/><i>TCP/UDP via smoltcp</i>"]
        ZPLAT["zpico-platform-*<br/><i>clock, malloc, libc stubs</i>"]
    end

    subgraph "XRCE-DDS Backend"
        RMW_X["nros-rmw-xrce<br/><i>XrceSession, entity mgmt</i>"]
        XSYS["xrce-sys<br/><i>Micro-XRCE-DDS FFI</i>"]
        XSMOL["xrce-smoltcp<br/><i>UDP via smoltcp</i>"]
    end

    subgraph "Board Crates"
        BOARD["nros-mps2-an385<br/>nros-esp32<br/>nros-stm32f4<br/>nros-threadx-*<br/>nros-nuttx-*"]
    end

    subgraph "Drivers"
        DRV["lan9118-smoltcp<br/>lan9118-lwip<br/>openeth-smoltcp<br/>virtio-net-netx"]
    end

    subgraph "Interfaces"
        IFACE["nros-rcl-interfaces<br/>nros-builtin-interfaces"]
    end

    APP --> NROS
    APP --> BOARD

    NROS --> NODE
    NROS --> CORE
    NROS --> RMW
    NROS --> PARAMS
    NROS -.->|rmw-zenoh| RMW_Z
    NROS -.->|rmw-xrce| RMW_X
    NROS -.->|rmw-cffi| CFFI

    NODE --> RMW
    NODE --> CORE
    NODE --> PARAMS
    NODE -.->|param-services| IFACE
    PARAMS --> CORE
    CORE --> SERDES
    CORE --> MACROS
    CFFI --> RMW
    C_API --> NROS

    RMW_Z --> RMW
    RMW_Z --> ZPICO
    ZPICO --> ZSMOL
    ZPICO --> ZPLAT

    RMW_X --> RMW
    RMW_X --> XSYS
    XSYS --> XSMOL

    BOARD --> NROS
    BOARD --> ZPLAT
    BOARD --> ZSMOL
    BOARD --> DRV

    style NROS fill:#1864ab,color:#fff
    style RMW fill:#c92a2a,color:#fff
    style NODE fill:#2b8a3e,color:#fff
    style RMW_Z fill:#d9480f,color:#fff
    style RMW_X fill:#d9480f,color:#fff
```

Dashed arrows indicate feature-gated optional dependencies. Solid arrows are unconditional.

## Feature Axes

nano-ros uses three orthogonal compile-time axes. Each axis is mutually exclusive, enforced by `compile_error!()` in the `nros` façade. Zero features on an axis is valid (reduced functionality).

```mermaid
graph LR
    subgraph "RMW Backend (pick one)"
        Z[rmw-zenoh]
        X[rmw-xrce]
        CF[rmw-cffi]
    end

    subgraph "Platform (pick one)"
        P1[platform-posix]
        P2[platform-zephyr]
        P3[platform-bare-metal]
        P4[platform-freertos]
        P5[platform-nuttx]
        P6[platform-threadx]
    end

    subgraph "ROS Edition (pick one)"
        R1[ros-humble]
        R2[ros-iron]
    end

    subgraph "Cross-cutting (any combination)"
        S1[std]
        S2[alloc]
        S3[safety-e2e]
        S4[param-services]
        S5[ffi-sync]
    end
```

A typical embedded configuration:

```toml
[dependencies]
nros = { features = ["rmw-zenoh", "platform-bare-metal", "ros-humble"] }
```

A desktop/test configuration:

```toml
[dependencies]
nros = { features = ["rmw-zenoh", "platform-posix", "ros-humble", "std"] }
```

## RMW Abstraction

The `nros-rmw` crate defines the middleware boundary as a trait hierarchy. All core logic (`nros-node`, `nros-c`) is generic over `S: Session` and knows nothing about Zenoh, XRCE-DDS, or any specific transport.

```mermaid
classDiagram
    class Rmw {
        <<trait>>
        +open(config: &RmwConfig) Result~Session~
    }

    class Session {
        <<trait>>
        +create_publisher(topic, qos) Result~PublisherHandle~
        +create_subscriber(topic, qos) Result~SubscriberHandle~
        +create_service_server(service) Result~ServiceServerHandle~
        +create_service_client(service) Result~ServiceClientHandle~
        +drive_io(timeout_ms)
        +close()
    }

    class Publisher {
        <<trait>>
        +publish_raw(data: &[u8])
        +publish~M: RosMessage~(msg, buf)
    }

    class Subscriber {
        <<trait>>
        +has_data() bool
        +try_recv_raw(buf) Option~usize~
        +try_recv~M~(buf) Option~M~
        +register_waker(waker: &Waker)
    }

    class ServiceServerTrait {
        <<trait>>
        +has_request() bool
        +handle_request~S: RosService~(req_buf, reply_buf)
    }

    class ServiceClientTrait {
        <<trait>>
        +send_request_raw(data)
        +try_recv_reply_raw(buf) Option~usize~
        +register_waker(waker: &Waker)
    }

    Rmw --> Session : creates
    Session --> Publisher : creates
    Session --> Subscriber : creates
    Session --> ServiceServerTrait : creates
    Session --> ServiceClientTrait : creates
```

### Zenoh Backend

`nros-rmw-zenoh` implements the RMW traits on top of zenoh-pico via a C shim (`zpico.c`). Key responsibilities:

- **Key expression formatting** — maps ROS topic/service names to Zenoh keyexprs (`<domain>/<topic>/<type>/TypeHashNotSupported`)
- **Liveliness tokens** — ROS 2 graph discovery (compatible with `rmw_zenoh_cpp`)
- **AtomicWaker** — event-driven async waking from zenoh-pico C callbacks
- **FFI reentrancy guard** (`ffi-sync` feature) — wraps zpico calls in `critical_section::with()` for mixed-priority RTOS tasks

### XRCE-DDS Backend

`nros-rmw-xrce` implements the RMW traits on top of Micro-XRCE-DDS-Client. It uses an agent-based model: a lightweight client on the MCU communicates with an agent process on a gateway host that creates DDS entities.

### C FFI Backend

`nros-rmw-cffi` provides a vtable-based adapter (`nros_rmw_vtable_t`) allowing non-Rust transports to implement the `Session` trait through a C function table. Third-party RTOS vendors can plug in their own transport without writing Rust.

## Executor and Node

The executor is the runtime core. It manages callback registration, network I/O, and dispatch — all on the stack with zero heap allocation in `no_std` mode.

```mermaid
graph LR
    subgraph REG ["Registered Entities"]
        SUB["Subscription\n+ callback"]
        SVC["Service\n+ handler"]
        TMR["Timer\n+ callback"]
        ACT["ActionServer\n+ handlers"]
        GC["GuardCondition"]
    end

    subgraph EXEC ["Executor (S, MAX_CBS, CB_ARENA)"]
        ARENA["Callback Arena\nflat byte storage\nfor handles + closures"]
        ENTRIES["Dispatch Table\noffset + fn pointers\n(type-erased)"]
        SESSION["S: Session\nnetwork connection"]
        TRIGGER["Trigger\nAny | All | One | Predicate"]
    end

    subgraph SPIN ["spin_once(timeout_ms)"]
        IO["1. session.drive_io(timeout)"]
        CHECK["2. Check has_data() per entry"]
        DISPATCH["3. Call try_process() on ready"]
    end

    SUB --> ARENA
    SVC --> ARENA
    TMR --> ARENA
    ACT --> ARENA
    GC --> ARENA

    SESSION --> IO
    TRIGGER --> CHECK
    IO --> CHECK --> DISPATCH
    ENTRIES --> DISPATCH
```

### Const-Generic Zero-Cost Opt-Out

When `MAX_CBS = 0` and `CB_ARENA = 0`, the arrays are zero-sized. This means manual-polling code (using `create_node()` + `try_recv()` without callbacks) pays zero overhead for the callback infrastructure.

### Spin Variants

| Method                                   | `no_std` | Description                                                        |
|------------------------------------------|----------|--------------------------------------------------------------------|
| `spin_once(timeout_ms)`                  | Yes      | Single iteration: drive I/O, dispatch ready callbacks              |
| `spin_one_period(period_ms, elapsed_ms)` | Yes      | Caller-timed loop (caller provides clock + sleep)                  |
| `spin_blocking(SpinOptions)`             | No       | Loop with optional timeout/max_callbacks                           |
| `spin_period(Duration)`                  | No       | Wall-clock-timed loop                                              |
| `spin_async()`                           | Yes      | Yields between iterations via `poll_fn`; works with Embassy, tokio |

### Node Factory

`Node<'a, S>` borrows the session from the executor and creates typed communication handles:

```mermaid
graph LR
    EX["Executor::open(&config)"] --> NODE["executor.create_node(name)"]
    NODE --> PUB["node.create_publisher&lt;M&gt;(topic)"]
    NODE --> SUB["node.create_subscription&lt;M&gt;(topic)"]
    NODE --> SRV["node.create_service&lt;Svc&gt;(name)"]
    NODE --> CLI["node.create_client&lt;Svc&gt;(name)"]
    NODE --> AS["node.create_action_server&lt;A&gt;(name)"]
    NODE --> AC["node.create_action_client&lt;A&gt;(name)"]

    PUB --> P["EmbeddedPublisher&lt;M&gt;"]
    SUB --> S["Subscription&lt;M&gt;"]
    SRV --> SS["EmbeddedServiceServer&lt;Svc&gt;"]
    CLI --> SC["EmbeddedServiceClient&lt;Svc&gt;"]
    AS --> ASH["ActionServer&lt;A&gt; (5 channels)"]
    AC --> ACH["ActionClient&lt;A&gt;"]
```

Handles can be used in two modes:

1. **Callback mode** — register with `executor.add_subscription(sub, |msg| { ... })`, dispatched by `spin_once()`
2. **Manual-poll mode** — call `sub.try_recv()` or `client.call()` → `Promise` directly

### Executor Semantics

Two dispatch strategies, selected at construction:

- **RclcppExecutor** (default) — interleaved dispatch; each callback sees the latest data
- **LogicalExecutionTime (LET)** — all subscriptions are pre-sampled at spin start before any callback runs; ensures deterministic snapshot semantics for safety-critical systems

### Async Integration

The executor integrates with external async runtimes (tokio, Embassy) without bundling one:

```mermaid
sequenceDiagram
    participant RT as Async Runtime<br/>(tokio / Embassy)
    participant EX as Executor::spin_async()
    participant SUB as ZenohSubscriber
    participant ZP as zenoh-pico callback

    RT->>EX: poll Future
    EX->>SUB: register_waker(cx.waker())
    SUB-->>EX: Poll::Pending
    EX-->>RT: Pending (yield)

    ZP->>SUB: data arrives (C callback)
    SUB->>SUB: AtomicWaker::wake()
    SUB-->>RT: wake task

    RT->>EX: poll Future (re-poll)
    EX->>EX: drive_io() + dispatch callbacks
    EX-->>RT: Poll::Pending (yield again)
```

`AtomicWaker` bridges C-level zenoh-pico receive callbacks to Rust `Future` waking. No `block_on` is provided — users bring their own async runtime.

## Board Crates

Board crates provide a turn-key entry point for a specific hardware + RTOS combination. They initialize hardware, set up the network stack, and expose a `run(config, closure)` API.

```mermaid
graph TD
    subgraph "Board Crate (e.g. nros-mps2-an385)"
        RUN["run(config, |config| { ... })"]
        HW["Hardware Init<br/><i>Ethernet driver, clocks</i>"]
        NET["Network Stack<br/><i>smoltcp / lwIP / NetX / NuttX sockets</i>"]
        PLAT["Platform Primitives<br/><i>zpico-platform-* symbols</i>"]
        SEED["RNG Seed<br/><i>IP-based for unique Zenoh session IDs</i>"]
    end

    subgraph "User Application"
        OPEN["Executor::open(&config)"]
        NODE2["executor.create_node(...)"]
        SPIN["executor.spin_blocking(...)"]
    end

    RUN --> HW --> NET --> PLAT --> SEED
    SEED --> OPEN --> NODE2 --> SPIN
```

### Supported Boards

| Board Crate                 | Target         | RTOS       | Network Stack | Ethernet Driver       |
|-----------------------------|----------------|------------|---------------|-----------------------|
| `nros-mps2-an385`           | QEMU Cortex-M3 | Bare-metal | smoltcp       | lan9118-smoltcp       |
| `nros-mps2-an385-freertos`  | QEMU Cortex-M3 | FreeRTOS   | lwIP          | lan9118-lwip          |
| `nros-esp32`                | ESP32-C3       | Bare-metal | smoltcp       | WiFi (esp-hal)        |
| `nros-esp32-qemu`           | QEMU ESP32-C3  | Bare-metal | smoltcp       | openeth-smoltcp       |
| `nros-stm32f4`              | STM32F4        | Bare-metal | smoltcp       | STM32 Ethernet        |
| `nros-nuttx-qemu-arm`       | QEMU Cortex-A7 | NuttX      | NuttX sockets | virtio-net (built-in) |
| `nros-threadx-qemu-riscv64` | QEMU RISC-V    | ThreadX    | NetX Duo      | virtio-net-netx       |

### Platform Primitives

Each platform provides OS-level primitives that the transport libraries (zenoh-pico, XRCE-DDS) require at link time:

| Primitive | POSIX | Bare-metal | FreeRTOS | NuttX | ThreadX | Zephyr |
|-----------|-------|------------|----------|-------|---------|--------|
| Threading | pthreads | N/A (single-threaded) | xTaskCreate | pthreads | tx_thread_create | k_thread_create |
| Mutex | pthread_mutex | spin / critical-section | xSemaphore | pthread_mutex | tx_mutex | k_mutex |
| Clock | clock_gettime | DWT / cycle counter | xTaskGetTickCount | clock_gettime | tx_time_get | k_uptime_get |
| Sleep | usleep | busy-wait | vTaskDelay | usleep | tx_thread_sleep | k_sleep |
| Network | BSD sockets | smoltcp | lwIP | BSD sockets | NetX Duo | Zephyr sockets |
| RNG | /dev/urandom | IP-seeded PRNG | IP-seeded srand() | /dev/urandom | IP-seeded | sys_rand32_get |

## C API

`nros-c` is a thin FFI wrapper over `nros-node`, following the rclc naming convention. C headers are auto-generated by cbindgen from `#[repr(C)]` types.

```mermaid
graph TD
    subgraph "C Application"
        CMAIN["main.c<br/>nros_init(), nros_create_node(), ..."]
    end

    subgraph "nros-c (staticlib)"
        CAPI["C FFI functions<br/>#[unsafe(no_mangle)]"]
        CEXEC["CExecutor = Executor&lt;RmwSession, MAX, ARENA&gt;"]
    end

    subgraph "Rust Core"
        RNODE["nros-node"]
        RNROS["nros (façade)"]
    end

    CMAIN -->|"nros_init()<br/>nros_publish()"| CAPI
    CAPI --> CEXEC
    CEXEC --> RNODE
    CAPI --> RNROS
```

The C API resolves the generic `S: Session` parameter to the concrete backend type via `nros::internals::RmwSession`. All C structs (`nros_publisher_t`, `nros_subscription_t`, etc.) are `#[repr(C)]` with `pub` fields for cbindgen visibility.

## Message Codegen

Message types are generated from `.msg`/`.srv`/`.action` files — never hand-written.

```mermaid
graph LR
    MSG[".msg / .srv / .action files<br/>(bundled in packages/codegen/interfaces/)"]
    PARSER["rosidl-parser<br/><i>logos lexer + chumsky parser</i>"]
    CODEGEN["rosidl-codegen<br/><i>askama templates</i>"]
    BINDGEN["rosidl-bindgen<br/><i>orchestrator</i>"]
    CLI["cargo nano-ros generate-rust"]
    OUTPUT["generated/<br/>Serialize + Deserialize + RosMessage impls"]

    MSG --> PARSER --> CODEGEN --> BINDGEN
    CLI --> BINDGEN --> OUTPUT
```

No ROS 2 installation is required — bundled `.msg` files in `packages/codegen/interfaces/` provide all standard message definitions. Generated crate names use the `nros-` prefix (e.g., `nros-std-msgs`).

## Data Flow: Publish

```mermaid
sequenceDiagram
    participant App as Application
    participant Pub as EmbeddedPublisher
    participant CDR as CdrWriter
    participant RMW as ZenohPublisher
    participant ZP as zenoh-pico

    App->>Pub: publish(&msg)
    Pub->>CDR: new_with_header(buf)
    Note right of CDR: writes [0x00, 0x01, 0x00, 0x00]
    Pub->>CDR: msg.serialize(&mut writer)
    CDR-->>Pub: serialized bytes
    Pub->>RMW: publish_raw(&buf[..len])
    RMW->>ZP: zpico_publish(handle, data, len)
    ZP-->>ZP: z_publisher_put(...)
```

## Data Flow: Subscribe (Callback Mode)

```mermaid
sequenceDiagram
    participant ZP as zenoh-pico
    participant SUB as ZenohSubscriber
    participant EX as Executor::spin_once()
    participant ARENA as Arena Entry
    participant CB as User Callback

    ZP->>SUB: C receive callback<br/>atomic_store(data, READY)

    Note over EX: spin_once(timeout_ms)
    EX->>SUB: session.drive_io(timeout)
    EX->>ARENA: entries[i].has_data()?
    ARENA->>SUB: has_data() → true
    EX->>ARENA: entries[i].try_process()
    ARENA->>SUB: try_recv_raw(buf) → Some(len)
    ARENA->>ARENA: CdrReader::new(buf)<br/>M::deserialize(reader)
    ARENA->>CB: callback(&msg)
```

## Formal Verification

nano-ros includes two verification approaches, both running in CI:

- **Kani** — bounded model checking (160 harnesses, ~3 min). Checks memory safety, integer overflow, and panic freedom for CDR serialization, scheduling, and protocol logic.
- **Verus** — unbounded deductive proofs (102 proofs, ~1 sec). Proves functional correctness of algorithms, CDR roundtrips, and E2E safety properties.

Verification crates live in `packages/verification/` and are excluded from the main workspace to avoid Verus limitations with closures and function pointers in production code.

## Safety Features

| Feature              | Description                                                                                   | Compile Flag                              |
|----------------------|-----------------------------------------------------------------------------------------------|-------------------------------------------|
| E2E Safety           | CRC-32/ISO-HDLC integrity + sequence tracking (AUTOSAR E2E / EN 50159)                        | `safety-e2e`                              |
| FFI Reentrancy Guard | Wraps transport FFI calls in `critical_section::with()`                                       | `ffi-sync`                                |
| LET Semantics        | Logical Execution Time — deterministic snapshot dispatch                                      | `ExecutorSemantics::LogicalExecutionTime` |
| Mutex Backends       | `sync-spin` (default), `sync-critical-section` (RTIC/Embassy), or `RefCell` (single-threaded) | `sync-spin` / `sync-critical-section`     |

## TSN (Time-Sensitive Networking)

nano-ros is designed to integrate with IEEE 802.1 TSN for deterministic real-time Ethernet in automotive and industrial deployments. TSN and nano-ros form complementary safety layers — TSN provides network-level guarantees (bounded latency, jitter, fault containment), while nano-ros's E2E protocol provides application-level guarantees (data integrity, freshness).

### Safety Layer Model

```mermaid
block-beta
  columns 1

  block:L5:1
    A["Layer 5 — Application Safety\nnano-ros heartbeat, watchdog"]
  end
  block:L4:1
    B["Layer 4 — E2E Data Safety\nCRC-32, sequence counter, freshness (safety-e2e feature)"]
  end
  block:L3:1
    C["Layer 3 — Transport\nzenoh-pico / XRCE-DDS pub/sub, QoS"]
  end
  block:L2:1
    D["Layer 2 — Network Safety (TSN)\n802.1Qbv TAS, 802.1Qav CBS, 802.1Qci PSFP, 802.1CB FRER"]
  end
  block:L1:1
    E["Layer 1 — Physical\nEthernet CRC, link integrity"]
  end

  style L4 fill:#2b8a3e,color:#fff
  style L2 fill:#1864ab,color:#fff
```

Layers 4 (E2E) and 2 (TSN) are the two safety-critical layers. Layer 4 is implemented today via the `safety-e2e` feature. Layer 2 is available through RTOS-native TSN stacks.

### TSN Standards

| Standard     | Name                                       | Guarantee                                                   |
|--------------|--------------------------------------------|-------------------------------------------------------------|
| 802.1AS-2020 | Generalized Precision Time Protocol (gPTP) | Sub-microsecond clock sync                                  |
| 802.1Qbv     | Time-Aware Shaper (TAS)                    | Hard real-time bounded latency via gate control lists       |
| 802.1Qav     | Credit-Based Shaper (CBS)                  | Statistical bounded latency (Class A: 2 ms, Class B: 50 ms) |
| 802.1Qci     | Per-Stream Filtering and Policing (PSFP)   | Ingress policing, babbling idiot protection                 |
| 802.1CB      | Frame Replication and Elimination (FRER)   | Zero-delay failover, redundant paths                        |
| 802.1Qbu     | Frame Preemption (FPE)                     | Preempt low-priority frames for express traffic             |
| 802.1DG-2025 | Automotive TSN Profile                     | OEM reference profile for in-vehicle Ethernet               |

### RTOS TSN Support

TSN capabilities are accessed through the platform's native networking stack, not through nano-ros directly. Each RTOS provides different levels of TSN support:

| RTOS     | TSN Stack             | gPTP | TAS (Qbv) | CBS (Qav) | FPE (Qbu) | Certification   |
|----------|-----------------------|------|-----------|-----------|-----------|-----------------|
| ThreadX  | NetX Duo TSN          | Yes  | Yes       | Yes       | Yes       | IEC 61508 SIL 4 |
| FreeRTOS | NXP GenAVB/TSN        | Yes  | Yes       | Yes       | No        | —               |
| Zephyr   | Native gPTP + drivers | Yes  | Partial   | Partial   | No        | —               |
| NuttX    | —                     | No   | No        | No        | No        | —               |

ThreadX + NetX Duo provides the most complete TSN support with safety certification. The NetX Duo TSN APIs (`nx_shaper_cbs_*`, `nx_shaper_tas_*`, `nx_shaper_fpe_*`) are available in `external/netxduo/tsn/`.

### TSN Hardware Platforms

| Platform                   | MCU           | TSN Features                     | RTOS Path                 |
|----------------------------|---------------|----------------------------------|---------------------------|
| NXP MIMXRT1180-EVK         | i.MX RT1180   | Integrated 5-port GbE TSN switch | FreeRTOS + GenAVB/TSN     |
| NXP FRDM-MCXE31B           | MCX E31       | 10/100M Ethernet + TSN           | ThreadX + NetX Duo        |
| TI AM243x LaunchPad        | Sitara AM243x | PRU-ICSSG with gPTP, TAS, CBS    | FreeRTOS (enet-tsn-stack) |
| Microchip SAM E70 Xplained | SAME70        | QAV (CBS) via GMAC               | Zephyr                    |

### Integration Architecture

TSN operates below the nano-ros transport layer. The RTOS network stack configures TSN shapers and filters on the Ethernet MAC, providing deterministic delivery guarantees to all traffic — including zenoh-pico sessions — without any changes to nano-ros application code.

```mermaid
graph TD
    subgraph "nano-ros Application"
        APP["Executor + Node<br/>publish / subscribe"]
    end

    subgraph "nano-ros Transport"
        E2E["E2E Safety<br/>CRC-32 + sequence (optional)"]
        RMW_BACK["RMW Backend<br/>zenoh-pico / XRCE-DDS"]
    end

    subgraph "RTOS Network Stack"
        SOCK["Sockets / lwIP / NetX Duo"]
        TSN_SHAPER["TSN Shapers<br/>TAS (Qbv) · CBS (Qav) · FPE (Qbu)"]
        PTP["gPTP Clock Sync<br/>802.1AS"]
    end

    subgraph "Hardware"
        MAC["TSN-capable Ethernet MAC"]
        PHY["Ethernet PHY"]
    end

    APP --> E2E --> RMW_BACK
    RMW_BACK --> SOCK
    SOCK --> TSN_SHAPER
    PTP --> TSN_SHAPER
    TSN_SHAPER --> MAC --> PHY

    style E2E fill:#2b8a3e,color:#fff
    style TSN_SHAPER fill:#1864ab,color:#fff
    style PTP fill:#1864ab,color:#fff
```

For detailed TSN analysis, see [docs/research/tsn-safety-island-assessment.md](../research/tsn-safety-island-assessment.md) and [docs/design/zonal-vehicle-architecture.md](zonal-vehicle-architecture.md).

## Summary

```mermaid
block-beta
  columns 3

  block:row1:3
    A1["Application Code"]
  end

  block:row2:3
    B1["nros (façade) — re-exports + feature-axis gates"]
  end

  block:row3:3
    C1["nros-node\nExecutor, Node"]
    C2["nros-params\nParameterServer"]
    C3["nros-c\nC FFI (rclc)"]
  end

  block:row4:3
    D1["nros-core — RosMessage, RosService, RosAction\nnros-serdes — CDR serialization"]
  end

  block:row5:3
    E1["nros-rmw — Session, Publisher, Subscriber traits"]
  end

  block:row6:3
    F1["rmw-zenoh\nzenoh-pico"]
    F2["rmw-xrce\nXRCE-DDS"]
    F3["rmw-cffi\nC vtable"]
  end

  block:row7:3
    G1["Board Crates — HW init, network stack, run() API\nDrivers — lan9118, openeth, virtio-net"]
  end

  row1 --> row2
  row2 --> row3
  row3 --> row4
  row4 --> row5
  row5 --> row6
  row6 --> row7
```
