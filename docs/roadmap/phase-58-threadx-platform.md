# Phase 58: ThreadX Platform Support

**Goal**: Add `platform-threadx` as a new platform axis value, enabling nros nodes on Eclipse ThreadX + NetX Duo. Validate with two targets: Linux simulation port (PoC, fastest iteration) and QEMU RISC-V 64-bit virt machine (official ThreadX QEMU port with virtio-net Ethernet). ThreadX's IEC 61508 SIL 4 / ISO 26262 ASIL D certifications combined with nano-ros's Kani/Verus formal verification create a uniquely strong safety argument.

**Status**: In Progress (58.1–58.7 done)
**Priority**: Medium
**Depends on**: Phase 42 (Extensible RMW), Phase 43 (RMW-agnostic embedded API), Phase 51 (Board crate `run()` API)

## Overview

Eclipse ThreadX (formerly Azure RTOS) is the only open-source RTOS with IEC 61508 SIL 4 and ISO 26262 ASIL D certifications (SGS-TUV Saar). Its picokernel architecture has the smallest footprint of any mainstream RTOS (~2 KB ROM, ~1 KB RAM), sub-microsecond context switches, and unique preemption-threshold scheduling. ThreadX has 14+ billion deployments and is MIT-licensed via the Eclipse Foundation.

The integrated NetX Duo networking stack provides BSD-compatible sockets, TSN APIs (CBS, TAS, FPE), TLS (FIPS 140-2 certified), and is certified to the same IEC 61508 SIL 4 standard as the kernel.

### Why ThreadX (Not Another RTOS)

| Criterion            | ThreadX                               | FreeRTOS (Phase 54)      | NuttX (Phase 55) | Zephyr                     |
|----------------------|---------------------------------------|--------------------------|------------------|----------------------------|
| Safety certification | **IEC 61508 SIL 4, ISO 26262 ASIL D** | None (SafeRTOS separate) | None             | In progress (SIL 3 target) |
| Certified networking | **NetX Duo SIL 4**                    | No                       | No               | No                         |
| Certified TLS        | **NetX Secure FIPS 140-2**            | No                       | No               | No                         |
| TSN APIs             | CBS, TAS, FPE, PTP                    | Via NXP GenAVB only      | PTP only         | 2 drivers (Qav, Qbv)       |
| Min footprint        | ~2 KB ROM                             | ~5–10 KB                 | ~8 KB+           | ~8 KB+                     |
| License              | MIT                                   | MIT                      | Apache 2.0       | Apache 2.0                 |

ThreadX's unique value is the safety certification stack — certified kernel + certified networking + certified TLS, all open-source. Combined with nano-ros's formal verification (Kani + Verus), this creates a layered safety argument unavailable on any other platform.

### Why Two Targets

**1. Linux simulation port** — ThreadX ships an official `ports/linux/gnu/` that runs the full kernel as pthreads on a Linux host. The `threadx-learn-samples` repo includes `nx_linux_network_driver.c` (AF_PACKET raw socket driver) for real Ethernet. This gives immediate validation with zero cross-compilation:

- Same CI runner as `just quality`
- Real TCP/UDP networking over TAP/bridge interfaces
- Full ThreadX kernel fidelity (threads, mutexes, queues, event flags, timers)
- Fastest iteration for zenoh-pico + NetX Duo integration

**2. QEMU RISC-V 64-bit virt** — ThreadX v6.4.2 added an official QEMU port at `ports/risc-v64/gnu/example_build/qemu_virt/` with PLIC, UART, and timer. The virt machine supports virtio-net over MMIO (8 slots at `0x10001000`, IRQs 1–8). This target validates the real embedded integration:

- Official ThreadX port (in-tree, maintained)
- Real interrupt model (PLIC + CLINT)
- Real embedded architecture (rv64gc)
- virtio-net for TAP networking (needs a new NetX Duo driver, ~700–1000 LOC C)

### Existing Infrastructure

**Already implemented** (no work needed):
- nros executor is generic over `Session` trait — works with any backend
- Board crate `run()` pattern established (Phase 51)
- Feature orthogonality enforced (Phase 42)
- QEMU TAP bridge infrastructure: `scripts/qemu/setup-network.sh`, launch scripts, QemuProcess test fixture
- Ferrous Systems `threadx-sys` demonstrates Rust + ThreadX FFI pattern
- ThreadX BSD socket layer (`nxd_bsd.c`) provides POSIX-compatible socket API

**Needs implementation** (this phase):
- `platform-threadx` feature flag chain through all crates
- zpico-sys / xrce-sys build.rs ThreadX compilation branches
- zenoh-pico NetX Duo network transport (~300–500 LOC C)
- virtio-net NetX Duo driver (~700–1000 LOC C)
- Linux simulation board crate (`nros-threadx-linux`)
- QEMU RISC-V board crate (`nros-threadx-qemu-riscv64`)
- Examples: pubsub, service, action
- Integration tests

## Architecture

