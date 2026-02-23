# Phase 54: FreeRTOS Platform Support

**Goal**: Add `platform-freertos` as a new platform axis value, enabling nros nodes on FreeRTOS + lwIP boards (STM32, NXP, Renesas, etc.) via both zenoh-pico and XRCE-DDS backends. Validate on QEMU MPS2-AN385 (Cortex-M3 + LAN9118 Ethernet) before requiring real hardware.

**Status**: In Progress (54.1–54.11 done, 54.10 deferred, 54.12 remaining)
**Priority**: Medium
**Depends on**: Phase 42 (Extensible RMW), Phase 43 (RMW-agnostic embedded API), Phase 51 (Board crate `run()` API)

## Overview

FreeRTOS is the most widely deployed embedded RTOS. Combined with lwIP (the de facto standard embedded TCP/IP stack), it covers the majority of networked MCU platforms: STM32 (CubeMX), NXP (MCUXpresso), Renesas (FSP), TI, Raspberry Pi Pico, etc.

### Why lwIP (Not FreeRTOS+TCP)

Both zenoh-pico and Micro-XRCE-DDS-Client ship FreeRTOS support with two IP stack variants. We chose **lwIP** over FreeRTOS+TCP because:

| Criterion             | lwIP                                           | FreeRTOS+TCP                              |
|-----------------------|------------------------------------------------|-------------------------------------------|
| zenoh-pico multicast  | Full (peer discovery works)                    | **Missing** (`#error`) — client-mode only |
| TCP_NODELAY           | Yes (low-latency messaging)                    | **Missing** (Nagle adds up to 200ms)      |
| Vendor adoption       | Near-universal (ESP32, STM32, NXP, TI, Xilinx) | Niche (AWS IoT, Renesas)                  |
| Flash footprint       | 10–40 KB                                       | ~60 KB                                    |
| Latency floor         | Raw API: zero context switches                 | Always 1 context switch (IP task queue)   |
| Bare-metal fallback   | `NO_SYS=1` mode                                | Requires FreeRTOS kernel                  |
| MISRA / formal proofs | No                                             | Yes                                       |

The zenoh-pico FreeRTOS+TCP variant lacks UDP multicast (needed for zenoh scouting/peer discovery) and TCP_NODELAY (needed for low-latency ROS 2 messaging). These are blocking gaps. FreeRTOS+TCP can be added as a sub-variant later if needed.

### Why QEMU MPS2-AN385 (Not Real Hardware First)

No QEMU STM32 machine has Ethernet emulation (not mainline, not any maintained fork). However, mainline QEMU's **MPS2-AN385** (Cortex-M3 + LAN9118 Ethernet) — the same machine nano-ros already uses for bare-metal tests — supports FreeRTOS:

- **FreeRTOS has an official `CORTEX_M3_MPS2_QEMU_GCC` demo** with semihosting support
- FreeRTOS-Plus-TCP ships a LAN9118 network driver for this target
- The existing TAP bridge infrastructure (`scripts/qemu/setup-network.sh`, `launch-mps2-an385.sh`) works unchanged
- No external QEMU forks or hardware purchases needed

