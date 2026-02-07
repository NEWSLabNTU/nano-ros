# ESP32-C3 Development Setup

Guide for setting up ESP32-C3 development with nano-ros.

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

Required for networked nano-ros examples (WiFi + zenoh):

```bash
just build-zenoh-pico-riscv
```

Output: `build/esp32-zenoh-pico/libzenohpico.a`

## Project Structure

ESP32 examples live in `examples/esp32/`:

```
examples/esp32/
├── hello-world/           # Minimal blink + UART print
│   ├── .cargo/config.toml # RISC-V target + espflash runner
│   ├── Cargo.toml         # esp-hal 1.0 dependencies
│   └── src/main.rs        # Entry point
└── (future)
    ├── bsp-talker/        # WiFi publisher (Phase 22.4)
    └── bsp-listener/      # WiFi subscriber (Phase 22.4)
```

Each example is a standalone package (excluded from the workspace) because it requires nightly + `build-std`.

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
