# nano-ros Integration Tests

Integration tests for nano-ros communication, platform backends, and ROS 2 interoperability.

## Overview

nano-ros uses a Rust-based test framework with rstest fixtures in `crates/nano-ros-tests/`. This provides:

- **Type safety** - Compile-time error checking
- **RAII cleanup** - Automatic process cleanup via `Drop` trait
- **Parallel execution** - Tests run concurrently with proper isolation
- **Cached builds** - Binary builds are cached across test runs
- **IDE support** - Full debugging and code navigation

## Running Tests

### Quick Start

```bash
# Unit tests only (no external deps)
just test-unit

# Standard tests (needs qemu-system-arm + zenohd)
just test

# All tests (needs Zephyr, ROS 2, cmake, etc.)
just test-all

# Run integration tests via cargo directly
cargo test -p nano-ros-tests --tests -- --nocapture
```

### Test Groups

```bash
just test-unit          # Unit tests + Miri (no external deps)
just test-qemu          # QEMU bare-metal tests (needs qemu-system-arm)
just test-integration   # All Rust integration tests (needs zenohd)
just test-zephyr        # Zephyr E2E tests (needs west + TAP network)
just test-ros2          # ROS 2 interop tests (needs ROS 2 + rmw_zenoh_cpp)
just test-c             # C API tests (needs cmake + zenohd)
just test-docker-qemu   # QEMU networked tests in Docker (needs docker)
```

## Directory Structure

```
tests/
├── README.md           # This file
├── c-tests.sh          # C API integration tests (CMake-based)
├── c-msg-gen-tests.sh  # C message generation tests
├── ros2-interop.sh     # ROS 2 interop tests (shell-based)
├── zephyr/             # Zephyr native_sim tests (shell-based)
│   └── run-c.sh        # Zephyr C examples test
└── simple-workspace/   # Standalone build verification

crates/nano-ros-tests/  # Rust test crate
├── Cargo.toml
├── src/
│   ├── lib.rs          # Test utilities (wait_for_pattern, count_pattern)
│   └── fixtures/
│       ├── mod.rs
│       ├── binaries.rs     # Binary build helpers (cached)
│       ├── qemu.rs         # QemuProcess fixture (RAII)
│       ├── ros2.rs         # ROS 2 process helpers
│       └── zenohd_fixture.rs # ZenohRouter fixture (RAII)
└── tests/
    ├── emulator.rs     # QEMU Cortex-M3 tests
    ├── nano2nano.rs    # nano-ros ↔ nano-ros tests
    ├── platform.rs     # Platform detection tests
    └── rmw_interop.rs  # ROS 2 interop tests
```

## Test Suites

### emulator
Tests on QEMU Cortex-M3 emulator:
- CDR serialization verification
- Node API tests
- Type metadata tests

**Requirements:** `qemu-system-arm`, `thumbv7m-none-eabi` target

### nano2nano
Tests communication between nano-ros nodes:
- Basic pub/sub with zenohd router
- Message delivery verification

**Requirements:** `zenohd` in PATH

### platform
Tests platform and toolchain detection:
- QEMU ARM availability
- ARM toolchain detection
- Embedded target availability
- Zephyr workspace detection

**Requirements:** None (detection tests)

### rmw_interop
Tests interoperability with ROS 2 using rmw_zenoh_cpp:

**Pub/Sub Tests:**
- nano-ros → ROS 2 communication
- ROS 2 → nano-ros communication
- Communication matrix (all directions)
- Key expression format verification

**Service Tests:**
- nano-ros server → ROS 2 client
- ROS 2 server → nano-ros client
- Service discovery

**Action Tests:**
- nano-ros action server ↔ ROS 2 action client
- ROS 2 action server ↔ nano-ros action client

**Discovery Tests:**
- `ros2 node list` shows nano-ros nodes
- `ros2 topic list` shows nano-ros topics
- `ros2 service list` shows nano-ros services

**QoS Tests:**
- BEST_EFFORT ↔ BEST_EFFORT (works)
- RELIABLE ↔ RELIABLE (works)
- RELIABLE → BEST_EFFORT (works)
- BEST_EFFORT → RELIABLE (expected to fail)

**Benchmark Tests:**
- First-message latency measurement
- Message throughput measurement

**Requirements:** `zenohd`, ROS 2 Humble, `rmw_zenoh_cpp`, `example_interfaces`

Tests gracefully skip when ROS 2 is not available.

### zephyr (shell-based)
Tests Zephyr native_sim integration:
- Zephyr talker → native subscriber
- TAP network communication

**Requirements:** West workspace, TAP network interface

```bash
# Setup (one time)
./zephyr/setup.sh
sudo ./scripts/setup-zephyr-network.sh

# Run tests
just test-zephyr
```

### c (shell-based)
Tests C API integration using CMake-built examples:
- C talker → C listener pub/sub communication
- Verifies nano-ros-c FFI bindings work correctly

