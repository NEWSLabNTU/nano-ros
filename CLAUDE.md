# nano-ros

Lightweight ROS 2 client for embedded real-time systems (Zephyr, NuttX). `no_std` compatible.

## Workspace Structure

```
nano-ros/
├── packages/
│   ├── core/                      # The nano-ros library stack
│   │   ├── nros/              # Unified API (re-exports all sub-crates)
│   │   ├── nros-core/         # Core types, traits, node abstraction
│   │   ├── nros-serdes/       # CDR serialization
│   │   ├── nros-macros/       # #[derive(RosMessage)] proc macros
│   │   ├── nros-params/       # Parameter server
│   │   ├── nros-rmw/          # Transport abstraction (middleware traits)
│   │   ├── nros-node/         # High-level node API + parameter_services
│   │   └── nros-c/            # C API for embedded systems
│   ├── zpico/                     # Zenoh-pico transport backend
│   │   ├── nros-rmw-zenoh/        # Safe Rust API for zenoh-pico
│   │   ├── zpico-sys/             # FFI + C shim + zenoh-pico submodule
│   │   ├── zpico-smoltcp/         # TCP/UDP via smoltcp IP stack
│   │   ├── zpico-zephyr/          # Zephyr RTOS BSP (C, CMake)
│   │   ├── zpico-platform-mps2-an385/ # QEMU ARM FFI symbols (no nros deps)
│   │   ├── zpico-platform-esp32/  # ESP32-C3 WiFi FFI symbols
│   │   ├── zpico-platform-esp32-qemu/ # ESP32-C3 QEMU FFI symbols
│   │   └── zpico-platform-stm32f4/   # STM32F4 FFI symbols
│   ├── boards/                    # Board support crates (user API)
│   │   ├── nros-mps2-an385/       # QEMU ARM board (Node API)
│   │   ├── nros-esp32/            # ESP32-C3 WiFi board
│   │   ├── nros-esp32-qemu/       # ESP32-C3 QEMU board
│   │   └── nros-stm32f4/          # STM32F4 board
│   ├── drivers/                   # Hardware drivers
│   │   ├── lan9118-smoltcp/       # LAN9118 Ethernet driver for smoltcp
│   │   └── openeth-smoltcp/       # OpenCores Ethernet driver for smoltcp
│   ├── interfaces/                # Generated ROS 2 types
│   │   └── rcl-interfaces/        # nros-rcl-interfaces + nros-builtin-interfaces
│   │       └── generated/         # manually maintained (nros- prefixed)
│   ├── testing/                   # Test infrastructure
│   │   └── nros-tests/            # Integration test crate
│   ├── verification/              # Formal verification
│   │   ├── nros-ghost-types/      # Ghost model types (workspace member)
│   │   └── nros-verification/     # Verus deductive proofs (excluded from workspace)
│   ├── reference/                 # Low-level platform reference implementations
│   │   └── qemu-smoltcp-bridge/   # smoltcp bridge library
│   └── codegen/                   # Message binding generator (cargo nano-ros)
│       ├── packages/              # Cargo workspace (cargo-nano-ros, rosidl-*, etc.)
│       └── interfaces/            # Bundled .msg/.srv files
├── examples/                  # Standalone example packages (4-level: platform/lang/rmw/use-case)
│   ├── native/                # Desktop/Linux examples
│   │   ├── rust/zenoh/           # Rust + zenoh (talker, listener, service-*, action-*, custom-msg)
│   │   ├── rust/xrce/            # Rust + XRCE-DDS (talker, listener, service-*, action-*)
│   │   └── c/zenoh/              # C + zenoh (talker, listener, custom-msg)
│   ├── qemu-arm/              # QEMU bare-metal ARM (MPS2-AN385)
│   │   └── rust/
│   │       ├── zenoh/            # Networked (talker, listener)
│   │       ├── core/             # nros-core only (cdr-test, wcet-bench)
│   │       └── standalone/       # No nros deps (lan9118)
│   ├── qemu-esp32/            # QEMU ESP32-C3 (RISC-V)
│   │   └── rust/zenoh/           # Networked (talker, listener)
│   ├── esp32/                 # ESP32-C3 hardware
│   │   └── rust/
│   │       ├── zenoh/            # Networked (talker, listener)
│   │       └── standalone/       # No nros deps (hello-world)
│   ├── stm32f4/               # STM32F4 microcontrollers
│   │   └── rust/
│   │       ├── zenoh/            # Networked (talker, polling, rtic)
│   │       ├── core/             # nros-core only (embassy)
│   │       └── standalone/       # No nros deps (smoltcp)
│   └── zephyr/                # Zephyr RTOS
│       ├── rust/zenoh/           # Rust (talker, listener, service-*, action-*)
│       └── c/zenoh/              # C (talker, listener)
├── scripts/zenohd/            # Zenohd build scripts
│   ├── build.sh               # Build zenohd from submodule
│   └── zenoh/                 # Zenoh 1.6.2 submodule
├── scripts/zephyr/            # Zephyr setup scripts
│   ├── setup.sh               # Initialize workspace
│   └── setup-network.sh       # Configure TAP interface
├── cmake/                     # CMake find modules for external users
│   └── FindNanoRos.cmake      # Top-level find module → NanoRos::NanoRos target
├── docker/                    # Docker development environment
│   ├── Dockerfile.qemu-arm    # QEMU 7.2 + ARM toolchain
│   └── docker-compose.yml     # Container orchestration
├── external/                  # Reference projects (git-ignored)
├── tests/                     # Test scripts and docs
├── docs/                      # Detailed documentation
├── zephyr-workspace -> ../nano-ros-workspace/  # Symlink to Zephyr workspace
└── west.yml                   # Zephyr west manifest
```

