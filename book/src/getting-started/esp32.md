# ESP32 (esp-hal, bare-metal Rust)

Single-node starter on ESP32-C3 using the bare-metal `esp-hal` Rust
path — no ESP-IDF — running under the Espressif QEMU fork (OpenETH
ethernet). For the ESP-IDF component path (C / C++ apps), see
[ESP32 (ESP-IDF component)](./integration-esp-idf.md).

> **Prereqs.** `nros setup qemu-esp32-baremetal` prepares the build: the
> riscv cross-gcc and `espflash` from a pinned index into the shared
> store at `~/.nros/sdk` — you do not hand-install cross-compilers.
> Running under QEMU additionally needs Espressif's QEMU fork
> (`nros setup --tool esp32-qemu`, source-built) and — for zenoh — a
> ROS 2 install to provide the router.

> **Time budget.** This chapter is the longest embedded setup in the
> book (~a dozen steps, two source builds, a nightly toolchain): plan
> an afternoon, not ten minutes. If you have not had a first win yet,
> take the [First Project](first-project.md) host flow first — it is
> the ten-minute one — and come back.

## Setup

Build the in-tree `nros` CLI (Phase 218):

```bash
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish
```

Provision the board (and RMW):

```bash
nros setup qemu-esp32-baremetal --rmw zenoh     # --rmw defaults to zenoh; xrce | cyclonedds also valid
```

This pulls the SDK sources nano-ros owns (zenoh-pico + mbedtls
submodules for zenoh; analogous for xrce / cyclonedds) plus the riscv
cross-gcc and `espflash` into `${NROS_HOME:-~/.nros}/sdk`. It does NOT
provide a zenoh router — that is ROS 2's `rmw_zenohd`, so the zenoh Run
step below needs a ROS install (`--rmw cyclonedds` needs no daemon at
all). `esp-hal` itself is a Cargo dependency the
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
├── .cargo/                    # config.toml + nros-board.toml
│                              # (target = riscv32imc-unknown-none-elf lives in nros-board.toml)
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
locator   = "tcp/10.0.2.2:9800"
```

## Build

> **Contributors:** the in-tree fixture/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#esp32).

Copy the example out, generate bindings, build with the pinned nightly,
and pack the flash image:

```bash
# once per checkout location — bindings + the [patch.crates-io] table:
NROS_REPO_DIR=<path-to-nano-ros> nros sync

# nightly because the board config builds core/alloc from source
# ([unstable] build-std in .cargo/nros-board.toml). The pinned channel
# is tools/rust-toolchain.toml's; any recent nightly with the rust-src
# component works:
#   rustup toolchain install nightly && rustup component add rust-src --toolchain nightly
#   rustup target add riscv32imc-unknown-none-elf --toolchain nightly
cargo +nightly build --release

# pack the ELF into the flash image QEMU boots (espflash comes from
# `nros setup qemu-esp32-baremetal`, on PATH via activate):
espflash save-image --chip esp32c3 --flash-size 4mb --merge \
    target/riscv32imc-unknown-none-elf/release/esp32_qemu_talker talker.bin
```

First build cross-compiles core/alloc + every dep (~5 min); rebuilds are
seconds. If you build a C-flavored variant, the riscv cross-gcc the zpico
shim needs is also provisioned by the same `nros setup`.

## Run

The `esp32c3` machine exists only in **Espressif's QEMU fork** — stock
`qemu-system-riscv32` knows only `virt`. The fork is source-built by:

```bash
nros setup --tool esp32-qemu    # clones + builds espressif/qemu (needs
                                # libglib2-dev libpixman-dev libgcrypt-dev;
                                # the command names them if absent)
```

```bash
# 1. Bring up the router (ROS's `rmw_zenohd`) on the port the example
#    dials (9800 — the deploy `locator` above):
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:9800"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. Boot the flash image (the file packed in Build above):
qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
    -drive file=talker.bin,if=mtd,format=raw \
    -nic user,model=open_eth
# Serial output:
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
```

> **Contributors:** the in-tree run lane that boots the talker in QEMU is in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#esp32).

```bash
# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

**Readiness signal.** QEMU ESP32: ~15 seconds after boot.

If no `Publishing:` line:

1. Wrong locator → talker logs `zenoh open failed` and retries.
   Confirm the router is reachable on the host IP (`10.0.2.2:9800`).
2. Confirm `.cargo/nros-board.toml` sets `target =
   "riscv32imc-unknown-none-elf"` (ESP32-C3). The tutorial does not
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