### Linux Simulation

```
┌──────────────────────────────────────────────────┐
│                 User Application                  │
│        Executor::open() + Node + Pub/Sub          │
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
│ │ ThreadX system.c │ │ │ │ NetX Duo BSD   │ │
│ │ NetX Duo BSD     │ │ │ │ transport      │ │
│ │ network.c        │ │ │ └────────────────┘ │
│ └──────────────────┘ │ │                    │
└───────────┬──────────┘ └─────────┬──────────┘
            │                       │
            └───────────┬───────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│         ThreadX Kernel (Linux simulation)         │
│     tx_thread (pthreads), tx_mutex, tx_queue      │
│     NetX Duo (BSD sockets via raw socket driver)  │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│     Board Crate (nros-threadx-linux)              │
│      tx_kernel_enter(), nx_system_initialize()    │
│      nx_linux_network_driver, IP config           │
└──────────────────────────────────────────────────┘
```

Linux simulation test topology:

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ zenohd   │    │ ThreadX  │    │ ThreadX  │  │
│  │ on bridge│    │ Linux sim│    │ Linux sim│  │
│  │ 192.0.3.1│    │ +NetX    │    │ +NetX    │  │
│  │          │    │ +zenoh-  │    │ +zenoh-  │  │
│  │          │    │  pico    │    │  pico    │  │
│  │          │    │ talker   │    │ listener │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘  │
│       │               │               │         │
│  ─────┴───────────────┴───────────────┴─────    │
│       br-qemu     tap-qemu0      tap-qemu1      │
└─────────────────────────────────────────────────┘
```

### QEMU RISC-V 64-bit virt

```
┌──────────────────────────────────────────────────┐
│                 User Application                  │
│        Executor::open() + Node + Pub/Sub          │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│                    nros-node                      │
│          Executor<S, MAX_CBS, CB_ARENA>           │
└───────────────────────┬──────────────────────────┘
                        │ Session trait
┌───────────────────────┴──────────────────────────┐
│   nros-rmw-zenoh (ShimSession)                   │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│      zpico-sys (zenoh-pico + shim)               │
│  ThreadX system.c + NetX Duo BSD network.c       │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│       ThreadX Kernel + NetX Duo                   │
│   tx_thread, tx_mutex, tx_queue, tx_event_flags   │
│   NetX Duo TCP/UDP via virtio-net driver          │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│   Board Crate (nros-threadx-qemu-riscv64)        │
│     PLIC, CLINT timer, UART, virtio-net init      │
│     tx_kernel_enter(), nx_system_initialize()     │
└──────────────────────────────────────────────────┘
```

QEMU RISC-V test topology:

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ zenohd   │    │ QEMU     │    │ QEMU     │  │
│  │ on bridge│    │ riscv64  │    │ riscv64  │  │
│  │ 192.0.3.1│    │ virt     │    │ virt     │  │
│  │          │    │ ThreadX  │    │ ThreadX  │  │
│  │          │    │ +NetX    │    │ +NetX    │  │
│  │          │    │ +virtio  │    │ +virtio  │  │
│  │          │    │ talker   │    │ listener │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘  │
│       │               │               │         │
│  ─────┴───────────────┴───────────────┴─────    │
│       br-qemu     tap-qemu0      tap-qemu1      │
└─────────────────────────────────────────────────┘
```

### Key Design Decisions

**1. Linux simulation first, QEMU RISC-V second**

The Linux simulation port lets us validate the full ThreadX + NetX Duo + zenoh-pico integration on the host without cross-compilation, custom Ethernet drivers, or QEMU bring-up. Once the software stack works on Linux sim, porting to QEMU RISC-V is primarily a driver and board init exercise.

**2. zenoh-pico ThreadX platform: custom network transport over NetX Duo BSD sockets**

zenoh-pico has `src/system/threadx/` but only serial transport (no TCP/UDP). Rather than writing a full platform layer from scratch, we write a minimal network transport layer using NetX Duo's BSD socket API (`socket()`, `connect()`, `recv()`, `send()`, `select()`). The ThreadX platform's `system.c` handles threading (tasks, mutexes, event flags) and clock — we reuse that. Only the network path needs new code.

**3. Event flags instead of condition variables**

ThreadX uses event flags (`tx_event_flags_*`) where POSIX uses condvars. zenoh-pico's ThreadX platform already handles this mapping in `system.c`. The condvar-to-event-flags translation is semantically equivalent for zenoh-pico's use case (wake-one, wait-for-event).

**4. NetX Duo BSD socket limitations**

NetX Duo's `select()` only supports `readfds` (no `writefds`/`exceptfds`). zenoh-pico's event loop may need a compile-time guard (`#ifdef ZENOH_THREADX`) to skip write-readiness polling. For TCP, the write buffer is almost always ready on a local network, so this is unlikely to cause issues in practice.

**5. virtio-net MMIO (not PCI) for QEMU RISC-V**

