# ESP32 (esp-hal, bare-metal Rust)

Single-node starter on ESP32-C3 / ESP32-S3 using the bare-metal
`esp-hal` Rust path — no ESP-IDF. For the ESP-IDF component path
(C / C++ apps), see [ESP32 (ESP-IDF
component)](./integration-esp-idf.md).

> **Prereqs.** `nros setup esp32` is the single command that
> prepares your machine. It fetches a prebuilt esp-hal toolchain and
> the chosen RMW host daemon from a pinned index into the shared
> store at `~/.nros/sdk` — you do not hand-install cross-compilers,
> and you do not need ROS 2 installed.

## Setup

Install the `nros` CLI once per machine:

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
export PATH="$HOME/.nros/bin:$PATH"
```

Provision the board (and RMW):

```bash
nros setup esp32 --rmw zenoh     # --rmw defaults to zenoh; xrce | cyclonedds also valid
```

This pulls the prebuilt esp-hal toolchain, the SDK sources, and the
RMW host daemon (`zenohd` for zenoh, the Micro-XRCE-DDS agent for
xrce) into `~/.nros/sdk`.

## Project layout

Each example is a standalone Cargo package targeting
`riscv32imc-unknown-none-elf` (ESP32-C3) or `xtensa-esp32s3-none-elf`
(ESP32-S3). The board crate (`nros-board-esp32` or
`nros-board-esp32-qemu`) wraps the wifi / esp-hal init.

```text
examples/esp32/rust/talker/
├── Cargo.toml
├── .cargo/config.toml         # target = riscv32imc-unknown-none-elf
│                              # runner = espflash flash --monitor
├── config.toml                # wifi credentials + zenoh locator
├── package.xml
├── generated/
└── src/main.rs                # esp-hal init → nros_app_main
```

The `Cargo.toml` pulls `nros-board-esp32` (real hardware) or
`nros-board-esp32-qemu` (QEMU ESP32 / ESP32-C3 fork).

## Configure

`config.toml` carries wifi + zenoh:

```toml
[wifi]
ssid     = "your-wifi-ssid"
password = "your-wifi-password"

[zenoh]
locator   = "tcp/192.168.1.100:7447"     # host running zenohd
domain_id = 0

# Optional static IP (commented out — defaults to DHCP):
# [network]
# ip      = "10.0.0.100"
# gateway = "10.0.0.1"
# prefix  = 24
```

Wi-Fi credentials can also be supplied via `SSID=… PASSWORD=…`
environment variables on the build command — preferred for real
networks since the file ships in git history.

For QEMU ESP32 (no real wifi) the example tree at
`examples/qemu-esp32-baremetal/` uses the loopback path via
`nros-board-esp32-qemu`.

## Build

```bash
# Real hardware (ESP32-C3):
cd examples/esp32/rust/talker
cargo build --release

# QEMU ESP32 (qemu-system-xtensa or qemu-system-riscv32):
just esp32 build           # builds for the QEMU board crate
```

## Run

```bash
# Real hardware:
cd examples/esp32/rust/talker
cargo run --release        # invokes `espflash flash --monitor`
# Expected serial output:
#   ESP32-C3 booting...
#   Wifi connected: 192.168.1.42
#   Published: 1
#   Published: 2

# QEMU ESP32:
just esp32 talker          # boots the talker binary in qemu-system-xtensa

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** Real hardware: after `espflash flash --monitor`,
expect the Wi-Fi connect line + `Published: 1` within 10 seconds.
QEMU ESP32: ~15 seconds. If no `Published:` line:

1. Wi-Fi credentials wrong → `Wifi connect failed` in serial log.
2. Wrong locator → talker logs `zenoh open failed` and retries.
   Confirm `zenohd` is reachable on the host IP from the board's
   subnet.
3. ESP32-C3 vs ESP32-S3: confirm `.cargo/config.toml` target matches
   your chip (`riscv32imc-unknown-none-elf` vs `xtensa-esp32s3-none-elf`).
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- esp-hal Rust:
  [`examples/esp32/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/esp32/rust/talker)
- QEMU ESP32 talker:
  [`examples/qemu-esp32-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-esp32-baremetal/rust/talker)
- Board crates:
  [`packages/boards/nros-board-esp32/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32),
  [`packages/boards/nros-board-esp32-qemu/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32-qemu)

## Next

- Subscriber + service + action peer directories under the same
  `examples/esp32/rust/`.
- ESP-IDF component path for C / C++ apps:
  [ESP32 (ESP-IDF component)](./integration-esp-idf.md).
- PlatformIO library path:
  [PlatformIO library](./integration-platformio.md).
- ESP32-S3 (Xtensa) — same code shape; the toolchain swap is
  `rustup target add xtensa-esp32s3-none-elf` and a different board
  crate.
