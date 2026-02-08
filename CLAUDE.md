# nano-ros

Lightweight ROS 2 client for embedded real-time systems (Zephyr, NuttX). `no_std` compatible.

## Workspace Structure

```
nano-ros/
├── crates/
│   ├── nano-ros/              # Unified API (re-exports all sub-crates)
│   ├── nano-ros-core/         # Core types, traits, node abstraction
│   ├── nano-ros-serdes/       # CDR serialization
│   ├── nano-ros-macros/       # #[derive(RosMessage)] proc macros
│   ├── nano-ros-params/       # Parameter server
│   ├── nano-ros-transport/    # Transport abstraction (zenoh backend)
│   ├── nano-ros-node/         # High-level node API + parameter_services
│   ├── nano-ros-tests/        # Integration test crate
│   ├── nano-ros-bsp-qemu/     # QEMU MPS2-AN385 Board Support Package
│   ├── nano-ros-bsp-stm32f4/  # STM32F4 Board Support Package
│   ├── nano-ros-bsp-zephyr/   # Zephyr RTOS Board Support Package (C)
│   ├── rcl-interfaces/        # Generated ROS 2 interface types
│   │   └── generated/         # cargo nano-ros generate-rust output
│   │       ├── rcl_interfaces/    # Parameter service types
│   │       └── builtin_interfaces/ # Time, Duration types
│   ├── zenoh-pico-shim/       # Safe Rust API for zenoh-pico
│   └── zenoh-pico-shim-sys/   # FFI + C shim + zenoh-pico submodule
├── colcon-nano-ros/           # Message binding generator (cargo nano-ros)
├── examples/                  # Standalone example packages (see examples/README.md)
│   ├── native/                # Desktop/Linux examples
│   │   ├── rs-talker/            # Rust publisher
│   │   ├── rs-listener/          # Rust subscriber
│   │   ├── rs-service-*/         # Rust service examples
│   │   ├── rs-action-*/          # Rust action examples
│   │   └── c-*/                  # C language examples
│   ├── qemu/                  # QEMU bare-metal ARM (uses bsp-qemu)
│   │   ├── bsp-talker/           # Simplified BSP publisher
│   │   ├── bsp-listener/         # Simplified BSP subscriber
│   │   └── rs-*/                 # Full Rust examples
│   ├── stm32f4/               # STM32F4 microcontrollers (uses bsp-stm32f4)
│   │   └── bsp-talker/           # Simplified BSP publisher
│   ├── zephyr/                # Zephyr RTOS (uses bsp-zephyr)
│   │   ├── c-talker/             # C BSP publisher
│   │   ├── c-listener/           # C BSP subscriber
│   │   └── rs-*/                 # Rust examples
│   └── platform-integration/  # Low-level reference implementations
│       ├── qemu-smoltcp-bridge/  # smoltcp bridge library
│       └── stm32f4-*/            # STM32F4 networking examples
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

# Message bindings
just generate-bindings      # Regenerate all generated/ dirs (uses bundled interfaces)
just clean-bindings         # Remove all generated/ dirs (including rcl-interfaces)
just regenerate-bindings    # clean-bindings + generate-bindings

# Test groups (by infrastructure requirement)
just test-unit          # Unit tests only (no external deps)
just test-miri          # Miri UB detection (nano-ros-serdes, nano-ros-core, nano-ros-params)
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
just setup   # Installs: rustup targets, cargo-nextest, cargo-nano-ros
             # Checks for: arm-none-eabi-gcc, qemu-system-arm, cmake
```

For missing system dependencies on Ubuntu:
```bash
sudo apt install gcc-arm-none-eabi qemu-system-arm cmake
```

## Environment Variables

Examples use `Context::from_env()` for configuration:

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `ZENOH_LOCATOR` | Router address (e.g., `tcp/192.168.1.1:7447`) | `tcp/127.0.0.1:7447` |
| `ZENOH_MODE` | Session mode: `client` or `peer` | `client` |

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
- **Reusable tests** belong in `crates/nano-ros-tests/tests/` (Rust integration tests) or `tests/` (shell scripts)
- **Temporary/exploratory tests** can be run directly in the Bash tool, but should be converted to proper test scripts once the feature is validated
- Test scripts in `tests/` should have justfile entries for easy invocation (e.g., `just test-ros2-interop-debug`)
- ROS 2 interop tests requiring `rmw_zenoh_cpp` go in `crates/nano-ros-tests/tests/rmw_interop.rs` or `tests/ros2-interop-debug.sh`

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