## Build Commands

```bash
just setup          # Install toolchains, cargo tools, check system deps
just build          # Generate bindings + build workspace (native + embedded) + examples
just build-zenohd   # Build zenohd 1.6.2 from submodule (for integration tests)
just check          # Format + clippy
just quality        # Format + clippy + unit tests (no external deps)
just doc            # Generate docs

# Formal verification
just verify-kani    # Kani bounded model checking (82 harnesses)
just verify-verus   # Verus unbounded deductive proofs (67 proofs)
just verify         # Both Kani + Verus

# Message bindings
just generate-bindings      # Regenerate all generated/ dirs (uses bundled interfaces)
just clean-bindings         # Remove all generated/ dirs (including rcl-interfaces)
just regenerate-bindings    # clean-bindings + generate-bindings

# Test groups (by infrastructure requirement)
just test-unit          # Unit tests only (no external deps)
just test-miri          # Miri UB detection (nros-serdes, nros-core, nros-params)
just test-qemu          # QEMU bare-metal tests (needs qemu-system-arm)
just test-qemu-esp32    # ESP32-C3 QEMU tests (needs qemu-system-riscv32 + espflash)
just test-integration   # All Rust integration tests (builds zenohd automatically)
just test               # test-unit + test-miri + test-qemu + test-integration
just test-zephyr        # Zephyr E2E tests (needs west + TAP)
just test-ros2          # ROS 2 interop tests (needs ROS 2 + rmw_zenoh)
just test-c             # C API tests (needs cmake)
just test-all           # Everything
```

### First-Time Setup

```bash
just setup   # Installs: rustup targets, cargo-nextest, cargo-nano-ros,
             # XRCE Agent, socat. Checks for: arm-none-eabi-gcc, qemu-system-arm, cmake
```

For missing system dependencies on Ubuntu:
```bash
sudo apt install gcc-arm-none-eabi qemu-system-arm cmake socat
```

## Environment Variables

Examples use `Context::from_env()` for configuration:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `ZENOH_LOCATOR` | Router address (e.g., `tcp/192.168.1.1:7447`) | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE` | Session mode: `client` or `peer` | `client` |

Build-time environment variables:

| Variable | Description | Required |
|----------|-------------|----------|
| `ZENOH_PICO_DIR` | CMake install prefix for pre-built zenoh-pico (use with `system-zenohpico` feature on `zpico-sys`) | Only with `system-zenohpico` |

## Development Practices

### Quality Checks
**Always run `just quality` after completing a task.**

### System Packages
**Never install system packages directly.** Inform the user what's needed:
```
QEMU ARM emulator required. Please run: sudo apt install qemu-system-arm
```

### Privileged Commands
**Never execute sudo commands directly.** Provide the command for the user to run.

### Unused Variables
- Rename to `_name` with a comment explaining why
- Use `#[allow(dead_code)]` for test struct fields