The RISC-V virt machine supports both virtio-mmio and PCIe. We use MMIO because it avoids PCI enumeration complexity (~200 LOC for MMIO transport vs ~1500+ LOC for PCI stack + driver). NuttX, Zephyr, and Linux all use MMIO on this machine for RTOS use cases.

**6. No `zpico-platform-threadx` crate**

Like FreeRTOS, zenoh-pico's built-in ThreadX platform (`src/system/threadx/system.c`) provides all system primitives (tasks, mutexes, clock, memory). A separate platform crate is unnecessary. The board crate handles hardware-specific init (PLIC, timer, network driver).

**7. `just setup-threadx` for source acquisition**

ThreadX, NetX Duo, and the learn-samples repo (Linux network driver) are cloned to `external/` alongside FreeRTOS and NuttX sources.

## Feature Flag Chain

```
nros/Cargo.toml:
  platform-threadx = [
      "nros-node/platform-threadx",
      "nros-rmw-zenoh?/platform-threadx",
      "nros-rmw-xrce?/platform-threadx",
  ]

nros-node/Cargo.toml:
  platform-threadx = ["nros-rmw-zenoh?/platform-threadx", "nros-rmw-xrce?/platform-threadx"]

nros-rmw-zenoh/Cargo.toml:
  platform-threadx = ["zpico-sys/threadx"]

nros-rmw-xrce/Cargo.toml:
  platform-threadx = ["xrce-sys/threadx"]

zpico-sys/Cargo.toml:
  threadx = []          # new feature (alongside posix, zephyr, bare-metal, freertos, nuttx)

xrce-sys/Cargo.toml:
  threadx = []          # new feature (alongside posix, bare-metal, zephyr, freertos, nuttx)

nros-c/Cargo.toml:
  platform-threadx = ["nros/platform-threadx"]
```

Mutual exclusivity: `posix`, `zephyr`, `bare-metal`, `freertos`, `nuttx`, `threadx` — enforced by `compile_error!` in `nros/src/lib.rs` and `panic!` in `zpico-sys/build.rs` + `xrce-sys/build.rs`.

## Environment Variables

| Variable             | Description                                                  | Required |
|----------------------|--------------------------------------------------------------|----------|
| `THREADX_DIR`        | Path to ThreadX kernel source (contains `common/`, `ports/`) | Yes      |
| `THREADX_CONFIG_DIR` | Path to `tx_user.h` (ThreadX kernel config)                  | Yes      |
| `NETX_DIR`           | Path to NetX Duo source (contains `common/`, `addons/`)      | Yes      |
| `NETX_CONFIG_DIR`    | Path to `nx_user.h` (NetX Duo config)                        | Yes      |

These are only needed by zpico-sys and xrce-sys build.rs when the `threadx` feature is active.

## Work Items

- [x] 58.1 — Feature flag wiring
- [x] 58.2 — `just setup-threadx` dependency acquisition
- [x] 58.3 — zpico-sys build.rs ThreadX + NetX Duo compilation
- [x] 58.4 — zenoh-pico NetX Duo BSD socket network transport
- [x] 58.5 — Linux simulation board crate (`nros-threadx-linux`)
- [x] 58.6 — Rust zenoh examples — Linux simulation (pubsub, service, action)
- [x] 58.7 — Linux simulation integration tests + `just test-threadx-linux` recipe
- [x] 58.8 — virtio-net NetX Duo driver
- [ ] 58.9 — QEMU RISC-V board crate (`nros-threadx-qemu-riscv64`)
- [ ] 58.10 — Rust zenoh examples — QEMU RISC-V (pubsub, service, action)
- [ ] 58.11 — QEMU RISC-V integration tests + `just test-threadx` recipe
- [ ] 58.12 — xrce-sys build.rs ThreadX compilation branch
- [ ] 58.13 — Documentation

### 58.1 — Feature flag wiring

Add `platform-threadx` / `threadx` features to all crates in the chain. Update mutual exclusivity checks from 5-way to 6-way. Also backfilled missing `platform-nuttx` / `nuttx` features that were in build.rs but not in Cargo.toml files.

**Status**: Done

**Files**:
- `packages/core/nros/Cargo.toml` — added `platform-nuttx` and `platform-threadx` features
- `packages/core/nros/src/lib.rs` — expanded `compile_error!` to 6-way platform exclusivity
- `packages/core/nros-node/Cargo.toml` — added `platform-nuttx` and `platform-threadx`
- `packages/zpico/nros-rmw-zenoh/Cargo.toml` — added `platform-nuttx`, `platform-threadx`
- `packages/xrce/nros-rmw-xrce/Cargo.toml` — added `platform-nuttx`, `platform-threadx`
- `packages/zpico/zpico-sys/Cargo.toml` — added `nuttx = []`, `threadx = []` features + updated check-cfg
- `packages/zpico/zpico-sys/build.rs` — added `use_threadx` to backend detection + 6-way exclusivity
- `packages/xrce/xrce-sys/Cargo.toml` — added `nuttx = []`, `threadx = []` features
- `packages/xrce/xrce-sys/build.rs` — added `threadx` to mutual exclusivity
- `packages/core/nros-c/Cargo.toml` — added `platform-nuttx`, `platform-threadx`