The gap is an **lwIP `netif` driver for LAN9118** (FreeRTOS-Plus-TCP has one, lwIP does not). This is bounded work (~200–300 LOC C) using the same MMIO registers as the existing `lan9118-smoltcp` Rust driver.

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ zenohd   │    │ QEMU     │    │ QEMU     │  │
│  │ on bridge│    │ mps2-an385│   │ mps2-an385│  │
│  │ 192.0.3.1│    │ FreeRTOS │    │ FreeRTOS │  │
│  │          │    │ +lwIP    │    │ +lwIP    │  │
│  │          │    │ +zenoh-  │    │ +zenoh-  │  │
│  │          │    │  pico    │    │  pico    │  │
│  │          │    │ talker   │    │ listener │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘  │
│       │               │               │         │
│  ─────┴───────────────┴───────────────┴─────    │
│       br-qemu     tap-qemu0      tap-qemu1      │
└─────────────────────────────────────────────────┘
```

Same topology as existing bare-metal QEMU tests — FreeRTOS+lwIP inside the guest instead of bare-metal+smoltcp.

### Existing Infrastructure

**Already implemented** (no work needed):
- zenoh-pico FreeRTOS+lwIP platform: `zenoh-pico/src/system/freertos/system.c` + `lwip/network.c`
- zenoh-pico FreeRTOS primitives: tasks, recursive mutexes, condition vars, clock, memory
- Micro-XRCE-DDS-Client FreeRTOS+TCP transport: `udp_transport_freertos_plus_tcp.c`
- nros executor is generic over `Session` trait — works with any backend
- Board crate `run()` pattern established (Phase 51)
- Feature orthogonality enforced (Phase 42)
- QEMU MPS2-AN385 infrastructure: launch scripts, TAP bridge, QemuProcess test fixture
- LAN9118 Rust driver: `packages/drivers/lan9118-smoltcp/` (register definitions reusable)

**Needs implementation** (this phase):
- `platform-freertos` feature flag chain through all crates
- zpico-sys / xrce-sys build.rs FreeRTOS compilation branches
- LAN9118 lwIP `netif` driver (C, ~200–300 LOC)
- FreeRTOS QEMU board crate (`nros-mps2-an385-freertos`)
- Examples: pubsub, service, action in both Rust and C
- Integration tests with `just test-freertos` recipe

## Architecture

```
┌──────────────────────────────────────────────────┐
│                 User Application                 │
│        Executor::open() + Node + Pub/Sub         │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│                    nros-node                      │
│          Executor<S, MAX_CBS, CB_ARENA>           │
└───────────────────────┬──────────────────────────┘
                        │ Session trait
            ┌───────────┴───────────┐
            │                       │
┌───────────┴──────────┐ ┌─────────┴──────────┐
│   nros-rmw-zenoh     │ │   nros-rmw-xrce    │
│   (ShimSession)      │ │   (XrceSession)    │
└───────────┬──────────┘ └─────────┬──────────┘
            │                       │
┌───────────┴──────────┐ ┌─────────┴──────────┐
│      zpico-sys       │ │     xrce-sys       │
│ zenoh-pico + shim    │ │ XRCE-DDS client    │
│ ┌──────────────────┐ │ │ ┌────────────────┐ │
│ │ FreeRTOS system.c│ │ │ │ FreeRTOS UDP   │ │
│ │ lwIP network.c   │ │ │ │ transport      │ │
│ └──────────────────┘ │ │ └────────────────┘ │
└───────────┬──────────┘ └─────────┬──────────┘
            │                       │
            └───────────┬───────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│              FreeRTOS + lwIP                      │
│    Tasks, Mutexes, BSD Sockets, DHCP, DNS        │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│      Board Crate (e.g. nros-mps2-an385-freertos) │
│    HAL init, Ethernet driver, lwIP netif, run()  │
└──────────────────────────────────────────────────┘
```

### Key Design Decisions

**1. Multi-threaded by default (`Z_FEATURE_MULTI_THREAD=1`)**

FreeRTOS has real tasks and mutexes. zenoh-pico's FreeRTOS platform uses `xTaskCreate()`, `xSemaphoreCreateRecursiveMutex()`, `xEventGroupCreate()` — all real primitives, not stubs. This enables zenoh-pico's background read loop and event-driven I/O, unlike bare-metal polling.

**2. lwIP socket API (POSIX-compatible)**

lwIP's socket layer (`lwip/sockets.h`) provides `socket()`, `connect()`, `select()`, `recv()`, `send()` — the same POSIX API surface used by the zenoh-pico POSIX backend. The shim's `select()`-based polling in `zenoh_shim.c` works as-is. No smoltcp bridge needed.

**3. No `zpico-platform-freertos` crate**

Unlike bare-metal platforms that need a `zpico-platform-*` crate to provide system primitives (`z_malloc`, `z_clock_now`, socket stubs), FreeRTOS+lwIP provides all of these through zenoh-pico's built-in `system.c` and `lwip/network.c`. A separate platform crate is unnecessary unless a specific board needs custom overrides.

**4. Board crate provides HAL + lwIP init, user provides Executor**

Following the Phase 51 pattern, the board crate's `run()` function:
1. Initializes HAL (clocks, GPIO, Ethernet MAC)
2. Configures lwIP (netif, IP address, DHCP)
3. Starts the FreeRTOS scheduler (if not already running)
4. Calls user closure — user creates `Executor::open()` + nodes

**5. Build integration via `cc` crate (like bare-metal), not external CMake**

zpico-sys build.rs compiles zenoh-pico sources via the `cc` crate, selecting FreeRTOS+lwIP platform files. The user provides `FREERTOS_DIR` and `LWIP_DIR` environment variables pointing to their FreeRTOS and lwIP source trees. This mirrors how ESP32 and STM32 HAL crates handle vendor SDK paths.

**6. QEMU MPS2-AN385 as first validation target**

Rather than requiring a physical STM32 board, the first FreeRTOS board crate targets QEMU's MPS2-AN385. FreeRTOS has an official demo for this machine (`CORTEX_M3_MPS2_QEMU_GCC`), and nano-ros already has the TAP bridge infrastructure. The only new C code is the LAN9118 lwIP `netif` driver. Hardware board crates (STM32F767 Nucleo, etc.) follow as a separate phase once the QEMU path is validated.

## Feature Flag Chain

```
nros/Cargo.toml:
  platform-freertos = [
      "nros-node/platform-freertos",
      "nros-rmw-zenoh?/platform-freertos",
      "nros-rmw-xrce?/platform-freertos",
  ]

