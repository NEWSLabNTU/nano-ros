# nros Integration Tests

Integration tests for nros communication, platform backends, and ROS 2 interoperability.

## Overview

nros uses a Rust-based test framework with rstest fixtures in `packages/testing/nano-ros-tests/`. This provides:

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
just test-qemu-esp32    # ESP32-C3 QEMU tests (needs qemu-system-riscv32 + espflash)
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
├── c-msg-gen-tests.sh  # C message generation tests
├── ros2-interop.sh     # ROS 2 interop tests (shell-based)
├── zephyr/             # Zephyr native_sim tests (shell-based)
│   └── run-c.sh        # Zephyr C examples test
└── simple-workspace/   # Standalone build verification

packages/testing/nano-ros-tests/  # Rust test crate
├── Cargo.toml
├── src/
│   ├── lib.rs          # Test utilities (wait_for_pattern, count_pattern)
│   ├── esp32.rs        # ESP32-C3 QEMU helpers (guard functions, flash, launch)
│   └── fixtures/
│       ├── mod.rs
│       ├── binaries.rs     # Binary build helpers (cached)
│       ├── qemu.rs         # QemuProcess fixture (RAII)
│       ├── ros2.rs         # ROS 2 process helpers
│       └── zenohd_fixture.rs # ZenohRouter fixture (RAII)
└── tests/
    ├── emulator.rs         # QEMU Cortex-M3 tests (ARM)
    ├── esp32_emulator.rs   # QEMU ESP32-C3 tests (RISC-V)
    ├── nano2nano.rs        # nros ↔ nros tests
    ├── platform.rs         # Platform detection tests
    └── rmw_interop.rs      # ROS 2 interop tests
```

## Test Suites

### emulator
Tests on QEMU Cortex-M3 emulator:
- CDR serialization verification
- Node API tests
- Type metadata tests

**Requirements:** `qemu-system-arm`, `thumbv7m-none-eabi` target

### esp32_emulator
Tests on QEMU ESP32-C3 emulator (Espressif fork):
- Build verification (nightly toolchain + zenoh-pico RISC-V)
- Boot test (BSP banner via UART)
- Networked E2E (talker → listener via zenohd + TAP)

**Requirements:** `qemu-system-riscv32` (Espressif fork), `espflash`, nightly toolchain, `riscv32imc-unknown-none-elf` target, zenoh-pico RISC-V library

For networked tests: TAP interfaces (`sudo ./scripts/qemu/setup-network.sh`), zenohd

```bash
just test-qemu-esp32    # Run all ESP32 QEMU tests
```

### nano2nano
Tests communication between nros nodes:
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
- nros → ROS 2 communication
- ROS 2 → nros communication
- Communication matrix (all directions)
- Key expression format verification

**Service Tests:**
- nros server → ROS 2 client
- ROS 2 server → nros client
- Service discovery

**Action Tests:**
- nros action server ↔ ROS 2 action client
- ROS 2 action server ↔ nros action client

**Discovery Tests:**
- `ros2 node list` shows nros nodes
- `ros2 topic list` shows nros topics
- `ros2 service list` shows nros services

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

### c_api (Rust-managed)
Tests C API integration using CMake-built examples:
- C talker and C listener build verification
- C talker and C listener startup/initialization
- C talker → C listener pub/sub communication

**Requirements:** `cmake`, `zenohd`, Rust toolchain

```bash
just test-c             # Run all C tests
just test-c verbose     # Verbose output
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

Create tests in `packages/testing/nano-ros-tests/tests/`:

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
| `ZenohRouter::start(port)` | Starts zenohd on fixed port, auto-cleanup |
| `build_native_talker()` | Builds and caches native-rs-talker binary |
| `build_native_listener()` | Builds and caches native-rs-listener binary |
| `build_esp32_qemu_talker()` | Builds and caches ESP32 QEMU talker (nightly) |
| `build_esp32_qemu_listener()` | Builds and caches ESP32 QEMU listener (nightly) |
| `QemuProcess::run()` | Runs QEMU ARM with semihosting, auto-cleanup |
| `start_esp32_qemu()` | Starts QEMU ESP32-C3 instance, auto-cleanup |
| `Ros2Process::topic_echo()` | Runs ros2 topic echo, auto-cleanup |
| `Ros2Process::topic_pub()` | Runs ros2 topic pub, auto-cleanup |