### Testing
- **Reusable tests** belong in `packages/testing/nros-tests/tests/` (Rust integration tests) or `tests/` (shell scripts)
- **Temporary/exploratory tests** can be run directly in the Bash tool, but should be converted to proper test scripts once the feature is validated
- Test scripts in `tests/` should have justfile entries for easy invocation (e.g., `just test-ros2-interop-debug`)
- ROS 2 interop tests requiring `rmw_zenoh_cpp` go in `packages/testing/nros-tests/tests/rmw_interop.rs` or `tests/ros2-interop-debug.sh`

### QEMU Networked Test Rules
For QEMU tests involving pub/sub communication via zenohd + TAP networking:
- **Each QEMU peer must use a different TAP device** (e.g., talker on `tap-qemu0`, listener on `tap-qemu1`). This applies to all QEMU platforms (ARM and ESP32-C3).
- **Start the subscriber first, then the publisher.** Zenoh doesn't buffer messages for unknown subscribers.
- **Add 5s stabilization delay** between subscriber connection and publisher start, to allow subscription propagation through zenohd.
- **Verify zenohd on the bridge IP** (e.g., `192.0.3.1:7447`), not just localhost. QEMU instances reach zenohd via the bridge.
- **Use `max-threads = 1` nextest test groups** for tests sharing a fixed zenoh port.
- See `tests/README.md` section "QEMU Networked Test Practices" for full details and example ordering.

### Test Output and Logs

All Rust tests run through **cargo-nextest** which provides concise colored progress output. Test results are automatically saved as JUnit XML.

**Output modes (all `just test-*` recipes accept a `verbose` argument):**
```bash
just test-integration           # Concise: colored progress bar, failures shown at end
just test-integration verbose   # Verbose: all test output streamed live
```

**JUnit XML logs** are generated automatically by nextest (configured in `.config/nextest.toml`):
- Written to: `target/nextest/default/junit.xml`
- Contains per-test pass/fail status and stdout/stderr for failing tests
- Each nextest invocation overwrites the file (re-run a specific suite to get its XML)
- View with: `just test-report` (requires `junit-cli-report-viewer`)

**Non-nextest tests** (QEMU semihosting, C shell scripts) use `tests/run-test.sh` wrapper:
- Captures output to timestamped log files in `test-logs/latest/`
- Prints one-line `[PASS]`/`[FAIL]` summary per test
- `--qemu` flag parses semihosting `[PASS]`/`[FAIL]` markers

**Configuration files:**
- `.config/nextest.toml` — nextest profiles, JUnit output, test groups (e.g., zephyr max-threads=1)
- `tests/run-test.sh` — wrapper for non-cargo tests

### Temporary Files
- Create all temporary files (scripts, test data, scratch work) in `$project/tmp/` directory (not `/tmp`)
- Use Write/Edit tools to create files (avoid cat + heredoc patterns)
- The `tmp/` directory is git-ignored and can be cleaned freely

### Writing Tests

**Integration tests** go in `packages/testing/nros-tests/tests/`. Each file is a test suite:
```
packages/testing/nros-tests/
├── src/              # Test utilities and fixtures
│   ├── lib.rs        # wait_for_pattern(), count_pattern(), etc.
│   ├── fixtures/     # rstest fixtures (ZenohRouter, binary builders)
│   ├── process.rs    # Managed child process helpers
│   ├── qemu.rs       # QEMU process management
│   ├── ros2.rs       # ROS 2 process helpers
│   └── zephyr.rs     # Zephyr native_sim helpers
└── tests/            # Integration test suites
    ├── nano2nano.rs  # nros ↔ nros pub/sub
    ├── services.rs   # Service server/client tests
    ├── actions.rs    # Action server/client tests
    ├── emulator.rs   # QEMU bare-metal tests
    ├── zephyr.rs     # Zephyr E2E tests
    ├── rmw_interop.rs # ROS 2 interop tests
    └── ...           # Other suites
```

