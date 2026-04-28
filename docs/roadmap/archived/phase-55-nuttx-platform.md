# Phase 55: NuttX Platform Support

**Goal**: Add `platform-nuttx` as a new platform axis value, enabling nros nodes on NuttX via both zenoh-pico and XRCE-DDS backends. Validate on QEMU ARM virt machine (Cortex-A7 + virtio-net) before requiring real hardware.

**Status**: Complete
**Priority**: Medium
**Depends on**: Phase 42 (Extensible RMW), Phase 43 (RMW-agnostic embedded API), Phase 51 (Board crate `run()` API)

## Overview

NuttX is the third major embedded RTOS alongside Zephyr and FreeRTOS. micro-ROS already uses NuttX with XRCE-DDS in production, validating the use case. NuttX's strong POSIX compliance (POSIX.1-2008: pthreads, BSD sockets, `select()`, `clock_gettime()`) makes it uniquely suited because zenoh-pico's `unix/` platform layer works with minimal adaptation — unlike FreeRTOS which needed lwIP, custom drivers, and a dedicated platform layer.

### Why NuttX Is Simpler Than FreeRTOS (Phase 54)

| Aspect | FreeRTOS (Phase 54) | NuttX (this phase) |
|--------|--------------------|--------------------|
| zenoh-pico platform | Built-in `freertos/` platform | Reuse `unix/` with `ZENOH_NUTTX` define |
| Networking | lwIP (external, compiled by build.rs) | NuttX built-in BSD sockets |
| Custom Ethernet driver | Yes (LAN9118 lwIP netif, ~300 LOC C) | No (NuttX has virtio-net) |
| Rust target | `thumbv7m-none-eabi` (Cortex-M, no_std) | `armv7a-nuttx-eabi` (Cortex-A, std) |
| Build integration | cc crate compiles FreeRTOS+lwIP | NuttX build system + cargo |
| QEMU machine | mps2-an385 (Cortex-M3) | virt (Cortex-A7) |
| zpico-platform crate | Not needed (zenoh-pico built-in) | Not needed (unix platform provides all) |

Key simplifications:
1. **No custom Ethernet driver** — NuttX's virtio-net driver is built into the kernel (~300 LOC C eliminated)
2. **No external networking stack** — NuttX has its own TCP/IP stack with BSD sockets. No lwIP compilation.
3. **Rust `std` support** — NuttX targets support `std`, so examples use standard Rust (not `no_std`/`no_main`)
4. **XRCE-DDS has upstream NuttX support** — micro-ROS validates this in production

### Why QEMU ARM virt (Not MPS2-AN385)

NuttX requires an MMU-capable core (Cortex-A class), not Cortex-M. The QEMU `-M virt` machine provides:
- Cortex-A7 CPU
- virtio-net network interface (NuttX has built-in driver)
- Same TAP bridge infrastructure (`tap-qemu0`, `tap-qemu1`, `br-qemu`) works unchanged

### Existing Infrastructure

**Already implemented** (no work needed):
- zenoh-pico `unix/` platform: `system.c` + `network.c` use standard POSIX (pthreads, BSD sockets, select)
- XRCE-DDS NuttX toolchain: `xrce-sys/micro-xrce-dds-client/toolchains/nuttx_toolchain.cmake`
- NuttX Rust targets: Tier 3 with `std` (`armv7a-nuttx-eabi`, `thumbv7a-nuttx-eabi`, `riscv32imac-nuttx-none-elf`)
- nros executor is generic over `Session` trait — works with any backend
- Board crate `run()` pattern established (Phase 51)
- Feature orthogonality enforced (Phase 42)
- QEMU TAP bridge infrastructure: `scripts/qemu/setup-network.sh`, launch scripts, QemuProcess test fixture

