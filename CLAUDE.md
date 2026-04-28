# nano-ros

Lightweight ROS 2 client for embedded real-time systems (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std` compatible.

### Naming Convention

- **nano-ros** — project name (prose, docs, user-facing text)
- **nros** — code shorthand (crate names, Rust/C identifiers, Kconfig `CONFIG_NROS_*`)
- **nano_ros** — C header dir (`nano_ros/`), CMake targets (`NanoRos::NanoRos`), CMake function (`nano_ros_generate_interfaces()`)

## Workspace Structure

```
nano-ros/
├── packages/
│   ├── core/           # nros, nros-core, nros-serdes, nros-macros, nros-params, nros-rmw, nros-node, nros-c, nros-cpp
│   ├── zpico/          # Zenoh-pico backend: nros-rmw-zenoh, zpico-sys, zpico-smoltcp, zpico-zephyr, platform-*
│   ├── xrce/           # XRCE-DDS backend: nros-rmw-xrce, xrce-sys, xrce-smoltcp, xrce-zephyr, platform-*
│   ├── boards/         # Board support: nros-board-mps2-an385, nros-board-mps2-an385-freertos, nros-board-nuttx-qemu-arm, nros-board-threadx-linux, nros-board-threadx-qemu-riscv64, nros-board-esp32, nros-board-esp32-qemu, nros-board-stm32f4
│   ├── drivers/        # lan9118-smoltcp, lan9118-lwip, openeth-smoltcp, virtio-net-netx
│   ├── interfaces/     # rcl-interfaces (generated/, checked into git)
│   ├── testing/        # nros-tests (integration test crate)
│   ├── verification/   # nros-ghost-types, nros-verification (Verus proofs, excluded from workspace)
│   ├── reference/      # qemu-smoltcp-bridge
│   └── codegen/        # cargo-nano-ros, rosidl-*, bundled .msg/.srv files
├── examples/           # 4-level: platform/lang/rmw/use-case (native, qemu-arm-baremetal, qemu-arm-freertos, qemu-arm-nuttx, qemu-riscv64-threadx, threadx-linux, qemu-esp32-baremetal, esp32, stm32f4, zephyr)
├── external/           # Third-party SDK sources (git-ignored): freertos-kernel, lwip, nuttx, nuttx-apps, threadx, netxduo
├── scripts/            # zenohd build, Zephyr setup
├── docker/             # QEMU dev environment
├── tests/              # Shell-based test scripts
├── docs/               # Guides, reference, design, roadmap
├── zephyr/             # Zephyr module (Kconfig, CMakeLists.txt, cmake/)
└── CMakeLists.txt      # Top-level CMake (Corrosion, nros-c + nros-cpp + codegen)
```

## Build Commands

```bash
just setup              # Install everything: workspace + verification + platforms + services (idempotent)
just doctor             # Diagnose install status (read-only; exit 1 if anything missing)
just freertos setup     # Download FreeRTOS kernel + lwIP (included in just setup)
just nuttx setup        # Download NuttX RTOS + apps (included in just setup)
just threadx_linux setup     # Download ThreadX kernel + NetX Duo (Linux sim)
just threadx_riscv64 setup   # Download ThreadX kernel + NetX Duo (QEMU RISC-V)
just build              # Generate bindings + build workspace + examples
just build-zenohd       # Build zenohd from submodule (alias: just zenohd setup)
just check              # Format check + clippy
just ci                 # Check + test
just doc                # Generate docs
just verify             # Kani + Verus verification
just generate-bindings  # Regenerate all generated/ dirs
```

Test commands:
```bash
# Project-level
just test-unit              # Unit tests only (no external deps, ~5s)
just test-miri              # Miri UB detection
just test                   # Unit + integration + miri (excludes zephyr/ros2/large_msg)
just test-all               # Everything (all platforms in one nextest run) + miri + C codegen
just ci                     # check + test

# Per-platform (just <platform> test|test-all|ci)
just qemu test              # QEMU bare-metal tests (non-networked)
just qemu test-all          # + networked E2E and RTIC tests
just native test            # Native integration tests (needs zenohd)
just native test-all        # + ROS 2 interop, large_msg, C/C++ API
just freertos test          # FreeRTOS QEMU E2E (needs arm-none-eabi-gcc)
just nuttx test             # NuttX QEMU E2E (needs nightly + qemu-system-arm)
just threadx_linux test     # ThreadX Linux sim E2E (needs ThreadX/NetX)
just threadx_riscv64 test   # ThreadX RISC-V QEMU E2E
just zephyr test            # Zephyr E2E (needs west; native_sim uses NSOS on host loopback)
just zephyr test-all        # + XRCE + C examples
just esp32 test             # ESP32 QEMU E2E
just <platform> ci          # Platform-specific check + test
```

First-time: `just setup` installs everything (workspace + verification + all platforms + services). Use `just doctor` to verify the install. Per-module: `just <module> setup` / `just <module> doctor` where modules are `workspace`, `verification`, `qemu`, `freertos`, `nuttx`, `threadx_linux`, `threadx_riscv64`, `esp32`, `zephyr`, `xrce`, `zenohd`.

## Environment Variables

Configuration via `.env` file: copy `.env.example` to `.env` (gitignored) and uncomment values. Loaded automatically by justfile and direnv.

**Use `direnv allow` once after cloning** so `cargo nextest run …`, `cargo build`, `cmake …`, etc., pick up the SDK paths and `FREERTOS_PORT` automatically. Without direnv (or without manual exports), running cargo directly outside of `just <plat> …` panics in `zpico-sys/build.rs` with `"FREERTOS_PORT not set"`. The `.envrc` defaults match the justfile defaults; an `.env` file overrides both. Setup:
```bash
sudo apt install direnv                              # if not installed
echo 'eval "$(direnv hook bash)"' >> ~/.bashrc       # or fish/zsh
direnv allow                                          # one-time per checkout
```

Runtime: `ROS_DOMAIN_ID` (default `0`), `ZENOH_LOCATOR` (default `tcp/127.0.0.1:7447`), `ZENOH_MODE` (`client`/`peer`).

FreeRTOS/NuttX/ThreadX build-time variables are **auto-resolved** by justfile recipes and `.envrc` (defaulting to `third-party/<sdk>/` paths populated by `just freertos setup` / `just nuttx setup` / `just threadx_linux setup`). Override via env vars if sources are elsewhere:
- `FREERTOS_DIR` — FreeRTOS kernel source (default: `third-party/freertos/kernel`)
- `FREERTOS_PORT` — portable layer (default: `GCC/ARM_CM3`)
- `LWIP_DIR` — lwIP source (default: `third-party/freertos/lwip`)
- `FREERTOS_CONFIG_DIR` — `FreeRTOSConfig.h` + `lwipopts.h` (default: board crate's `config/`)
- `NUTTX_DIR` — NuttX RTOS source (default: `third-party/nuttx/nuttx`)
- `NUTTX_APPS_DIR` — NuttX apps source (default: `third-party/nuttx/nuttx-apps`)
- `THREADX_DIR` — ThreadX kernel source (default: `third-party/threadx/kernel`)
- `THREADX_CONFIG_DIR` — ThreadX config directory (default: board crate's `config/`)
- `NETX_DIR` — NetX Duo source (default: `third-party/threadx/netxduo`)
- `NETX_CONFIG_DIR` — NetX Duo config directory (default: board crate's `config/`)

Buffer tuning: see [docs/reference/environment-variables.md](docs/reference/environment-variables.md).

## Development Practices

### Quality Checks
**Always run `just ci` after completing a task.**

### System Packages & Privileges
**Never install system packages or run sudo directly.** Inform the user what's needed.

### Unused Variables
- Rename to `_name` with a comment explaining why
- Use `#[allow(dead_code)]` for test struct fields

### Testing
- **Reusable tests** → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (shell scripts)
- **Temporary tests** → Bash tool directly, convert to proper tests once validated
- Test scripts in `tests/` should have justfile entries
- Use `just test-*` recipes. All accept a `verbose` argument for live output.
- **Tests must fail on unmet preconditions** — if a required env var, binary, or tool is missing, the test MUST fail (return error/panic), not silently skip and report PASS. Use `assert!()` or `bail!()` for precondition checks. Silent skips that count as PASS hide real failures. This applies to ALL tests.
  - Same rule for **runtime failures** inside a test body: if the test's subject didn't boot, didn't produce the expected output, or otherwise failed to reach the assertion point, the test must panic — not silently early-return. "Lenient" silent skips (e.g., "listener didn't reach readiness, skip") are tech debt; fix the underlying issue instead of masking it.
  - The `nros_tests::skip!` macro panics with a `[SKIPPED]` prefix, so it satisfies this rule. A bare `eprintln!` + `return` does not (it reports PASS).
  - **Only exception**: known-unsupported combinations in an `rstest` `#[values]` parametrised matrix may silent-return, acting as the per-case equivalent of `#[ignore]` (which rstest doesn't support per-case). Wire these through a single `skip_reason(…)` helper that documents the reason.
- JUnit XML: `target/nextest/default/junit.xml` (auto-generated by nextest)
- Non-nextest tests use `tests/run-test.sh` wrapper → logs in `test-logs/latest/`
- See `tests/README.md` for full test infrastructure docs

### QEMU Networked Test Rules
- **Slirp networking** — QEMU platforms (bare-metal, FreeRTOS, NuttX, ThreadX RISC-V, ESP32) use slirp user-mode networking. No TAP devices, bridges, or `sudo` needed.
- **Per-platform zenohd ports** — each platform has a fixed port in `nros_tests::platform` (baremetal=7450, freertos=7451, nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456). Use `ZenohRouter::start(platform::FREERTOS.zenohd_port)`, not hardcoded ports.
- **Bridge-networked platforms** — ThreadX Linux sim (veth) uses bridge networking and needs `ZenohRouter::start_on("0.0.0.0", port)` instead of `start(port)`. Zephyr native_sim migrated to NSOS (Phase 81) and uses `127.0.0.1` on the host like any other loopback-bound test.
- **Start subscriber first, then publisher.** Zenoh doesn't buffer for unknown subscribers.
- **5–10s stabilization delay** between subscriber connection and publisher start
- **Per-platform nextest groups** — each platform has its own `max-threads = 1` group (e.g., `qemu-freertos`). Platforms run in parallel; tests within a platform are serial.

### Temporary Files
- Create in `$project/tmp/` (git-ignored), not `/tmp`
- Use Write/Edit tools (avoid cat + heredoc)
- **Build test scripts**: when iterating on cmake/cargo build commands, write them as reusable scripts in `tmp/` (e.g., `tmp/build-riscv64-talker.sh`) instead of running long one-liner commands repeatedly
- **Debug/test scripts**: when running repeated multi-step commands (QEMU launch, GDB debug sessions, build+run combos), always write them as `tmp/*.sh` scripts first, then run the script. Never repeat long commands inline.

### `.gitignore` Practices
- **Every workspace-excluded crate** (examples, board crates in `exclude`, standalone packages) must have a per-directory `.gitignore` with at least `/target/`. Add `/generated/` if the crate uses `cargo nano-ros generate`.
- **Every native C++ example** must have a per-directory `.gitignore` with `/build/` (CMake builds in-tree). Zephyr C++ examples don't need this since they build in the Zephyr workspace.
- Root `.gitignore` only for repo-wide patterns
- Always use leading `/` (e.g., `/target/` not `target/`)
- When adding `--target-dir` for build isolation, add the dir to the example's `.gitignore`

### Examples are Standalone Projects
**Each example under `examples/` is a self-contained starting template that users are expected to copy out of the nano-ros source tree** and adapt for their own application — different RTOS port, different message types, different topology. The example tree is pedagogical: every example shows the full call sequence (`nros::init` → `create_node` → `create_publisher` → spin loop) explicitly so a copied-out example reads as a complete, runnable project without needing the nano-ros internals on hand.

Implications:
- **No shared example-only helpers in `nros-cpp` / `nros-c`.** A copied-out example must build against the public `find_package(NanoRos)` surface alone. Hiding boilerplate behind `nros::examples::*` headers (or similar) breaks the copy-out workflow because the helper would either travel with the example (coupling) or have to be rewritten back in (zero benefit). The boilerplate **is** the lesson.
- **`*_DIR` env vars / `-D` injection are the SDK-path contract.** External SDK locations (`THREADX_DIR`, `NETX_DIR`, `FREERTOS_DIR`, `LWIP_DIR`, `NUTTX_DIR`, `NUTTX_APPS_DIR`, …) are passed in by the user's build script when they're outside the nano-ros tree. The same env vars are auto-resolved by the in-tree justfile recipes for the convenience of contributors, but the example's cmake files must accept them as the *only* discovery mechanism (env or `-D`, never project-tree heuristics).
- **Per-example `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` must build in isolation.** No reliance on the workspace's `Cargo.toml`, no walk-up to project root for cmake includes (other than the example's own `cmake/` subdir), no `[patch.crates-io]` that points outside the example.
- **Per-platform `cmake/<plat>-support.cmake` files are part of the example tree.** They live next to the example, not at project root, so they travel with a copied-out example. Layer-2 cmake modules (`nros-threadx.cmake`, `nros-freertos.cmake`, `nros-nuttx.cmake`) ship via `find_package(NanoRos)` — copied-out examples reach them through the install prefix the user passes, same as any other `find_package` consumer.

### CMake Path Convention for Examples
Examples must work when copied outside the nano-ros project tree. **Never hard-code project-relative paths in example CMakeLists.txt or support cmake files.** This means:
- No `set(CMAKE_TOOLCHAIN_FILE "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/toolchain/...")` in example cmake files
- No path expressions that assume a fixed directory depth within the project
- No heuristic search for the project root (e.g., `get_filename_component(_ROOT "${CMAKE_CURRENT_LIST_FILE}/../../.." ABSOLUTE)`) in support cmake modules
- No defaulting SDK paths to `${_ROOT}/external/<sdk>` — all external SDK paths must be passed explicitly

Instead, pass absolute paths from build scripts:
- **Test scripts** (`freertos_qemu.rs`, justfile): pass `-DCMAKE_TOOLCHAIN_FILE=<abs_path>`, `-DTHREADX_DIR=<abs_path>`, etc. on the cmake command line
- **`CMAKE_PREFIX_PATH`**: always passed from the build script pointing to `build/install/`
- **SDK paths** (`THREADX_DIR`, `NETX_DIR`, `FREERTOS_DIR`, `LWIP_DIR`, `NUTTX_DIR`): always passed as `-D` variables or env vars from the build script, never defaulted relative to the project tree
- **Board config dirs** (`THREADX_CONFIG_DIR`, `FREERTOS_CONFIG_DIR`): passed as `-D` from the build script
- Paths internal to the example directory tree (e.g., `../../../cmake/freertos-support.cmake` relative to the example's own cmake support directory) are fine — these are within the example's portable subtree

### Parallel Build Isolation
Nextest runs test files in parallel. When multiple tests build the same example with different features, use `--target-dir` to isolate output directories (e.g., `target-safety/`, `target-zero-copy/`). See `fixtures/binaries.rs` for examples.

### Roadmap Documents (`docs/roadmap/`)
Phase docs follow a standard structure:
- **Header**: Goal, Status, Priority, Depends on
- **Overview**: Background and motivation
- **Architecture/Design**: Diagrams, key decisions
- **Work Items**: Checklist (`- [ ] 54.1 — Title`) at top, then `### 54.1 — Title` subsections with details and `**Files**` list
- **Acceptance Criteria**: Checklist (`- [ ]` items) — testable conditions for phase completion
- **Notes**: Caveats, gotchas, implementation details
- Mark items `- [x]` when complete. Completed phases move to `docs/roadmap/archived/`.

## Key Design Patterns

### Zenoh Version Unification
All zenoh components pinned to **1.7.2** (compatible with rmw_zenoh_cpp). zenohd built from `third-party/zenoh/zenoh/` submodule; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Test infra auto-uses `build/zenohd/zenohd` when available.

### Rust Edition 2024
- `unsafe extern "C" { ... }` (extern blocks require `unsafe`)
- `#[unsafe(no_mangle)]` (no_mangle requires `unsafe`)
- Unsafe operations inside `unsafe fn` need explicit `unsafe { ... }` blocks
- `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]` (420+ FFI operations)

### API Alignment
- **Rust API**: follows rclrs 0.7.0 naming; **C API**: follows rclc naming
- `create_publisher()`, `create_subscription()`, `create_service()`, `create_client()`
- `create_action_server()`, `create_action_client()`
- Types: `Publisher<M>`, `Subscription<M>`, `Service<S>`, `Client<S>`, `ActionServer<A>`, `ActionClient<A>`
- Error: `RclrsError`

### `no_std` Support
All core crates support `#![no_std]` with optional `std`/`alloc` features.

### Message Types
Generated via `cargo nano-ros generate-rust` from `package.xml`. **Never hand-write message types.** See [message-generation.md](docs/guides/message-generation.md) and [creating-examples.md](docs/guides/creating-examples.md).

- Example `generated/` dirs are gitignored, recreated by `just generate-bindings`
- Only `packages/interfaces/rcl-interfaces/generated/` is checked into git (uses `nros-` prefixed names)
- `.cargo/config.toml` is manually maintained per example (`[patch.crates-io]` + platform settings)
- Bundled interfaces at `packages/codegen/interfaces/` (no ROS 2 env needed)
- `nros-core` re-exports `heapless` for generated code

### C API
See [docs/reference/c-api-cmake.md](docs/reference/c-api-cmake.md) for CMake integration, code generation, and system install.

**nros-c thin wrapper principle:** `nros-c` must be a thin FFI wrapper over `nros-node` — delegate to Rust types, don't reimplement logic. New C API features must first be implemented in `nros-node`, then wrapped.

**cbindgen header generation:** C headers are auto-generated from Rust `#[repr(C)]` types by cbindgen v0.29 during `cargo build`. The generated `nros_generated.h` is included by thin per-module header stubs. All struct fields on `#[repr(C)]` types must be `pub` for cbindgen to include them. `visibility.h`, `platform.h`, and `types.h` (for `nros_service_type_t`) remain hand-written. Platform FFI imports in `platform.rs` use `/// cbindgen:ignore` to avoid conflicts with `static inline` definitions.

### C++ API
See [docs/guides/cpp-api.md](docs/guides/cpp-api.md) for the getting started guide.

`nros-cpp` is a freestanding C++14 library (header-only C++ + Rust FFI staticlib) wrapping `nros-node` directly via typed `extern "C"` FFI. Mirrors rclcpp naming (`Node`, `Publisher<M>`, `Subscription<M>`, `Service<S>`, `Client<S>`, `ActionServer<A>`, `ActionClient<A>`, `Timer`, `GuardCondition`, `Executor`). Error handling via `nros::Result` + `NROS_TRY` macro.

**Message codegen:** `cargo nano-ros generate-cpp` or CMake `nano_ros_generate_interfaces(... LANGUAGE CPP)`. Generated types use ROS 2 standard namespaces (e.g., `std_msgs::msg::Int32`).

**Optional std mode:** Define `NROS_CPP_STD` for `std::string`, `std::function`, and `std::chrono` convenience overloads. Not required — freestanding mode uses `const char*`, C function pointers, and integer milliseconds.

**Zephyr integration:** `CONFIG_NROS_CPP_API=y` + `nros_generate_interfaces(... LANGUAGE CPP)`.

**C++ action client status:** `ActionServer<A>` and `ActionClient<A>` are implemented but use **blocking** `zpico_get` for `send_goal` and `get_result`. This causes hangs on FreeRTOS QEMU where the condvar is never signaled. Phase 77 will add a non-blocking async path using `zpico_get_start`/`zpico_get_check` polled by the executor. The `test_freertos_cpp_action_e2e` test is `#[ignore]`d until Phase 77. The C++ action server also hangs during `create_action_server` on FreeRTOS QEMU (zenoh-pico deadlock when declaring 5 entities) — this may be a separate zenoh-pico issue.

### Platform Backends
Three orthogonal axes (NEVER cross-imply):
- **RMW backend** (one): `rmw-zenoh`, `rmw-xrce`
- **Platform** (one): `platform-posix`, `platform-zephyr`, `platform-bare-metal`, `platform-freertos`, `platform-nuttx`, `platform-threadx`
- **ROS edition** (one): `ros-humble`, `ros-iron`

Mutual exclusivity enforced at compile-time. Zero features on an axis is valid (reduced functionality).
Default features: `std` only. Platform features forwarded via Cargo `?` syntax.

**Cross-cutting:** `unstable-zenoh-api` enables zero-copy receive (orthogonal to axes above).

### Spin/Yield Wake Primitives (per platform)
`zpico_spin_once` (event-driven wake on data arrival; 77.16 / 77.17 done):
- POSIX / Zephyr: `_z_condvar_wait_until` on `g_spin_cv`
- FreeRTOS: `xSemaphoreTake(g_spin_sem, pdMS_TO_TICKS(…))`
- NuttX: `sem_timedwait(&g_spin_sem_posix, &abs_deadline)` (pthread condvar hangs on NuttX — Phase 55.12)
- Bare-metal smoltcp / serial: single-thread `zp_read` loop (77.18 will add WFI)

Cooperative yield (Phase 77.22 — planned `PlatformYield` trait):
- POSIX / NuttX: `sched_yield()`
- Zephyr: `k_yield()`
- FreeRTOS: `vPortYield()` (C shim for `taskYIELD` macro)
- ThreadX: `tx_thread_relinquish()`
- Bare-metal default: `core::hint::spin_loop()` (pure CPU hint, safe everywhere)
- Bare-metal opt-in: `cortex_m::asm::wfi()` via a board-crate `BoardIdle` trait — deep idle, requires board to arm an IRQ source or the CPU deadlocks. Precedent: STM32F4, MPS2-AN385 board crates already use `wfi()` in their idle loops.

None of the RTOS yields are ISR-safe. `core::hint::spin_loop()` is.

### Board Crate Transport Features
Board crates use Cargo features to select the communication transport:
- **`ethernet`** (default for MPS2-AN385, STM32F4, ESP32-QEMU) or **`wifi`** (default for ESP32) — TCP/UDP via `zpico-smoltcp`
- **`serial`** — UART via `zpico-serial` (bare-metal only) or zenoh-pico built-in serial (ESP32, Zephyr, etc.)

`Config` struct fields are `#[cfg(feature = "...")]`-gated per transport (e.g., MAC/IP under `ethernet`, baudrate under `serial`). At least one transport must be enabled (`compile_error!` enforced). Both can be enabled simultaneously — runtime selection via the zenoh locator string.

ESP32 and ESP32-QEMU use zenoh-pico's built-in serial implementation (no `zpico-serial` dependency). Only bare-metal board crates (`nros-board-mps2-an385`, `nros-board-stm32f4`) depend on `zpico-serial`.

Examples select non-default transport with `default-features = false, features = ["serial"]`.

### Parameter Services
Enable with `param-services` feature in `nros-node`. Provides `~/get_parameters`, `~/set_parameters`, etc. Uses `nros-rcl-interfaces` types. Handlers return `Box<Response>` (large heapless arrays).

### Formal Verification
- **Kani**: 160 bounded model checking harnesses. `just verify-kani` (~3 min)
- **Verus**: 102 unbounded deductive proofs. `just verify-verus` (~1 sec)
- Key Verus rules: `external_type_specification` without `external_body` = transparent enum; with = opaque. Never add `verify = true` to production crates with fn pointers/closures.
- See [docs/guides/verus-verification.md](docs/guides/verus-verification.md)

### ROS 2 Interop
rmw_zenoh-compatible protocol. Key format: `<domain>/<topic>/<type>/TypeHashNotSupported`. See [docs/reference/rmw_zenoh_interop.md](docs/reference/rmw_zenoh_interop.md).

## Development Phases

Completed phases archived in `docs/roadmap/archived/`. See [docs/roadmap/](docs/roadmap/) for details.

| Phase | Focus | Status |
|-------|-------|--------|
| 23 | Arduino precompiled library | Not Started |
| 41 | Iron type hash support | Not Started |
| 64 | Embedded transport tuning guide | In Progress (64.1 done, 64.2 remaining) |
| 65 | nano-ros book (mdbook user guide) | Complete |
| 69 | Cross-platform C/C++ examples + integration tests | Complete (all 10 platforms; NuttX C E2E fixed via usleep + z_clock_t patches) |
| 71 | DDS backend on `nros-platform` capability traits — infrastructure block (cooperative runtime, async transport, size-probed buffers, smoltcp multicast bridge, POSIX `PlatformUdp` validation, generic `nros-platform/global-allocator` feature, slice-offset bug in `ServiceServerTrait::handle_request`, A9 example client cooperative-runtime starvation, async waker bridge for `DdsServiceClient` / `DdsSubscriber`). Native POSIX + Zephyr `qemu_cortex_a9` ship end-to-end. Per-platform examples (FreeRTOS / NuttX / ThreadX / bare-metal / ESP32-QEMU / Zephyr native_sim) tracked under Phase 97. | Archived |
| 73 | Memory efficiency + zero-copy receive | Complete (SUBSCRIBER_BUFFERS removal deferred) |
| 75 | Relocatable CMake install convention for C/C++ | Complete |
| 76 | RTOS scheduling configuration via config.toml | Complete (FreeRTOS; ThreadX/NuttX/Zephyr deferred to future work) |
| 77 | Async action client (eliminate blocking zpico_get) | In Progress (77.1–77.5 done) |
| 78 | Colcon build type (`nros.<lang>.<platform>`) | Not Started |
| 79 | Unified platform abstraction layer | Complete |
| 80 | Unified network interface for nros-platform | Not Started |
| 81 | Fix Zephyr native_sim multi-instance E2E tests (zeth0 TAP contention) | Complete (27/27 Zephyr tests pass) |
| 82 | Blocking service client must take an executor | Complete |
| 83 | C/C++ thin-wrapper compliance (arena-authoritative goal state + CDR header centralization) | Complete (Phase 91.B closed 11 follow-up `use nros_rmw::*` / `use nros_core::*` import sites missed in the original landing) |
| 85 | Test-suite consolidation & speedup (214 → ~80 tests, dedupe 4-platform RTOS matrix, shared build cache, replace sleeps with ready-probes) | Complete (85.11 abandoned; test-count + wall-time targets carried to Phase 89) |
| 86 | `nros-lifecycle-msgs` codegen crate + REP-2002 lifecycle services (`~/change_state`, `~/get_state`, …) for C/Rust APIs | Complete (86.1–86.10; pinned `rmw_zenoh` interop test verifies acceptance) |
| 87 | nros-cpp compile-time storage-size derivation (shared types crate + probe crate; replaces hand-coded `4 * ptr_bytes` math in `build.rs`) | Complete |
| 88 | Unified leveled logging (`nros-log` crate with pluggable RTOS/host sinks; ROS-style named loggers + severity filtering) | Not Started |
| 89 | `just test-all` triage: close ~25 E2E failures/timeouts + restore per-platform nextest parallelism that `.config/nextest.toml` collapsed | Complete (archived; 89.11 dropped, all others landed; clean `just test-all` → 675/675 pass) |
| 90 | PX4 RMW (`nros-rmw-uorb`) + `nros-px4` board crate — uORB-based pub/sub through typed-trampoline registry, `nros::uorb` direct typed API, ROS-name → uORB-topic map (TOML → `phf`), SITL E2E test wired to vendored PX4-Autopilot via Phase 98 fixtures, `nros_px4::run_async` proper-waker chain (uORB callback → `AtomicWaker::wake` → `ScheduleNow` → `WorkItemCell` poll, with bounded `park_max` Sleep as timer safety net) | v1 + 90.5b Complete (90.1–90.8 + 90.5b L1/L2/L3 landed; 90.4b services deferred post-v1) |
| 91 | Code antipattern fixup (Phase 83 thin-wrapper follow-ups, cbindgen-as-SSoT C headers, three-layer cmake abstraction for ThreadX/FreeRTOS/NuttX, hardcoded test ports/paths, platform `seed()` dedup, routing-info centralization) | Complete (archived; A/B/C/D/E1/E3/E4/F/G; E2 dropped by design — examples are standalone projects, boilerplate is the lesson) |
| 92 | Zephyr DDS talker↔listener interop on `qemu_cortex_a9` (real IP stack, IGMP, GEM driver — same code path production DDS-on-Zephyr deployments use) | Complete (archived; `scripts/zephyr/cortex-a9-rust-patch.sh` ships the upstream Zephyr workspace patches, wired into `just zephyr setup` / `build` / `build-fixtures`; interop test passes 6.16 s end-to-end) |
| 93 | C and C++ Doxygen completion + RMW/platform porting surface (Groups A–G user-facing C/C++; Groups H–L RMW + platform trait contracts, platform vtable C header, *-cffi Doxygen sites, porting-guide C path, rustdoc deploy of porter crates) | Complete (archived) |
| 95 | Example coverage parity — closed the `(platform × lang × backend × use-case)` matrix on already-supported backends with 51 new example crates: A Zephyr xrce-rust svc/action (4), B Zephyr dds-rust svc/action/async (5), C Zephyr cpp-xrce (6), D Zephyr cpp-dds (6), E Zephyr c-dds (6), F native dds-rust svc/action (4), G native c-dds (6), H native cpp-dds (6). In-phase prerequisites landed: Phase 71.6 (Zephyr `#[global_allocator]` + critical-section impl + cortex_a9 Rust target wiring for nros-c/nros-cpp staticlibs), `nros-rmw-dds` dual-feature struct refactor (`std`+`nostd-runtime` simultaneous activation), `dds` added to `install-local-posix`'s RMW loop (per-RMW lib namespacing already worked). Cross-instance/process E2E for B-cortex_a9, C-cpp/xrce, F-native-svc/action remain `#[ignore]`d behind unrelated dust-dds SEDP / xrce-cpp-API session-demux bugs (tracked under Phase 96) — example crates themselves all build and reach readiness | Archived |
| 96 | Phase 95 cross-process E2E follow-ups — three independent fixes that each turn one or more `#[ignore]`d tests back on: 96.1 cpp/xrce session-key collision (cpp `init()` hardcoded `hash("nros_cpp")` → all cpp processes shared one XRCE session on the same agent → topics didn't cross-route; fixed by adding `init(locator, domain_id, session_name)` overload + per-example distinct names), 96.2 `test_talker_param_declaration` flake (replaced fixed-window stdout scan with `wait_for_pattern("Counter start value", 15s)`), 96.3 cross-link to Phase 71.28 / 71.29 (dust-dds service SEDP discovery + Cortex-A9 GEM RX queue tuning) | 96.1 mostly closed (talker_listener + service E2Es pass; action E2E still ignored on separate data-path bug); 96.2 + 96.3 closed |
| 97 | DDS per-platform examples + cross-platform E2E — finishes Phase 71's example block. 97.1 generic prerequisites (critical-section feature, linker / heap tuning per board, Kconfig copy-in), 97.2 per-platform `PlatformUdp` smoke binaries, 97.3 bare-metal MPS2-AN385 + ESP32-QEMU DDS examples, 97.4 per-platform DDS pubsub E2E (FreeRTOS / NuttX / ThreadX-RISC-V / ThreadX-Linux / baremetal / ESP32-QEMU; Zephyr native_sim blocked by upstream NSOS gap), 97.5 optional CycloneDDS / FastDDS interop + dust-dds upstream | Not Started |
| 98 | PX4-Autopilot vendoring + SITL E2E infrastructure — `third-party/px4/PX4-Autopilot` + `third-party/px4/px4-rs` as shallow recursive submodules, `just px4 setup/build-sitl/test-sitl` recipes, `EXTERNAL_MODULES_LOCATION` plumbing for nano-ros example modules, `Px4Sitl::boot_in()` fixture reuse from `px4-sitl-tests` | Complete (Phase 90 SITL test passes end-to-end via this infra) |
| 99 | Zero-copy raw pub/sub API — `SlotLending` + `SlotBorrowing` traits in `nros-rmw` (GAT-based), `PublishLoan` + `RecvView` in `nros-node` w/ per-publisher `TxArena` (99.A–99.E arena path; 99.F zenoh-pico via `z_bytes_from_static_buf`; 99.G XRCE-DDS via `uxr_prepare_output_stream`); compile-time gated behind `lending` / `rmw-lending` features. (Renumbered from a local Phase 97 collision after upstream's Phase 97 DDS work landed.) | In Progress (traits + arena + Zenoh + XRCE lending impls landed; uORB arena path landed) |

## Quick Reference

See `book/src/reference/build-commands.md` for manual testing, ROS 2 interop, Docker, QEMU, and Zephyr setup commands. Build the book with `just book`.

## Documentation Index

```
book/src/              # User-facing documentation (mdbook)
├── getting-started/   # installation, native, zephyr, freertos, nuttx, threadx, bare-metal, esp32, ros2-interop
├── user-guide/        # rmw-backends, configuration, message-generation, serial-transport, troubleshooting
├── porting/           # overview, custom-rmw, custom-platform, custom-board
├── reference/         # rust-api, c-api, cpp-api, rmw-api, platform-api, environment-variables, build-commands
├── concepts/          # architecture, no-std, platform-model
└── internals/         # rmw-api-design, rmw-zenoh-protocol, scheduling-models,
                       # verification, realtime-analysis, safety,
                       # zenoh-pico + xrce-dds symbol refs,
                       # creating-examples, platform-porting-pitfalls, contributing

docs/                  # Contributor/internal documentation
├── reference/         # api-comparison-rclrs, rmw-h-analysis, xrce-dds-analysis, executor-fairness-analysis
├── design/            # rmw-layer-design, example-directory-layout, zonal-vehicle-architecture
├── research/          # Internal research
└── roadmap/           # Active + archived phases
```