The `tests/` directory at project root contains shell-based test scripts (C tests, Zephyr C tests, ROS 2 interop shell tests).

**Running tests:** Use `just test-*` recipes. Avoid writing large test scripts in the Bash tool. Only use Bash for temporary one-off test commands.
```bash
just test-unit          # Unit tests (no deps)
just test-miri          # Miri UB detection on embedded-safe crates
just test-integration   # All integration tests (needs zenohd)
just test-zephyr        # Zephyr E2E (needs west + TAP)
just test-ros2          # ROS 2 interop (needs ROS 2)
just test-c             # C API tests (needs cmake)
just test-report        # View JUnit XML report (needs junit-cli-report-viewer)
```

**Unit tests** (per-crate `#[cfg(test)]` modules) go in each crate's source files as usual.

## Key Design Patterns

### Zenoh Version Unification
All zenoh components are pinned to **1.6.2** for compatibility with rmw_zenoh_cpp (ros-humble-zenoh-cpp-vendor 0.1.8):
- **zenohd**: Built from submodule at `scripts/zenohd/zenoh/` via `just build-zenohd` → `build/zenohd/zenohd`
- **zenoh-pico**: Submodule at `packages/zpico/zpico-sys/zenoh-pico/` (1.6.2)
- **rmw_zenoh_cpp**: Bundles zenoh-c 1.6.2

Test infrastructure (`nros-tests`) and shell scripts automatically use the local build at `build/zenohd/zenohd` when available, falling back to the system `zenohd`.

### Rust Edition 2024
All crates use Rust edition 2024. Key syntax changes from edition 2021:

- **Extern blocks require `unsafe`**: `unsafe extern "C" { ... }`
- **no_mangle requires `unsafe`**: `#[unsafe(no_mangle)]`
- **Unsafe fn bodies require explicit blocks**: Unsafe operations inside `unsafe fn` need `unsafe { ... }` blocks

The `nros-c` crate keeps `#![allow(unsafe_op_in_unsafe_fn)]` because it's a pure C FFI wrapper with 420+ unsafe operations where adding explicit blocks would add verbosity without safety improvement.

### API Alignment

The nros API follows established ROS 2 client library conventions:

- **Rust API**: Follows [rclrs](external/ros2_rust) (ROS 2 Rust client) 0.7.0 naming
- **C API**: Follows rclc (ROS 2 C client) naming

Key naming rules:
- `create_publisher()`, `create_subscription()` (not `create_subscriber`)
- `create_service()`, `create_client()`
- `create_action_server()`, `create_action_client()`
- Clean type names: `Publisher<M>`, `Subscription<M>`, `Service<S>`, `Client<S>`, `ActionServer<A>`, `ActionClient<A>`
- Error type: `RclrsError` for the unified error enum

### `no_std` Support
All core crates support `#![no_std]` with optional `std`/`alloc` features.

### Message Types
Generated per-project using `cargo nano-ros generate-rust` from `package.xml`. See [docs/guides/message-generation.md](docs/guides/message-generation.md).

**All examples must use generated message bindings** — never hand-write message types. Each example has a `package.xml` declaring its ROS interface dependencies and a `generated/` directory with the output of `cargo nano-ros generate-rust`. See [docs/guides/creating-examples.md](docs/guides/creating-examples.md) for the full guide.

**Example `generated/` directories are gitignored** and recreated by `just generate-bindings` (called automatically by `just build`). Only `packages/interfaces/rcl-interfaces/generated/` is checked into git (workspace member — cargo requires member paths on disk). These internal packages use `nros-` prefixed names (`nros-builtin-interfaces`, `nros-rcl-interfaces`) to avoid lockfile collisions with user-generated packages of the same ROS 2 name.