**Needs implementation** (this phase):
- `platform-nuttx` feature flag chain through all crates
- zenoh-pico `ZENOH_NUTTX` define (RNG adaptation, platform.h dispatch)
- zpico-sys / xrce-sys build.rs NuttX compilation branches
- NuttX QEMU board crate (`nros-board-nuttx-qemu-arm`)
- Examples: pubsub, service, action in both Rust and C
- Integration tests with `just test-nuttx` recipe

## Architecture

```
┌──────────────────────────────────────────────────┐
│          User Application (std Rust)              │
│      Executor::open() + Node + Pub/Sub            │
│      println!, std::io, std::time all work        │
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
│ │ unix/system.c    │ │ │ │ POSIX transport│ │
│ │ (ZENOH_NUTTX)    │ │ │ │ (NuttX compat) │ │
│ │ unix/network.c   │ │ │ └────────────────┘ │
│ └──────────────────┘ │ │                    │
└───────────┬──────────┘ └─────────┬──────────┘
            │                       │
            └───────────┬───────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│                 NuttX Kernel                      │
│     pthreads, BSD Sockets, clock_gettime(),       │
│     virtio-net driver, /dev/urandom               │
└───────────────────────┬──────────────────────────┘
                        │
┌───────────────────────┴──────────────────────────┐
│   Board Crate (nros-board-nuttx-qemu-arm)              │
│     NuttX defconfig, build-nuttx.sh, Config       │
└──────────────────────────────────────────────────┘
```

Same topology as existing bare-metal and FreeRTOS QEMU tests:

```
┌─────────────────────────────────────────────────┐
│  Host (Linux)                                    │
│                                                  │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐  │
│  │ zenohd   │    │ QEMU     │    │ QEMU     │  │
│  │ on bridge│    │ virt     │    │ virt     │  │
│  │ 192.0.3.1│    │ NuttX    │    │ NuttX    │  │
│  │          │    │ +zenoh-  │    │ +zenoh-  │  │
│  │          │    │  pico    │    │  pico    │  │
│  │          │    │ talker   │    │ listener │  │
│  └────┬─────┘    └────┬─────┘    └────┬─────┘  │
│       │               │               │         │
│  ─────┴───────────────┴───────────────┴─────    │
│       br-qemu     tap-qemu0      tap-qemu1      │
└─────────────────────────────────────────────────┘
```

### Key Design Decisions

**1. Reuse zenoh-pico `unix/` platform (not a new platform layer)**

NuttX provides the same POSIX surface as Linux: pthreads, BSD sockets, `select()`, `clock_gettime()`, `malloc()`/`free()`. The only gap is `getrandom()` (Linux-specific syscall). Solution: add `ZENOH_NUTTX` elif branches for RNG using `/dev/urandom`, and reuse everything else from `unix/system.c` and `unix/network.c`.

**2. No external networking stack**

NuttX has its own TCP/IP stack (derived from uIP but heavily modified) with a standard BSD socket interface. No lwIP, no smoltcp, no `zpico-smoltcp` bridge needed. This is the simplest networking integration of any embedded platform.

**3. Rust `std` support**

NuttX Rust targets support `std` (unlike bare-metal/FreeRTOS). Examples use standard `fn main()`, `println!`, `std::time`, etc. Requires nightly + `-Z build-std` (Tier 3 targets).

**4. NuttX build system wraps cargo**