### 58.2 — `just setup-threadx` dependency acquisition

Add `just setup-threadx` recipe that:
1. Shallow-clones `eclipse-threadx/threadx` (`v6.4.5.202504_rel`) to `external/threadx/`
2. Shallow-clones `eclipse-threadx/netxduo` (`v6.4.5.202504_rel`) to `external/netxduo/`
3. Shallow-clones `eclipse-threadx/threadx-learn-samples` (main) to `external/threadx-learn-samples/` (contains `nx_linux_network_driver.c`)
4. Prints environment variable configuration
5. Idempotent — skips if already present, warns on tag mismatch

Pinned version declared as `THREADX_TAG` justfile variable. All repos gitignored via `/external/` in `.gitignore`. Updated `just setup` from 9 steps to 10 steps.

**Status**: Done

**Files**: `justfile`, `.gitignore`

### 58.3 — zpico-sys build.rs ThreadX + NetX Duo compilation

Added `build_zenoh_pico_threadx()` function to `zpico-sys/build.rs`. When `use_threadx` is true:
1. Read `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` env vars with validation
2. Compile zenoh-pico core sources + custom system.c + network.c + shim
3. Include paths: ThreadX kernel headers, NetX Duo headers (including BSD addon), user config headers
4. Define `ZENOH_GENERIC` + `ZENOH_THREADX`, set `Z_FEATURE_MULTI_THREAD=1`
5. Auto-detect port headers: Linux sim → `ports/linux/gnu/include`, RISC-V → `ports/risc-v64/gnu/include`

Note: The board crate compiles ThreadX kernel + NetX Duo library; zpico-sys only uses their headers.

**Status**: Done

**Files**: `packages/zpico/zpico-sys/build.rs`

### 58.4 — zenoh-pico NetX Duo BSD socket network transport

Write a network transport layer for zenoh-pico using NetX Duo's BSD socket API. This replaces the serial-only transport in `src/system/threadx/network.c`.

Implements zenoh-pico's network interface functions using NetX Duo BSD:
- `_z_open_tcp()` → `socket(AF_INET, SOCK_STREAM, 0)` + `connect()`
- `_z_listen_tcp()` → `socket()` + `bind()` + `listen()`
- `_z_close_tcp()` → `soc_close()`
- `_z_read_tcp()` → `recv()`
- `_z_send_tcp()` → `send()`
- `_z_open_udp_unicast()` → `socket(AF_INET, SOCK_DGRAM, 0)` + `connect()`
- `_z_read_udp_unicast()` → `recv()`
- `_z_send_udp_unicast()` → `sendto()`

Also includes a custom ThreadX system layer (`zenoh_threadx_system.c`) providing threading (TX_THREAD), mutex (TX_MUTEX), condvar (TX_MUTEX+TX_SEMAPHORE), clock (tx_time_get), sleep (tx_thread_sleep), memory (tx_byte_allocate/tx_byte_release), and random functions. A custom platform header (`zenoh_threadx_platform.h`) defines the types. The generic platform header dispatches to ThreadX types when `ZENOH_THREADX` is defined.

**Status**: Done

**Files**:
- `packages/zpico/zpico-sys/c/platform/zenoh_threadx_network.c` — **New** (~320 LOC) — BSD socket transport
- `packages/zpico/zpico-sys/c/platform/zenoh_threadx_system.c` — **New** (~300 LOC) — System layer
- `packages/zpico/zpico-sys/c/platform/zenoh_threadx_platform.h` — **New** — Platform types
- `packages/zpico/zpico-sys/c/platform/zenoh_generic_platform.h` — Updated to dispatch for ThreadX

### 58.5 — Linux simulation board crate (`nros-threadx-linux`)

Create a board crate for ThreadX Linux simulation.

```
packages/boards/nros-threadx-linux/
├── Cargo.toml
├── build.rs              # Compile ThreadX Linux port + NetX Duo + Linux network driver via cc
├── config/
│   ├── tx_user.h         # ThreadX config (priority count, stack checking, etc.)
│   └── nx_user.h         # NetX Duo config (BSD sockets enabled, packet pool size)
└── src/
    ├── lib.rs            # Re-exports, println!, exit helpers
    ├── config.rs         # Config builder (IP, MAC, gateway, zenoh locator, domain_id)
    └── node.rs           # run() — ThreadX kernel init, NetX Duo init, network driver, app thread
```

**build.rs** responsibilities:
1. Compile ThreadX Linux port: `ports/linux/gnu/src/*.c` (8 files)
2. Compile ThreadX common: `common/src/*.c` (kernel objects)
3. Compile NetX Duo common: `common/src/*.c` (TCP, UDP, IP, ARP, BSD sockets)
4. Compile `nx_linux_network_driver.c` from threadx-learn-samples
5. Include paths: ThreadX, NetX Duo, config dir
6. Link with `-lpthread`

