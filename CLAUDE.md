# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std`.

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C idents, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nano_ros_generate_interfaces()`)

## Workspace
`packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen}/`, `examples/`, `third-party/` (gitignored SDKs), `zephyr/` module. Run `ls packages/` for current crate list.

## Build
- `just setup` / `just doctor` / `just check` / `just ci` (check + test-all) / `just verify` (Kani+Verus) / `just generate-bindings`
- `just <module> setup`: workspace, verification, qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32, zephyr, xrce, zenohd

**Build tiers** (each strict superset): `build` (workspace + transports) ⊂ `build-examples` ⊂ `build-all` (= `build-examples` + `build-test-fixtures`).

**Test tiers** (each strict superset): `test-unit` (~5s) ⊂ `test-integration` (~30s) ⊂ `test` ⊂ `test-all` (+ heavy QEMU/Zephyr/ROS-interop + `test-doc` + `test-miri` + C codegen).

Per-platform: `just <plat> test|test-all|ci`. GNU `parallel` auto-used; `RUSTC_WRAPPER=sccache` auto-detected.

## Environment
`.env` (gitignored). **Run `direnv allow` once after clone** else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`.

Runtime: `ROS_DOMAIN_ID` (0), `ZENOH_LOCATOR` (`tcp/127.0.0.1:7447`), `ZENOH_MODE`.

SDK paths auto from `third-party/<sdk>/`; override `<SDK>_DIR` env. See `docs/reference/environment-variables.md`.

## Practices
- **Always `just ci` after task.** **Never `sudo`** — tell user.
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash, then promote.
- **Tests must fail on unmet preconditions.** `assert!()`/`bail!()` for missing env/binary. `nros_tests::skip!` panics with `[SKIPPED]` (OK). Bare `eprintln!`+`return` reports PASS — never. Same rule runtime: panic, not silent early-return. Exception: `rstest #[values]` matrix unsupported via `skip_reason()`.
- JUnit XML at `target/nextest/default/junit.xml`. Non-nextest → `tests/run-test.sh` → `test-logs/latest/`.
- Temp files in `$project/tmp/` (gitignored), not `/tmp`. Use Write/Edit, not heredoc. Repeated multi-step → `tmp/*.sh` script.
- `.gitignore`: every workspace-excluded crate has per-dir `.gitignore` with `/target/` (and `/generated/` if codegen). Native C++ examples need `/build/`. Always leading `/`. Add `--target-dir` paths.
- **Parallel build isolation:** nextest tests with different features building same example MUST use `--target-dir` (e.g. `target-safety/`, `target-zero-copy/`). See `fixtures/binaries.rs`.

### QEMU Networked Tests
- Slirp networking (no TAP/sudo/bridges).
- Per-platform zenohd ports in `nros_tests::platform`: baremetal=7450, freertos=7451, nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456. `ZenohRouter::start(platform::FREERTOS.zenohd_port)`.
- Bridge-net (threadx-linux veth): `ZenohRouter::start_on("0.0.0.0", port)`.
- Subscriber first, then publisher. 5–10s stabilization.
- Per-platform nextest groups (`max-threads = 1`); platforms parallel.

### Examples = Standalone Projects
**Each `examples/` dir is self-contained, copy-out template.**
- No shared example-only helpers in `nros-cpp`/`nros-c` — boilerplate IS lesson.
- `*_DIR` env / `-D` injection = SDK-path contract. Example cmake accepts env or `-D` only — never project-tree heuristics.
- Per-example `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` build in isolation. No workspace reliance, no walk-up.
- Per-platform `cmake/<plat>-support.cmake` in example tree. Layer-2 (`nros-{threadx,freertos,nuttx}.cmake`) ship via `find_package(NanoRos)`.

### CMake Path Convention
- Never hard-code project-relative paths in example cmake **or in
  `packages/<crate>/CMakeLists.txt`, `cmake/*.cmake` modules, build.rs,
  or any in-tree script**. Each subproject (`packages/dds/<name>`,
  `packages/core/<name>`, `examples/<dir>`) must build standalone — no
  walking up the source tree.
- No `../../../cmake/...`, no project-root heuristics, no
  `${_ROOT}/external/<sdk>` defaults, no `$<source_dir>/../../../scripts/...`
  in `install(...)` rules.
