# Serial Transport

This guide covers using serial (UART) transport with nros on embedded targets. Serial transport connects an MCU directly to a zenoh router over UART, without needing an IP stack.

## Overview

nros supports two transport mechanisms for connecting embedded devices to a zenoh network:

| Transport | Crate | Use Case |
|-----------|-------|----------|
| TCP/UDP (Ethernet/WiFi) | `zpico-smoltcp` | MCUs with Ethernet MAC or WiFi radio |
| Serial (UART) | `zpico-serial` | MCUs with only UART, or point-to-point links |

Serial transport uses zenoh-pico's built-in COBS framing protocol over UART. No IP stack is required — the MCU sends and receives zenoh frames directly over a serial link to a host running the zenoh router (`rmw_zenohd`).

### When to Use Serial

- **Small MCUs** — Cortex-M0/M0+ with no Ethernet MAC and insufficient RAM for smoltcp
- **Point-to-point** — Direct UART connection to a host, no network infrastructure needed
- **Debugging** — Serial output is visible in any terminal, easy to inspect
- **Mixed topology** — Some nodes on Ethernet, others on serial, all bridged through the router

### Architecture

```
┌──────────┐     UART      ┌──────────────┐     TCP      ┌──────────┐
│   MCU    │───────────────│  rmw_zenohd  │─────────────│  ROS 2   │
│  (nros)  │  COBS frames  │  (router)    │  zenoh net   │  node    │
└──────────┘               └──────────────┘              └──────────┘
```

The MCU connects to the zenoh router using a `serial/...` locator. The router bridges serial-connected nodes to the rest of the zenoh network (including ROS 2 nodes using rmw_zenoh).

## Platform Support

Serial transport support varies by platform:

| Platform | Serial Implementation | Extra Crate Needed |
|----------|----------------------|--------------------|
| Bare-metal (MPS2-AN385) | `zpico-serial` + UART driver | Yes |
| ESP32-QEMU (esp-hal, no IDF) | `zpico-serial` path, same as bare-metal | Yes |
| Zephyr | zenoh-pico built-in (`uart_poll_in/out`) | No |
| FreeRTOS / NuttX | zenoh-pico built-in (POSIX `/dev/ttyXXX`) | No |
| ThreadX | zenoh-pico built-in (HAL DMA) | No |

On non-bare-metal platforms, serial just works by using a `serial/...` locator — no extra crates needed. `zpico-serial` only fills the gap for bare-metal targets where custom FFI symbols replace zenoh-pico's system layer.

## Board Crate Feature Selection

Each board crate uses Cargo features to select the transport:

```toml
# Board crates are registry-style names resolved by the `nros sync`
# patch table (see examples/README.md), not path deps:

# Use serial transport (disable default ethernet/wifi)
nros-board-mps2-an385 = { version = "*", default-features = false, features = ["serial"] }

# Use ethernet transport (default)
nros-board-mps2-an385 = { version = "*" }

# Both transports (runtime selection via locator string)
nros-board-mps2-an385 = { version = "*", features = ["serial"] }
```

### Available Features by Board

| Board Crate | Default | Alternative | Both |
|-------------|---------|-------------|------|
| `nros-board-mps2-an385` | `ethernet` | `serial` | `ethernet,serial` |
| `nros-board-esp32-qemu` | `ethernet` | `serial` | `ethernet,serial` |

When both features are enabled, the transport is selected at runtime by the zenoh locator string in `Config`:
- `"tcp/192.0.3.1:7448"` → Ethernet/WiFi
- `"serial/UART_0#baudrate=115200"` → Serial

## Quick Start: QEMU Serial Example

This mirrors what the in-tree serial e2e test
(`packages/testing/nros-tests/tests/emulator.rs`) actually does: a
**socat PTY pair** links QEMU's UART to the router, so both ends exist
before either side starts.

### 1. Build the Serial Talker

```bash
nros sync                       # materialize generated/ + the patch table
cd examples/qemu-arm-baremetal/rust/serial-talker
cargo build --release
```

### 2. Create the PTY pair and start the router

```bash
# Two linked PTYs; QEMU gets one end, the router the other:
socat -d -d pty,raw,echo=0,link=/tmp/nros-serial-qemu \
            pty,raw,echo=0,link=/tmp/nros-serial-router &

# Router LISTENS on its end of the pair:
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["serial//tmp/nros-serial-router#baudrate=115200"]' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &
```

> The router is ROS 2's `rmw_zenohd` (phase-362 retired the vendored
> `zenohd`). It takes **no** command-line configuration — argv is not
> parsed, so a `--connect` flag is not rejected, it is simply unread.
> Configuration travels in `ZENOH_CONFIG_OVERRIDE`, and the router
> **listens** on the serial endpoint; the MCU side dials it.

### 3. Boot QEMU with UART0 on the other end

The leaf's default `cargo run` runner uses semihosting only (no serial
device) — boot QEMU explicitly, wiring UART0 to the pair:

```bash
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 \
    -display none -monitor none \
    -icount shift=auto -semihosting-config enable=on,target=native \
    -chardev serial,id=ser0,path=/tmp/nros-serial-qemu \
    -serial chardev:ser0 \
    -kernel target/thumbv7m-none-eabi/release/serial-talker
```

(`-display none -monitor none`, not `-nographic` — `-nographic`
implies `-serial mon:stdio`, which hijacks UART0 for the monitor.)

### 4. Subscribe from Host

```bash
# From a ROS 2 node (the talker publishes best-effort; force the QoS
# or a RELIABLE echo silently receives nothing):
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

## Configuration

### Serial Config

Board crates provide a `serial_default()` constructor:

```rust
use nros_board_mps2_an385::{Config, run};

