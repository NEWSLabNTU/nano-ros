# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std` compatible.

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C identifiers, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nano_ros_generate_interfaces()`)

## Workspace

```
packages/
├── core/         # nros, nros-core, nros-serdes, nros-macros, nros-params, nros-rmw, nros-node, nros-c, nros-cpp, nros-platform*
├── zpico/        # Zenoh-pico backend (nros-rmw-zenoh, zpico-sys, zpico-platform-*)
├── xrce/         # XRCE-DDS backend
├── dds/          # dust-dds backend (nros-rmw-dds)
├── boards/       # nros-board-* (mps2-an385, stm32f4, esp32, threadx-*, nuttx-qemu-arm, …)
├── drivers/      # lan9118-*, openeth-smoltcp, virtio-net-netx, threadx-netx-sys, nros-smoltcp
├── interfaces/   # rcl-interfaces (generated/, in git)
├── testing/      # nros-tests
├── verification/ # nros-ghost-types, nros-verification (Verus, excluded from workspace)
├── reference/    # qemu-smoltcp-bridge, stm32f4-porting/*
└── codegen/      # cargo-nano-ros, rosidl-*, bundled .msg/.srv
examples/         # platform/lang/rmw/use-case (native, qemu-arm-baremetal, qemu-arm-freertos, qemu-arm-nuttx, qemu-riscv64-threadx, threadx-linux, qemu-esp32-baremetal, esp32, stm32f4, zephyr, px4)
third-party/      # SDK sources (gitignored): freertos, nuttx, threadx, netxduo, lwip, zenoh, dust-dds, px4
zephyr/           # Zephyr module
```

## Build

```bash
just setup              # install everything (workspace + verification + platforms + services)
just doctor             # diagnose install (read-only)
just build              # bindings + workspace + examples
just check              # fmt + clippy
just ci                 # check + test
just verify             # Kani + Verus
just generate-bindings  # regenerate generated/ dirs
just <module> setup     # per-module setup; modules: workspace, verification, qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32, zephyr, xrce, zenohd
```

Tests: `just test-unit`, `just test-miri`, `just test`, `just test-all`, `just <plat> test|test-all|ci`.

## Environment

`.env` (gitignored) overrides defaults. **Run `direnv allow` once after clone** so cargo/cmake outside `just` pick up SDK paths. Without it, `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`.

Runtime: `ROS_DOMAIN_ID` (0), `ZENOH_LOCATOR` (`tcp/127.0.0.1:7447`), `ZENOH_MODE` (client/peer).

SDK paths auto-resolved from `third-party/<sdk>/`; override via env: `FREERTOS_DIR`, `FREERTOS_PORT` (`GCC/ARM_CM3`), `LWIP_DIR`, `FREERTOS_CONFIG_DIR`, `NUTTX_DIR`, `NUTTX_APPS_DIR`, `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR`. See `docs/reference/environment-variables.md`.

## Practices

- **Always `just ci` after task.**
- **Never `sudo`** — tell user what's needed.
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash, then promote.
- **Tests must fail on unmet preconditions.** `assert!()`/`bail!()` for missing env/binary. `nros_tests::skip!` panics with `[SKIPPED]` (OK). Bare `eprintln!`+`return` reports PASS — never. Same rule for runtime: must panic, not silent early-return. Only exception: `rstest #[values]` matrix unsupported combos via `skip_reason()` helper.
- JUnit XML at `target/nextest/default/junit.xml`. Non-nextest tests → `tests/run-test.sh` → `test-logs/latest/`.
- Temp files in `$project/tmp/` (gitignored), not `/tmp`. Use Write/Edit, not heredoc. Repeated multi-step commands (QEMU, GDB, build+run) → `tmp/*.sh` script first.
- `.gitignore`: every workspace-excluded crate has per-dir `.gitignore` with `/target/` (and `/generated/` if uses codegen). Native C++ examples need `/build/`. Always leading `/`. Add `--target-dir` paths.
- **Parallel build isolation:** nextest tests with different features building the same example must use `--target-dir` (e.g. `target-safety/`, `target-zero-copy/`). See `fixtures/binaries.rs`.