- **Drivers pass absolute paths.** The `just`-recipe / outer build
  script knows the layout and supplies it via cmake `-D…=$PWD/...` or
  env var:
  - `-DCMAKE_TOOLCHAIN_FILE=`, `-D<SDK>_DIR=`, `-DCMAKE_PREFIX_PATH=$pwd/build/install/`,
    `-D<BOARD>_CONFIG_DIR=`.
  - Project-internal scripts shipped to the install: pass via a
    cache var like `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL_SOURCE=$PWD/scripts/...`;
    the project's `install(PROGRAMS ...)` reads the cache var, errors
    out if it isn't absolute.
- **Find-program / find-package fallbacks may use install-relative
  paths.** Once installed, a CMake config at `<prefix>/lib/cmake/<Pkg>/`
  legitimately knows that companion files live at
  `<prefix>/share/<pkg>/` — `${CMAKE_CURRENT_LIST_DIR}/../../share/<pkg>`
  resolves inside the install layout, not the source tree, and is
  fine. The forbidden pattern is the source-tree variant
  (`${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/<dir>`).

### Roadmap Docs
`docs/roadmap/`: header (Goal/Status/Priority/Depends on) → Overview → Architecture → Work Items + `### N.M — Title` + `**Files**` → Acceptance → Notes. `- [x]` done. Completed → `archived/`.

## Key Patterns
- **Zenoh pinned 1.7.2** (rmw_zenoh_cpp compat). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024**: `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]` (420+ FFI ops).
- **API**: Rust mirrors rclrs 0.7.0; C mirrors rclc. `create_publisher/subscription/service/client/action_*`. `Publisher<M>`. Error `RclrsError`.
- **`no_std`**: all core crates `#![no_std]` + optional `std`/`alloc`.
- **Messages**: `cargo nano-ros generate-rust` from `package.xml`. **Never hand-write.** Example `generated/` gitignored. Only `packages/interfaces/rcl-interfaces/generated/` in git (`nros-` prefix). Bundled at `packages/codegen/interfaces/`. `nros-core` re-exports `heapless`.
- **C API** (`docs/reference/c-api-cmake.md`): **thin wrapper** delegates to `nros-node`, no logic re-impl. cbindgen 0.29 → `nros_generated.h`. `#[repr(C)]` fields `pub`. Hand-written: `visibility.h`, `platform.h`, `types.h`. Platform FFI uses `/// cbindgen:ignore`.
- **C++ API**: `nros-cpp` freestanding C++14 over typed extern "C" FFI to `nros-node`. Mirrors rclcpp. Error `nros::Result` + `NROS_TRY`. Codegen `cargo nano-ros generate-cpp` or CMake `nano_ros_generate_interfaces(... LANGUAGE CPP)`. Std opt-in via `NROS_CPP_STD`. Zephyr `CONFIG_NROS_CPP_API=y`. Action client/server blocking `zpico_get` hangs FreeRTOS QEMU; `test_freertos_cpp_action_e2e` `#[ignore]` until Phase 77 async.