### QEMU Networked Test Practices

When writing QEMU tests that involve network communication (pub/sub via zenohd + TAP), follow these rules to avoid flaky tests:

**1. Each QEMU peer must use a different TAP device.**

Never share a TAP device between two QEMU instances. Each peer gets its own TAP interface on the bridge:

```
Talker:   tap-qemu0, MAC 02:00:00:00:00:01, IP 192.0.3.10
Listener: tap-qemu1, MAC 02:00:00:00:00:02, IP 192.0.3.11
Bridge:   qemu-br, IP 192.0.3.1 (zenohd listens here)
```

This applies to all QEMU platforms (ARM MPS2-AN385 and ESP32-C3).

**2. Start the subscriber first, then the publisher.**

The subscriber must be running and have registered its subscription with zenohd before the publisher starts sending. Otherwise messages are lost because zenoh doesn't buffer for unknown subscribers.

**3. Add stabilization delay between subscription and publish.**

After the subscriber reports it's connected and subscribed, wait 5 seconds before starting the publisher. This gives zenohd time to propagate the subscription to other sessions.

**4. Verify zenohd is reachable on the bridge IP, not just localhost.**

QEMU instances connect to zenohd via the bridge IP (e.g., `192.0.3.1:7447`), not `127.0.0.1`. Always verify reachability on the bridge IP:

```rust
assert!(wait_for_addr("192.0.3.1:7447", Duration::from_secs(5)));
```

**5. Wait for port to be free before starting zenohd on a fixed port.**

If firmware hardcodes a zenoh locator (e.g., `tcp/192.0.3.1:7447`), the port is fixed. Check that no prior zenohd is still holding the port:

```rust
assert!(wait_for_port_free(7447, Duration::from_secs(10)));
```

**6. Use nextest test groups with `max-threads = 1` for port-sharing tests.**

Tests that share a fixed port (like 7447) must run sequentially. Configure this in `.config/nextest.toml`:

```toml
[test-groups.esp32-emulator]
max-threads = 1

[[profile.default.overrides]]
filter = "binary(esp32_emulator)"
test-group = "esp32-emulator"
```

**Example E2E test ordering:**

```
1. Verify port 7447 is free
2. Start zenohd on 0.0.0.0:7447
3. Verify zenohd reachable on bridge IP 192.0.3.1:7447
4. Start listener on tap-qemu1, wait for "Waiting for messages..."
5. Sleep 5s (subscription propagation)
6. Start talker on tap-qemu0, wait for "Done publishing..."
7. Verify listener received messages
```

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
| QEMU ARM tests | `just test-qemu` | qemu-system-arm |
| QEMU ESP32 tests | `just test-qemu-esp32` | qemu-system-riscv32 + espflash + TAP |
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

### QEMU ARM tests fail
- Check QEMU installed: `qemu-system-arm --version`
- Check ARM target: `rustup target list | grep thumbv7m`

### ESP32 QEMU tests fail
- Check Espressif QEMU: `qemu-system-riscv32 --version`
- Check RISC-V target: `rustup +nightly target list --installed | grep riscv32imc`
- Check espflash: `espflash --version`
- Check zenoh-pico RISC-V: `ls build/esp32-zenoh-pico/libzenohpico.a`
- Check TAP interfaces: `ip link show tap-qemu0 && ip link show tap-qemu1`
- If port 7447 is busy: `pkill -x zenohd` and wait for TIME_WAIT to expire

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
| `tests/c-tests.sh` | `tests/c_api.rs` |

Benefits of Rust tests:
- Type-safe process management
- Automatic cleanup (no orphan processes)
- Better error messages with stack traces
- IDE debugging support
- Parallel test execution