**`run()` function** sequence:
1. Configure `NX_LINUX_INTERFACE_NAME` (TAP interface name from config)
2. Call `tx_kernel_enter()` — ThreadX takes over, calls `tx_application_define()`
3. In `tx_application_define()`:
   a. Create packet pool (`nx_packet_pool_create()`)
   b. Create IP instance (`nx_ip_create()` with `nx_linux_network_driver`)
   c. Enable TCP/UDP (`nx_tcp_enable()`, `nx_udp_enable()`)
   d. Enable BSD socket layer (`bsd_initialize()`)
   e. Set static IP (from Config)
   f. Create application thread (runs user closure)
4. Application thread calls user closure with `Config`
5. User creates `Executor::open()` + nodes

**Config builder** mirrors other board crates:
- `Config::default()` — talker preset (192.0.3.10, MAC 52:54:00:12:34:56, gateway 192.0.3.1, interface `tap-qemu0`)
- `Config::listener()` — listener preset (192.0.3.11, MAC 52:54:00:12:34:57, interface `tap-qemu1`)
- `with_ip()`, `with_mac()`, `with_gateway()`, `with_zenoh_locator()`, `with_domain_id()`, `with_interface()`

**Dependencies**:
- No direct nros dependencies — the board crate is a thin init wrapper
- Uses `cc` build-dep to compile ThreadX kernel + NetX Duo + Linux network driver
- No `cortex-m-rt` or semihosting (runs on host Linux)

**Status**: Done

**Files**:
- `packages/boards/nros-threadx-linux/Cargo.toml`
- `packages/boards/nros-threadx-linux/build.rs` — Compiles ThreadX Linux port, NetX Duo, BSD sockets, Linux network driver
- `packages/boards/nros-threadx-linux/c/app_define.c` — `tx_application_define()`: packet pool, IP instance, TCP/UDP/BSD enable, app thread
- `packages/boards/nros-threadx-linux/config/tx_user.h` — ThreadX kernel config
- `packages/boards/nros-threadx-linux/config/nx_user.h` — NetX Duo config (BSD sockets enabled)
- `packages/boards/nros-threadx-linux/src/lib.rs` — Re-exports `Config` and `run`
- `packages/boards/nros-threadx-linux/src/config.rs` — Config builder (IP, MAC, gateway, interface, zenoh locator, domain_id)
- `packages/boards/nros-threadx-linux/src/node.rs` — `run()`: banner, FFI setup, `tx_kernel_enter()`

### 58.6 — Rust zenoh examples — Linux simulation (pubsub, service, action)

```
examples/threadx-linux/rust/zenoh/
├── talker/              # Pub: std_msgs/Int32 on /chatter
├── listener/            # Sub: std_msgs/Int32 on /chatter
├── service-server/      # Srv: example_interfaces/AddTwoInts
├── service-client/      # Cli: example_interfaces/AddTwoInts
├── action-server/       # Act: example_interfaces/Fibonacci
└── action-client/       # Act: example_interfaces/Fibonacci
```

Each example has:
```
├── Cargo.toml           # deps: nros, nros-threadx-linux, generated msg types
├── .cargo/config.toml   # target = x86_64-unknown-linux-gnu, patch.crates-io
├── package.xml          # For cargo-nano-ros message generation
├── .gitignore           # /target/, /generated/
└── src/main.rs
```

**Entry point pattern** (standard Rust — ThreadX Linux sim supports `std`):
```rust
use nros::prelude::*;
use nros_threadx_linux::{Config, run};
use std_msgs::msg::Int32;

fn main() {
    run(Config::default(), |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker");
        let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
        let mut node = executor.create_node("talker")?;
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        // ...
        Ok::<(), NodeError>(())
    })
}
```

**Build target**: `x86_64-unknown-linux-gnu` (host native).

**Status**: Done

**Files**: `examples/threadx-linux/rust/zenoh/{talker,listener,service-server,service-client,action-server,action-client}/` — each with `Cargo.toml`, `.cargo/config.toml`, `package.xml`, `.gitignore`, `src/main.rs`

### 58.7 — Linux simulation integration tests + `just test-threadx-linux` recipe

Automated integration tests using ThreadX Linux simulation binaries.

**Tests** in `packages/testing/nros-tests/tests/threadx_linux.rs`:
- **Build tests** (6): Verify `cargo build` succeeds for all ThreadX Linux examples
- **E2E network tests** (3): `test_threadx_linux_pubsub_e2e`, `test_threadx_linux_service_e2e`, `test_threadx_linux_action_e2e`
  - Each test: start zenohd → launch server/listener on tap-qemu0 → launch client/talker on tap-qemu1 → verify output markers
  - ThreadX Linux sim binaries run as native Linux processes (no QEMU)
  - Skip gracefully when TAP bridge not available

