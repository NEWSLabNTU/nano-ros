# ESP32-C3 Development Setup

Guide for setting up ESP32-C3 development with nros.

## Hardware

| Board | Chip | Arch | WiFi | Price |
|-------|------|------|------|-------|
| ESP32-C3-DevKitC | ESP32-C3 | RISC-V (RV32IMC) | 802.11 b/g/n | ~$8 |
| ESP32-C6-DevKitC | ESP32-C6 | RISC-V (RV32IMAC) | WiFi 6 | ~$12 |

ESP32-C3 is the primary target. It uses upstream Rust (no forked compiler).

## Prerequisites

### 1. Rust Toolchains

```bash
# Stable (for workspace builds)
rustup target add riscv32imc-unknown-none-elf

# Nightly (required for ESP32 examples -- build-std)
rustup toolchain install nightly
rustup component add --toolchain nightly rust-src
```

Or use `just setup` which installs both automatically.

### 2. RISC-V GCC Cross-Compiler

Required for building zenoh-pico C library for RISC-V.

```bash
# Ubuntu/Debian
sudo apt install gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf
```

The `picolibc` package provides C standard library headers (`stdint.h`, `stdlib.h`, etc.) for bare-metal RISC-V targets.

### 3. Flashing Tool

```bash
cargo install espflash --locked
```

Also installed by `just setup`.

### 4. USB Permissions (Linux)

ESP32-C3 dev boards use USB-UART bridges. Add a udev rule to avoid needing `sudo` for flashing:

```bash
sudo tee /etc/udev/rules.d/99-esp32.rules << 'EOF'
# CP210x (Silicon Labs USB-UART)
SUBSYSTEMS=="usb", ATTRS{idVendor}=="10c4", ATTRS{idProduct}=="ea60", MODE="0666"
# CH340/CH341
SUBSYSTEMS=="usb", ATTRS{idVendor}=="1a86", ATTRS{idProduct}=="7523", MODE="0666"
# FTDI
SUBSYSTEMS=="usb", ATTRS{idVendor}=="0403", MODE="0666"
# ESP32-C3 built-in USB-JTAG
SUBSYSTEMS=="usb", ATTRS{idVendor}=="303a", ATTRS{idProduct}=="1001", MODE="0666"
EOF
sudo udevadm control --reload-rules
sudo udevadm trigger
```

## Quick Start

### Build Hello World

```bash
cd examples/esp32/hello-world
cargo +nightly build --release
```

### Flash to Device

Connect the ESP32-C3 board via USB, then:

```bash
cd examples/esp32/hello-world
cargo +nightly run --release
```

This builds, flashes, and opens the serial monitor (`espflash flash --monitor` is configured as the cargo runner).

### Build zenoh-pico for RISC-V

Required for networked nros examples (WiFi + zenoh):

```bash
just build-zenoh-pico-riscv
```

Output: `build/esp32-zenoh-pico/libzenohpico.a`

## Project Structure

ESP32 support spans three directories:

```
packages/boards/
├── nros-esp32/            # WiFi BSP crate (esp-radio + smoltcp)
└── nros-esp32-qemu/       # QEMU BSP crate (OpenETH + smoltcp, no WiFi deps)

packages/drivers/
└── openeth-smoltcp/       # OpenCores Ethernet MAC driver for smoltcp

packages/zpico/
├── zpico-platform-esp32/      # WiFi FFI symbols (z_random, z_clock, etc.)
└── zpico-platform-esp32-qemu/ # QEMU FFI symbols

examples/esp32/rust/
├── zenoh/
│   ├── talker/            # WiFi publisher (nros-esp32 BSP)
│   └── listener/          # WiFi subscriber (nros-esp32 BSP)
└── standalone/
    └── hello-world/       # Minimal UART print (no nros deps)

examples/qemu-esp32-baremetal/rust/zenoh/
├── talker/                # QEMU publisher (nros-esp32-qemu BSP)
└── listener/              # QEMU subscriber (nros-esp32-qemu BSP)
```

All ESP32 examples are standalone packages (excluded from the workspace) because they require nightly + `build-std`.

## ESP-HAL Crate Versions

These are the crate versions used for ESP32-C3 support (pinned with `~` to avoid breaking updates):

| Crate | Version | Purpose |
|-------|---------|---------|
| `esp-hal` | ~1.0.0 | Hardware Abstraction Layer |
| `esp-backtrace` | ~0.18.0 | Panic handler + backtrace |
| `esp-println` | ~0.16.0 | UART print output |
| `esp-bootloader-esp-idf` | ~0.4.0 | ESP-IDF bootloader compatibility |

All crates require the `esp32c3` feature flag. The `unstable` feature on `esp-hal` is needed for `delay` and other commonly-used modules.

## Cargo Configuration

Each ESP32 example needs `.cargo/config.toml`:

