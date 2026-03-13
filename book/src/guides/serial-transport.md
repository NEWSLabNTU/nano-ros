# Serial Transport

This guide covers using serial (UART) transport with nros on embedded targets. Serial transport connects an MCU directly to a zenoh router over UART, without needing an IP stack.

## Overview

nros supports two transport mechanisms for connecting embedded devices to a zenoh network:

| Transport | Crate | Use Case |
|-----------|-------|----------|
| TCP/UDP (Ethernet/WiFi) | `zpico-smoltcp` | MCUs with Ethernet MAC or WiFi radio |
| Serial (UART) | `zpico-serial` | MCUs with only UART, or point-to-point links |

Serial transport uses zenoh-pico's built-in COBS framing protocol over UART. No IP stack is required — the MCU sends and receives zenoh frames directly over a serial link to a host running zenohd.

### When to Use Serial

- **Small MCUs** — Cortex-M0/M0+ with no Ethernet MAC and insufficient RAM for smoltcp
- **Point-to-point** — Direct UART connection to a host, no network infrastructure needed
- **Debugging** — Serial output is visible in any terminal, easy to inspect
- **Mixed topology** — Some nodes on Ethernet, others on serial, all bridged through zenohd

### Architecture

```
┌──────────┐     UART      ┌──────────────┐     TCP      ┌──────────┐
│   MCU    │───────────────│   zenohd     │─────────────│  ROS 2   │
│  (nros)  │  COBS frames  │  (router)    │  zenoh net   │  node    │
└──────────┘               └──────────────┘              └──────────┘
```

The MCU connects to zenohd using a `serial/...` locator. zenohd bridges serial-connected nodes to the rest of the zenoh network (including ROS 2 nodes using rmw_zenoh).

## Platform Support

Serial transport support varies by platform:

| Platform | Serial Implementation | Extra Crate Needed |
|----------|----------------------|--------------------|
| Bare-metal (MPS2-AN385, STM32F4) | `zpico-serial` + UART driver | Yes |
| ESP32 / ESP32-QEMU | zenoh-pico built-in (ESP-IDF serial) | No |
| Zephyr | zenoh-pico built-in (`uart_poll_in/out`) | No |
| FreeRTOS / NuttX | zenoh-pico built-in (POSIX `/dev/ttyXXX`) | No |
| ThreadX | zenoh-pico built-in (HAL DMA) | No |

On non-bare-metal platforms, serial just works by using a `serial/...` locator — no extra crates needed. `zpico-serial` only fills the gap for bare-metal targets where custom FFI symbols replace zenoh-pico's system layer.

## Board Crate Feature Selection

Each board crate uses Cargo features to select the transport:

```toml
# Use serial transport (disable default ethernet/wifi)
nros-mps2-an385 = { path = "...", default-features = false, features = ["serial"] }

# Use ethernet transport (default)
nros-mps2-an385 = { path = "..." }

# Both transports (runtime selection via locator string)
nros-mps2-an385 = { path = "...", features = ["serial"] }
```

### Available Features by Board

| Board Crate | Default | Alternative | Both |
|-------------|---------|-------------|------|
| `nros-mps2-an385` | `ethernet` | `serial` | `ethernet,serial` |
| `nros-stm32f4` | `ethernet` | `serial` | `ethernet,serial` |
| `nros-esp32` | `wifi` | `serial` | `wifi,serial` |
| `nros-esp32-qemu` | `ethernet` | `serial` | `ethernet,serial` |

When both features are enabled, the transport is selected at runtime by the zenoh locator string in `Config`:
- `"tcp/192.0.3.1:7448"` → Ethernet/WiFi
- `"serial/UART_0#baudrate=115200"` → Serial

## Quick Start: QEMU Serial Example

### 1. Build the Serial Talker

```bash
cd examples/qemu-arm-baremetal/rust/zenoh/serial-talker
cargo nano-ros generate
cargo build --release
```

### 2. Run in QEMU

```bash
cargo run --release
```

QEMU starts with `-serial pty` and prints the PTY path:

```
char device redirected to /dev/pts/3 (label serial0)
```

### 3. Connect zenohd

In another terminal, start zenohd with the serial link:

```bash
zenohd --connect serial//dev/pts/3#baudrate=115200
```

The MCU's messages are now bridged to the zenoh network. Any zenoh subscriber (including ROS 2 nodes) can receive them.

### 4. Subscribe from Host

```bash
# Using zenoh CLI
z_sub -k "0/chatter/**"

# Or from a ROS 2 node
ros2 topic echo /chatter std_msgs/msg/Int32
```

## Configuration

### Serial Config

Board crates provide a `serial_default()` constructor:

```rust
use nros_mps2_an385::{Config, run};

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
    .with_zenoh_locator("serial/UART_0#baudrate=921600");
```

### Locator Format

Serial locators follow zenoh-pico convention:

| Format | Example | Use Case |
|--------|---------|----------|
| `serial/<dev>#baudrate=<baud>` | `serial/UART_0#baudrate=115200` | Device name (Zephyr, ESP-IDF, bare-metal) |
| `serial/<tx>.<rx>#baudrate=<baud>` | `serial/0.1#baudrate=115200` | Pin numbers (Arduino) |

## QEMU PTY Testing

### How It Works

QEMU's `-serial pty` flag redirects the emulated UART to a host pseudo-terminal (PTY). This creates a virtual serial port that zenohd can connect to, enabling full end-to-end testing without physical hardware.

```
┌──────────────────────────────────────────────────────────┐
│                       Host                               │
│  ┌─────────┐    ┌───────────┐    ┌────────────────────┐  │
│  │ zenohd  │◄──►│ /dev/pts/N│◄──►│ QEMU MPS2-AN385   │  │
│  │         │    │  (PTY)    │    │ -serial pty         │  │
│  └────┬────┘    └───────────┘    │ UART0 ──► firmware  │  │
│       │                         └────────────────────┘  │
│       │ zenoh network                                    │
│  ┌────▼────┐                                             │
│  │ z_sub   │                                             │
│  │ or ROS 2│                                             │
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

1. Build the serial-talker example
2. Launch QEMU with `-serial pty`
3. Parse the PTY path from QEMU stderr
4. Start zenohd with `--connect serial//dev/pts/N#baudrate=115200`
5. Subscribe and verify message delivery

Run it with:

```bash
just test-qemu
```

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

- **Wrong PTY path** — Check that zenohd connects to the correct `/dev/pts/N`
- **Baud rate mismatch** — MCU and zenohd must use the same baud rate
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
nros-esp32 = { path = "...", default-features = false, features = ["serial"] }
```

The default locator is `serial/UART_0#baudrate=115200`. ESP32's USB-JTAG-Serial peripheral or UART0/UART1 can be used.