### QEMU Networked Tests
- Slirp networking on QEMU platforms (no TAP/sudo/bridges).
- Per-platform zenohd ports in `nros_tests::platform`: baremetal=7450, freertos=7451, nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456. Use `ZenohRouter::start(platform::FREERTOS.zenohd_port)`.
- Bridge-networked (threadx-linux veth): `ZenohRouter::start_on("0.0.0.0", port)`.
- Subscriber first, then publisher. 5–10s stabilization between sub-ready and pub-start.
- Per-platform nextest groups (`max-threads = 1`); platforms run in parallel.

### Examples = Standalone Projects
**Each `examples/` dir is self-contained, copy-out template.** Implications:
- No shared example-only helpers in `nros-cpp`/`nros-c` — boilerplate IS the lesson.
- `*_DIR` env / `-D` injection are the SDK-path contract. Example cmake must accept env or `-D` only — never project-tree heuristics.
- Per-example `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` build in isolation. No workspace `Cargo.toml` reliance, no walk-up to project root.
- Per-platform `cmake/<plat>-support.cmake` lives in example tree. Layer-2 modules (`nros-threadx.cmake`, `nros-freertos.cmake`, `nros-nuttx.cmake`) ship via `find_package(NanoRos)`.

### CMake Path Convention
- Never hard-code project-relative paths in example `CMakeLists.txt` or support cmake.
- No fixed-depth `../../../cmake/...`, no project-root heuristics, no `${_ROOT}/external/<sdk>` defaults.
- Build scripts pass absolute paths: `-DCMAKE_TOOLCHAIN_FILE=`, `-D<SDK>_DIR=`, `-DCMAKE_PREFIX_PATH=$pwd/build/install/`, `-D<BOARD>_CONFIG_DIR=`. Internal example-tree paths fine.

### Roadmap Docs (`docs/roadmap/`)
Header (Goal, Status, Priority, Depends on) → Overview → Architecture → Work Items checklist + `### N.M — Title` subsections + `**Files**` → Acceptance → Notes. Mark `- [x]` done. Completed → `docs/roadmap/archived/`.

## Key Patterns

- **Zenoh pinned to 1.7.2** (rmw_zenoh_cpp compatible). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024**: `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]` (420+ FFI ops).
- **API**: Rust mirrors rclrs 0.7.0; C mirrors rclc. `create_publisher/subscription/service/client/action_*`. Types `Publisher<M>`, etc. Error: `RclrsError`.
- **`no_std`**: all core crates `#![no_std]` + optional `std`/`alloc`.
- **Messages**: `cargo nano-ros generate-rust` from `package.xml`. **Never hand-write.** Example `generated/` gitignored. Only `packages/interfaces/rcl-interfaces/generated/` in git (uses `nros-` prefix). Bundled at `packages/codegen/interfaces/`. `nros-core` re-exports `heapless`.
- **C API**: see `docs/reference/c-api-cmake.md`. **Thin wrapper principle:** must delegate to `nros-node`, no logic re-impl. Headers auto-generated by cbindgen 0.29 → `nros_generated.h`. `#[repr(C)]` fields must be `pub`. Hand-written: `visibility.h`, `platform.h`, `types.h`. Platform FFI uses `/// cbindgen:ignore`.
- **C++ API**: `nros-cpp` is freestanding C++14 over typed extern "C" FFI to `nros-node`. Mirrors rclcpp. Error: `nros::Result` + `NROS_TRY`. Codegen: `cargo nano-ros generate-cpp` or CMake `nano_ros_generate_interfaces(... LANGUAGE CPP)`. Std mode opt-in via `NROS_CPP_STD`. Zephyr: `CONFIG_NROS_CPP_API=y`. Action client/server use blocking `zpico_get` → hangs FreeRTOS QEMU; `test_freertos_cpp_action_e2e` `#[ignore]` until Phase 77 async path.