nros-node/Cargo.toml:
  platform-freertos = ["nros-rmw-zenoh?/platform-freertos", "nros-rmw-xrce?/platform-freertos"]

nros-rmw-zenoh/Cargo.toml:
  platform-freertos = ["zpico-sys/freertos"]

nros-rmw-xrce/Cargo.toml:
  platform-freertos = ["xrce-sys/freertos"]

zpico-sys/Cargo.toml:
  freertos = []          # new feature (alongside posix, zephyr, bare-metal)

xrce-sys/Cargo.toml:
  freertos = []          # new feature (alongside posix, bare-metal, zephyr)

nros-c/Cargo.toml:
  platform-freertos = ["nros/platform-freertos"]
```

Mutual exclusivity: `posix`, `zephyr`, `bare-metal`, `freertos` — enforced by compile-time panic in build.rs.

## Environment Variables

| Variable              | Description                                                       | Required |
|-----------------------|-------------------------------------------------------------------|----------|
| `FREERTOS_DIR`        | Path to FreeRTOS kernel source (contains `include/`, `portable/`) | Yes      |
| `FREERTOS_PORT`       | FreeRTOS portable layer (e.g., `GCC/ARM_CM3`, `GCC/ARM_CM7`)     | Yes      |
| `LWIP_DIR`            | Path to lwIP source (contains `src/include/`, `src/core/`)        | Yes      |
| `FREERTOS_CONFIG_DIR` | Path to directory containing `FreeRTOSConfig.h` and `lwipopts.h` | Yes      |

These are only needed by zpico-sys and xrce-sys build.rs when the `freertos` feature is active.

## Work Items

- [x] 54.1 — Feature flag wiring
- [x] 54.2 — zpico-sys build.rs FreeRTOS+lwIP compilation
- [x] 54.3 — xrce-sys build.rs FreeRTOS compilation
- [x] 54.4 — Platform header and shim adjustments
- [x] 54.5 — `just setup` FreeRTOS+lwIP dependency acquisition
- [x] 54.6 — LAN9118 lwIP netif driver
- [x] 54.7 — FreeRTOS QEMU config (FreeRTOSConfig.h, lwipopts.h, linker script)
- [x] 54.8 — QEMU board crate (nros-mps2-an385-freertos)
- [x] 54.9 — Rust zenoh examples (pubsub, service, action)
- [ ] 54.10 — C zenoh examples (pubsub, service, action) — **Deferred** (nros-c Phase 49 migration in progress)
- [x] 54.11 — Integration tests + `just test-freertos` recipe
- [ ] 54.12 — Documentation

### 54.1 — Feature flag wiring

Add `platform-freertos` / `freertos` features to all crates in the chain. Update mutual exclusivity checks in `zpico-sys/build.rs` and `xrce-sys/build.rs` to include `freertos`.

**Status**: Done

**Files**: `nros/Cargo.toml`, `nros-node/Cargo.toml`, `nros-rmw-zenoh/Cargo.toml`, `nros-rmw-xrce/Cargo.toml`, `zpico-sys/Cargo.toml`, `xrce-sys/Cargo.toml`, `nros-c/Cargo.toml`, `zpico-sys/build.rs`, `xrce-sys/build.rs`

### 54.2 — zpico-sys build.rs FreeRTOS+lwIP compilation

Add a FreeRTOS compilation branch in `zpico-sys/build.rs` that:
1. Reads `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` env vars
2. Compiles zenoh-pico core sources (transport, protocol, API — same as other platforms)
3. Compiles `src/system/freertos/system.c` (threading, clock, memory)
4. Compiles `src/system/freertos/lwip/network.c` (TCP/UDP sockets)
5. Adds include paths: FreeRTOS headers, lwIP headers, user config headers
6. Generates config header with `Z_FEATURE_MULTI_THREAD=1`, `Z_FEATURE_LINK_TCP=1`, `Z_FEATURE_LINK_UDP_UNICAST=1`
7. Compiles the C shim (`zenoh_shim.c`) with FreeRTOS defines

The shim needs minimal changes: the `select()`-based polling path already works with lwIP's POSIX-compatible socket API. May need a `#ifdef ZENOH_FREERTOS` guard for FreeRTOS-specific includes or logging.

