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

Build the in-tree `nros` CLI (Phase 218):

```bash
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros
```

Provision the board (and RMW):

```bash
nros setup esp32 --rmw zenoh     # --rmw defaults to zenoh; xrce | cyclonedds also valid
```

This pulls the SDK sources nano-ros owns (zenoh-pico + mbedtls
submodules for zenoh; analogous for xrce / cyclonedds) and lands the
RMW host daemon (`zenohd` for zenoh, the Micro-XRCE-DDS agent for
xrce) under `${NROS_HOME:-~/.nros}/sdk` (the activate file puts the
in-tree CLI on PATH; legacy `${NROS_HOME:-~/.nros}/bin/` install
remains supported transitionally). `esp-hal` itself is a Cargo dependency the
example pulls in at build time, not a separately-installed toolchain;
the only cross-toolchain you may need to add by hand is the rustup
target — once per host:

```bash
rustup target add riscv32imc-unknown-none-elf      # ESP32-C3
```

## Project layout

Each example is a standalone Cargo package targeting
`riscv32imc-unknown-none-elf` (ESP32-C3). The board crate
(`nros-board-esp32` or `nros-board-esp32-qemu`) wraps the wifi /
esp-hal init.

> **ESP32-S3 (Xtensa) is NOT supported today.** The tutorial targets
> the RISC-V ESP32-C3 only. Xtensa targets do not ship via `rustup`
> (they require the `espup` toolchain installer) and the in-tree
> board crate is RISC-V only; this gap is tracked separately.

```text
examples/esp32/rust/talker/
├── Cargo.toml
├── .cargo/config.toml         # target = riscv32imc-unknown-none-elf
│                              # runner = espflash flash --monitor
├── nros.toml                  # wifi credentials + zenoh locator
├── package.xml
├── generated/                 # codegen output — build.rs runs
│                              #   `nros generate-rust` on first
│                              #   `cargo build`; gitignored.
└── src/main.rs                # esp-hal init → nros_app_main
```

The `Cargo.toml` pulls `nros-board-esp32` (real hardware) or
`nros-board-esp32-qemu` (QEMU ESP32 / ESP32-C3 fork).

## Configure

`nros.toml` carries the transport stack. Verbatim from the in-tree
[`examples/esp32/rust/talker/nros.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/esp32/rust/talker/nros.toml):

```toml
# nano-ros config (direct mode). See
# docs/design/0004-configuration-and-transports.md.

[node]
domain_id = 0

[[transport]]
kind    = "wifi"
ssid     = "MyNetwork"
password = "secret"
locator = "tcp/192.168.1.1:7447"
```

Wi-Fi credentials can also be supplied via `SSID=… PASSWORD=…`
environment variables on the build command — preferred for real
networks since the file ships in git history.

For QEMU ESP32 (no real wifi) the example tree at
`examples/qemu-esp32-baremetal/` uses an ethernet transport via
`nros-board-esp32-qemu`. Verbatim from
[`examples/qemu-esp32-baremetal/rust/talker/nros.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-esp32-baremetal/rust/talker/nros.toml):

```toml
# nano-ros config (direct mode). See
# docs/design/0004-configuration-and-transports.md.

[node]
domain_id = 0

[[transport]]
kind    = "ethernet"
ip      = "10.0.2.50/24"
mac     = "02:00:00:00:00:01"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7454"
```

## Build

```bash
# Real hardware (ESP32-C3) — the example's build.rs invokes
# `nros generate-rust` automatically, so the `generated/` dir
# populates on first build (gitignored, not pre-committed):
cd examples/esp32/rust/talker
cargo build --release

# QEMU ESP32 (qemu-system-riscv32). `just esp32 build` builds the
# real-hardware example fixtures; `just esp32 build-qemu` (which
# `just esp32 talker` depends on) is the QEMU-board variant:
just esp32 build-qemu
```

## Run

```bash
# QEMU ESP32. First bring up zenohd on the esp32 fixture port (7454):
just esp32 zenohd &
# Then boot the talker binary in qemu-system-riscv32 (esp32c3):
just esp32 talker
# Expected serial output (per src/main.rs):
#   Declaring publisher on /chatter (std_msgs/Int32)
#   Publisher declared
#   Published: 0
#   Published: 1
#   ...

# Real hardware. Make sure a `zenohd` on the host is reachable from
# the Wi-Fi locator in `nros.toml`, then:
cd examples/esp32/rust/talker
cargo run --release        # invokes `espflash flash --monitor`
# Expected serial output:
#   ESP32-C3 booting...
#   Wifi connected: 192.168.1.42
#   Published: 0
#   Published: 1
#   ...

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

**Readiness signal.** Real hardware: after `espflash flash --monitor`,
expect the Wi-Fi connect line + `Published: 0` within 10 seconds.
QEMU ESP32: ~15 seconds **after** a warm cache — the `just esp32
talker` recipe re-runs `build-qemu` every invocation, so a first /
cold run adds ~25 s of build time on top. If no `Published:` line:

1. Wi-Fi credentials wrong → `Wifi connect failed` in serial log.
2. Wrong locator → talker logs `zenoh open failed` and retries.
   Confirm `zenohd` is reachable on the host IP from the board's
   subnet.
3. Confirm `.cargo/config.toml` target is
   `riscv32imc-unknown-none-elf` (ESP32-C3). The tutorial does not
   support ESP32-S3 (Xtensa) yet.
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
- ESP32-S3 (Xtensa) — not supported today. The Xtensa toolchain
  does not ship via `rustup` (it requires
  [`espup`](https://github.com/esp-rs/espup)), and there is no
  in-tree Xtensa board crate. Stick with ESP32-C3 (RISC-V) for now.