**`.cargo/config.toml` is manually maintained** per example. Each contains `[patch.crates-io]` entries pointing to the local workspace crates, along with platform-specific `[build]` and `[target.*]` settings. The codegen tool does not touch these files.

**Bundled interfaces**: Standard .msg files (`std_msgs`, `builtin_interfaces`) are shipped at `packages/codegen/interfaces/` so codegen works without a ROS 2 environment. The ament index takes precedence when available; bundled files fill gaps.

**heapless re-export**: `nros-core` re-exports `heapless` (`pub use heapless;`) so generated code can reference `nros_core::heapless::String<256>` etc. without requiring a separate `heapless` dependency.

**Inline codegen mode**: `rosidl-codegen` supports an inline mode (`NanoRosCodegenMode::Inline`) where generated code uses `nros_core::` prefixed imports and `super::` relative paths for cross-package references. This is used for single-crate scenarios; the standard `cargo nano-ros generate-rust` (separate crates per package) remains the primary workflow.

**Installing cargo-nano-ros:**
```bash
# From the nros repository root
just install-cargo-nano-ros

# Or manually:
cargo install --path packages/codegen/packages/cargo-nano-ros --locked

# Or from git (external users):
cargo install --git https://github.com/jerry73204/nano-ros --path packages/codegen/packages/cargo-nano-ros
```

**Regenerating bindings:**
```bash
just generate-bindings      # Regenerate all (uses bundled interfaces, no ROS 2 needed)
just regenerate-bindings    # Clean + regenerate from scratch
```

**Building the C codegen library (for CMake integration):**
```bash
just build-codegen-lib
```

### C API and CMake Integration
C examples use `FindNanoRos.cmake` (at `cmake/FindNanoRos.cmake`) which wraps the internal `FindNanoRosC.cmake` (at `packages/core/nros-c/cmake/`). Usage:
```cmake
list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/cmake")
find_package(NanoRos REQUIRED)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
```
This provides include dirs, static library, and platform link libs (pthread, dl, m) automatically.

**C code generation** uses `nano_ros_generate_interfaces()` (from `nano_ros_generate_interfaces.cmake`). The codegen tool is bundled as `libnano_ros_codegen_c.a` — no external `nros` binary needed. Build it with `just build-codegen-lib` before running CMake. The CMake module `FindNanoRosCodegen.cmake` compiles a thin C wrapper at configure time.

### Platform Backends
Selected via feature flags: `platform-posix` (desktop), `platform-zephyr` (Zephyr RTOS), `platform-bare-metal` (bare-metal).
The `zenoh` feature is an alias for `platform-posix` + `alloc`.

### Parameter Services
Enable with `param-services` feature in `nros-node`:
```toml
nros-node = { version = "*", features = ["param-services"] }
```
- Provides ROS 2 parameter service handlers (`~/get_parameters`, `~/set_parameters`, etc.)
- Uses generated `nros-rcl-interfaces` types from `packages/interfaces/rcl-interfaces/generated/`
- Handlers return `Box<Response>` due to large heapless arrays (~1MB per ParameterValue)

### Formal Verification

Two complementary verification tools are used:

- **Kani** (bounded model checking) — `#[cfg(kani)]` harnesses inside production crates. 82 harnesses across nros-serdes, nros-core, nros-params, nros-c. Run with `just verify-kani`.
- **Verus** (unbounded deductive proofs) — separate crate at `packages/verification/nros-verification/` (excluded from workspace). 67 proofs across scheduling, time arithmetic, CDR serialization, GoalStatus state machine, parameter types, and E2E data path. Includes 10 E2E proofs (bug existence, publish chain, executor delivery, post-fix correctness). Run with `just verify-verus`.

```bash
just verify          # Run both Kani + Verus (requires both toolchains)
just verify-kani     # Kani only (~3 min, requires cargo-kani: just setup)
just verify-verus    # Verus only (~1 sec, requires Verus: just setup-verus)
```

`just verify-kani` runs `cargo kani` on each of the 4 crates sequentially. Kani's `goto-cc` (CBMC 6.8.0) can occasionally crash with `unexpected end of input stream` (exit status 70) — this is an intermittent CBMC toolchain issue, not a code bug. Retry resolves it.