### Platform Backends
Three orthogonal axes (mutual exclusion enforced at compile-time, zero on an axis OK):
- **RMW**: `rmw-zenoh`, `rmw-xrce`, `rmw-dds`
- **Platform**: `platform-posix|zephyr|bare-metal|freertos|nuttx|threadx`
- **ROS edition**: `ros-humble|iron`

Default: `std`. Cross-cutting: `unstable-zenoh-api` for zero-copy receive.

### Spin/Yield (per platform)
`zpico_spin_once` event-driven wake on data:
- POSIX/Zephyr: `_z_condvar_wait_until` on `g_spin_cv`
- FreeRTOS: `xSemaphoreTake(g_spin_sem, …)`
- NuttX: `sem_timedwait(&g_spin_sem_posix, …)` (pthread condvar hangs — Phase 55.12)
- Bare-metal: single-thread `zp_read` loop

Cooperative yield (Phase 77.22 `PlatformYield`): POSIX/NuttX `sched_yield()`, Zephyr `k_yield()`, FreeRTOS `vPortYield()`, ThreadX `tx_thread_relinquish()`, bare-metal default `core::hint::spin_loop()`, opt-in `cortex_m::asm::wfi()` via `BoardIdle` trait. RTOS yields not ISR-safe; `spin_loop()` is.

### smoltcp Multicast (bare-metal)
- `Interface::join_multicast_group(addr)` requires multicast addr; smoltcp 0.12 returns `Unaddressable` for `0.0.0.0`. Pass GROUP addr (`239.255.0.1`), not local-bind.
- `set_recv_timeout(_, 0)` in `define_smoltcp_platform!` macro = non-blocking poll. Pre-Phase-97.3 silently fell back to `SOCKET_TIMEOUT_MS` (10s).
- LAN9118 emulator filter rejects multicast unless `MAC_CR.MCPAS` set; promiscuous (`PRMS`) recommended for QEMU `-nic socket,…`.
- `MAX_UDP_SOCKETS` default 4 (was 2). RTPS needs 3/participant; zenoh/xrce use 0..=1.

### NetX Duo BSD (ThreadX)
- `SO_RCVTIMEO` takes `struct nx_bsd_timeval *`, NOT `INT` ms. Wrong type → `wait_option = NX_WAIT_FOREVER` → deadlock. Use `nros-platform-threadx::set_recv_timeout_ms`.
- `fcntl(F_SETFL, O_NONBLOCK)` works (toggles `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING`).
- NSOS-NetX shim translates `SO_RCVTIMEO` for threadx-linux (NetX BSD ↔ Linux POSIX). Accepts both INT-ms and `nx_bsd_timeval` shapes.

### Board Transport Features
Cargo features select transport: `ethernet` (default for MPS2-AN385/STM32F4/ESP32-QEMU) or `wifi` (ESP32) → TCP/UDP via `zpico-smoltcp`; `serial` → UART via `zpico-serial` (bare-metal) or zenoh-pico built-in (ESP32, Zephyr). `Config` fields `#[cfg(feature = "...")]`-gated. At least one transport required (`compile_error!`). Both can coexist (locator selects). ESP32/ESP32-QEMU use zenoh-pico's serial (no `zpico-serial` dep).

### Parameter Services
`param-services` feature in `nros-node` → `~/get_parameters`, `~/set_parameters`, etc. Uses `nros-rcl-interfaces`. Handlers return `Box<Response>`.

### Verification
- Kani: 160 bounded harnesses. `just verify-kani` (~3 min)
- Verus: 102 unbounded proofs. `just verify-verus` (~1 sec)
- Verus rules: `external_type_specification` w/o `external_body` = transparent enum; with = opaque. Never `verify = true` on production crates with fn pointers/closures. See `docs/guides/verus-verification.md`.

### ROS 2 Interop
rmw_zenoh-compatible. Key: `<domain>/<topic>/<type>/TypeHashNotSupported`. See `docs/reference/rmw_zenoh_interop.md`.

