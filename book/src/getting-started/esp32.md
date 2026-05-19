# ESP32 (esp-hal, bare-metal Rust)

Single-node starter on ESP32-C3 / ESP32-S3 using the bare-metal
`esp-hal` Rust path — no ESP-IDF. For the ESP-IDF component path
(C / C++ apps), see [ESP32 (ESP-IDF
component)](./integration-esp-idf.md).

> **Prereqs.** Clone with `just setup tier=default` already run.
> `espflash` on `PATH` (`cargo install espflash`). For ESP32-C3 the
> `riscv32imc-unknown-none-elf` Rust target is the toolchain;
> `just esp32 setup` pulls it.

## Project layout

Each example is a standalone Cargo package targeting
`riscv32imc-unknown-none-elf` (ESP32-C3) or `xtensa-esp32s3-none-elf`
(ESP32-S3). The board crate (`nros-board-esp32` or
`nros-board-esp32-qemu`) wraps the wifi / esp-hal init.

```text
examples/esp32/rust/zenoh/talker/
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
[network]
ssid     = "your-wifi-ssid"
password = "your-wifi-password"

[zenoh]
locator   = "tcp/192.168.1.100:7447"     # host running zenohd
domain_id = 0
```

For QEMU ESP32 (no real wifi) the board crate falls back to the
host loopback via TAP.

## Build

```bash
# Real hardware (ESP32-C3):
just esp32 setup           # rustup target add riscv32imc-unknown-none-elf
cd examples/esp32/rust/zenoh/talker
cargo build --release

# QEMU ESP32 (qemu-system-xtensa or qemu-system-riscv32):
just esp32 build           # builds for the QEMU board crate
```

## Run

```bash
# Real hardware:
cd examples/esp32/rust/zenoh/talker
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
  [`examples/esp32/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/esp32/rust/zenoh/talker)
- QEMU ESP32 talker:
  [`examples/esp32-qemu/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/esp32-qemu/rust/zenoh/talker)
- Board crates:
  [`packages/boards/nros-board-esp32/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32),
  [`packages/boards/nros-board-esp32-qemu/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32-qemu)

## Next

- Subscriber + service + action peer directories under the same
  `examples/esp32/rust/zenoh/`.
- ESP-IDF component path for C / C++ apps:
  [ESP32 (ESP-IDF component)](./integration-esp-idf.md).
- PlatformIO library path:
  [PlatformIO library](./integration-platformio.md).
- ESP32-S3 (Xtensa) — same code shape; the toolchain swap is
  `rustup target add xtensa-esp32s3-none-elf` and a different board
  crate.
