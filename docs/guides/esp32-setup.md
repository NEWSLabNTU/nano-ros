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

# Nightly (required for ESP32 examples — build-std)
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

### Build zenoh-pico for RISC-V

Required for networked nros examples (zenoh):

```bash
just build-zenoh-pico-riscv
```

Output: `build/esp32-zenoh-pico/libzenohpico.a`

## Project Structure

ESP32-C3 QEMU (OpenETH) support spans these directories:

```
packages/boards/
└── nros-board-esp32-qemu/       # QEMU BSP crate (OpenETH + smoltcp)

packages/drivers/
└── openeth-smoltcp/       # OpenCores Ethernet MAC driver for smoltcp

packages/zpico/
└── zpico-platform-esp32-qemu/ # QEMU FFI symbols

examples/qemu-esp32-baremetal/rust/
├── talker/                # QEMU publisher (nros-board-esp32-qemu BSP)
└── listener/              # QEMU subscriber (nros-board-esp32-qemu BSP)
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
- Add `"alloc"` to `build-std` if heap allocation is needed (networked examples)
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
- Must use zenohd from submodule (`just build-zenohd`) — system zenohd may be incompatible
- Each QEMU peer uses a separate TAP device (`tap-qemu0`, `tap-qemu1`)

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
