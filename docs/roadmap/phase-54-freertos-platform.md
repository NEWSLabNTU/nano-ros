# Phase 54: FreeRTOS Platform Support

**Goal**: Add `platform-freertos` as a new platform axis value, enabling nros nodes on FreeRTOS + lwIP boards (STM32, NXP, Renesas, etc.) via both zenoh-pico and XRCE-DDS backends.

**Status**: Not Started
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

### Existing Infrastructure

**Already implemented** (no work needed):
- zenoh-pico FreeRTOS+lwIP platform: `zenoh-pico/src/system/freertos/system.c` + `lwip/network.c`
- zenoh-pico FreeRTOS primitives: tasks, recursive mutexes, condition vars, clock, memory
- Micro-XRCE-DDS-Client FreeRTOS+TCP transport: `udp_transport_freertos_plus_tcp.c`
- nros executor is generic over `Session` trait — works with any backend
- Board crate `run()` pattern established (Phase 51)
- Feature orthogonality enforced (Phase 42)

**Needs implementation** (this phase):
- `platform-freertos` feature flag chain through all crates
- zpico-sys build.rs FreeRTOS compilation branch
- xrce-sys build.rs FreeRTOS compilation branch
- At least one board crate + example

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
│              Board Crate (e.g. nros-nucleo-f767)  │
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

## Feature Flag Chain

```
nros/Cargo.toml:
  platform-freertos = [
      "nros-node/platform-freertos",
      "nros-rmw-zenoh?/platform-freertos",
      "nros-rmw-xrce?/platform-freertos",
  ]

nros-node/Cargo.toml:
  platform-freertos = []

nros-rmw-zenoh/Cargo.toml:
  platform-freertos = ["zpico-sys/freertos"]

nros-rmw-xrce/Cargo.toml:
  platform-freertos = ["xrce-sys/freertos"]

zpico-sys/Cargo.toml:
  freertos = []          # new feature (alongside posix, zephyr, bare-metal)

xrce-sys/Cargo.toml:
  freertos = []          # new feature (alongside posix, bare-metal, zephyr)

nros-c/Cargo.toml:
  platform-freertos = [
      "nros-node/platform-freertos",
      "nros-rmw-zenoh?/platform-freertos",
  ]
```

Mutual exclusivity: `posix`, `zephyr`, `bare-metal`, `freertos` — enforced by compile-time panic in build.rs.

## Environment Variables

| Variable              | Description                                                       | Required |
|-----------------------|-------------------------------------------------------------------|----------|
| `FREERTOS_DIR`        | Path to FreeRTOS kernel source (contains `include/`, `portable/`) | Yes      |
| `FREERTOS_PORT`       | FreeRTOS portable layer (e.g., `GCC/ARM_CM7`, `GCC/RISC-V`)       | Yes      |
| `LWIP_DIR`            | Path to lwIP source (contains `src/include/`, `src/core/`)        | Yes      |
| `FREERTOS_CONFIG_DIR` | Path to directory containing `FreeRTOSConfig.h` and `lwipopts.h`  | Yes      |

These are only needed by zpico-sys and xrce-sys build.rs when the `freertos` feature is active.

## Work Items

- [ ] 54.1 — Feature flag wiring
- [ ] 54.2 — zpico-sys build.rs FreeRTOS+lwIP compilation
- [ ] 54.3 — xrce-sys build.rs FreeRTOS compilation
- [ ] 54.4 — Platform header and shim adjustments
- [ ] 54.5 — First board crate: STM32F767 Nucleo (zenoh)
- [ ] 54.6 — Example: FreeRTOS talker + listener
- [ ] 54.7 — Integration test
- [ ] 54.8 — Documentation

### 54.1 — Feature flag wiring

Add `platform-freertos` / `freertos` features to all crates in the chain. Update mutual exclusivity checks in `zpico-sys/build.rs` and `xrce-sys/build.rs` to include `freertos`.

**Files**: `nros/Cargo.toml`, `nros-node/Cargo.toml`, `nros-rmw-zenoh/Cargo.toml`, `nros-rmw-xrce/Cargo.toml`, `zpico-sys/Cargo.toml`, `xrce-sys/Cargo.toml`, `nros-c/Cargo.toml`

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

**Files**: `packages/zpico/zpico-sys/build.rs`, possibly `packages/zpico/zpico-sys/c/shim/zenoh_shim.c`

### 54.3 — xrce-sys build.rs FreeRTOS compilation

Add a FreeRTOS branch in `xrce-sys/build.rs` that:
1. Reads the same env vars as 54.2
2. Compiles XRCE-DDS client core sources
3. Skips `src/c/util/time.c` (platform crate provides `uxr_millis`/`uxr_nanos`)
4. Adds FreeRTOS + lwIP include paths
5. Optionally compiles `src/c/profile/transport/ip/udp/udp_transport_freertos_plus_tcp.c` for native FreeRTOS+TCP UDP, or uses the custom transport callback interface with lwIP sockets

**Files**: `packages/xrce/xrce-sys/build.rs`

### 54.4 — Platform header and shim adjustments

Create `zpico-sys/c/platform/zenoh_freertos_platform.h` if needed, or verify that zenoh-pico's built-in `include/zenoh-pico/system/platform/freertos/lwip.h` is sufficient.

Review `zenoh_shim.c` for any `#ifdef ZENOH_ZEPHYR` or `#ifdef ZPICO_SMOLTCP` guards that need a FreeRTOS equivalent. The lwIP socket path should mostly match the existing `else` (non-smoltcp, non-zephyr) code path that uses `select()`.

