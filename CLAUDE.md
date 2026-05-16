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
- **Always use nightly for `rustfmt` / `cargo fmt`.** `rustfmt.toml` enables nightly-only options (`imports_granularity = "Crate"`, `format_code_in_doc_comments = true`); the stable toolchain warns and skips them, producing a different output than CI. Run `cargo +nightly fmt` (or `rustup run nightly cargo fmt`).
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash, then promote.
- **Tests must fail on unmet preconditions.** `assert!()`/`bail!()` for missing env/binary. `nros_tests::skip!` panics with `[SKIPPED]` (OK). Bare `eprintln!`+`return` reports PASS — never. Same rule runtime: panic, not silent early-return. Exception: `rstest #[values]` matrix unsupported via `skip_reason()`.
- JUnit XML at `target/nextest/default/junit.xml`. Non-nextest → `tests/run-test.sh` → `test-logs/latest/`.
- Temp files in `$project/tmp/` (gitignored), not `/tmp`. Use Write/Edit, not heredoc. Repeated multi-step → `tmp/*.sh` script.
- `.gitignore`: every workspace-excluded crate has per-dir `.gitignore` with `/target/` (and `/generated/` if codegen). Native C++ examples need `/build/`. Always leading `/`. Add `--target-dir` paths.
- **Parallel build isolation:** nextest tests with different features building same example MUST use `--target-dir` (e.g. `target-safety/`, `target-zero-copy/`). See `fixtures/binaries.rs`.
- **Narrow platform build first.** Platform-specific failure → run `just <platform> build-all` (e.g. `just zephyr build-all`) before root `just build-all`. Closest variant (`just esp32 build`, `just qemu build`) if no `-all`.
- **Submodule rebase on pull.** Inspect submodules after pull/rebase. Remote pointer advanced + local submodule work → enter submodule, fetch, rebase local onto upstream, check out superproject's expected commit, record in parent commit. Never leave submodule at older local commit when remote moved.
- **No POSIX-style Rust constructor sections on Zephyr/native_sim.** `nros-cpp` ships weak `nros_app_register_backends` default; `nros_cpp_init` explicitly registers linked CFFI backend (Phase 127.C.4, `ffdde60f`). Don't assume `ctor`/linker-set registration runs on Zephyr — wire backend init explicitly.

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
- **cbindgen output as canonical FFI** (Phase 118.D): nros-cpp `*.hpp` headers `#include "nros_cpp_ffi.h"` directly; per-file hand-written `extern "C"` redeclaration blocks have been removed (drift broke Phase 112 once). `qos.hpp` keeps a fallback redef under `#ifndef NROS_CPP_FFI_H` for callers that don't pull the cbindgen header. Three exceptions: `parameter.hpp` cross-references nros-c's `<nros/parameter.h>`; `action_{client,server}.hpp` `reinterpret_cast` `goal_id` at FFI callsites because cbindgen renders `*const [u8; 16]` as ptr-to-array. `set_callbacks` excluded from cbindgen via `[export.exclude]` (Rust `Option<extern "C" fn>` becomes opaque `Option_*` struct C++ can't construct); declared locally with plain fn-ptr typedefs. cbindgen variants prefixed with enum name (`prefix_with_name = true`) to avoid C++ name collisions.
- **Probe-only opaque sizes** (Phase 118.B / 87.6): `EXECUTOR_OPAQUE_U64S` etc. derived from `nros::sizes::EXECUTOR_SIZE` via `nros_sizes_build` rlib probe — no hand-math upper bound. Per-consumer `const _: () = assert!(size_of::<Ty>() <= STORAGE_SIZE …)` enforces compile-time correctness. Probe=0 only on `cargo check --no-default-features` (warns + 1-word placeholder; resulting rlib must not be linked). `CppContext` adds explicit `CPP_CONTEXT_OVERHEAD = 8` (u32 domain_id + alignment padding) on top of `Executor`.
- **Wrapper timing** (Phase 118.C.b): `Future::wait()`, `Stream::wait_next()`, `Executor::spin(duration_ms)` budget by wall-clock via `nros_cpp_time_ns()`. Iteration-count loops collapse on early-wake from signaled condvars (keep-alives, discovery gossip).

### Platform Backends
Three orthogonal axes (compile-time mutual excl, zero on axis OK):
- **RMW**: `rmw-zenoh|rmw-xrce|rmw-dds|rmw-cyclonedds`
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}`
- **ROS edition**: `ros-{humble,iron}`

RMW backend host-language policy (frozen 2026-05-07): see `book/src/internals/rmw-backends.md`. Rule: backend's host language matches its underlying library's native language unless overridden. Today: dust-dds=Rust, cyclonedds=C++, XRCE=Rust→C (115.K.2), zenoh-pico=Rust (deferred), uORB=Rust (won't-do).

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

### XRCE Embedded Build (Phase 115.K.2 + 118)
`nros-rmw-xrce-cffi` (C FFI shim) gates `UCLIENT_PROFILE_{UDP,TCP,SERIAL}` + `UCLIENT_PLATFORM_POSIX` + `transport_posix_{udp,serial}.c` source files on `target_os = linux|macos|*bsd`. Bare-metal targets (`target_os = "none"`) get only `UCLIENT_PROFILE_{DISCOVERY,CUSTOM_TRANSPORT,STREAM_FRAMING}` and must inject their own custom transport. `just check-workspace-embedded` excludes `nros-rmw-xrce{,-cffi}` (header-only K.2 backend's `internal.h` references UDP types unconditionally — upstream design issue).

### Verification
- Kani: 160 bounded harnesses. `just verify-kani` (~3 min)
- Verus: 102 unbounded proofs. `just verify-verus` (~1 sec)
- Verus: `external_type_specification` w/o `external_body` = transparent enum; with = opaque. Never `verify = true` on production crates with fn pointers/closures. See `docs/guides/verus-verification.md`.

### ROS 2 Interop
rmw_zenoh-compatible. Key `<domain>/<topic>/<type>/TypeHashNotSupported`. See `docs/reference/rmw_zenoh_interop.md`.

## Phases
Active in `docs/roadmap/`, completed in `docs/roadmap/archived/`. Run `ls docs/roadmap/` for status.

Phase 117 (Cyclone DDS RMW + Autoware safety-island): Cyclone DDS submodule pinned tag `0.10.5` at `third-party/dds/cyclonedds/` (matches `ros-humble-cyclonedds` 0.10.5). `nros-rmw-cyclonedds` standalone CMake project at `packages/dds/nros-rmw-cyclonedds/` (NOT a Cargo crate); registers C++ vtable via `nros_rmw_cffi_register`. **Goal: full wire-compat with stock `rmw_cyclonedds_cpp`**. `NANO_ROS_RMW=cyclonedds` CMake option auto-pulls Cyclone backend + flips on `NROS_RMW_CYCLONEDDS=1` macro that triggers register call inside `nros::init`. Driver: `just cyclonedds {setup,build,build-rmw,test,doctor,clean}`. Pub/sub + services + raw-CDR data plane wired (117.1–117.9 done). Stock-RMW interop pending (117.X.1 rosidl_adapter codegen → 117.X.2 topic prefix conventions `rt/`/`rq/`/`rr/` → 117.X.3 replace `ServiceEnvelope` with upstream `cdds_request_header_t` → 117.X.4 type-name mangling verification → 117.X.5 service QoS alignment → 117.12 POSIX E2E vs stock RMW). Interim 117.7.B `ServiceEnvelope` works for nano-ros↔nano-ros only; 117.X.3 supersedes.

Phase 124 (RMW zero-copy + dispatch + ABI extensions): six `nros_rmw_vtable_t` slot additions, each with a matching Rust trait method + C/C++ wrapper + routing test (cross-language discipline from Phase 122). Threads: **A** zero-copy (`pub_loan`/`pub_commit`/`pub_discard`/`sub_borrow`/`sub_release` + arena fallback when NULL); **B** wake-callback + condvar (`set_wake_callback` supersedes `set_wake_signal` — deleted, no alias; `Executor.wake_cv` condvar-blocked spin; guard-condition C/C++ surface; ISR-safe `nros_platform_condvar_signal_from_isr`); **C** `service_server_available`; **D** `try_recv_sequence` (burst-take, runtime emits `try_recv_raw` loop fallback when NULL); **E** `publish_streamed` (size_cb + chunk_cb, staging-buffer fallback when NULL); **F** `ping_session`. NULL-slot policy: every slot has a runtime fallback or surfaces `RET_UNSUPPORTED` — no backend obligation creep. Backend native impls: zenoh-pico has E.3 (`z_bytes_writer` streamed publish) + F.2 (`zp_send_keep_alive` ping) + A.4 (native loan). Deferred: D.3 zenoh ring-buffer batch (upstream `rmw_zenoh` loops too — single-slot `SubscriberBuffer` would need a ring rewrite), Cyclone/dust-dds native take, XRCE E.3/F.2 (header-only K.2 backend needs a Rust-side `XrceRmw` adapter). Phase keeps wait semantics in the platform layer — no upstream `rmw_wait`/waitset, no RTOS stubs.

## Quick Reference
`book/src/reference/build-commands.md`: manual testing, ROS 2 interop, Docker, QEMU, Zephyr. Build book: `just book`.

Docs: `book/src/` (user, mdbook) — getting-started, user-guide, porting, reference, concepts, internals. `docs/` (contributor) — reference, design, research, roadmap.