`just verify-verus` runs `cargo verus verify` in the verification crate. Requires the Verus toolchain in `tools/` (installed by `just setup-verus`).

Key Verus patterns:
- `external_type_specification` without `external_body` makes enums **transparent** (variant matching works)
- `external_type_specification` with `external_body` makes types **opaque** (no variant matching)
- `assume_specification[Type::method](self_: &Type, ...)` links production fn to spec — `&self` becomes `self_: &Type`
- Never add `[package.metadata.verus] verify = true` to production crates with fn pointers or closures (causes THIR erasure crash)

See [docs/guides/verus-verification.md](docs/guides/verus-verification.md) for full coding practices.

### ROS 2 Interop
Uses rmw_zenoh-compatible protocol. Key format for Humble:
- Data keyexpr: `<domain>/<topic>/<type>/TypeHashNotSupported`
- Liveliness: `@ros2_lv/.../<type>/RIHS01_<hash>/<qos>`

See [docs/reference/rmw_zenoh_interop.md](docs/reference/rmw_zenoh_interop.md).

## Development Phases

Completed phases (1-15, 17-18, 20-21, 25-29, 32) are archived in `docs/roadmap/archived/`.

| Phase | Focus | Status |
|-------|-------|--------|
| 16 | ROS 2 Interop Completion | In Progress |
| 22 | ESP32-C3 platform support | In Progress |
| 23 | Arduino precompiled library | Not Started |
| 24 | RPi Pico W platform support | Not Started |
| 31 | Verus unbounded verification | In Progress |
| 33 | Crate rename (`nros-*` / `zpico-*`) | Complete |
| 34 | RMW abstraction + XRCE-DDS | In Progress |
| 36 | Multi-backend integration tests | Not Started |

**Phase 16**: Core implementation complete. Remaining: ROS 2 integration tests (services, actions, discovery), Iron+ type hash (future).

**Phase 33**: Complete. All crates renamed from `nano-ros-*` to `nros-*` / `zpico-*`. Transport split into `nros-rmw` + `nros-rmw-zenoh`. Platform crates split into `zpico-platform-*` + `nros-*` board crates. See `docs/design/rmw-layer-design.md`.

**Phase 34**: 34.1-34.8 complete. RMW factory trait, zenoh backend, board refactor, XRCE-DDS FFI (`xrce-sys`), UDP transport (`xrce-smoltcp`), RMW implementation (`nros-rmw-xrce`), platform symbols, integration test infrastructure.

**Phase 36**: Multi-backend integration tests. XRCE service test binaries, hardened pub/sub tests, `xrce` feature on `nros` crate. See `docs/roadmap/phase-36-multi-backend-integration-tests.md`.

See [docs/roadmap/](docs/roadmap/) for details.

### Distribution UX (Future)

Planned improvements for toolchain distribution:
- **crates.io publishing**: `cargo-nano-ros`, `nros-core`, `nros-serdes`, pre-generated standard message crates — eliminates `[patch.crates-io]`
- **Pre-built binaries**: GitHub releases for `nros` binary
- **`cargo nano-ros init`**: Template scaffolding for new projects
- **C single-archive release**: library + headers + cmake modules + codegen binary

## Documentation Index

```
docs/
├── guides/          # Getting started, setup, how-to
├── reference/       # Protocol specs, comparisons
├── design/          # Active architecture docs
│   └── archived/    # Superseded design docs
├── research/        # Autoware porting analysis
└── roadmap/         # Active phases (16, 22-24, 32)
    └── archived/    # Completed phases (1-15, 17-21, 25-31)
```

Key docs: [getting-started](docs/guides/getting-started.md), [creating-examples](docs/guides/creating-examples.md), [message-generation](docs/guides/message-generation.md), [troubleshooting](docs/guides/troubleshooting.md), [rmw-layer-design](docs/design/rmw-layer-design.md), [rmw_zenoh interop](docs/reference/rmw_zenoh_interop.md), [tests/README](tests/README.md).

## Quick Reference