**Status**: Done

**Files**: `packages/zpico/zpico-sys/build.rs`, `packages/zpico/zpico-sys/c/shim/zenoh_shim.c`, `packages/zpico/zpico-sys/src/lib.rs`, `packages/zpico/nros-rmw-zenoh/src/lib.rs`, `packages/zpico/nros-rmw-zenoh/src/zpico.rs`

### 54.3 — xrce-sys build.rs FreeRTOS compilation

Add a FreeRTOS branch in `xrce-sys/build.rs` that:
1. Reads the same env vars as 54.2
2. Compiles XRCE-DDS client core sources
3. Skips `src/c/util/time.c` (platform crate provides `uxr_millis`/`uxr_nanos`)
4. Adds FreeRTOS + lwIP include paths
5. Optionally compiles `src/c/profile/transport/ip/udp/udp_transport_freertos_plus_tcp.c` for native FreeRTOS+TCP UDP, or uses the custom transport callback interface with lwIP sockets

**Status**: Done

**Files**: `packages/xrce/xrce-sys/build.rs`

### 54.4 — Platform header and shim adjustments

Create `zpico-sys/c/platform/zenoh_freertos_platform.h` if needed, or verify that zenoh-pico's built-in `include/zenoh-pico/system/platform/freertos/lwip.h` is sufficient.

Review `zenoh_shim.c` for any `#ifdef ZENOH_ZEPHYR` or `#ifdef ZPICO_SMOLTCP` guards that need a FreeRTOS equivalent. The lwIP socket path should mostly match the existing `else` (non-smoltcp, non-zephyr) code path that uses `select()`.

**Status**: Done

**Files**: `packages/zpico/zpico-sys/c/shim/zenoh_shim.c`

### 54.5 — `just setup` FreeRTOS+lwIP dependency acquisition

**Status**: Done

Added `just setup-freertos` recipe that:
1. Shallow-clones FreeRTOS kernel (`V11.2.0`) to `external/freertos-kernel/`
2. Shallow-clones lwIP (`STABLE-2_2_1_RELEASE`) to `external/lwip/`
3. Prints environment variable configuration for all four required vars
4. Idempotent — skips if already present, warns if tag mismatch

Pinned versions are declared as justfile variables (`FREERTOS_KERNEL_TAG`, `LWIP_TAG`) for easy bumping. The `just setup` recipe mentions `just setup-freertos` as an optional step. Added `/external/` to `.gitignore`.

**Files**: `justfile`, `.gitignore`

### 54.6 — LAN9118 lwIP netif driver

Write a C lwIP `netif` driver for the LAN9118 Ethernet controller (QEMU MPS2-AN385). This is the key new C code enabling FreeRTOS networking on QEMU.

**Location**: `packages/drivers/lan9118-lwip/` (new, C-only library)

```
packages/drivers/lan9118-lwip/
├── include/
│   └── lan9118_lwip.h     # Public API: lan9118_lwip_init(), lwIP netif callbacks
└── src/
    └── lan9118_lwip.c     # ~200–300 LOC
```

**Implementation**:
- MMIO register definitions: reuse from `packages/drivers/lan9118-smoltcp/src/regs.rs` (same hardware)
- Reference: FreeRTOS-Plus-TCP LAN9118 driver at `FreeRTOS-Plus-TCP/source/portable/NetworkInterface/MPS2_AN385/ether_lan9118/`
- lwIP `netif` interface: implement `init()`, `linkoutput()` (TX), and input polling (RX → `netif->input()`)
- Base address: `0x40200000` (MPS2-AN385 default, same as existing driver)
- Init sequence: software reset → TX FIFO config → PHY config → MAC enable (mirrors `lan9118-smoltcp`)
- Frame handling: read/write via LAN9118 TX/RX data FIFOs, check status words