let config = Config::serial_default();
// Defaults: baudrate=115200, locator="serial/UART_0#baudrate=115200"

run(config, |config| {
    let exec_config = ExecutorConfig::new(config.zenoh_locator)
        .domain_id(config.domain_id);
    let mut executor = Executor::open(&exec_config)?;
    // ...
    Ok(())
})
```

### Custom Baud Rate

```rust
let config = Config::serial_default()
    .with_baudrate(921600)
    .with_locator("serial/UART_0#baudrate=921600");
```

### Locator Format

Serial locators follow zenoh-pico convention:

| Format | Example | Use Case |
|--------|---------|----------|
| `serial/<dev>#baudrate=<baud>` | `serial/UART_0#baudrate=115200` | Device name (Zephyr, ESP-IDF, bare-metal) |
| `serial/<tx>.<rx>#baudrate=<baud>` | `serial/0.1#baudrate=115200` | Pin numbers (Arduino) |

## QEMU PTY Testing

### How It Works

QEMU redirects the emulated UART to a host pseudo-terminal. The in-tree flow uses a socat PTY *pair* (rather than QEMU's `-serial pty`) so both endpoints exist before either process starts — the router listens on one end, QEMU's UART0 is wired to the other, enabling full end-to-end testing without physical hardware.

```
┌──────────────────────────────────────────────────────────┐
│                       Host                               │
│  ┌─────────┐    ┌───────────┐    ┌────────────────────┐  │
│  │rmw_zenohd◄──►│ socat PTY │◄──►│ QEMU MPS2-AN385   │  │
│  │ (listen)│    │   pair    │    │ -serial chardev:…   │  │
│  └────┬────┘    └───────────┘    │ UART0 ──► firmware  │  │
│       │                         └────────────────────┘  │
│       │ zenoh network                                    │
│  ┌────▼────┐                                             │
│  │  ROS 2  │                                             │
│  │  echo   │                                             │
│  └─────────┘                                             │
└──────────────────────────────────────────────────────────┘
```

### QEMU Flags

The serial example's `.cargo/config.toml` uses:

```toml
[target.thumbv7m-none-eabi]
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -semihosting-config enable=on,target=native -serial pty -kernel"
```

Key flags:
- `-serial pty` — Expose UART0 as a host PTY
- `-nographic` — No display window
- `-semihosting-config enable=on,target=native` — Debug output via semihosting (separate from UART)
- No `-netdev` / `-net` — Serial transport doesn't need Ethernet

### `-icount shift=auto`

For reliable serial communication, add `-icount shift=auto` to synchronize QEMU's virtual clock with wall-clock time. Without this, QEMU runs the CPU at full speed, which can cause timing-sensitive serial handshakes to fail:

```toml
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -semihosting-config enable=on,target=native -icount shift=auto -serial pty -kernel"
```

### Automated Testing

The integration test `test_qemu_serial_pubsub_e2e` in `packages/testing/nros-tests/tests/emulator.rs` automates the full workflow:

1. Build the serial-talker example (prebuilt fixture)
2. Create socat PTY pairs (both ends exist before either process)
3. Start the router with `listen/endpoints=["serial/<pty>#baudrate=115200"]`
4. Boot QEMU via `start_mps2_an385_with_serial` (UART0 → the pair;
   `-display none -monitor none`)
5. Subscribe and verify message delivery

> **Contributors:** the in-tree lane that runs this test is in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#serial-transport-qemu).

## Baud Rate Tuning

### Recommended Rates

| Baud Rate | Use Case |
|-----------|----------|
| 115200 | Default, safe for all hardware |
| 460800 | Higher throughput, most USB-serial adapters |
| 921600 | Maximum for many MCU UARTs |

Higher baud rates increase throughput but may cause framing errors on noisy or long cables. QEMU ignores the baud rate (infinite speed), so rate tuning only matters on physical hardware.

### Buffer Sizing

zenoh-pico serial uses a 1500-byte MTU with COBS framing. The maximum wire frame is 1516 bytes. `zpico-serial` uses a 2048-byte RX ring buffer per port, which accommodates one full frame plus overhead.

For high-throughput scenarios, ensure the MCU's UART FIFO is drained frequently by calling `executor.spin_once()` in a tight loop.

## Troubleshooting

### "Session open failed" or Handshake Timeout

The zenoh serial handshake (Init → Ack) must complete within zenoh-pico's timeout. Common causes:

- **Wrong PTY path** — Check the router's listen endpoint names the same PTY link your QEMU `-chardev` uses
- **Baud rate mismatch** — MCU and router must use the same baud rate
- **QEMU timing** — Add `-icount shift=auto` to slow down QEMU's CPU clock

### No Messages Received

- **Locator mismatch** — Ensure the MCU's `zenoh_locator` matches what zenohd expects
- **Domain ID** — Both sides must use the same ROS 2 domain ID
- **zenohd not bridging** — Verify zenohd is connected to the serial port and also listening on TCP for subscribers

### UART Pin Conflicts

On physical hardware, ensure the UART TX/RX pins aren't shared with the debug console. Some boards use UART0 for debug output — use a different UART for zenoh transport, or disable debug prints.

### ESP32 Serial

ESP32 uses zenoh-pico's built-in ESP-IDF serial implementation. No `zpico-serial` dependency is needed. Select serial transport in the board crate:

```toml
nros-board-esp32-qemu = { path = "...", default-features = false, features = ["serial"] }
```

The default locator is `serial/UART_0#baudrate=115200`. ESP32's USB-JTAG-Serial peripheral or UART0/UART1 can be used.