### Platform Backends
Three orthogonal axes (compile-time mutual excl, zero on axis OK):
- **RMW**: `rmw-zenoh|rmw-xrce|rmw-dds`
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}`
- **ROS edition**: `ros-{humble,iron}`

Default `std`. Cross-cutting: `unstable-zenoh-api` (zero-copy receive).

### Spin/Yield
`zpico_spin_once` event-driven wake:
- POSIX/Zephyr: `_z_condvar_wait_until` on `g_spin_cv`
- FreeRTOS: `xSemaphoreTake(g_spin_sem, …)`
- NuttX: `sem_timedwait(&g_spin_sem_posix, …)` (pthread condvar hangs — Phase 55.12)
- Bare-metal: single-thread `zp_read` loop

Cooperative yield (`PlatformYield`): POSIX/NuttX `sched_yield()`, Zephyr `k_yield()`, FreeRTOS `vPortYield()`, ThreadX `tx_thread_relinquish()`, bare-metal `core::hint::spin_loop()` default, opt-in `cortex_m::asm::wfi()` via `BoardIdle`. RTOS yields not ISR-safe; `spin_loop()` is.

### smoltcp Multicast (bare-metal)
- `Interface::join_multicast_group(addr)` needs multicast addr; smoltcp 0.12 returns `Unaddressable` for `0.0.0.0`. Pass GROUP (`239.255.0.1`).
- `set_recv_timeout(_, 0)` in `define_smoltcp_platform!` = non-blocking poll.
- LAN9118 emulator filter rejects multicast unless `MAC_CR.MCPAS`; promiscuous (`PRMS`) recommended for QEMU `-nic socket,…`.
- `MAX_UDP_SOCKETS` default 4. RTPS needs 3/participant; zenoh/xrce 0..=1.

### NetX Duo BSD (ThreadX)
- `SO_RCVTIMEO` takes `struct nx_bsd_timeval *`, NOT `INT` ms. Wrong type → `wait_option = NX_WAIT_FOREVER` → deadlock. Use `nros-platform-threadx::set_recv_timeout_ms`.
- `fcntl(F_SETFL, O_NONBLOCK)` works (toggles `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING`).
- NSOS-NetX shim translates `SO_RCVTIMEO` for threadx-linux. Accepts INT-ms and `nx_bsd_timeval`.

### Board Transport Features
`ethernet` (default MPS2-AN385/STM32F4/ESP32-QEMU) or `wifi` (ESP32) → TCP/UDP via `zpico-smoltcp`; `serial` → UART via `zpico-serial` (bare-metal) or zenoh-pico built-in (ESP32, Zephyr). `Config` fields `#[cfg(feature)]`-gated. ≥1 transport required (`compile_error!`). Coexist OK (locator selects). ESP32/ESP32-QEMU use zenoh-pico's serial (no `zpico-serial`).

### Parameter Services
`param-services` feature in `nros-node` → `~/get_parameters`, `~/set_parameters`, etc. Uses `nros-rcl-interfaces`. Handlers return `Box<Response>`.

### Verification
- Kani: 160 bounded harnesses. `just verify-kani` (~3 min)
- Verus: 102 unbounded proofs. `just verify-verus` (~1 sec)
- Verus: `external_type_specification` w/o `external_body` = transparent enum; with = opaque. Never `verify = true` on production crates with fn pointers/closures. See `docs/guides/verus-verification.md`.

### ROS 2 Interop
rmw_zenoh-compatible. Key `<domain>/<topic>/<type>/TypeHashNotSupported`. See `docs/reference/rmw_zenoh_interop.md`.

## Phases
Active in `docs/roadmap/`, completed in `docs/roadmap/archived/`. Run `ls docs/roadmap/` for status.

Phase 117 (Cyclone DDS RMW + Autoware safety-island): Cyclone DDS submodule pinned tag `0.10.5` at `third-party/dds/cyclonedds/` (matches `ros-humble-cyclonedds` 0.10.5). `nros-rmw-cyclonedds` standalone CMake project at `packages/dds/nros-rmw-cyclonedds/` (NOT a Cargo crate); registers C++ vtable via `nros_rmw_cffi_register`. **Goal: full wire-compat with stock `rmw_cyclonedds_cpp`**. `NANO_ROS_RMW=cyclonedds` CMake option auto-pulls Cyclone backend + flips on `NROS_RMW_CYCLONEDDS=1` macro that triggers register call inside `nros::init`. Driver: `just cyclonedds {setup,build,build-rmw,test,doctor,clean}`. Pub/sub + services + raw-CDR data plane wired (117.1–117.9 done). Stock-RMW interop pending (117.X.1 rosidl_adapter codegen → 117.X.2 topic prefix conventions `rt/`/`rq/`/`rr/` → 117.X.3 replace `ServiceEnvelope` with upstream `cdds_request_header_t` → 117.X.4 type-name mangling verification → 117.X.5 service QoS alignment → 117.12 POSIX E2E vs stock RMW). Interim 117.7.B `ServiceEnvelope` works for nano-ros↔nano-ros only; 117.X.3 supersedes.

## Quick Reference
`book/src/reference/build-commands.md`: manual testing, ROS 2 interop, Docker, QEMU, Zephyr. Build book: `just book`.

Docs: `book/src/` (user, mdbook) — getting-started, user-guide, porting, reference, concepts, internals. `docs/` (contributor) — reference, design, research, roadmap.
