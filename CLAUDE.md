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
│   │   └── generated/         # cargo nano-ros generate output
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
just build          # Build with no_std
just build-zenohd   # Build zenohd 1.6.2 from submodule (for integration tests)
just check          # Format + clippy
just quality        # Format + clippy + unit tests (no external deps)
just doc            # Generate docs

# Test groups (by infrastructure requirement)
just test-unit          # Unit tests + Miri (no external deps)
just test-qemu          # QEMU bare-metal tests (needs qemu-system-arm)
just test-integration   # All Rust integration tests (builds zenohd automatically)
just test               # test-unit + test-qemu + test-integration
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
just test-integration   # All integration tests (needs zenohd)
just test-zephyr        # Zephyr E2E (needs west + TAP)
just test-ros2          # ROS 2 interop (needs ROS 2)
just test-c             # C API tests (needs cmake)
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
Generated per-project using `cargo nano-ros generate` from `package.xml`. See [docs/message-generation.md](docs/message-generation.md).

**Installing cargo-nano-ros:**
```bash
# From the nano-ros repository root
just install-cargo-nano-ros

# Or manually:
cargo install --path colcon-nano-ros/packages/cargo-nano-ros --locked
```

**Regenerating bindings in examples (requires ROS 2 environment):**
```bash
source /opt/ros/humble/setup.bash
just generate-bindings
```

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

See [docs/rmw_zenoh_interop.md](docs/rmw_zenoh_interop.md).

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

**Phase 16 Status**: Core implementation complete (Rust API, C API, protocol). Parameter service registration wired into executor (C.2 complete). Remaining:
- Integration tests requiring ROS 2 environment
- Iron+ type hash support (future work)

See [docs/roadmap/](docs/roadmap/) for details.

## Documentation Index

| Topic | Location |
|-------|----------|
| Testing | [tests/README.md](tests/README.md) |
| Test coverage | [docs/test-coverage.md](docs/test-coverage.md) |
| Troubleshooting | [docs/troubleshooting.md](docs/troubleshooting.md) |
| Message generation | [docs/message-generation.md](docs/message-generation.md) |
| Zephyr setup | [docs/zephyr-setup.md](docs/zephyr-setup.md) |
| ROS 2 interop protocol | [docs/rmw_zenoh_interop.md](docs/rmw_zenoh_interop.md) |
| Embedded integration | [docs/embedded-integration.md](docs/embedded-integration.md) |
| RTIC design | [docs/rtic-integration-design.md](docs/rtic-integration-design.md) |
| Memory requirements | [docs/memory-requirements.md](docs/memory-requirements.md) |
| WCET analysis | [docs/wcet-analysis.md](docs/wcet-analysis.md) |
| Schedulability | [docs/schedulability-analysis.md](docs/schedulability-analysis.md) |
| Real-time lints | [docs/realtime-lint-guide.md](docs/realtime-lint-guide.md) |
| Actions API | [docs/roadmap/phase-6-actions.md](docs/roadmap/phase-6-actions.md) |
| ROS 2 Interop (Phase 16) | [docs/roadmap/phase-16-ros2-interop-completion.md](docs/roadmap/phase-16-ros2-interop-completion.md) |
| QEMU/physical devices | [docs/qemu-physical-device-compatibility.md](docs/qemu-physical-device-compatibility.md) |
| Phase roadmaps | [docs/roadmap/](docs/roadmap/) |

## Quick Reference

### Manual Testing
```bash
# Build zenohd first (one-time)
just build-zenohd

# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: Talker
cd examples/native-rs-talker && cargo run

# Terminal 3: Listener
cd examples/native-rs-listener && cargo run
```

### ROS 2 Interop
```bash
# Terminal 1: Router
./build/zenohd/zenohd --listen tcp/127.0.0.1:7447

# Terminal 2: nano-ros talker
cd examples/native-rs-talker && cargo run

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

See [docs/roadmap/phase-6-actions.md](docs/roadmap/phase-6-actions.md) for API details.

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

See [docs/zephyr-setup.md](docs/zephyr-setup.md) for details.

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

See [docs/qemu-physical-device-compatibility.md](docs/qemu-physical-device-compatibility.md) for QEMU/physical device analysis.

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
    --binary examples/qemu-rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker

# Terminal 3: Listener (192.0.2.11)
./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 \
    --binary examples/qemu-rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener
```

Run `just qemu-help` for more options.
