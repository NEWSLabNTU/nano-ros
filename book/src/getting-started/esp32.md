# ESP32 (esp-hal, bare-metal Rust)

Single-node starter on ESP32-C3 using the bare-metal `esp-hal` Rust
path — no ESP-IDF — running under the Espressif QEMU fork (OpenETH
ethernet). For the ESP-IDF component path (C / C++ apps), see
[ESP32 (ESP-IDF component)](./integration-esp-idf.md).

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
(`nros-board-esp32-qemu`) wraps the OpenETH / esp-hal init.

> **ESP32-S3 (Xtensa) is NOT supported today.** The tutorial targets
> the RISC-V ESP32-C3 only. Xtensa targets do not ship via `rustup`
> (they require the `espup` toolchain installer) and the in-tree
> board crate is RISC-V only; this gap is tracked separately.

```text
examples/qemu-esp32-baremetal/rust/talker/
├── Cargo.toml                 # deps + [package.metadata.nros.deploy.qemu-esp32-baremetal]
├── .cargo/config.toml         # target = riscv32imc-unknown-none-elf
├── package.xml
├── generated/                 # codegen output — build.rs runs
│                              #   `nros generate-rust` on first
│                              #   `cargo build`; gitignored.
└── src/                       # lib.rs component class + main.rs entry
```

## Configure

Deploy config lives in the app's `Cargo.toml` (baked at compile time;
the board's default `Config` supplies the remaining smoltcp knobs like
the MAC). The QEMU ESP32 board uses OpenETH ethernet via
`nros-board-esp32-qemu`. Verbatim from
[`examples/qemu-esp32-baremetal/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-esp32-baremetal/rust/talker/Cargo.toml):

```toml
[package.metadata.nros.deploy.qemu-esp32-baremetal]
rmw       = "zenoh"
domain_id = 0
ip        = "10.0.2.50"
gateway   = "10.0.2.2"
locator   = "tcp/10.0.2.2:7454"
```

## Build

```bash
# QEMU ESP32 (qemu-system-riscv32). `just esp32 build-qemu` (which
# `just esp32 talker` depends on) builds the QEMU-board variant; the
# example's build.rs invokes `nros generate-rust` automatically, so
# the `generated/` dir populates on first build (gitignored).
just esp32 build-qemu
```

## Run

```bash
# QEMU ESP32. First bring up zenohd on the esp32 fixture port (7454):
just esp32 zenohd &
# Then boot the talker binary in qemu-system-riscv32 (esp32c3):
just esp32 talker
# Expected serial output (per src/lib.rs):
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   ...

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

**Readiness signal.** QEMU ESP32: ~15 seconds **after** a warm cache —
the `just esp32 talker` recipe re-runs `build-qemu` every invocation,
so a first / cold run adds ~25 s of build time on top. If no
`Publishing:` line:

1. Wrong locator → talker logs `zenoh open failed` and retries.
   Confirm `zenohd` is reachable on the host IP (`10.0.2.2:7454`).
2. Confirm `.cargo/config.toml` target is
   `riscv32imc-unknown-none-elf` (ESP32-C3). The tutorial does not
   support ESP32-S3 (Xtensa) yet.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- QEMU ESP32 talker:
  [`examples/qemu-esp32-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-esp32-baremetal/rust/talker)
- Board crate:
  [`packages/boards/nros-board-esp32-qemu/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32-qemu)

## Next

- Subscriber + service + action peer directories under the same
  `examples/qemu-esp32-baremetal/rust/`.
- ESP-IDF component path for C / C++ apps:
  [ESP32 (ESP-IDF component)](./integration-esp-idf.md).
- ESP32-S3 (Xtensa) — not supported today. The Xtensa toolchain
  does not ship via `rustup` (it requires
  [`espup`](https://github.com/esp-rs/espup)), and there is no
  in-tree Xtensa board crate. Stick with ESP32-C3 (RISC-V) for now.