### Manual Testing
```bash
# Build zenohd first (one-time)
just build-zenohd

# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rust/zenoh/listener && RUST_LOG=info cargo run --features zenoh
```

### ROS 2 Interop
```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nros talker
cd examples/native/rust/zenoh/talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: ROS 2 listener
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

### Actions
ROS 2 actions support long-running tasks with feedback and cancellation.

```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Action server (Fibonacci example)
cd examples/native/rust/zenoh/action-server && cargo run

# Terminal 3: Action client
cd examples/native/rust/zenoh/action-client && cargo run
```

**Zephyr action tests:**
```bash
just build-zephyr-actions      # Build server and client
just test-rust-zephyr-actions  # Run E2E tests (requires TAP setup)
```

See `docs/roadmap/phase-6-actions.md` for API details.

### Zephyr Setup
```bash
./scripts/zephyr/setup.sh              # Initialize workspace + create symlink
sudo ./scripts/zephyr/setup-network.sh # Configure TAP network
just test-zephyr                       # Run tests
```

The `zephyr-workspace` symlink points to the actual workspace (default: `../nano-ros-workspace/`).
Scripts use this symlink to locate the workspace. For custom workspace locations, update the symlink:
```bash
ln -sfn /path/to/custom-workspace zephyr-workspace
```

See [docs/guides/zephyr-setup.md](docs/guides/zephyr-setup.md) for details.

### Docker Development Environment

Docker provides QEMU 7.2 (from Debian bookworm) which fixes TAP networking issues present in Ubuntu 22.04's QEMU 6.2.

```bash
# One-time setup: add yourself to docker group
sudo usermod -aG docker $USER
# Log out and back in, or run: newgrp docker

# Build and use Docker environment
just docker-build              # Build nano-ros-qemu image
just docker-shell              # Interactive shell
just docker-test-qemu          # Run QEMU tests in container
just docker-help               # Show all Docker commands
```

See `docs/reference/qemu-physical-device-compatibility.md` for QEMU/physical device analysis.

### QEMU Bare-Metal Testing

Run bare-metal Cortex-M3 examples on QEMU (MPS2-AN385 machine with LAN9118 Ethernet).

```bash
# Build prerequisites
just build-zenoh-pico-arm     # Build zenoh-pico for ARM Cortex-M3
just build-examples-qemu      # Build all QEMU examples

# Non-networked tests (no setup required)
just test-qemu-basic          # Run serialization test
just test-qemu-lan9118        # Run Ethernet driver test

# Networked talker/listener test (Docker Compose - recommended)
just docker-qemu-test         # Runs zenohd, talker, listener in separate containers
```

**Docker Compose Architecture:**
```
┌─────────────────────────────────────────────────────────────┐
│              Docker Network: 172.20.0.0/24                  │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   zenohd    │  │   talker    │  │      listener       │  │
│  │ 172.20.0.2  │  │ 172.20.0.10 │  │    172.20.0.11      │  │
│  │             │  │  ┌───────┐  │  │  ┌───────────────┐  │  │
│  │             │  │  │ QEMU  │  │  │  │     QEMU      │  │  │
│  │             │  │  │ ARM   │──┼──┼──│     ARM       │  │  │
│  │             │  │  │ TAP   │  │  │  │     TAP       │  │  │
│  │             │  │  └───────┘  │  │  └───────────────┘  │  │
│  └──────▲──────┘  └──────┼──────┘  └─────────┼───────────┘  │
│         └────────────────┴───────────────────┘              │
│                    NAT to zenohd                            │
└─────────────────────────────────────────────────────────────┘
```

Each container has isolated TAP networking with NAT to reach zenohd.

**Manual networked test (3 terminals, requires host TAP setup):**
```bash
# Terminal 1: Setup network + start router
just setup-qemu-network                    # Requires sudo
./build/zenohd/zenohd --listen tcp/0.0.0.0:7447

# Terminal 2: Talker (192.0.2.10)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 \
    --binary examples/qemu-arm/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker

# Terminal 3: Listener (192.0.2.11)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu-arm/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener
```

Run `just qemu-help` for more options.
