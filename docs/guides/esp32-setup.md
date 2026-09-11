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

The rustup target and toolchain are the one thing `nros setup` does not do for
you — everything below it does.

### 2. RISC-V GCC Cross-Compiler

Required for building the zenoh-pico C library for RISC-V. Provisioned by
`nros setup esp32-c3-baremetal` into `${NROS_HOME:-~/.nros}/sdk`; you do not
hand-install a cross-compiler. (Historically this was
`sudo apt install gcc-riscv64-unknown-elf picolibc-riscv64-unknown-elf`; the
`picolibc` half supplies the bare-metal C headers `stdint.h`, `stdlib.h`, …)

### 3. Flashing Tool

`espflash` is provisioned by the same `nros setup esp32-c3-baremetal`, pinned by
the index, and put on PATH by `source ./activate.sh`. Do not
`cargo install espflash` — that resolves against crates.io at whatever version
is current that day, which is the drift the pinning convention exists to
prevent (issue 0486).

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

Provision the board. This is the one command: it fetches the riscv cross-gcc,
`espflash` and the SDK sources the RMW needs (zenoh-pico + mbedtls for zenoh)
into the shared store at `${NROS_HOME:-~/.nros}/sdk`.

```bash
nros setup esp32-c3-baremetal --rmw zenoh
```

Then, from the example directory:

```bash
cd examples/esp32-c3-baremetal/rust/talker
nros sync     # generated/ msg crates + build/<image>/nros-cargo.toml
nros build    # or: nros build esp32-c3-baremetal
```

`nros setup` does **not** provide a zenoh router. Since RFC-0075 that is ROS's
`rmw_zenoh_cpp/rmw_zenohd`; `just zenohd` (contributor) or
`ros2 run rmw_zenoh_cpp rmw_zenohd` starts it. A `--rmw cyclonedds` image needs
no daemon at all.

The book's user-facing walkthrough of the same flow, with the QEMU run step
spelled out, is [ESP32 (esp-hal, bare-metal
Rust)](../../book/src/getting-started/esp32.md).

> **Contributors:** provision with `just setup esp32-c3-baremetal` — the
> module recipes skip `_setup-common`, so the cross Rust targets, the pinned
> corrosion, the CLI and the resolver would all be missing. The in-tree build
> and test lanes are then `just esp32 build-qemu`, `just esp32 test-basic` and
> `just esp32 test`; `just --list esp32` is the current set.

## Project Structure

ESP32-C3 QEMU (OpenETH) support spans these directories:

```
packages/boards/
└── nros-board-esp32-qemu/       # QEMU BSP crate (OpenETH + smoltcp)

packages/drivers/
└── openeth-smoltcp/       # OpenCores Ethernet MAC driver for smoltcp

packages/rmw/zenoh/
└── zpico-platform-esp32-qemu/ # QEMU FFI symbols

examples/esp32-c3-baremetal/rust/
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

## Cargo configuration — you do not write one

An ESP32 example carries no `.cargo/config.toml`, and nothing about the riscv
target is stated in the example at all. The example states its board in
`system.toml` — verbatim from
`examples/esp32-c3-baremetal/rust/talker/system.toml`:

```toml
[system]
name = "esp32_qemu_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "esp32_qemu_talker"
class = "esp32_qemu_talker::Talker"
name = "talker"
entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]

