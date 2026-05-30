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
├── nros.toml                  # [node] + [[transport kind="wifi"]]
├── package.xml
├── generated/
└── src/main.rs                # esp-hal init → nros_app_main
```

The `Cargo.toml` pulls `nros-board-esp32` (real hardware) or
`nros-board-esp32-qemu` (QEMU ESP32 / ESP32-C3 fork).

## Configure

`nros.toml` carries wifi creds + locator inside a single
`[[transport]] kind = "wifi"` block. Direct-mode schema; the
locator rides the transport.

```toml
[node]
domain_id = 0

[[transport]]
kind     = "wifi"
ssid     = "your-wifi-ssid"
password = "your-wifi-password"
locator  = "tcp/192.168.1.100:7447"      # host running zenohd

# Optional static IP (omit for DHCP). Same schema as the ethernet
# transport — CIDR `ip`, gateway:
# ip      = "192.168.1.50/24"
# gateway = "192.168.1.1"
```

Wi-Fi credentials can also be baked at build time via `SSID=…
PASSWORD=…` environment variables read by
`scripts/build/fixtures-build.sh` — preferred for real networks
since `nros.toml` ships in git history. The env vars are
build-time only; they don't override the file at runtime.

For QEMU ESP32 (no real wifi) the example tree at
`examples/qemu-esp32-baremetal/` uses the loopback path via
`nros-board-esp32-qemu`.

## Build

```bash
# Real hardware (ESP32-C3):
cd examples/esp32/rust/talker
cargo build --release

# QEMU ESP32-C3 (RISC-V) — use the canonical builder, NOT a bare
# `cargo build` from the workspace root:
just esp32 build-examples
```

## Run

```bash
# Real hardware (ESP32-C3):
cd examples/esp32/rust/talker
cargo run --release        # invokes `espflash flash --monitor`
# Expected serial output:
#   ESP32-C3 booting...
#   Wifi connected: 192.168.1.42
#   Published: 0
#   Published: 1

# QEMU ESP32-C3 — start zenohd FIRST (port 7454), THEN the talker:
just esp32 zenohd          # router on 127.0.0.1:7454
just esp32 talker          # boots the talker in qemu-system-riscv32

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** Real hardware: after `espflash flash --monitor`,
expect the Wi-Fi connect line + `Published: 0` within 10 seconds.
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
- ESP32-S3 (Xtensa) is **not** currently supported. Xtensa targets
  aren't shipped via rustup (there is no `rustup target add
  xtensa-esp32s3-none-elf` — Xtensa requires Espressif's `espup`
  toolchain on top of a custom `+esp` channel, plus a different
  board crate). Only the ESP32-C3 (RISC-V) path is wired today;
  the workspace `just/esp32.just` comment even calls this out.