### Temporary Scripts
- Create temporary scripts in `$project/tmp/` directory (not `/tmp`)
- Use Write/Edit tools to create files (avoid cat + heredoc patterns)
- The `tmp/` directory is git-ignored and can be cleaned freely

### Writing Tests

**Integration tests** go in `crates/nano-ros-tests/tests/`. Each file is a test suite:
```
crates/nano-ros-tests/
├── src/              # Test utilities and fixtures
│   ├── lib.rs        # wait_for_pattern(), count_pattern(), etc.
│   ├── fixtures/     # rstest fixtures (ZenohRouter, binary builders)
│   ├── process.rs    # Managed child process helpers
│   ├── qemu.rs       # QEMU process management
│   ├── ros2.rs       # ROS 2 process helpers
│   └── zephyr.rs     # Zephyr native_sim helpers
└── tests/            # Integration test suites
    ├── nano2nano.rs  # nano-ros ↔ nano-ros pub/sub
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
- **zenoh-pico**: Submodule at `crates/zenoh-pico-shim-sys/zenoh-pico/` (1.6.2)
- **rmw_zenoh_cpp**: Bundles zenoh-c 1.6.2

Test infrastructure (`nano-ros-tests`) and shell scripts automatically use the local build at `build/zenohd/zenohd` when available, falling back to the system `zenohd`.

### Rust Edition 2024
All crates use Rust edition 2024. Key syntax changes from edition 2021:

- **Extern blocks require `unsafe`**: `unsafe extern "C" { ... }`
- **no_mangle requires `unsafe`**: `#[unsafe(no_mangle)]`
- **Unsafe fn bodies require explicit blocks**: Unsafe operations inside `unsafe fn` need `unsafe { ... }` blocks

The `nano-ros-c` crate keeps `#![allow(unsafe_op_in_unsafe_fn)]` because it's a pure C FFI wrapper with 420+ unsafe operations where adding explicit blocks would add verbosity without safety improvement.

### API Alignment

The nano-ros API follows established ROS 2 client library conventions:

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

**Example `generated/` directories are gitignored** and recreated by `just generate-bindings` (called automatically by `just build`). Only `crates/rcl-interfaces/generated/` is checked into git (workspace member — cargo requires member paths on disk).

**`.cargo/config.toml` is manually maintained** per example. Each contains `[patch.crates-io]` entries pointing to the local workspace crates, along with platform-specific `[build]` and `[target.*]` settings. The codegen tool does not touch these files.

**Bundled interfaces**: Standard .msg files (`std_msgs`, `builtin_interfaces`) are shipped at `colcon-nano-ros/interfaces/` so codegen works without a ROS 2 environment. The ament index takes precedence when available; bundled files fill gaps.

**heapless re-export**: `nano-ros-core` re-exports `heapless` (`pub use heapless;`) so generated code can reference `nano_ros_core::heapless::String<256>` etc. without requiring a separate `heapless` dependency.

**Inline codegen mode**: `rosidl-codegen` supports an inline mode (`NanoRosCodegenMode::Inline`) where generated code uses `nano_ros_core::` prefixed imports and `super::` relative paths for cross-package references. This is used for single-crate scenarios; the standard `cargo nano-ros generate-rust` (separate crates per package) remains the primary workflow.

