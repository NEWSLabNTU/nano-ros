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
│   ├── boards/         # Board support: nros-mps2-an385, nros-mps2-an385-freertos, nros-nuttx-qemu-arm, nros-threadx-linux, nros-threadx-qemu-riscv64, nros-esp32, nros-esp32-qemu, nros-stm32f4
│   ├── drivers/        # lan9118-smoltcp, lan9118-lwip, openeth-smoltcp, virtio-net-netx
│   ├── interfaces/     # rcl-interfaces (generated/, checked into git)
│   ├── testing/        # nros-tests (integration test crate)
│   ├── verification/   # nros-ghost-types, nros-verification (Verus proofs, excluded from workspace)
│   ├── reference/      # qemu-smoltcp-bridge
│   └── codegen/        # cargo-nano-ros, rosidl-*, bundled .msg/.srv files
├── examples/           # 4-level: platform/lang/rmw/use-case (native, qemu-arm-baremetal, qemu-arm-freertos, qemu-arm-nuttx, qemu-riscv64-threadx, threadx-linux, qemu-esp32-baremetal, esp32, stm32f4, zephyr)
├── external/           # Third-party SDK sources (git-ignored): freertos-kernel, lwip, nuttx, nuttx-apps, threadx, netxduo, threadx-learn-samples
├── scripts/            # zenohd build, Zephyr setup
├── docker/             # QEMU dev environment
├── tests/              # Shell-based test scripts
├── docs/               # Guides, reference, design, roadmap
├── zephyr/             # Zephyr module (Kconfig, CMakeLists.txt, cmake/)
└── CMakeLists.txt      # Top-level CMake (Corrosion, nros-c + nros-cpp + codegen)
```

## Build Commands

```bash
just setup              # Install toolchains, cargo tools, download FreeRTOS/NuttX/ThreadX SDKs
just setup-freertos     # Download FreeRTOS kernel + lwIP (included in just setup)
just setup-nuttx        # Download NuttX RTOS + apps (included in just setup)
just setup-threadx      # Download ThreadX kernel + NetX Duo (included in just setup)
just build              # Generate bindings + build workspace + examples
just build-zenohd       # Build zenohd from submodule
just check              # Format check + clippy
just quality            # Format + check + test
just doc                # Generate docs
just verify             # Kani + Verus verification
just generate-bindings  # Regenerate all generated/ dirs
```

Test groups:
```bash
just test-unit          # Unit tests (no external deps)
just test-miri          # Miri UB detection
just test-qemu          # QEMU bare-metal tests
just test-integration   # Rust integration tests (builds zenohd automatically)
just test               # unit + miri + qemu + integration
just test-zephyr        # Zephyr E2E (needs west + TAP)
just test-zephyr-xrce   # Zephyr E2E — XRCE (needs west + TAP + Agent)
just test-ros2          # ROS 2 interop (needs ROS 2 + rmw_zenoh)
just test-c             # C API tests (needs cmake)
just test-freertos      # FreeRTOS QEMU E2E (needs qemu-system-arm + arm-none-eabi-gcc)
just test-nuttx         # NuttX QEMU E2E (needs nightly + qemu-system-arm)
just test-threadx       # ThreadX E2E — Linux sim + QEMU RISC-V (needs ThreadX/NetX + qemu-system-riscv64)
just test-threadx-linux # ThreadX Linux simulation E2E (needs ThreadX/NetX + CAP_NET_RAW)
just test-all           # Everything (includes NuttX + FreeRTOS + ThreadX in one nextest run)
```

First-time: `just setup` installs everything (toolchains, cargo tools, system deps, FreeRTOS/NuttX/ThreadX SDKs).

## Environment Variables

Configuration via `.env` file: copy `.env.example` to `.env` (gitignored) and uncomment values. Loaded automatically by justfile and direnv.

Runtime: `ROS_DOMAIN_ID` (default `0`), `ZENOH_LOCATOR` (default `tcp/127.0.0.1:7447`), `ZENOH_MODE` (`client`/`peer`).

FreeRTOS/NuttX/ThreadX build-time variables are **auto-resolved** by justfile recipes (defaulting to `external/` paths from `just setup-freertos` / `just setup-nuttx` / `just setup-threadx`). Override via env vars if sources are elsewhere:
- `FREERTOS_DIR` — FreeRTOS kernel source (default: `external/freertos-kernel`)
- `FREERTOS_PORT` — portable layer (default: `GCC/ARM_CM3`)
- `LWIP_DIR` — lwIP source (default: `external/lwip`)
- `FREERTOS_CONFIG_DIR` — `FreeRTOSConfig.h` + `lwipopts.h` (default: board crate's `config/`)
- `NUTTX_DIR` — NuttX RTOS source (default: `external/nuttx`)
- `NUTTX_APPS_DIR` — NuttX apps source (default: `external/nuttx-apps`)
- `THREADX_DIR` — ThreadX kernel source (default: `external/threadx`)
- `THREADX_CONFIG_DIR` — ThreadX config directory (default: board crate's `config/`)
- `NETX_DIR` — NetX Duo source (default: `external/netxduo`)
- `NETX_CONFIG_DIR` — NetX Duo config directory (default: board crate's `config/`)

Buffer tuning: see [docs/reference/environment-variables.md](docs/reference/environment-variables.md).

## Development Practices

### Quality Checks
**Always run `just quality` after completing a task.**

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
- JUnit XML: `target/nextest/default/junit.xml` (auto-generated by nextest)
- Non-nextest tests use `tests/run-test.sh` wrapper → logs in `test-logs/latest/`
- See `tests/README.md` for full test infrastructure docs

### QEMU Networked Test Rules
- **Each QEMU peer must use a different TAP device** (talker on `tap-qemu0`, listener on `tap-qemu1`)
- **Start subscriber first, then publisher.** Zenoh doesn't buffer for unknown subscribers.
- **5s stabilization delay** between subscriber connection and publisher start
- **Verify zenohd on bridge IP** (e.g., `192.0.3.1:7447`), not just localhost
- **Use `max-threads = 1` nextest test groups** for tests sharing a fixed zenoh port

### Temporary Files
- Create in `$project/tmp/` (git-ignored), not `/tmp`
- Use Write/Edit tools (avoid cat + heredoc)

### `.gitignore` Practices
- **Every workspace-excluded crate** (examples, board crates in `exclude`, standalone packages) must have a per-directory `.gitignore` with at least `/target/`. Add `/generated/` if the crate uses `cargo nano-ros generate`.
- **Every native C++ example** must have a per-directory `.gitignore` with `/build/` (CMake builds in-tree). Zephyr C++ examples don't need this since they build in the Zephyr workspace.
- Root `.gitignore` only for repo-wide patterns
- Always use leading `/` (e.g., `/target/` not `target/`)
- When adding `--target-dir` for build isolation, add the dir to the example's `.gitignore`

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
All zenoh components pinned to **1.7.2** (compatible with rmw_zenoh_cpp). zenohd built from `scripts/zenohd/zenoh/` submodule; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Test infra auto-uses `build/zenohd/zenohd` when available.

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

### Platform Backends
Three orthogonal axes (NEVER cross-imply):
- **RMW backend** (one): `rmw-zenoh`, `rmw-xrce`
- **Platform** (one): `platform-posix`, `platform-zephyr`, `platform-bare-metal`, `platform-freertos`, `platform-nuttx`, `platform-threadx`
- **ROS edition** (one): `ros-humble`, `ros-iron`

Mutual exclusivity enforced at compile-time. Zero features on an axis is valid (reduced functionality).
Default features: `std` only. Platform features forwarded via Cargo `?` syntax.

**Cross-cutting:** `unstable-zenoh-api` enables zero-copy receive (orthogonal to axes above).

### Board Crate Transport Features
Board crates use Cargo features to select the communication transport:
- **`ethernet`** (default for MPS2-AN385, STM32F4, ESP32-QEMU) or **`wifi`** (default for ESP32) — TCP/UDP via `zpico-smoltcp`
- **`serial`** — UART via `zpico-serial` (bare-metal only) or zenoh-pico built-in serial (ESP32, Zephyr, etc.)

`Config` struct fields are `#[cfg(feature = "...")]`-gated per transport (e.g., MAC/IP under `ethernet`, baudrate under `serial`). At least one transport must be enabled (`compile_error!` enforced). Both can be enabled simultaneously — runtime selection via the zenoh locator string.