**lwIP netif callbacks**:
```c
err_t lan9118_lwip_init(struct netif *netif);       // Hardware init, set linkoutput/output
err_t lan9118_lwip_output(struct netif *netif, struct pbuf *p);  // TX: pbuf → TX FIFO
void  lan9118_lwip_poll(struct netif *netif);        // RX: poll RX FIFO → netif->input()
```

Polling is appropriate for the QEMU environment. On real hardware, interrupt-driven RX would replace `lan9118_lwip_poll()`.

**Status**: Done

**Files**: `packages/drivers/lan9118-lwip/include/lan9118_lwip.h`, `packages/drivers/lan9118-lwip/src/lan9118_lwip.c`, `packages/drivers/lan9118-lwip/CMakeLists.txt`

### 54.7 — FreeRTOS QEMU config (FreeRTOSConfig.h, lwipopts.h, linker script)

Create board-specific configuration files for FreeRTOS + lwIP on QEMU MPS2-AN385.

**Location**: `packages/boards/nros-mps2-an385-freertos/config/`

```
packages/boards/nros-mps2-an385-freertos/config/
├── FreeRTOSConfig.h    # FreeRTOS kernel config
├── lwipopts.h          # lwIP stack config
└── mps2_an385.ld       # Linker script (with heap section)
```

**FreeRTOSConfig.h** key settings:
- `configCPU_CLOCK_HZ`: 25 MHz (MPS2-AN385 QEMU default)
- `configTOTAL_HEAP_SIZE`: 64 KB (zenoh-pico needs ~12–20 KB; lwIP needs ~8–16 KB)
- `configMAX_PRIORITIES`: 8
- `configMINIMAL_STACK_SIZE`: 256 words
- `configUSE_RECURSIVE_MUTEXES`: 1 (required by zenoh-pico)
- `configUSE_COUNTING_SEMAPHORES`: 1
- `configUSE_TIMERS`: 1
- `configTIMER_TASK_STACK_DEPTH`: 512 words
- Semihosting-compatible `configASSERT()` for QEMU debugging

**lwipopts.h** key settings:
- `NO_SYS`: 0 (threaded mode — requires FreeRTOS)
- `LWIP_SOCKET`: 1 (BSD socket API for zenoh-pico)
- `LWIP_TCP`: 1, `LWIP_UDP`: 1
- `LWIP_NETCONN`: 1 (needed by socket layer)
- `TCP_NODELAY`: 1 (low-latency messaging)
- `MEM_SIZE`: 16384 (lwIP heap)
- `MEMP_NUM_PBUF`: 32
- `PBUF_POOL_SIZE`: 24
- `TCP_MSS`: 1460
- `TCP_SND_BUF`: 4 * TCP_MSS
- `LWIP_NETIF_STATUS_CALLBACK`: 1

**Linker script** (`mps2_an385.ld`):
- Based on FreeRTOS `CORTEX_M3_MPS2_QEMU_GCC` demo linker script
- 4 MB RAM at 0x21000000 (QEMU MPS2-AN385 SSRAM)
- Sections: `.text`, `.data`, `.bss`, `.heap` (for FreeRTOS heap_4.c)
- Stack at end of RAM

**Status**: Done

**Files**: `packages/boards/nros-mps2-an385-freertos/config/FreeRTOSConfig.h`, `config/lwipopts.h`, `config/arch/cc.h`, `config/mps2_an385.ld`

### 54.8 — QEMU board crate (nros-mps2-an385-freertos)

Create the FreeRTOS board crate for QEMU MPS2-AN385, following the Phase 51 pattern established by `nros-mps2-an385` (bare-metal).

**Location**: `packages/boards/nros-mps2-an385-freertos/`

```
packages/boards/nros-mps2-an385-freertos/
├── Cargo.toml
├── build.rs            # Compile FreeRTOS kernel + lwIP + LAN9118 netif via cc
├── config/             # FreeRTOSConfig.h, lwipopts.h, mps2_an385.ld (from 54.7)
└── src/
    ├── lib.rs          # Re-exports, entry macro, println!, exit_success/exit_failure
    ├── config.rs       # Config builder (IP, gateway, zenoh locator, domain_id)
    ├── node.rs         # run() — FreeRTOS init, lwIP init, LAN9118 netif, scheduler start
    └── error.rs        # Board error type
```