Unlike FreeRTOS (where cargo's build.rs compiles everything), NuttX has its own build system. The board crate includes `build-nuttx.sh` that: configures NuttX, calls `cargo build` for the Rust app, and links everything into a bootable ELF.

**5. QEMU ARM virt as validation target**

Cortex-A7 + virtio-net. NuttX has built-in virtio-net driver — no custom Ethernet driver needed (eliminates ~300 LOC C vs FreeRTOS). Same TAP bridge infrastructure.

## Feature Flag Chain

```
nros/Cargo.toml:
  platform-nuttx = [
      "nros-node/platform-nuttx",
      "nros-rmw-zenoh?/platform-nuttx",
      "nros-rmw-xrce?/platform-nuttx",
  ]

nros-node/Cargo.toml:
  platform-nuttx = ["nros-rmw-zenoh?/platform-nuttx", "nros-rmw-xrce?/platform-nuttx"]

nros-rmw-zenoh/Cargo.toml:
  platform-nuttx = ["zpico-sys/nuttx"]

nros-rmw-xrce/Cargo.toml:
  platform-nuttx = ["xrce-sys/nuttx"]

zpico-sys/Cargo.toml:
  nuttx = []          # new feature (alongside posix, zephyr, bare-metal, freertos)

xrce-sys/Cargo.toml:
  nuttx = []          # new feature (alongside posix, bare-metal, zephyr, freertos)

nros-c/Cargo.toml:
  platform-nuttx = ["nros/platform-nuttx"]
```

Mutual exclusivity: `posix`, `zephyr`, `bare-metal`, `freertos`, `nuttx` — enforced by `compile_error!` in `nros/src/lib.rs` and `panic!` in `zpico-sys/build.rs` + `xrce-sys/build.rs`.

## Environment Variables

| Variable         | Description                                            | Required |
|------------------|--------------------------------------------------------|----------|
| `NUTTX_DIR`      | Path to NuttX OS source (contains `include/`, `arch/`) | Yes      |
| `NUTTX_APPS_DIR` | Path to NuttX apps source (contains `Application.mk`)  | Yes      |

Simpler than FreeRTOS (no separate `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR`). NuttX provides all system headers in one place.

## Work Items

- [x] 55.1 — Feature flag wiring
- [x] 55.2 — zenoh-pico `ZENOH_NUTTX` platform adaptation
- [x] 55.3 — zpico-sys build.rs NuttX compilation branch
- [x] 55.4 — xrce-sys build.rs NuttX compilation branch
- [x] 55.5 — `just setup-nuttx` dependency acquisition
- [x] 55.6 — NuttX QEMU defconfig
- [x] 55.7 — QEMU board crate (`nros-board-nuttx-qemu-arm`)
- [x] 55.8 — Rust zenoh examples (pubsub, service, action)
- [x] 55.9 — C zenoh examples (pubsub, service, action)
- [x] 55.10 — Integration tests + `just test-nuttx` recipe
- [x] 55.11 — Documentation
- [x] 55.12 — E2E network tests (pubsub, service, action over QEMU TAP bridge)

### 55.1 — Feature flag wiring

Add `platform-nuttx` / `nuttx` features to all crates in the chain. Update mutual exclusivity checks from 4-way to 5-way.

**Files** (same set as 54.1):
- `packages/core/nros/Cargo.toml` — add `platform-nuttx` feature
- `packages/core/nros/src/lib.rs` — expand `compile_error!` from 3 platforms to 5 (10 pairwise combos)
- `packages/core/nros-node/Cargo.toml` — add `platform-nuttx`
- `packages/zpico/nros-rmw-zenoh/Cargo.toml` — add `platform-nuttx = ["zpico-sys/nuttx"]`
- `packages/xrce/nros-rmw-xrce/Cargo.toml` — add `platform-nuttx = ["xrce-sys/nuttx"]`
- `packages/zpico/zpico-sys/Cargo.toml` — add `nuttx = []` feature
- `packages/zpico/zpico-sys/build.rs` — add `use_nuttx` to backend detection + 5-way exclusivity
- `packages/xrce/xrce-sys/Cargo.toml` — add `nuttx = []` feature
- `packages/xrce/xrce-sys/build.rs` — add `nuttx` to 5-way exclusivity
- `packages/core/nros-c/Cargo.toml` — add `platform-nuttx = ["nros/platform-nuttx"]`

### 55.2 — zenoh-pico `ZENOH_NUTTX` platform adaptation

Minimal changes to the zenoh-pico submodule to recognize NuttX as a unix-compatible platform:

1. **`zenoh-pico/include/zenoh-pico/system/common/platform.h`** (line 28) — Add `|| defined(ZENOH_NUTTX)` to the unix.h include guard so NuttX uses POSIX type definitions (pthread types, socket types, clock types):
   ```c
   #if defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(ZENOH_BSD) || defined(ZENOH_NUTTX)
   #include "zenoh-pico/system/platform/unix.h"
   ```

2. **`zenoh-pico/src/system/unix/system.c`** — Add `ZENOH_NUTTX` elif for 5 RNG functions (`z_random_u8/u16/u32/u64`, `z_random_fill`). NuttX doesn't guarantee `getrandom()` (Linux syscall), use `/dev/urandom`:
   ```c
   #elif defined(ZENOH_NUTTX)
       int fd = open("/dev/urandom", O_RDONLY);
       if (fd >= 0) { read(fd, &ret, sizeof(ret)); close(fd); }
   ```

3. **`zenoh-pico/src/system/unix/network.c`** — Add `ZENOH_NUTTX` alongside `ZENOH_LINUX` for:
   - `MSG_NOSIGNAL` flag in `_z_send_tcp` (NuttX supports this)
   - `freeaddrinfo` cleanup paths
   - Guard `<ifaddrs.h>` include: NuttX may not have `getifaddrs()` (BSD extension, not POSIX base)

**Risk**: `<ifaddrs.h>` / `getifaddrs()` may not be available on NuttX. This affects UDP multicast interface enumeration. Workaround: `#ifndef ZENOH_NUTTX` guard around `getifaddrs()` usage, default to all-interfaces for multicast. Alternatively, NuttX may have it with `CONFIG_NET_NETDEV_IFINDEX`.

**Files**:
- `packages/zpico/zpico-sys/zenoh-pico/include/zenoh-pico/system/common/platform.h`
- `packages/zpico/zpico-sys/zenoh-pico/src/system/unix/system.c`
- `packages/zpico/zpico-sys/zenoh-pico/src/system/unix/network.c`

### 55.3 — zpico-sys build.rs NuttX compilation branch

Add a NuttX compilation branch in `packages/zpico/zpico-sys/build.rs`. This is a hybrid of the posix and embedded paths — uses the unix platform sources but cross-compiles via `cc` crate (not CMake).

When `use_nuttx` is true:
1. Read `NUTTX_DIR` env var for include paths (`$NUTTX_DIR/include`)
2. Generate config header with `Z_FEATURE_MULTI_THREAD=1` (NuttX has real pthreads), `Z_FEATURE_LINK_TCP=1`, `Z_FEATURE_LINK_UDP_UNICAST=1`
3. Compile zenoh-pico core sources (api, collections, link, net, protocol, session, transport, utils — same file set as embedded)
4. Compile `src/system/common/` (shared platform code)
5. Compile `src/system/unix/system.c` and `src/system/unix/network.c` (the unix platform, NOT freertos/ or zephyr/)
6. Skip `unix/tls.c` (no TLS for embedded)
7. Define `ZENOH_NUTTX` for all compilations
8. Compile C shim (`zenoh_shim.c`) — the existing `select()` code path works as-is with NuttX sockets

**Files**: `packages/zpico/zpico-sys/build.rs`

### 55.4 — xrce-sys build.rs NuttX compilation branch

Add a NuttX branch in `packages/xrce/xrce-sys/build.rs`. NuttX's POSIX compatibility means:
1. Compile XRCE-DDS client core sources (same as posix)
2. Compile `src/c/util/time.c` (NuttX has `clock_gettime()`)
3. Add NuttX include paths from `NUTTX_DIR` env var
4. Define `UCLIENT_PLATFORM_POSIX` (NuttX is POSIX-compatible)
5. Custom transport via NuttX BSD sockets (same as posix UDP transport)

**Files**: `packages/xrce/xrce-sys/build.rs`

### 55.5 — `just setup-nuttx` dependency acquisition

Add `just setup-nuttx` recipe that:
1. Shallow-clones `apache/nuttx` (`nuttx-12.8.0`) to `third-party/nuttx/nuttx/`
2. Shallow-clones `apache/nuttx-apps` (same tag) to `third-party/nuttx/nuttx-apps/`
3. Prints environment variable configuration (`NUTTX_DIR`, `NUTTX_APPS_DIR`)
4. Idempotent — skips if already present, warns if tag mismatch

Both repos are already gitignored via the `/external/` pattern. Update `just setup` to mention `just setup-nuttx` as optional.

**Files**: `justfile`

### 55.6 — NuttX QEMU defconfig

Create NuttX `defconfig` for QEMU ARM virt with networking + POSIX support, based on upstream `boards/arm/armv7-a/qemu-armv7a/configs/netnsh/defconfig`.

Key additional settings:
```
CONFIG_NET=y
CONFIG_NET_TCP=y
CONFIG_NET_UDP=y
CONFIG_PTHREADS=y
CONFIG_PTHREAD_MUTEX_TYPES=y
CONFIG_PTHREAD_MUTEX_BOTH=y
CONFIG_DEV_URANDOM=y
CONFIG_DRIVERS_VIRTIO_NET=y
CONFIG_BUILD_FLAT=y
CONFIG_DEFAULT_TASK_STACKSIZE=8192
CONFIG_SCHED_WAITPID=y
```

**Files**: `packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/defconfig`

### 55.7 — QEMU board crate (`nros-board-nuttx-qemu-arm`)

Create the NuttX board crate for QEMU ARM virt, following the Phase 51 pattern.

```
packages/boards/nros-board-nuttx-qemu-arm/
├── Cargo.toml
├── build.rs              # Link against NuttX libs, set linker script
├── nuttx-config/defconfig
├── scripts/build-nuttx.sh  # Configure + build NuttX with Rust app
└── src/
    ├── lib.rs            # Re-exports, exit helpers
    ├── config.rs         # Config builder (IP, gateway, zenoh locator, domain_id)
    └── node.rs           # run() — calls user closure, NuttX handles OS init
```

**Simpler than FreeRTOS board crate** because:
- No FreeRTOS kernel / lwIP compilation in build.rs
- No custom Ethernet driver (LAN9118 lwIP netif not needed)
- Rust `std` works — `println!` is native (NuttX provides stdout via serial)
- `run()` just calls the user closure; NuttX boots and initializes networking before the app

**`build-nuttx.sh`** sequence:
1. Copy defconfig to NuttX build tree, run `make olddefconfig`
2. Build NuttX (which calls `cargo build` for the Rust component)
3. Output: `nuttx` ELF binary bootable by QEMU

**Config builder** mirrors `nros-board-mps2-an385`:
- `Config::default()` — talker preset (192.0.3.10, gateway 192.0.3.1)
- `Config::listener()` — listener preset (192.0.3.11)
- Builder methods: `with_ip()`, `with_gateway()`, `with_zenoh_locator()`, `with_domain_id()`

**Files**: `packages/boards/nros-board-nuttx-qemu-arm/`

### 55.8 — Rust zenoh examples (pubsub, service, action)

```
examples/qemu-arm-nuttx/rust/zenoh/
├── talker/              # Pub: std_msgs/Int32 on /chatter
├── listener/            # Sub: std_msgs/Int32 on /chatter
├── service-server/      # Srv: example_interfaces/AddTwoInts
├── service-client/      # Cli: example_interfaces/AddTwoInts
├── action-server/       # Act: example_interfaces/Fibonacci
└── action-client/       # Act: example_interfaces/Fibonacci
```

Each example has:
```
├── Cargo.toml           # deps: nros, nros-board-nuttx-qemu-arm, generated msg types
├── .cargo/config.toml   # target = armv7a-nuttx-eabi, -Z build-std, patch.crates-io
├── package.xml          # For cargo-nano-ros message generation
├── .gitignore           # /target/, /generated/
└── src/main.rs
```

**Key difference from bare-metal examples**: Because NuttX targets support `std`, the examples use standard Rust entry points (not `#![no_std]` / `#![no_main]`):

```rust
use nros::prelude::*;
use nros_board_nuttx_qemu_arm::{Config, run};
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

**Build target**: `armv7a-nuttx-eabi` (Cortex-A7), requires nightly + `-Z build-std`.

**Files**: `examples/qemu-arm-nuttx/rust/zenoh/`

### 55.9 — C zenoh examples (pubsub, service, action)

```
examples/qemu-arm-nuttx/c/zenoh/
├── Makefile             # NuttX app Makefile (builds nros-c + all examples)
├── talker/main.c
├── listener/main.c
├── service-server/main.c
├── service-client/main.c
├── action-server/main.c
└── action-client/main.c
```

Cross-compile `nros-c` as a static library for `armv7a-nuttx-eabi` with `--features "rmw-zenoh,platform-nuttx,ros-humble"`, then link C examples against it via NuttX's build system.

**Files**: `examples/qemu-arm-nuttx/c/zenoh/`

### 55.10 — Integration tests + `just test-nuttx` recipe

Add automated QEMU-based integration tests.

**QEMU launch command**:
```bash
qemu-system-arm -M virt -cpu cortex-a7 -nographic \
    -kernel nuttx \
    -nic tap,ifname=tap-qemu0,script=no,downscript=no
```

Same TAP bridge topology as existing QEMU tests.

**Test files**:
- `packages/testing/nros-tests/tests/nuttx_qemu.rs` — pubsub, service, action tests
- `.config/nextest.toml` — add `nuttx-qemu` test group with `max-threads = 1`

**Justfile recipes**:
```
just build-examples-nuttx      # Build all NuttX QEMU examples (Rust + C)
just test-nuttx                # Run NuttX QEMU integration tests
just test-nuttx verbose=false  # With live output
```

**QEMU test rules** (same as existing bare-metal/FreeRTOS):
- Each QEMU peer uses a different TAP device (talker on `tap-qemu0`, listener on `tap-qemu1`)
- Start subscriber/server first, then publisher/client
- 5s stabilization delay between subscriber connection and publisher start
- Verify zenohd on bridge IP (192.0.3.1:7447)
- `max-threads = 1` for tests sharing the TAP bridge

**Files**: `packages/testing/nros-tests/tests/nuttx_qemu.rs`, `.config/nextest.toml`, `justfile`

### 55.11 — Documentation

- Update `CLAUDE.md`:
  - Add `nros-board-nuttx-qemu-arm` to workspace structure under `packages/boards/`
  - Add `qemu-arm-nuttx` to examples list
  - Update platform backends section to include `platform-nuttx`
  - Add `just test-nuttx` to test groups table
  - Add `just setup-nuttx` to build commands
  - Add `NUTTX_DIR`, `NUTTX_APPS_DIR` to environment variables section
  - Update phase table
- Add `docs/guides/nuttx-setup.md` covering:
  - NuttX source acquisition (`just setup-nuttx`)
  - Environment variable configuration
  - QEMU testing workflow (`just test-nuttx`)
  - NuttX defconfig customization
  - Building and running
- Update `docs/reference/environment-variables.md` with NuttX build-time variables

**Files**: `CLAUDE.md`, `docs/guides/nuttx-setup.md`, `docs/reference/environment-variables.md`

### 55.12 — E2E network tests (pubsub, service, action)

Replace the placeholder network tests in `nuttx_qemu.rs` with real E2E tests that verify message exchange between NuttX QEMU instances via zenohd + TAP bridge.

**Architecture**: Each NuttX example binary is compiled for `armv7a-nuttx-eabi` and currently links against NuttX's std but does NOT include the NuttX kernel (unlike FreeRTOS where `build.rs` compiles the kernel into the binary). For E2E tests:

1. The NuttX kernel must be built separately via `build-nuttx.sh` → `$NUTTX_DIR/nuttx`
2. The Rust app binary must be integrated into the NuttX image (as a builtin app or via ROMFS + ELF loading)
3. The resulting NuttX+app ELF is booted by QEMU

**NuttX QEMU launch** (per instance):
```bash
qemu-system-arm -M virt -cpu cortex-a7 -nographic \
    -kernel <nuttx-image> \
    -nic tap,ifname=tap-qemu0,script=no,downscript=no
```

**Test flow** (e.g., pubsub):
1. Check prerequisites: NuttX kernel built, TAP bridge up, zenohd available
2. Build example binaries (cached via OnceCell)
3. Start zenohd on `tcp/0.0.0.0:7447` (bridge IP 192.0.3.1)
4. Launch listener QEMU on `tap-qemu1` → wait for "Waiting for messages..." marker
5. Launch talker QEMU on `tap-qemu0` → wait for "Publishing messages..."
6. Wait for listener to print "Received" (up to 30s for NuttX boot + zenoh connection)
7. Kill both QEMU instances, verify message count

**QemuProcess extension**: Add `start_nuttx_virt()` method to `qemu.rs` for ARM virt machine with TAP networking. Uses `ManagedProcess` for output pattern matching.

**TAP bridge prerequisite**: Tests check for `tap-qemu0` and `tap-qemu1` interfaces. Skip with helpful message if not found (`sudo ./scripts/qemu/setup-network.sh`).

**IP assignments** (hardcoded in board crate Config):
- Talker: 192.0.3.10 on `tap-qemu0`
- Listener: 192.0.3.11 on `tap-qemu1`
- Server: 192.0.3.12 on `tap-qemu0`
- Client: 192.0.3.13 on `tap-qemu1`

**Tests** (3 E2E + 1 boot test):
- `test_nuttx_boot` — verify NuttX kernel boots to NSH prompt
- `test_nuttx_pubsub_e2e` — talker + listener exchange Int32 messages
- `test_nuttx_service_e2e` — service server + client complete AddTwoInts calls
- `test_nuttx_action_e2e` — action server + client complete Fibonacci goal

**Key difference from native tests**: NuttX binaries have hardcoded `tcp/192.0.3.1:7447` locator, so zenohd must use fixed port 7447 (not ephemeral). Tests use `nuttx-qemu` nextest group with `max-threads = 1`.

**Dependency**: Full E2E requires NuttX build integration (board crate `build.rs` that links NuttX kernel into cargo output, or `build-nuttx.sh` extended to embed Rust apps). Until then, tests skip gracefully with `require_nuttx_e2e()`.

**Files**: `packages/testing/nros-tests/tests/nuttx_qemu.rs`, `packages/testing/nros-tests/src/qemu.rs`

## Future Extensions (Out of Scope)

- **RISC-V NuttX target**: `riscv32imac-nuttx-none-elf` on `qemu-system-riscv32 -M virt` with virtio-net
- **Hardware board crate: ESP32** — NuttX runs on ESP32 with Wi-Fi; alternative to ESP-IDF
- **Hardware board crate: STM32** — NuttX on STM32F7/H7 with Ethernet
- **NuttX SMP**: Multi-core support (ESP32-S3, i.MX RT1170)
- **XRCE-DDS examples on NuttX**: Full XRCE example (micro-ROS already uses this in production)
- **NuttX protected build**: Separate kernel/user space for memory protection
- **Upstream zenoh-pico NuttX PR**: Contribute `ZENOH_NUTTX` support back to eclipse-zenoh/zenoh-pico

## Risks

1. **`getifaddrs()` / `<ifaddrs.h>`**: NuttX may not have this BSD extension needed by zenoh-pico's UDP multicast interface enumeration in `unix/network.c`. Workaround: `#ifdef` guard around `getifaddrs()` usage, default to all-interfaces for multicast. Alternatively, NuttX may support it with `CONFIG_NET_NETDEV_IFINDEX`.
2. **NuttX Rust Tier 3 targets**: Require nightly + `-Z build-std`. May have toolchain stability issues. Same constraint as Zephyr Rust examples.
3. **NuttX build system integration**: Different from the cargo-centric workflow used by other platforms. The `build-nuttx.sh` script adds complexity vs pure `cargo build`.
4. **QEMU virtio-net + NuttX**: Need to verify NuttX's virtio-net driver works correctly with TAP bridge networking in QEMU ARM virt machine.

## Acceptance Criteria

- [ ] `platform-nuttx` feature compiles cleanly for `armv7a-nuttx-eabi`
- [ ] Mutual exclusivity enforced: enabling `platform-nuttx` + any other platform → build error
- [ ] Feature flag chain works: `nros` → `nros-node` → `nros-rmw-zenoh` → `zpico-sys` all forward correctly
- [ ] zenoh-pico unix platform compiles with `ZENOH_NUTTX` define (RNG uses /dev/urandom)
- [ ] QEMU board crate `run()` starts NuttX app + zenoh-pico session
- [ ] Rust pubsub example exchanges messages over QEMU TAP bridge via zenohd
- [ ] Rust service example completes request/response cycle on QEMU
- [ ] Rust action example completes goal/result cycle on QEMU
- [ ] C pubsub example exchanges messages over QEMU TAP bridge via zenohd
- [ ] C service example completes request/response cycle on QEMU
- [ ] C action example completes goal/result cycle on QEMU
- [ ] `just test-nuttx` runs all QEMU integration tests and passes
- [ ] `just quality` passes (NuttX board crate excluded from default workspace if `NUTTX_DIR` unset)
- [ ] Orthogonality preserved: `platform-nuttx` does not imply any RMW backend or ROS edition

## Notes

- **No smoltcp or lwIP needed**: NuttX has its own TCP/IP stack with BSD sockets. The `zpico-smoltcp` and `lan9118-lwip` crates are not used. This is the simplest networking integration of any embedded platform.
- **Rust `std` support**: Unlike bare-metal (`no_std`) and FreeRTOS (`no_std`), NuttX targets support `std`. Standard `println!`, `std::thread`, `std::io`, and `std::time` are available. The nros executor API remains the same — `std` just provides convenience for the application layer.
- **Nightly required**: NuttX Rust targets are Tier 3, requiring `nightly` and `-Z build-std`. This is the same requirement as Zephyr Rust examples.
- **No custom Ethernet driver**: Unlike FreeRTOS (which needed a LAN9118 lwIP netif driver, ~200–300 LOC C), NuttX's virtio-net driver is built into the kernel. Eliminates a significant chunk of C code.
- **NuttX Shell (NSH)**: NuttX boots to an interactive shell for debugging: `ifconfig`, `ping`, `ls /dev/`, etc. Helpful during development.
- **XRCE is the stronger combo**: micro-ROS validates NuttX+XRCE in production. Zenoh-pico on NuttX is novel but feasible given POSIX compatibility.
- **NuttX build system**: NuttX apps are typically built as part of the NuttX tree (flat build) or as external ELFs (kernel build). For QEMU validation, flat build is simplest — the Rust app is compiled by cargo and linked into the NuttX image by the NuttX build system.
- **POSIX compliance gaps**: While NuttX is broadly POSIX-compliant, minor gaps exist: `pthread_cancel()` requires `CONFIG_CANCELLATION_POINTS`, `getaddrinfo()` requires `CONFIG_NETDB_DNSCLIENT`. The defconfig in 55.6 enables all required options.
- **MSG_NOSIGNAL**: NuttX supports `MSG_NOSIGNAL` for `send()`, so the `ZENOH_LINUX` code path in zenoh-pico's `_z_send_tcp` works. The `ZENOH_NUTTX` define should follow the `ZENOH_LINUX` path for TCP send flags and memory management in `network.c`.
