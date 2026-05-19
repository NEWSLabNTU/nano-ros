# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std`.

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C idents, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nros_generate_interfaces()`)

## Workspace
`packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen}/`, `examples/`, `third-party/` (gitignored SDKs), `zephyr/` module. Run `ls packages/` for current crate list.

`packages/drivers/` covers three sibling categories that settled during Phase 80: transport-bridge crates (`nros-smoltcp` — smoltcp ↔ zenoh-pico, formerly inside `zpico-sys`), peripheral / MAC drivers (`lan9118-smoltcp`, `openeth-smoltcp`, `cmsdk-uart`, `stm32f4-usart`, `virtio-net-netx`, `nvidia-ivc`, `nsos-netx`), and vendor `*-sys` bindings (`freertos-lwip-sys`, `threadx-netx-sys`, `zephyr-posix-sys`, `nuttx-sys`). Board crates pick the bridge + driver(s) they need; platform crates stay free of networking code.

## Build
- `just setup` / `just doctor` / `just check` / `just ci` (check + test-all) / `just verify` (Kani+Verus) / `just generate-bindings`
- `just <module> setup`: workspace, verification, qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32, zephyr, xrce, zenohd, rmw_zenoh, orin_spe, cyclonedds, platformio, esp_idf, px4

**SDK tiers** (Phase 142, `just setup tier=<tier>`, defaults to `default` or `NROS_SETUP_TIER` env):
- `minimal` — workspace, verification, zenohd (Rust-only contributors)
- `default` — minimal + qemu, freertos, nuttx, threadx_{linux,riscv64}, esp32, zephyr, xrce, rmw_zenoh, orin_spe, cyclonedds, platformio (full `just ci` coverage)
- `extended` — default + esp_idf, px4 (every Phase 139 integration shell runnable)

Tiers are strict supersets; never move a module between tiers without bumping `docs/development/sdk-tiers.md` AND the orchestrator switch in `justfile::_orchestrate`. Policy: a module joins `default` iff ≤ 500 MB / ≤ 5 min wall-clock install AND exercised by `just test-all` AND idempotent. ARM FVP, NVIDIA SDK Manager, license-gated installs stay opt-in entirely (run `just <module> setup` out-of-band).

**Build tiers** (each strict superset): `build` (workspace + transports) ⊂ `build-examples` ⊂ `build-all` (= `build-examples` + `build-test-fixtures`). Per-platform tier: `just <plat> build` ⊂ `build-examples` ⊂ `build-fixtures` ⊂ `build-all`. Orchestration in `justfile` + `just/*.just`.

**Test tiers** (each strict superset): `test-unit` (~5s) ⊂ `test-integration` (~30s) ⊂ `test` ⊂ `test-all` (+ heavy QEMU/Zephyr/ROS-interop + `test-doc` + `test-miri` + C codegen).

Per-platform: `just <plat> test|test-all|ci`. `<plat>` = target families (`qemu`, `zephyr`, board groups). Support services (`zenohd`, `cyclonedds`) are NOT platform scopes. Platform-specific build fail → rerun narrow `just <plat> build|build-examples|build-fixtures|build-all` before root `just build-all`. GNU `parallel` auto-used; `RUSTC_WRAPPER=sccache` auto-detected.

## Environment
`.env` (gitignored). **Run `direnv allow` once after clone** else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`.

Runtime: `ROS_DOMAIN_ID` (0), `ZENOH_LOCATOR` (`tcp/127.0.0.1:7447`), `ZENOH_MODE`.

SDK paths auto from `third-party/<sdk>/`; override `<SDK>_DIR` env. See `docs/reference/environment-variables.md`.

## Practices
- **Always `just ci` after task.** **Never `sudo`** — tell user.
- **`just format` before broad changes** (Rust + C/C++ + Python).
- **Always use nightly for `rustfmt` / `cargo fmt`.** `rustfmt.toml` enables nightly-only options (`imports_granularity = "Crate"`, `format_code_in_doc_comments = true`); the stable toolchain warns and skips them, producing a different output than CI. Run `cargo +nightly fmt` (or `rustup run nightly cargo fmt`).
- **C/C++ style:** `.clang-format` LLVM-based, 4-space indent, 100-col limit.
- **Linear history:** `git pull --rebase` or `git fetch` + `git rebase`. Never merge commits unless user explicitly asks.
- **Submodule rebase on superproject pull:** if pull advances submodule pointer AND local work exists in submodule → enter submodule, fetch remote, rebase local work onto updated upstream, check out superproject's expected commit, record resulting submodule commit in parent. Never leave submodule at older local commit when remote pointer advanced.
- **Don't modify vendored/generated:** `third-party/`, `packages/interfaces/*/generated/`, build output dirs — unless task explicitly requires regeneration. Preserve worktree changes.
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
- **Patched `qemu-system-arm`** (Phase 143): `just qemu setup-qemu` builds `third-party/qemu/qemu` @ stable-11.0 + `third-party/qemu/patches/` into `build/qemu/bin/qemu-system-arm`. Test harness picks it up automatically via `nros_tests::qemu::qemu_system_arm_path()` — no env-var needed. System `qemu-system-arm` is the fallback. New test code MUST use `nros_tests::qemu::qemu_system_arm_cmd()` instead of `Command::new("qemu-system-arm")`. New justfile recipes MUST gate through `{{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }}`. See `book/src/internals/qemu-patched-binary.md`.

### Examples = Standalone Projects
**Each `examples/` dir is self-contained, copy-out template.**
- Canonical shape `examples/<plat>/<lang>/<example>/` (collapsed — RMW selected at build time via Cargo features for Rust + cmake `-DNROS_RMW=<rmw>` for C/C++ + Kconfig `prj-<rmw>.conf` overlay on Zephyr). Phase 118 + 168 collapsed every `<plat>/<lang>/<rmw>/<example>/` triple onto the single dir; legacy `<rmw>/<case>/` siblings deleted on Zephyr (Phase 168.6.C). Sibling categories: `examples/bridges/<name>/` (cross-RMW gateways), `examples/templates/<name>/` (multi-platform copy-out recipes — Pattern A workspaces etc.). Carve-outs: `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` (one-board-one-RMW reference, not collapsed).
- **Non-example binaries live elsewhere.** Tests/benches/smokes are NOT in `examples/`:
  - `packages/testing/nros-bench/<name>/` — perf, fairness, stress, large-msg
  - `packages/testing/nros-smoke/<name>/` — driver/board bringup (no nros API)
  - `packages/testing/nros-tests/bins/<name>/` — fixture binaries built by integration tests
  Each is a standalone Cargo package with empty `[workspace]` table (they nest under the `nros-tests` member dir).
- Variant naming: suffix form (`talker-rtic`, `service-client-async`, `talker-rtic-mixed`) so variants sort with peers.
- No shared example-only helpers in `nros-cpp`/`nros-c` — boilerplate IS lesson.
- `*_DIR` env / `-D` injection = SDK-path contract. Example cmake accepts env or `-D` only — never project-tree heuristics.
- Per-example `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` build in isolation. No workspace reliance, no walk-up.
- **C/C++ consumption shape (Phase 137 / 140 / 144 / native examples):** `set(NANO_ROS_PLATFORM <plat>) + set(NANO_ROS_RMW <rmw>) + add_subdirectory(<repo-root> nano_ros) + target_link_libraries(<app> PRIVATE NanoRos::NanoRos) + nros_platform_link_app(<app>)`. Per-platform CMake glue lives at `cmake/platform/nano-ros-<plat>.cmake`; per-board overlays at `cmake/board/nano-ros-board-<board>.cmake`. **There is no `find_package(NanoRos)` path** — Phase 140 deleted it along with `just install-local`, the `build/install/` layout, every `install(...)` rule and every `Config.cmake.in` template. Cross-RTOS users (Zephyr, ESP-IDF, PlatformIO, NuttX, PX4) consume via the Phase 139 shells at `integrations/<rtos>/` — those shells re-export the same root CMake under each RTOS's native package manager.
- **Coverage matrix lives in `examples/README.md` ("Coverage matrix" + "Intentionally empty cells" sections) — authoritative for which `<plat>/<lang>/<rmw>` triples exist.** Deliberately empty cells: `{qemu-arm-baremetal, qemu-esp32-baremetal, esp32, stm32f4}/{c,cpp}/*` (no bare-metal C/C++ harness — `nros-c`/`nros-cpp` assume hosted RTOS for startup/heap/libc) and `px4/{c,rust}/*` (PX4 is uORB-only, and Phase 115.K.4 collapsed uORB to a C++-only port — `examples/px4/cpp/uorb/nros-register-check/` is the canonical surface). Do not add directories to these cells without first lifting the underlying constraint; Phase 118 lint blocks untriaged cells.

### CMake Path Convention
- Never hard-code project-relative paths in example cmake **or in
  `packages/<crate>/CMakeLists.txt`, `cmake/*.cmake` modules, build.rs,
  or any in-tree script**. Each subproject (`packages/dds/<name>`,
  `packages/core/<name>`, `examples/<dir>`) must build standalone — no
  walking up the source tree.
- No `../../../cmake/...`, no project-root heuristics, no
  `${_ROOT}/external/<sdk>` defaults, no `$<source_dir>/../../../scripts/...`
  in `install(...)` rules.
- **Drivers pass NANO_ROS_PLATFORM / NANO_ROS_RMW (+ board-specific
  cache vars) before `add_subdirectory(<repo-root>)`.** The
  `just`-recipe / outer build script knows the layout and supplies
  it via cmake `-D…=$PWD/...` or env var:
  - `-DNANO_ROS_PLATFORM=`, `-DNANO_ROS_RMW=`, `-DCMAKE_TOOLCHAIN_FILE=`,
    `-D<SDK>_DIR=`, `-D<BOARD>_CONFIG_DIR=`.
  - Third-party SDKs (Cyclone DDS, NetX Duo, FreeRTOS-Kernel) still
    pass `-DCMAKE_PREFIX_PATH=` to their own install prefixes — the
    deletion was about NanoRos's own install prefix, not third-party
    libraries the project depends on.
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
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}` — `[platform.bare-metal]` in `zenoh_platforms.toml` carries `arch = ["cortex-m3", "riscv32imc"]`; build.rs's first-match dispatch picks the right arch per target triple (so `qemu-arm-baremetal` + `ESP32-C3` share the same platform entry). Phase 148.
- **ROS edition**: `ros-{humble,iron}`

RMW backend host-language policy (frozen 2026-05-07; dust-dds row retired 2026-05-19 per Phase 169): see `book/src/internals/rmw-backends.md`. Rule: backend's host language matches its underlying library's native language unless overridden. Today: cyclonedds=C++, XRCE=Rust→C (115.K.2), zenoh-pico=Rust (deferred), uORB=Rust (won't-do). dust-dds=Rust **retired** (Phase 169 — repeated bring-up failures on every embedded target; consolidating DDS on Cyclone).

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
`nros-rmw-xrce-cffi` (C FFI shim) gates `UCLIENT_PROFILE_{UDP,TCP,SERIAL}` + `UCLIENT_PLATFORM_POSIX` + `transport_posix_{udp,serial}.c` source files on `target_os = linux|macos|*bsd`. Bare-metal targets (`target_os = "none"`) get only `UCLIENT_PROFILE_{DISCOVERY,CUSTOM_TRANSPORT,STREAM_FRAMING}` and must inject their own custom transport. `just check-workspace-embedded` excludes `nros-rmw-xrce{,-cffi,-cffi-staticlib}` (header-only K.2 backend's `internal.h` references UDP types unconditionally — upstream design issue; the staticlib sibling needs panic_handler resolution at compile time which only works on hosted targets). Phase 160.L added the `-staticlib` sibling so Corrosion can import a real `staticlib` target without forcing the cffi rlib crate to emit one.

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

Phase 128 + 129 (RMW selection cleanup + platform-agnostic backends, archived 2026-05-17): RMW pick driven by Cargo manifest + CMake `target_link_libraries`, no code-level selection. `NROS_RMW` env-var fallback for runtime multi-RMW pick; bridge mode via `Executor::open_multi(&[SessionSpec])` + `create_node_on(name, rmw)` + `nros-bridge` crate (TOML loader behind `config` feature; C/C++ surface in `<nros/bridge.h>`/`<nros/bridge.hpp>`). Per-backend cmake interface libs `NanoRos::Rmw::<name>`. Backend linker-section discovery via `linkme` (with fallback `nros_rmw_register_backend!` macro for unsupported targets like NuttX). Phase 129 retired `zpico-platform-shim` + `xrce-platform-shim`: every `z_*`/`_z_*`/`uxr_*` symbol now comes from C alias TUs (`zpico-sys/c/zpico/platform_aliases.c`, `nros-rmw-xrce/src/platform_aliases.c`) that forward to canonical `nros_platform_*` ABI. IVC link-layer carved into standalone `zpico-link-ivc` crate. Generic platform header (`nros_zenoh_generic_platform.h`) types `z_clock_t = uint64_t`, opaque storage for `_z_task_t`/`_z_mutex_t`/`_z_condvar_t`. `nros-rmw-xrce-cffi`'s `transport_nros_udp.c` superseded per-platform `transport_posix_udp.c`/`transport_zephyr_udp.c`. `link-tcp`/`link-udp-unicast` cargo features deleted (vendor always compiles those transports; locator picks at runtime). `NET_*_SIZE` / `NET_*_ALIGN` exported unconditionally from `nros-platform`. **Active follow-ups:** phase 131 (examples tree revision) + phase 133/134/135 (CI sweep findings from first clean run on `main`).

Phase 131 (examples tree revision, active): canonical example shape `examples/<plat>/<lang>/<rmw>/<example>/`. Sibling categories `examples/bridges/<name>/` (cross-RMW gateways) + `examples/templates/<name>/` (multi-platform copy-out recipes — Pattern A workspaces). Tests/benches/smokes moved OUT of `examples/` into `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`. Variant naming uses suffix form (`talker-rtic`, `service-client-async`, `talker-rtic-mixed`). See `docs/roadmap/phase-131-examples-tree-revision.md` for migration table; older nested `async-*`/`rtic-*` parent dirs were renamed in 131.D.

## Quick Reference
`book/src/reference/build-commands.md`: manual testing, ROS 2 interop, Docker, QEMU, Zephyr. Build book: `just book`.

Docs: `book/src/` (user, mdbook) — getting-started, user-guide, porting, reference, concepts, internals. `docs/` (contributor) — reference, design, research, roadmap.