**build.rs** responsibilities:
1. Compile FreeRTOS kernel: `tasks.c`, `queue.c`, `list.c`, `timers.c`, `event_groups.c`, `stream_buffer.c`, `port.c` (from `$FREERTOS_PORT`), `heap_4.c`
2. Compile lwIP core: `tcp.c`, `udp.c`, `ip4.c`, `pbuf.c`, `mem.c`, `memp.c`, `netif.c`, `sockets.c`, `tcpip.c`, `sys_arch.c` (FreeRTOS port)
3. Compile LAN9118 lwIP netif driver (from 54.6)
4. Include paths: FreeRTOS headers, lwIP headers, config dir, LAN9118 driver headers
5. Link the resulting static library

**`run()` function** sequence:
1. Init NVIC (interrupt priorities for FreeRTOS)
2. Init SysTick (FreeRTOS tick source)
3. Init LAN9118 hardware (MAC address, PHY config)
4. Init lwIP (`tcpip_init()`, create `netif`, set IP config)
5. Start lwIP `netif` (link up)
6. Create application FreeRTOS task (runs user closure with sufficient stack)
7. Start FreeRTOS scheduler (`vTaskStartScheduler()`)

The application task runs the user closure, which creates `Executor::open()` + nodes. The lwIP `tcpip_thread` and zenoh-pico's background read task run as separate FreeRTOS tasks.

**Config builder** mirrors `nros-mps2-an385`:
- `Config::default()` — talker preset (192.0.3.10, gateway 192.0.3.1)
- `Config::listener()` — listener preset (192.0.3.11)
- `with_mac()`, `with_ip()`, `with_gateway()`, `with_zenoh_locator()`, `with_domain_id()`

**Semihosting**: `println!` via ARM semihosting (`SYS_WRITE0`), `exit_success()`/`exit_failure()` via `SYS_EXIT` for QEMU test automation.

**Dependencies**:
- `nros` with `rmw-zenoh,platform-freertos,ros-humble`
- `cortex-m-rt` (Cortex-M runtime, entry macro)
- `cortex-m-semihosting` (QEMU output)

**Status**: Done

**Files**: `packages/boards/nros-mps2-an385-freertos/Cargo.toml`, `build.rs`, `src/lib.rs`, `src/config.rs`, `src/node.rs`, `src/error.rs`

### 54.9 — Rust zenoh examples (pubsub, service, action)

Create Rust examples for FreeRTOS on QEMU following the 4-level convention.

**Location**: `examples/qemu-arm-freertos/rust/zenoh/`

```
examples/qemu-arm-freertos/rust/zenoh/
├── talker/              # Pub: std_msgs/Int32 on /chatter
├── listener/            # Sub: std_msgs/Int32 on /chatter
├── service-server/      # Srv: example_interfaces/AddTwoInts
├── service-client/      # Cli: example_interfaces/AddTwoInts
├── action-server/       # Act: example_interfaces/Fibonacci
└── action-client/       # Act: example_interfaces/Fibonacci
```

Each example has:
```
├── Cargo.toml           # deps: nros, nros-mps2-an385-freertos, generated msg types
├── .cargo/config.toml   # target, patch.crates-io, runner (QEMU launch)
├── package.xml          # For cargo-nano-ros message generation
├── .gitignore           # /target/, /generated/
└── src/main.rs
```

**Entry point pattern** (same as bare-metal QEMU examples):
```rust
#![no_std]
#![no_main]

use nros::prelude::*;
use nros_mps2_an385_freertos::{Config, println, run};
use panic_semihosting as _;

#[nros_mps2_an385_freertos::entry]
fn main() -> ! {
    run(Config::default(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;
        // ...
        Ok::<(), NodeError>(())
    })
}
```

**Pubsub**: Talker publishes 10 `Int32` messages, listener subscribes with `try_recv()` and exits after receiving 3+ messages (matches existing bare-metal pattern).

**Service**: Server registers `AddTwoInts` handler, client sends request and verifies response. Both use `spin_once()` polling. Client exits after receiving correct response.

**Action**: Server handles `Fibonacci` goal (computes N terms), client sends goal, polls for acceptance, then polls for result. Both use `spin_once()` polling. Client verifies result matches expected Fibonacci sequence.