**Files**: `packages/zpico/zpico-sys/c/platform/`, `packages/zpico/zpico-sys/c/shim/zenoh_shim.c`

### 54.5 — First board crate: STM32F767 Nucleo (zenoh)

Create `packages/boards/nros-nucleo-f767/` targeting the Nucleo-F767ZI board (ARM Cortex-M7, 512 KB RAM, Ethernet MAC, widely available ~$25). This board has a built-in Ethernet PHY making it ideal for a first FreeRTOS target.

Board crate structure:
```
packages/boards/nros-nucleo-f767/
├── Cargo.toml
└── src/
    ├── lib.rs          # pub mod, re-exports
    ├── config.rs       # Config (IP, gateway, zenoh locator, domain_id)
    ├── node.rs         # run() — HAL init, lwIP, FreeRTOS scheduler, user closure
    └── error.rs        # Board error type
```

Dependencies:
- `stm32f7xx-hal` (or `embassy-stm32` with FreeRTOS interop) for Ethernet MAC
- FreeRTOS kernel (via env var, compiled by build.rs or linked externally)
- lwIP (via env var, compiled by build.rs or linked externally)

The `run()` function:
1. Init clocks, GPIO, Ethernet MAC via HAL
2. Init lwIP: create netif, set IP/gateway/netmask, start DHCP if configured
3. Start FreeRTOS scheduler (or assume it's already running)
4. Call user closure from a FreeRTOS task with sufficient stack

**Note**: The exact HAL crate and init sequence depends on the chosen board. STM32F767 is the recommended first target, but a different board (NXP, Renesas) could be substituted.

### 54.6 — Example: FreeRTOS talker + listener

Create examples following the 4-level convention:
```
examples/freertos/
└── rust/zenoh/
    ├── talker/
    │   ├── Cargo.toml
    │   ├── .cargo/config.toml
    │   ├── package.xml
    │   └── src/main.rs
    └── listener/
        ├── Cargo.toml
        ├── .cargo/config.toml
        ├── package.xml
        └── src/main.rs
```

Each example uses `nros` with `rmw-zenoh,platform-freertos,ros-humble` features. The entry point calls the board crate's `run()`, then `Executor::open()` inside the closure.

### 54.7 — Integration test

Add a test in `packages/testing/nros-tests/tests/` (or `tests/`) that:
1. Builds the FreeRTOS talker/listener examples
2. Runs them (QEMU with FreeRTOS, or hardware-in-the-loop)
3. Verifies message exchange

If QEMU testing is feasible (e.g., QEMU STM32 with FreeRTOS+lwIP), add `just test-freertos` recipe. Otherwise, document the manual test procedure.

### 54.8 — Documentation

- Update `CLAUDE.md` workspace structure and platform backends sections
- Update `docs/guides/getting-started.md` with FreeRTOS quick start
- Add `docs/guides/freertos-setup.md` covering:
  - FreeRTOS + lwIP source acquisition
  - Environment variable configuration
  - Board-specific setup (STM32CubeMX project generation, etc.)
  - Building and flashing

## Future Extensions (Out of Scope)

- **FreeRTOS+TCP sub-variant**: Add once zenoh-pico upstream completes multicast and TCP_NODELAY support
- **C API on FreeRTOS**: Zephyr-style CMake module with `nros_cargo_build()` for FreeRTOS CMake projects
- **Additional boards**: NXP i.MX RT, Renesas RA, Raspberry Pi Pico W
- **XRCE-DDS on FreeRTOS**: Full XRCE example (needs Micro-XRCE-DDS Agent on the host)
- **FreeRTOS SMP**: Multi-core support (ESP32-S3, STM32H7 dual-core)

## Acceptance Criteria

- [ ] `platform-freertos` feature compiles cleanly for at least one target triple
- [ ] Mutual exclusivity enforced: enabling `platform-freertos` + `platform-posix` panics at build time
- [ ] Feature flag chain works: `nros` → `nros-node` → `nros-rmw-zenoh` → `zpico-sys` all forward correctly
- [ ] At least one board crate with `run()` builds
- [ ] Talker/listener example exchanges messages over Ethernet via zenohd router
- [ ] `just quality` passes (FreeRTOS-specific crates excluded from default workspace build if cross-compilation target unavailable)
- [ ] Orthogonality preserved: `platform-freertos` does not imply any RMW backend or ROS edition

## Notes

- **No smoltcp needed**: lwIP provides the full BSD socket API. The `zpico-smoltcp` crate is not used. This simplifies the board crate significantly compared to bare-metal platforms.
- **FreeRTOS heap**: zenoh-pico uses `pvPortMalloc()`/`vPortFree()` via its `system.c`. The user must configure `configTOTAL_HEAP_SIZE` large enough for zenoh buffers (~12–20 KB for typical pub/sub).
- **No `z_realloc`**: FreeRTOS's standard allocator doesn't support `realloc()` — zenoh-pico's FreeRTOS platform returns NULL. This is fine; zenoh-pico handles the fallback (alloc + copy + free) internally.
- **Priority tuning**: The zenoh-pico background read task runs at `configMAX_PRIORITIES / 2` by default. For real-time ROS 2 nodes, the user may need to adjust task priorities to ensure the application task preempts networking when needed.
- **lwIP `tcpip_thread`**: lwIP's threaded mode runs a `tcpip_thread` that processes protocol events. This is a FreeRTOS task separate from both the application task and zenoh-pico's read task. Typical FreeRTOS+lwIP applications need 3+ tasks minimum.