**Requirements:** `cmake`, `zenohd`, Rust toolchain

```bash
# Run C tests
./tests/c-tests.sh

# Verbose output
./tests/c-tests.sh --verbose

# Skip rebuild (use existing binaries)
./tests/c-tests.sh --skip-build
```

## Requirements

### Basic Tests
- `zenohd` in PATH
- Rust toolchain with `thumbv7m-none-eabi` target

### ROS 2 Interop Tests
- ROS 2 Humble (or later)
- `rmw_zenoh_cpp` middleware

```bash
# Install rmw_zenoh_cpp
sudo apt install ros-humble-rmw-zenoh-cpp
```

### QEMU Tests
- `qemu-system-arm`
- ARM embedded toolchain

```bash
# Install QEMU
sudo apt install qemu-system-arm
```

## Writing New Tests

Create tests in `crates/nano-ros-tests/tests/`:

```rust
use nano_ros_tests::fixtures::{zenohd_unique, ZenohRouter};
use rstest::rstest;

#[rstest]
fn test_my_feature(zenohd_unique: ZenohRouter) {
    // zenohd is automatically started and cleaned up
    let locator = zenohd_unique.locator();

    // Your test logic here
}
```

### Available Fixtures

| Fixture | Description |
|---------|-------------|
| `zenohd_unique` | Starts zenohd on unique port, auto-cleanup |
| `build_native_talker()` | Builds and caches native-rs-talker binary |
| `build_native_listener()` | Builds and caches native-rs-listener binary |
| `QemuProcess::run()` | Runs QEMU with semihosting, auto-cleanup |
| `Ros2Process::topic_echo()` | Runs ros2 topic echo, auto-cleanup |
| `Ros2Process::topic_pub()` | Runs ros2 topic pub, auto-cleanup |

### Test Utilities

```rust
use nano_ros_tests::{wait_for_pattern, count_pattern};

// Wait for pattern in output
let found = wait_for_pattern(&output, "Received:", Duration::from_secs(10));

// Count occurrences
let count = count_pattern(&output, "data:");
```

## CI Integration

### GitHub Actions Example

To run ROS 2 interop tests in CI, create a job with ROS 2 + rmw_zenoh installed:

```yaml
ros2-interop-tests:
  runs-on: ubuntu-latest
  container:
    image: ros:humble
  steps:
    - uses: actions/checkout@v4

    - name: Install dependencies
      run: |
        apt-get update
        apt-get install -y ros-humble-rmw-zenoh-cpp ros-humble-example-interfaces
        cargo install zenoh --locked

    - name: Run interop tests (shell)
      run: |
        source /opt/ros/humble/setup.bash
        ./tests/ros2-interop.sh all

    - name: Run interop tests (Rust)
      run: |
        source /opt/ros/humble/setup.bash
        cargo test -p nano-ros-tests --test rmw_interop -- --nocapture
```

### Test Categories for CI

| Test Suite | Command | Requirements |
|------------|---------|--------------|
| Unit tests | `just test-unit` | None |
| QEMU tests | `just test-qemu` | qemu-system-arm |
| Integration tests | `just test-integration` | zenohd |
| Zephyr tests | `just test-zephyr` | west + TAP |
| ROS 2 interop | `just test-ros2` | ROS 2 + rmw_zenoh |
| C API tests | `just test-c` | cmake + zenohd |
| Docker QEMU | `just test-docker-qemu` | docker |

Tests that require ROS 2 will gracefully skip if prerequisites are not met.

## Troubleshooting

### Tests timeout
- Ensure no stale `zenohd` processes: `pkill -x zenohd`
- Check for orphan test processes: `pkill -f native-rs-talker`

### ROS 2 tests skip
- Source ROS 2: `source /opt/ros/humble/setup.bash`
- Verify rmw_zenoh: `ros2 pkg list | grep rmw_zenoh`

### QEMU tests fail
- Check QEMU installed: `qemu-system-arm --version`
- Check ARM target: `rustup target list | grep thumbv7m`

## Migration from Shell Scripts

The Rust test framework replaces the previous shell-based tests:

| Shell Script | Rust Equivalent |
|--------------|-----------------|
| `tests/emulator/` | `tests/emulator.rs` |
| `tests/nano2nano/` | `tests/nano2nano.rs` |
| `tests/platform/` | `tests/platform.rs` |
| `tests/rmw-interop/` | `tests/rmw_interop.rs` |
| `tests/rmw-detailed/` | `tests/rmw_interop.rs` |
| `tests/smoltcp/` | Unit tests in crate |
| `tests/common/` | `src/lib.rs` + `src/fixtures/` |

Benefits of Rust tests:
- Type-safe process management
- Automatic cleanup (no orphan processes)
- Better error messages with stack traces
- IDE debugging support
- Parallel test execution
