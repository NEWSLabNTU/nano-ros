# nano-ros

Lightweight ROS 2 client for embedded real-time systems (Zephyr, FreeRTOS, NuttX). `no_std` compatible.

### Naming Convention

- **nano-ros** — project name (prose, docs, user-facing text)
- **nros** — code shorthand (crate names, Rust/C identifiers, Kconfig `CONFIG_NROS_*`)
- **nano_ros** — C header dir (`nano_ros/`), CMake targets (`NanoRos::NanoRos`), CMake function (`nano_ros_generate_interfaces()`)

## Workspace Structure

```
nano-ros/
├── packages/
│   ├── core/           # nros, nros-core, nros-serdes, nros-macros, nros-params, nros-rmw, nros-node, nros-c
│   ├── zpico/          # Zenoh-pico backend: nros-rmw-zenoh, zpico-sys, zpico-smoltcp, zpico-zephyr, platform-*
│   ├── xrce/           # XRCE-DDS backend: nros-rmw-xrce, xrce-sys, xrce-smoltcp, xrce-zephyr, platform-*
│   ├── boards/         # Board support: nros-mps2-an385, nros-mps2-an385-freertos, nros-esp32, nros-esp32-qemu, nros-stm32f4
│   ├── drivers/        # lan9118-smoltcp, lan9118-lwip, openeth-smoltcp
│   ├── interfaces/     # rcl-interfaces (generated/, checked into git)
│   ├── testing/        # nros-tests (integration test crate)
│   ├── verification/   # nros-ghost-types, nros-verification (Verus proofs, excluded from workspace)
│   ├── reference/      # qemu-smoltcp-bridge
│   └── codegen/        # cargo-nano-ros, rosidl-*, bundled .msg/.srv files
├── examples/           # 4-level: platform/lang/rmw/use-case (native, qemu-arm-baremetal, qemu-arm-freertos, qemu-esp32-baremetal, esp32, stm32f4, zephyr)
├── external/           # Third-party SDK sources (git-ignored): freertos-kernel, lwip, nuttx, nuttx-apps
├── scripts/            # zenohd build, Zephyr setup
├── docker/             # QEMU dev environment
├── tests/              # Shell-based test scripts
├── docs/               # Guides, reference, design, roadmap
├── zephyr/             # Zephyr module (Kconfig, CMakeLists.txt, cmake/)
└── CMakeLists.txt      # Top-level CMake (Corrosion, nros-c + codegen)
```

## Build Commands

```bash
just setup              # Install toolchains, cargo tools, download FreeRTOS/NuttX SDKs
just setup-freertos     # Download FreeRTOS kernel + lwIP (included in just setup)
just setup-nuttx        # Download NuttX RTOS + apps (included in just setup)
just build              # Generate bindings + build workspace + examples
just build-zenohd       # Build zenohd 1.6.2 from submodule
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
just test-all           # Everything (includes NuttX + FreeRTOS in one nextest run)
```

First-time: `just setup` installs everything (toolchains, cargo tools, system deps, FreeRTOS/NuttX SDKs).

## Environment Variables

Runtime: `ROS_DOMAIN_ID` (default `0`), `ZENOH_LOCATOR` (default `tcp/127.0.0.1:7447`), `ZENOH_MODE` (`client`/`peer`).

FreeRTOS/NuttX build-time variables are **auto-resolved** by justfile recipes (defaulting to `external/` paths from `just setup-freertos` / `just setup-nuttx`). Override via env vars if sources are elsewhere:
- `FREERTOS_DIR` — FreeRTOS kernel source (default: `external/freertos-kernel`)
- `FREERTOS_PORT` — portable layer (default: `GCC/ARM_CM3`)
- `LWIP_DIR` — lwIP source (default: `external/lwip`)
- `FREERTOS_CONFIG_DIR` — `FreeRTOSConfig.h` + `lwipopts.h` (default: board crate's `config/`)
- `NUTTX_DIR` — NuttX RTOS source (default: `external/nuttx`)

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
- Per-example `.gitignore` for build artifacts (`/target/`, `/generated/`)
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
All zenoh components pinned to **1.6.2** (compatible with rmw_zenoh_cpp). zenohd built from `scripts/zenohd/zenoh/` submodule; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Test infra auto-uses `build/zenohd/zenohd` when available.

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

### Platform Backends
Three orthogonal axes (NEVER cross-imply):
- **RMW backend** (one): `rmw-zenoh`, `rmw-xrce`
- **Platform** (one): `platform-posix`, `platform-zephyr`, `platform-bare-metal`, `platform-freertos`, `platform-nuttx`
- **ROS edition** (one): `ros-humble`, `ros-iron`

Mutual exclusivity enforced at compile-time. Zero features on an axis is valid (reduced functionality).
Default features: `std` only. Platform features forwarded via Cargo `?` syntax.

**Cross-cutting:** `unstable-zenoh-api` enables zero-copy receive (orthogonal to axes above).

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
| 49 | nros-c thin wrapper migration | Complete |
| 51 | Board crate `run()` API | In Progress |
| 53 | UDP + TLS transport support | Complete |
| 54 | FreeRTOS platform support (lwIP) | In Progress (54.1–54.11 done, 54.10 deferred, 54.12 remaining) |
| 55 | NuttX platform support | In Progress (55.1–55.10, 55.12 done, 55.11 remaining) |
| 56 | Verification refresh | Complete |
| 57 | Code quality improvements | Complete |
| 58 | ThreadX platform support (NetX Duo) | In Progress (58.1–58.7 done) |
| 59 | API documentation (rustdoc + Doxygen) | Complete |
| 60 | std/alloc feature consistency | Complete |
| 61 | FFI reentrancy guards (zpico + XRCE critical sections) | Complete |
| 62 | Event-driven async waking (AtomicWaker) | Complete |
| 63 | RTIC integration (examples + QEMU testing) | Not Started |

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