ESP32 and ESP32-QEMU use zenoh-pico's built-in serial implementation (no `zpico-serial` dependency). Only bare-metal board crates (`nros-mps2-an385`, `nros-stm32f4`) depend on `zpico-serial`.

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
| 54 | FreeRTOS platform support (lwIP) | Complete (54.10 deferred to Phase 69) |
| 55 | NuttX platform support | Complete |
| 58 | ThreadX platform support (NetX Duo) | Complete |
| 64 | Embedded transport tuning guide | In Progress (64.1 done, 64.2 remaining) |
| 65 | .env.example + environment docs | In Progress (35/36 done) |
| 67 | Serial transport + board crate transport abstraction | Complete |
| 68 | Alloc-free C/C++ bindings + executor simplification | Complete |
| 69 | Cross-platform C/C++ examples + integration tests | Not Started |
| 70 | DDS RMW backend (dust-dds) — POSIX | In Progress |
| 71 | Refactor dust-dds to platform-agnostic + bare-metal DDS | Not Started |
| 72 | Per-example config.toml files | Not Started |

## Quick Reference

See `book/src/reference/build-commands.md` for manual testing, ROS 2 interop, Docker, QEMU, and Zephyr setup commands. Build the book with `just book`.

## Documentation Index

```
book/src/            # User-facing documentation (mdbook)
├── getting-started/ # installation, first-app-rust, first-app-c, ros2-interop
├── concepts/        # architecture, no-std, rmw-backends, platform-model
├── guides/          # message-generation, creating-examples, qemu-bare-metal, esp32, troubleshooting
├── platforms/       # overview, posix, zephyr, freertos, nuttx, threadx
├── reference/       # rust-api, c-api, environment-variables, embedded-tuning, build-commands, rmw-zenoh-protocol
└── advanced/        # verification, realtime-analysis, safety, contributing

docs/                # Contributor/internal documentation
├── reference/       # api-comparison-rclrs, rmw-h-analysis, xrce-dds-analysis, executor-fairness-analysis
├── design/          # rmw-layer-design, example-directory-layout, zonal-vehicle-architecture
├── research/        # Internal research
└── roadmap/         # Active + archived phases
```