**Nextest config**: `threadx-linux` test group with `max-threads = 1` (TAP bridge exclusive access).

**Justfile**: `just test-threadx-linux` runs nextest with `threadx_linux` filter.

**Status**: Done

**Files**:
- `packages/testing/nros-tests/tests/threadx_linux.rs` — 17 tests (1 detection, 7 build, 3 E2E with ManagedProcess)
- `.config/nextest.toml` — `threadx-linux` test group (max-threads=1, 120s timeout)
- `justfile` — `THREADX_DIR`/`NETX_DIR` exports, `test-threadx-linux` recipe, `threadx-linux` excluded from `format`/`build-examples`/`check-examples`/`test`
- `Cargo.toml` — 6 ThreadX Linux examples added to workspace exclude list

### 58.8 — virtio-net NetX Duo driver

Write a virtio-net Ethernet driver for NetX Duo using the VirtIO MMIO transport.

**Location**: `packages/drivers/virtio-net-netx/`

```
packages/drivers/virtio-net-netx/
├── include/
│   └── virtio_net_nx.h       # Public API: virtio_net_nx_driver()
├── src/
│   ├── virtio_mmio.c          # VirtIO MMIO transport (~200–300 LOC)
│   ├── virtqueue.c            # Virtqueue management (~300–400 LOC)
│   └── virtio_net_nx.c        # NetX Duo driver glue (~200–300 LOC)
└── CMakeLists.txt
```

**VirtIO MMIO transport** (`virtio_mmio.c`):
- Register read/write at device base address (e.g., `0x10001000`)
- Feature negotiation (VIRTIO_NET_F_MAC, VIRTIO_NET_F_STATUS)
- Device status transitions (ACKNOWLEDGE → DRIVER → FEATURES_OK → DRIVER_OK)
- Virtqueue setup (descriptor table, available ring, used ring)

**Virtqueue management** (`virtqueue.c`):
- Split virtqueue format (descriptor table + avail ring + used ring)
- Buffer descriptor allocation/free (static pool)
- Notification (write to MMIO QueueNotify register)
- Used ring processing (interrupt handler)

**NetX Duo driver** (`virtio_net_nx.c`):
- `virtio_net_nx_driver(NX_IP_DRIVER *driver_req)` entry point
- `NX_LINK_INITIALIZE`: Probe VirtIO MMIO device, negotiate features, setup virtqueues
- `NX_LINK_ENABLE`: Fill RX virtqueue with pre-allocated buffers, enable interrupts
- `NX_LINK_PACKET_SEND`: Build TX descriptor (12-byte virtio-net header + Ethernet frame), enqueue, notify
- `NX_LINK_DEFERRED_PROCESSING`: Process RX used ring in thread context, call `nx_ip_packet_receive()`
- PLIC interrupt handler: Signal deferred processing via `tx_event_flags_set()`

**Reference implementations**:
- NuttX `drivers/virtio/virtio-net.c` (~600 LOC C)
- SLOF `lib/libvirtio/virtio-net.c` (~300 LOC C, minimal)
- rcore-os/virtio-drivers (Rust, `no_std`)

**QEMU command**:
```bash
qemu-system-riscv64 -M virt -nographic \
  -global virtio-mmio.force-legacy=false \
  -netdev tap,id=net0,ifname=tap-qemu0,script=no,downscript=no \
  -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0 \
  -kernel <binary>
```

**Estimated size**: ~700–1000 LOC C total.

**Files**: `packages/drivers/virtio-net-netx/`

### 58.9 — QEMU RISC-V board crate (`nros-threadx-qemu-riscv64`)

Create a board crate for ThreadX on QEMU RISC-V 64-bit virt.

```
packages/boards/nros-threadx-qemu-riscv64/
├── Cargo.toml
├── build.rs              # Compile ThreadX rv64 port + NetX Duo + virtio-net driver via cc
├── config/
│   ├── tx_user.h         # ThreadX config
│   ├── nx_user.h         # NetX Duo config
│   └── link.lds          # Linker script (RAM at 0x80000000)
├── c/
│   ├── entry.S            # Boot entry point (from ThreadX QEMU virt example)
│   ├── tx_initialize_low_level.S  # ThreadX low-level init
│   ├── board.c            # PLIC, UART, CLINT timer init
│   └── app_define.c       # tx_application_define() — NetX + virtio-net + app thread
└── src/
    ├── lib.rs            # Re-exports, println!, exit helpers
    ├── config.rs         # Config builder (IP, MAC, gateway, zenoh locator, domain_id)
    └── node.rs           # run() — calls tx_kernel_enter()
```

**build.rs** responsibilities:
1. Compile ThreadX RISC-V 64-bit port: `ports/risc-v64/gnu/src/*.c` + `*.S`
2. Compile ThreadX common: `common/src/*.c`
3. Compile NetX Duo common: `common/src/*.c`
4. Compile virtio-net NetX Duo driver (from 58.8)
5. Compile board C code (`c/*.c`, `c/*.S`)
6. Cross-compile with `riscv64-unknown-elf-gcc` (or `riscv64-linux-gnu-gcc`)
7. Include paths: ThreadX, NetX Duo, config dir, virtio-net driver