```toml
[target.riscv32imc-unknown-none-elf]
runner = "espflash flash --monitor"
rustflags = ["-C", "link-arg=-Tlinkall.x", "-C", "force-frame-pointers"]

[build]
target = "riscv32imc-unknown-none-elf"

[unstable]
build-std = ["core"]
```

Key points:
- `build-std = ["core"]` requires nightly Rust
- Add `"alloc"` to `build-std` if heap allocation is needed (WiFi examples)
- `-Tlinkall.x` is the RISC-V linker script from `esp-riscv-rt`

## QEMU ESP32-C3 Testing

Espressif's QEMU fork emulates ESP32-C3 with OpenCores Ethernet, enabling full E2E testing without physical hardware.

### Install Espressif QEMU

```bash
just setup                                    # Includes QEMU check
./scripts/esp32/install-espressif-qemu.sh     # Or install manually
```

Provides `qemu-system-riscv32` with the `-M esp32c3` machine type.

### Build and Run QEMU Examples

```bash
# Cross-compile zenoh-pico for RISC-V (one-time)
just build-zenoh-pico-riscv

# Build QEMU examples + create flash images
just build-examples-esp32-qemu

# Boot test (no networking)
just test-qemu-esp32-basic
```

### Networked E2E Tests

The QEMU tests use TAP networking to connect ESP32-C3 instances through zenohd:

```
┌──────────────────┐         ┌─────────┐         ┌──────────────────┐
│ QEMU ESP32-C3    │  TAP    │ zenohd  │  TAP    │ QEMU ESP32-C3    │
│  talker          │◄───────►│ (host)  │◄───────►│  listener        │
│  192.0.3.10      │  eth    │192.0.3.1│  eth    │  192.0.3.11      │
│ OpenETH + smoltcp│         │         │         │ OpenETH + smoltcp│
└──────────────────┘         └─────────┘         └──────────────────┘
```

Run the full test suite:

```bash
# Setup TAP network (one-time, requires sudo)
sudo ./scripts/qemu/setup-network.sh

# Run all ESP32-C3 QEMU tests (builds zenohd automatically)
just test-qemu-esp32
```

Tests include boot verification, ESP32-to-ESP32 pub/sub, and ESP32-to-native interop.

### Key Notes

- Requires `espflash` for flash image creation (`espflash save-image --merge`)
- Uses `-icount 3` for instruction timing (simulates 125MHz)
- Must use zenohd 1.6.2 from submodule (`just build-zenohd`) -- system zenohd may be incompatible
- Each QEMU peer uses a separate TAP device (`tap-qemu0`, `tap-qemu1`)

## WiFi BSP Examples

The WiFi examples use the `nros-esp32` BSP crate, which handles WiFi initialization, DHCP, and zenoh session setup.

### Build

WiFi credentials are passed as environment variables:

```bash
SSID=MyNetwork PASSWORD=secret just build-examples-esp32
```

### Flash and Monitor

Connect the ESP32-C3 board via USB, then flash:

```bash
cd examples/esp32/rust/zenoh/talker
espflash flash --monitor target/riscv32imc-unknown-none-elf/release/esp32-bsp-talker
```

### BSP API

The `nros-esp32` crate provides `run_node()` for a minimal setup:

```rust
use nros_esp32::prelude::*;

run_node(
    WifiConfig::new("MyNetwork", "password123"),
    |node| {
        let publisher = node.create_publisher("demo/esp32")?;
        loop {
            node.spin_once(1000);
            publisher.publish(&data)?;
        }
    },
)
```

For advanced configuration (static IP, custom zenoh locator):

```rust
run_node_with_config(
    WifiConfig::new("MyNetwork", "password123"),
    NodeConfig::new()
        .zenoh_locator("tcp/192.168.1.1:7447")
        .node_name("esp32_sensor")
        .ip_mode(IpMode::Dhcp),
    |node| { /* ... */ },
)
```

See `packages/boards/nros-esp32/` for full API documentation.

### Requirements

- Physical ESP32-C3 board (WiFi testing is not available in QEMU)
- WiFi network reachable by both ESP32 and the zenohd router host
- zenohd running on a host the ESP32 can reach over WiFi

## Troubleshooting

### `error: no matching package found` for esp-hal

Ensure you're using nightly: `cargo +nightly build --release`

### `error[E0463]: can't find crate for core`

The `build-std` config requires nightly and `rust-src`:
```bash
rustup component add --toolchain nightly rust-src
```

### `Permission denied` when flashing

Add the udev rules listed above, or use `sudo` temporarily:
```bash
sudo cargo +nightly run --release
```

### zenoh-pico build fails with `stdint.h: No such file or directory`

Install the picolibc C library headers:
```bash
sudo apt install picolibc-riscv64-unknown-elf
```