## Phases

Archived in `docs/roadmap/archived/`. See `docs/roadmap/` for active.

| # | Focus | Status |
|---|-------|--------|
| 23 | Arduino precompiled lib | Not Started |
| 41 | Iron type hash support | Not Started |
| 64 | Embedded transport tuning guide | In Progress (64.1 done) |
| 65 | nano-ros book | Complete |
| 69 | Cross-platform C/C++ examples + tests | Complete |
| 71 | DDS on `nros-platform` traits (POSIX + Zephyr A9) | Archived |
| 73 | Memory + zero-copy receive | Complete |
| 75 | Relocatable CMake install | Complete |
| 76 | RTOS scheduling via config.toml | Complete (FreeRTOS only) |
| 77 | Async action client | In Progress (77.1–77.5) |
| 78 | Colcon build type | Not Started |
| 79 | Unified platform abstraction | Complete |
| 80 | Unified network interface | Not Started |
| 81 | Zephyr native_sim multi-instance E2E | Complete |
| 82 | Blocking service client takes executor | Complete |
| 83 | C/C++ thin-wrapper compliance | Complete |
| 85 | Test-suite consolidation | Complete |
| 86 | `nros-lifecycle-msgs` + REP-2002 | Complete |
| 87 | nros-cpp compile-time storage sizes | Complete |
| 88 | Unified leveled logging (`nros-log`) | Not Started |
| 89 | `just test-all` triage (675/675) | Complete |
| 90 | PX4 RMW + `nros-px4` board (SITL E2E) | Archived (v1 + 90.5b Complete) |
| 91 | Antipattern fixup | Complete |
| 92 | Zephyr DDS on `qemu_cortex_a9` | Complete |
| 93 | C/C++ Doxygen + porting surface | Complete |
| 95 | Example coverage parity (51 new crates) | Archived |
| 96 | Phase 95 cross-process E2E follow-ups | Archived (Complete) |
| 97 | DDS per-platform examples + E2E | Archived (6/7 slices Complete; esp32-qemu deferred to 101) |
| 98 | PX4-Autopilot vendoring + SITL infra | Archived (Complete) |
| 99 | Zero-copy raw pub/sub API | Archived (v1 Complete; post-v1 99.J/99.K open) |
| 100 | AGX Orin SPE (Cortex-R5F + IVC) | Not Started |
| 101 | `portable-atomic-util::Arc` substitution | Not Started |
| 102 | RMW API alignment (`nros_rmw_ret_t` + entity structs) | Archived (Complete) |
| 103 | RMW typed-loan path | Archived (Cancelled — superseded by 99 + 99.L) |
| 104 | Multi-backend support (cross-domain bridges) | Not Started |
| 108 | RMW surface extensions (status events + full DDS-shaped QoS) | Not Started |
| 110 | RT execution model (incl. RMW `next_deadline_ms`; absorbs former Phase 105) | Not Started |

## Quick Reference

`book/src/reference/build-commands.md` for manual testing, ROS 2 interop, Docker, QEMU, Zephyr setup. Build book: `just book`.

## Doc Index

```
book/src/              # User-facing (mdbook)
├── getting-started/   # installation, native, zephyr, freertos, nuttx, threadx, bare-metal, esp32, ros2-interop
├── user-guide/        # rmw-backends, configuration, message-generation, serial-transport, troubleshooting
├── porting/           # custom-rmw, custom-platform, custom-board
├── reference/         # rust-api, c-api, cpp-api, rmw-api, platform-api, environment-variables, build-commands
├── concepts/          # architecture, no-std, platform-model
└── internals/         # rmw-api-design, rmw-zenoh-protocol, scheduling, verification, realtime, safety, …

docs/                  # Contributor/internal
├── reference/         # rmw-h-analysis, xrce-dds-analysis, executor-fairness
├── design/            # rmw-layer, example-directory-layout, zonal-vehicle
├── research/
└── roadmap/           # active + archived
```