**`run()` function**: Sets up Config, then calls into C `tx_kernel_enter()`. The C `tx_application_define()` handles ThreadX/NetX/virtio init and creates the application thread that calls back into Rust.

**Semihosting**: RISC-V semihosting for QEMU output. `println!` via UART at `0x10000000` (16550 UART in virt machine).

**Build target**: `riscv64gc-unknown-none-elf` (bare-metal RISC-V 64-bit).

**Files**: `packages/boards/nros-threadx-qemu-riscv64/`

### 58.10 — Rust zenoh examples — QEMU RISC-V (pubsub, service, action)

```
examples/qemu-riscv64-threadx/rust/zenoh/
├── talker/
├── listener/
├── service-server/
├── service-client/
├── action-server/
└── action-client/
```

Each example has:
```
├── Cargo.toml           # deps: nros, nros-threadx-qemu-riscv64, generated msg types
├── .cargo/config.toml   # target = riscv64gc-unknown-none-elf, patch.crates-io
├── package.xml
├── .gitignore
└── src/main.rs
```

**Entry point**: `#![no_std]` / `#![no_main]` (bare-metal RISC-V, no OS std).

**Build target**: `riscv64gc-unknown-none-elf`, `--release`.

**Files**: `examples/qemu-riscv64-threadx/rust/zenoh/`

### 58.11 — QEMU RISC-V integration tests + `just test-threadx` recipe

Automated QEMU-based integration tests.

**QEMU launch command**:
```bash
qemu-system-riscv64 -M virt -nographic \
  -global virtio-mmio.force-legacy=false \
  -netdev tap,id=net0,ifname=tap-qemu0,script=no,downscript=no \
  -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0 \
  -kernel <elf>
```

**Tests** in `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`:
- **E2E network tests** (3): pubsub, service, action over TAP bridge

**Test fixture**: Add `start_riscv64_virt()` method to `qemu.rs` for RISC-V virt machine.

**Nextest config**: `threadx-qemu` test group with `max-threads = 1`.

**Justfile**: `just test-threadx` runs both Linux sim and QEMU RISC-V tests.

**Files**: `packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs`, `packages/testing/nros-tests/src/qemu.rs`, `.config/nextest.toml`, `justfile`

### 58.12 — xrce-sys build.rs ThreadX compilation branch

Add a ThreadX branch in `xrce-sys/build.rs`:
1. Read ThreadX/NetX env vars
2. Compile XRCE-DDS client core sources
3. Add ThreadX + NetX Duo include paths
4. Define platform as ThreadX (custom transport via NetX Duo BSD sockets)

**Files**: `packages/xrce/xrce-sys/build.rs`

### 58.13 — Documentation

- Update `CLAUDE.md`:
  - Add `nros-threadx-linux`, `nros-threadx-qemu-riscv64` to workspace structure
  - Add `virtio-net-netx` to drivers list
  - Add `threadx-linux`, `qemu-riscv64-threadx` to examples list
  - Update platform backends to include `platform-threadx`
  - Add `just test-threadx`, `just test-threadx-linux` to test groups
  - Add `just setup-threadx` to build commands
  - Add ThreadX environment variables
  - Update phase table
- Add `docs/guides/threadx-setup.md`
- Update `docs/reference/environment-variables.md`

**Files**: `CLAUDE.md`, `docs/guides/threadx-setup.md`, `docs/reference/environment-variables.md`

## Future Extensions (Out of Scope)

- **ARM Cortex-M board crate**: Port ThreadX to QEMU MPS2-AN385 (Cortex-M3). ThreadX's `cortex_m3/gnu` port is nearly compatible. Would need LAN9118 NetX Duo driver (similar to FreeRTOS's LAN9118 lwIP netif).
- **NetX Duo TSN wrapper**: Expose `nx_shaper_cbs_*`, `nx_shaper_tas_*`, `nx_shaper_fpe_*` through `nros-tsn` API. Requires TSN-capable hardware (MIMXRT1180-EVK) for validation.
- **RTOSX KERNEL evaluation**: Drop-in API-compatible replacement for Eclipse ThreadX with current (2025) safety certifications. Evaluate for customers needing fresh certification artifacts.
- **C API on ThreadX**: C examples using nros-c (depends on Phase 49 completion)
- **XRCE-DDS examples on ThreadX**: Full XRCE example with Micro-XRCE-DDS Agent
- **ThreadX SMP**: Multi-core support (dual-core RISC-V, Cortex-A SMP)
- **Real hardware**: NXP MIMXRT1180-EVK (TSN switch), STM32 boards, NXP i.MX RT boards

## Risks