**Build target**: `thumbv7m-none-eabi` (Cortex-M3), `--release` for size optimization.

**Status**: Done

**Files**: `examples/qemu-arm-freertos/rust/zenoh/{talker,listener,service-server,service-client,action-server,action-client}/` — each with `Cargo.toml`, `.cargo/config.toml`, `package.xml`, `.gitignore`, `src/main.rs`

### 54.10 — C zenoh examples (pubsub, service, action)

Create C examples using `nros-c` for FreeRTOS on QEMU. These use CMake + the nros-c static library.

**Location**: `examples/qemu-arm-freertos/c/zenoh/`

```
examples/qemu-arm-freertos/c/zenoh/
├── CMakeLists.txt       # Top-level CMake (builds nros-c + all examples)
├── talker/
│   ├── CMakeLists.txt
│   └── main.c
├── listener/
│   ├── CMakeLists.txt
│   └── main.c
├── service-server/
│   ├── CMakeLists.txt
│   └── main.c
├── service-client/
│   ├── CMakeLists.txt
│   └── main.c
├── action-server/
│   ├── CMakeLists.txt
│   └── main.c
└── action-client/
    ├── CMakeLists.txt
    └── main.c
```

**Build approach**: Cross-compile `nros-c` as a static library for `thumbv7m-none-eabi` with `--features "rmw-zenoh,platform-freertos,ros-humble"`, then link C examples against it. The CMake build calls `cargo build` for `nros-c`, then compiles C sources with `arm-none-eabi-gcc` and links everything together.

Each C example follows the native C example patterns in `examples/native/c/zenoh/` but adapted for bare-metal:
- Uses `nros_executor_open()` / `nros_node_create()` / `nros_publisher_create()` C API
- Board init (LAN9118, lwIP, FreeRTOS) handled by the board crate's `run()` equivalent or a C `board_init()` function
- Output via semihosting `printf()` or ARM semihosting syscalls

**Status**: Deferred — nros-c Phase 49 migration (thin wrapper) is still in progress; C examples require stable C API.

**Files**: `examples/qemu-arm-freertos/c/zenoh/`

### 54.11 — Integration tests + `just test-freertos` recipe

Automated QEMU-based integration tests and justfile recipes.

**Test fixture**: `QemuProcess::start_mps2_an385_networked()` in `packages/testing/nros-tests/src/qemu.rs` launches MPS2-AN385 with LAN9118 TAP NIC and configurable MAC address.

**Tests** in `packages/testing/nros-tests/tests/freertos_qemu.rs`:

- **Build tests** (6): Verify `cargo build --release` succeeds for all FreeRTOS examples
- **E2E network tests** (3): `test_freertos_pubsub_e2e`, `test_freertos_service_e2e`, `test_freertos_action_e2e`
  - Each test: `require_freertos_e2e()` → start zenohd on port 7447 → boot server/listener QEMU on tap-qemu0 → wait for readiness marker → boot client/talker QEMU on tap-qemu1 → verify output markers
  - Pubsub: verifies `"Received"` count > 0
  - Service: verifies `"Response:"` count >= 4 and `"All service calls completed"`
  - Action: verifies `"Goal accepted"` and `"Action completed successfully"`
  - Skip gracefully when TAP bridge or zenohd not available

**Nextest config**: `freertos-qemu` test group with `max-threads = 1` (TAP bridge exclusive access).

**Justfile**: `just test-freertos` runs nextest with `freertos_qemu` filter.

**Status**: Done

**Files**: `packages/testing/nros-tests/src/qemu.rs`, `packages/testing/nros-tests/tests/freertos_qemu.rs`, `.config/nextest.toml`, `justfile`

### 54.12 — Documentation

- Update `CLAUDE.md`:
  - Add `nros-mps2-an385-freertos` to workspace structure under `packages/boards/`
  - Add `lan9118-lwip` to workspace structure under `packages/drivers/`
  - Add `qemu-arm-freertos` to examples list
  - Update platform backends section to include `platform-freertos`
  - Add `just test-freertos` to test groups table
  - Add `just setup-freertos` to build commands
- Update `docs/guides/getting-started.md` with FreeRTOS quick start
- Add `docs/guides/freertos-setup.md` covering:
  - FreeRTOS + lwIP source acquisition (`just setup-freertos`)
  - Environment variable configuration
  - QEMU testing workflow (`just test-freertos`)
  - Board-specific setup for real hardware (STM32CubeMX, etc.)
  - Building and flashing