[image.esp32-c3-baremetal]
board   = "esp32-c3-baremetal"
ip      = "10.0.2.50"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:9800"
```

The `entities` row is stated rather than probed, and this board is why: the
entity probe compiles a component for the **host**, and this leaf builds for a
foreign target with `build-std` against a board crate that has no host build
(RFC-0098 D8; retiring the workaround is issue 1265). It uses the same grammar
as `nano_ros_node_register(... ENTITIES ...)`, and `nros sync` cross-checks it
against the probe on any leaf where the probe does run.

Everything that choice implies lives in the board descriptor,
[`packages/boards/nros-board-esp32-qemu/nros-board.toml`](../../packages/boards/nros-board-esp32-qemu/nros-board.toml),
and `nros sync` composes it into `build/<image>/nros-cargo.toml`, which cargo
reads through `--config` (RFC-0098 D1/D4). Where each fact moved:

| was, in the leaf's `.cargo/config.toml` | now, in the board descriptor |
|---|---|
| `[build] target = "riscv32imc-unknown-none-elf"` | `cargo_config` `[build] target` |
| `[target.…] rustflags = ["-C", "link-arg=-Tlinkall.x", "-C", "force-frame-pointers"]` | `cargo_config` `[target.riscv32imc-unknown-none-elf] rustflags` — plus `-C link-arg=--gc-sections`, hoisted from the 2 of 4 leaves that carried it |
| `[unstable] build-std = ["core"]` | `cargo_config` `[unstable] build-std = ["core", "alloc"]` |
| `ESP_LOG` in the leaf's `[env]` | `cargo_config` `[env] ESP_LOG = "info"` |
| `NROS_SMOLTCP_MAX_UDP_SOCKETS=2` in the leaf's `[env]` | `[board.knobs.net] max_udp_sockets = 2` — a hardware budget, stated once (RFC-0049 board rung). The env front-end still wins over it |

Key points that did not move:

- `build-std` still requires nightly Rust with the `rust-src` component. The
  descriptor asks for `core` **and** `alloc`, because the networked examples
  allocate.
- `-Tlinkall.x` is the RISC-V linker script from `esp-riscv-rt`.
- There is **no cargo `runner`** for this board, so `cargo run` does not flash
  or boot anything. Flashing and QEMU both go through `espflash save-image`
  (below), which `nros setup esp32-c3-baremetal` provisions.

Switching the example to another board is editing `[image.*] board` and
re-running `nros sync` — **plus one more edit, today.** A single-package leaf is
its own entry, so it still names its board crate by hand in `[dependencies]`
(`nros-board-esp32-qemu = { version = "*" }` here). RFC-0098 D6 generates that
dependency for a workspace entry only, so a board change that touches just
`system.toml` makes `nros sync` report success and `nros build` fail inside the
leaf's own crate with `cannot find nros_board_<old> in the crate root`. Change
both, until
[issue 1305](../issues/1305-single-package-board-crate-dep-not-generated.md)
closes.

## QEMU ESP32-C3 Testing

Espressif's QEMU fork emulates ESP32-C3 with OpenCores Ethernet, enabling full E2E testing without physical hardware.

### Install Espressif QEMU

The `esp32c3` machine exists only in Espressif's fork — stock
`qemu-system-riscv32` knows only `virt`. It is source-built by the index:

```bash
nros setup --tool esp32-qemu
```

Provides `qemu-system-riscv32` with the `-M esp32c3` machine type.

### Build and pack a QEMU image

```bash
cd examples/esp32-c3-baremetal/rust/talker
nros sync
nros build esp32-c3-baremetal

# Pack the ELF into the flash image QEMU boots. `espflash` came from
# `nros setup esp32-c3-baremetal`; `activate.sh` puts it on PATH.
espflash save-image --chip esp32c3 --flash-size 4mb --merge \
    build/esp32-c3-baremetal/target/riscv32imc-unknown-none-elf/debug/esp32_qemu_talker \
    talker.bin
```

> **Contributors:** `just esp32 build-qemu` builds every ESP32 QEMU example and
> packs its flash image; `just esp32 test-basic` is the boot test.

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

Run the full test suite (contributor lane, from the checkout):

```bash
# Setup TAP network (one-time, requires sudo)
sudo ./scripts/qemu/setup-network.sh

# Run all ESP32-C3 QEMU tests
just esp32 test
```

Tests include boot verification, ESP32-to-ESP32 pub/sub, and ESP32-to-native interop.

### Key Notes

- Requires `espflash` for flash image creation (`espflash save-image --merge`)
- Uses `-icount 3` for instruction timing (simulates 125MHz)
- Use the router ROS ships (`just zenohd`, resolved by `nros_zenohd_bin`) — the vendored one was retired by RFC-0075
- Each QEMU peer uses a separate TAP device (`tap-qemu0`, `tap-qemu1`)

## Troubleshooting

### `has not been synced — missing the resolved model` / `generated/<pkg>` missing

`nros build` refuses before cargo sees anything, naming `nros sync`. Run it in
the example directory; it writes both the generated message crates and
`build/<image>/nros-cargo.toml` (RFC-0098 D2). Re-run it after editing
`package.xml` or `system.toml`.

### `error: no matching package found` for esp-hal

The board asks for nightly (`toolchain = "nightly"` in its descriptor). If you
are driving cargo yourself rather than using `nros build`, name it:

```bash
cd examples/esp32-c3-baremetal/rust    # the directory ABOVE the package
cargo +nightly build --manifest-path talker/Cargo.toml \
    --config talker/build/esp32-c3-baremetal/nros-cargo.toml
```

Run cargo from the parent directory: until phase-445 W6 deletes the leaf's
`.cargo/`, cargo would otherwise read it a second time and join the board's
`rustflags` onto themselves. After W6 the working directory stops mattering.

### `error[E0463]: can't find crate for core`

The board's `build-std` requires nightly and `rust-src`:
```bash
rustup component add --toolchain nightly rust-src
rustup target add riscv32imc-unknown-none-elf --toolchain nightly
```

### `Permission denied` when flashing

Add the udev rules listed above. There is no cargo `runner` for this board, so
`cargo run` will not flash: invoke `espflash` on the built ELF yourself
(`espflash --help` for the current flags).

### zenoh-pico build fails with `stdint.h: No such file or directory`

Install the picolibc C library headers:
```bash
sudo apt install picolibc-riscv64-unknown-elf
```