1. **zenoh-pico ThreadX platform maturity**: zenoh-pico's `src/system/threadx/system.c` provides threading/clock/memory but only serial network transport. The TCP/UDP network layer (58.4) is new code that may hit edge cases with NetX Duo's BSD socket semantics.
2. **NetX Duo BSD `select()` limitation**: Only `readfds` supported. If zenoh-pico relies on `writefds` for TCP flow control, the transport will need adaptation.
3. **virtio-net driver complexity**: The virtio-net NetX Duo driver (58.8) is ~700–1000 LOC of new C code with interrupt handling, DMA-like descriptor management, and ring buffer coordination. This is the highest-risk work item.
4. **RISC-V toolchain**: `riscv64gc-unknown-none-elf` Rust target is Tier 2 (no guaranteed builds). May need nightly + `-Z build-std`. The RISC-V GCC cross-compiler (`riscv64-unknown-elf-gcc` or `riscv64-linux-gnu-gcc`) must be installed.
5. **ThreadX Linux simulation fidelity**: The Linux port uses pthreads for cooperative scheduling. Timing-sensitive bugs that appear on real hardware may not manifest on the simulation. The QEMU RISC-V target mitigates this.
6. **NetX Duo compilation size**: NetX Duo is a substantial codebase. Cross-compiling it via `cc` crate adds build time and complexity. May need selective source file compilation (TCP + UDP + BSD + ARP + IP only, skip HTTP/MQTT/etc.).

## Acceptance Criteria

- [ ] `platform-threadx` feature compiles cleanly for `x86_64-unknown-linux-gnu` (Linux sim)
- [ ] `platform-threadx` feature compiles cleanly for `riscv64gc-unknown-none-elf` (QEMU RISC-V)
- [ ] Mutual exclusivity enforced: enabling `platform-threadx` + any other platform → build error
- [ ] Feature flag chain works: `nros` → `nros-node` → `nros-rmw-zenoh` → `zpico-sys` all forward correctly
- [ ] ThreadX Linux sim board crate starts ThreadX kernel + NetX Duo + zenoh-pico session
- [ ] Linux sim pubsub example exchanges messages over TAP bridge via zenohd
- [ ] Linux sim service example completes request/response cycle
- [ ] Linux sim action example completes goal/result cycle
- [ ] virtio-net NetX Duo driver transmits and receives Ethernet frames on QEMU RISC-V virt
- [ ] QEMU RISC-V board crate starts ThreadX + NetX Duo + virtio-net + zenoh-pico session
- [ ] QEMU RISC-V pubsub example exchanges messages over TAP bridge via zenohd
- [ ] QEMU RISC-V service example completes request/response cycle
- [ ] QEMU RISC-V action example completes goal/result cycle
- [ ] `just test-threadx-linux` runs Linux simulation integration tests and passes
- [ ] `just test-threadx` runs all ThreadX integration tests (Linux sim + QEMU RISC-V) and passes
- [ ] `just quality` passes (ThreadX board crates excluded from default workspace if `THREADX_DIR` unset)
- [ ] Orthogonality preserved: `platform-threadx` does not imply any RMW backend or ROS edition

## Notes

- **Safety certification scope**: The certified ThreadX versions are the 6.1.x series. Using HEAD of main or 6.4.x is uncertified code. For production safety use, pin to a certified version (or RTOSX KERNEL v7.0.0 which has current 2025 certification).
- **Certification artifact licensing**: ThreadX source is MIT. Safety artifacts (safety manual, V&V reports, trace matrices) require ThreadX Alliance membership (EUR 5K–25K/year) plus separate commercial license.
- **No smoltcp or lwIP needed**: NetX Duo provides the full TCP/IP stack with BSD sockets. Neither `zpico-smoltcp` nor `lan9118-lwip` crates are used.
- **ThreadX heap**: NetX Duo allocates packets from `NX_PACKET_POOL`. ThreadX allocates thread stacks and kernel objects from byte/block pools. The board crate must configure pool sizes large enough for zenoh-pico (~16–20 KB for typical pub/sub).
- **Linux sim `NET_ADMIN` capability**: The `nx_linux_network_driver.c` uses `AF_PACKET`/`SOCK_RAW`, which requires `CAP_NET_ADMIN` or root. Tests must run with appropriate privileges (same as existing TAP-based tests).
- **QEMU RISC-V memory**: The virt machine's DRAM starts at `0x80000000` with configurable size (default 128 MB). More than sufficient for ThreadX + NetX Duo + zenoh-pico + application.
- **Preemption-threshold**: ThreadX's unique scheduling feature. Not used in the initial port but available for future real-time tuning — safety-critical tasks can set a preemption threshold to reduce context switch overhead while maintaining priority ordering.
- **`tx_user.h` configuration**: Analogous to `FreeRTOSConfig.h`. Key settings: `TX_MAX_PRIORITIES` (32), `TX_TIMER_TICKS_PER_SECOND` (100), `TX_ENABLE_STACK_CHECKING`. Enable `TX_MISRA_ENABLE` for safety-certified builds.