**Installing cargo-nano-ros:**
```bash
# From the nano-ros repository root
just install-cargo-nano-ros

# Or manually:
cargo install --path colcon-nano-ros/packages/cargo-nano-ros --locked

# Or from git (external users):
cargo install --git https://github.com/jerry73204/nano-ros --path colcon-nano-ros/packages/cargo-nano-ros
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
C examples use `FindNanoRos.cmake` (at `cmake/FindNanoRos.cmake`) which wraps the internal `FindNanoRosC.cmake` (at `crates/nano-ros-c/cmake/`). Usage:
```cmake
list(APPEND CMAKE_MODULE_PATH "${NANO_ROS_ROOT}/cmake")
find_package(NanoRos REQUIRED)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
```
This provides include dirs, static library, and platform link libs (pthread, dl, m) automatically.

**C code generation** uses `nano_ros_generate_interfaces()` (from `nano_ros_generate_interfaces.cmake`). The codegen tool is bundled as `libnano_ros_codegen_c.a` — no external `nano-ros` binary needed. Build it with `just build-codegen-lib` before running CMake. The CMake module `FindNanoRosCodegen.cmake` compiles a thin C wrapper at configure time.

### Platform Backends
Selected via feature flags: `posix` (desktop), `zephyr` (Zephyr RTOS), `smoltcp` (bare-metal).

### Parameter Services
Enable with `param-services` feature in `nano-ros-node`:
```toml
nano-ros-node = { version = "*", features = ["param-services"] }
```
- Provides ROS 2 parameter service handlers (`~/get_parameters`, `~/set_parameters`, etc.)
- Uses generated `rcl_interfaces` types from `crates/rcl-interfaces/generated/`
- Handlers return `Box<Response>` due to large heapless arrays (~1MB per ParameterValue)

### ROS 2 Interop
Uses rmw_zenoh-compatible protocol. Key format for Humble:
- Data keyexpr: `<domain>/<topic>/<type>/TypeHashNotSupported`
- Liveliness: `@ros2_lv/.../<type>/RIHS01_<hash>/<qos>`

See [docs/reference/rmw_zenoh_interop.md](docs/reference/rmw_zenoh_interop.md).

## Development Phases

| Phase | Focus | Status |
|-------|-------|--------|
| 1 | CDR, types, proc macros | Complete |
| 2A | ROS 2 Interoperability | Complete |
| 2B | Zephyr integration | Complete |
| 3 | Services, parameters | Complete |
| 4 | Message generation | Complete |
| 5 | RTIC integration | Complete |
| 6 | Actions | Complete |
| 7 | API alignment (rclrs) | Complete |
| 8 | Embedded networking | Complete |
| 9 | Test infrastructure | Complete |
| 12 | QEMU bare-metal tests | Complete |
| 13 | Bare-metal API simplification | Complete |
| 14 | Platform BSP libraries | Planning |
| 16 | ROS 2 Interop Completion | In Progress |
| 17 | Full test coverage | Complete |
| 19 | Transport session configuration | Planning |
| 20 | Remaining work (TODO audit) | Planning |
| 21 | C API `no_std` backend | In Progress |
| 22 | ESP32-C3 platform support | Not Started |
| 23 | Arduino precompiled library | Not Started |
| 24 | RPi Pico W platform support | Not Started |
| 26 | Typed BSP API + example migration | Complete |
| 27 | Codegen automation | Complete |

**Phase 16 Status**: Core implementation complete (Rust API, C API, protocol). Parameter service registration wired into executor (C.2 complete). Remaining:
- Integration tests requiring ROS 2 environment
- Iron+ type hash support (future work)

See [docs/roadmap/](docs/roadmap/) for details.

### Distribution UX (Future)

Planned improvements for toolchain distribution:
- **crates.io publishing**: `cargo-nano-ros`, `nano-ros-core`, `nano-ros-serdes`, pre-generated standard message crates — eliminates `[patch.crates-io]`
- **Pre-built binaries**: GitHub releases for `nano-ros` binary
- **`cargo nano-ros init`**: Template scaffolding for new projects
- **C single-archive release**: library + headers + cmake modules + codegen binary

## Documentation Index

```
docs/
├── guides/          # Getting started, setup, how-to
├── reference/       # Protocol specs, comparisons, coverage
├── design/          # Architecture, real-time analysis
├── research/        # Autoware porting analysis
└── roadmap/         # Phase planning (phase-1 through phase-27)
```

Key docs: [getting-started](docs/guides/getting-started.md), [creating-examples](docs/guides/creating-examples.md), [message-generation](docs/guides/message-generation.md), [troubleshooting](docs/guides/troubleshooting.md), [rmw_zenoh interop](docs/reference/rmw_zenoh_interop.md), [codegen automation](docs/roadmap/phase-27-codegen-automation.md), [tests/README](tests/README.md).

## Quick Reference

### Manual Testing
```bash
# Build zenohd first (one-time)
just build-zenohd

# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native/rs-talker && RUST_LOG=info cargo run --features zenoh

# Terminal 3: Listener
cd examples/native/rs-listener && RUST_LOG=info cargo run --features zenoh
```

### ROS 2 Interop
```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nano-ros talker
cd examples/native/rs-talker && RUST_LOG=info cargo run --features zenoh

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
cd examples/native/rs-action-server && cargo run

# Terminal 3: Action client
cd examples/native/rs-action-client && cargo run
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
    --binary examples/qemu/rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker

# Terminal 3: Listener (192.0.2.11)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu/rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener
```

Run `just qemu-help` for more options.