- Update `docs/reference/environment-variables.md` with FreeRTOS build-time variables

**Files**: `CLAUDE.md`, `docs/guides/freertos-setup.md`, `docs/guides/getting-started.md`, `docs/reference/environment-variables.md`

## Future Extensions (Out of Scope)

- **Hardware board crate: STM32F767 Nucleo** — real hardware target with STM32 Ethernet MAC + lwIP. Follows same pattern as `nros-mps2-an385-freertos` but with `stm32f7xx-hal` and STM32-specific Ethernet driver. Requires physical Nucleo-F767ZI board (~$25).
- **FreeRTOS+TCP sub-variant**: Add once zenoh-pico upstream completes multicast and TCP_NODELAY support
- **C API on FreeRTOS**: Zephyr-style CMake module with `nros_cargo_build()` for FreeRTOS CMake projects
- **Additional boards**: NXP i.MX RT, Renesas RA, Raspberry Pi Pico W
- **XRCE-DDS on FreeRTOS**: Full XRCE example (needs Micro-XRCE-DDS Agent on the host)
- **FreeRTOS SMP**: Multi-core support (ESP32-S3, STM32H7 dual-core)

## Acceptance Criteria

- [ ] `platform-freertos` feature compiles cleanly for `thumbv7m-none-eabi`
- [ ] Mutual exclusivity enforced: enabling `platform-freertos` + `platform-posix` panics at build time
- [ ] Feature flag chain works: `nros` → `nros-node` → `nros-rmw-zenoh` → `zpico-sys` all forward correctly
- [ ] LAN9118 lwIP `netif` driver initializes and exchanges frames on QEMU MPS2-AN385
- [ ] QEMU board crate `run()` starts FreeRTOS scheduler + lwIP + zenoh-pico session
- [ ] Rust pubsub example exchanges messages over QEMU TAP bridge via zenohd
- [ ] Rust service example completes request/response cycle on QEMU
- [ ] Rust action example completes goal/result cycle on QEMU
- [ ] C pubsub example exchanges messages over QEMU TAP bridge via zenohd
- [ ] C service example completes request/response cycle on QEMU
- [ ] C action example completes goal/result cycle on QEMU
- [ ] `just test-freertos` runs all QEMU integration tests and passes
- [ ] `just quality` passes (FreeRTOS board crate excluded from default workspace if `FREERTOS_DIR` unset)
- [ ] Orthogonality preserved: `platform-freertos` does not imply any RMW backend or ROS edition

## Notes

- **No smoltcp needed**: lwIP provides the full BSD socket API. The `zpico-smoltcp` crate is not used. This simplifies the board crate significantly compared to bare-metal platforms.
- **FreeRTOS heap**: zenoh-pico uses `pvPortMalloc()`/`vPortFree()` via its `system.c`. The user must configure `configTOTAL_HEAP_SIZE` large enough for zenoh buffers (~12–20 KB for typical pub/sub).
- **No `z_realloc`**: FreeRTOS's standard allocator doesn't support `realloc()` — zenoh-pico's FreeRTOS platform returns NULL. This is fine; zenoh-pico handles the fallback (alloc + copy + free) internally.
- **Priority tuning**: The zenoh-pico background read task runs at `configMAX_PRIORITIES / 2` by default. For real-time ROS 2 nodes, the user may need to adjust task priorities to ensure the application task preempts networking when needed.
- **lwIP `tcpip_thread`**: lwIP's threaded mode runs a `tcpip_thread` that processes protocol events. This is a FreeRTOS task separate from both the application task and zenoh-pico's read task. Typical FreeRTOS+lwIP applications need 3+ tasks minimum.
- **QEMU MPS2-AN385 RAM**: 4 MB SSRAM at 0x21000000. More than sufficient for FreeRTOS + lwIP + zenoh-pico + application. Real MCUs will have less (256–512 KB).
- **LAN9118 vs real Ethernet MACs**: The LAN9118 is a simple MMIO Ethernet controller. Real STM32/NXP boards use DMA-based Ethernet MACs (e.g., STM32 ETH peripheral). The lwIP netif driver for LAN9118 is QEMU-specific; real boards use vendor-provided lwIP ports.
- **Semihosting performance**: ARM semihosting is slow (traps to QEMU host). Avoid `println!` in hot paths. Test output should be minimal (pass/fail markers only in loops).
