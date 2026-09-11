# ESP32 (esp-hal, bare-metal Rust)

Single-node starter on ESP32-C3 using the bare-metal `esp-hal` Rust
path — no ESP-IDF — running under the Espressif QEMU fork (OpenETH
ethernet). For the ESP-IDF component path (C / C++ apps), see
[ESP32 (ESP-IDF component)](./integration-esp-idf.md).

> **Prereqs.** `nros setup esp32-c3-baremetal` prepares the build: the
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
nros setup esp32-c3-baremetal --rmw zenoh     # --rmw defaults to zenoh; xrce | cyclonedds also valid
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
examples/esp32-c3-baremetal/rust/talker/
├── system.toml                # WHAT this deploys to — the one file you edit
├── Cargo.toml                 # Rust deps only; nothing here names a board
├── package.xml
├── generated/                 # generated message bindings (gitignored)
├── build/                     # everything `nros sync` generates (gitignored)
└── src/                       # lib.rs component class + main.rs entry
```

There is no `.cargo/` to edit. The `riscv32imc-unknown-none-elf` triple, the
`-Tlinkall.x` link group, the `[unstable] build-std` that compiles `core` and
`alloc` from source, `ESP_LOG`, the DRAM budgets and the `[patch.crates-io]`
rows all come from the board, and `nros sync` writes them into
`build/esp32-c3-baremetal/nros-cargo.toml` — a generated file the build reads
with `--config` (RFC-0098).

## Configure

The whole deployment statement is `system.toml`, beside the manifest, and it is
baked at compile time; the board's default `Config` supplies the remaining
smoltcp knobs like the MAC. The QEMU ESP32 board uses OpenETH ethernet via
`nros-board-esp32-qemu`. Verbatim from
[`examples/esp32-c3-baremetal/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/esp32-c3-baremetal/rust/talker/system.toml):

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
board = "esp32-c3-baremetal"
ip = "10.0.2.50"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:9800"
```

The `entities` line is this board's one extra obligation. Pool sizes are
normally *probed* from a host build of the component, and this leaf has no
host build — a foreign triple, `build-std`, and a board crate that only exists
for the target. So the image states what the node creates, and `nros sync`
sizes the pools from that (RFC-0098 D8). On `.bss`-tight ESP32-C3 that matters:
the stack is whatever the linker leaves after `.bss`, so a pool sized for an
entity this node never creates is stack it cannot use.

## Build

> **Contributors:** the in-tree fixture/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#esp32).

Copy the example out, generate bindings, build with the pinned nightly,
and pack the flash image:

```bash
# once per checkout location — message bindings + the generated
# build/esp32-c3-baremetal/nros-cargo.toml:
NROS_REPO_DIR=<path-to-nano-ros> nros sync

# nros build hands cargo the generated settings file. That file asks for
# `[unstable] build-std`, so a nightly carrying `rust-src` must be the ACTIVE
# toolchain — nros build picks no channel for you. The pinned channel is
# tools/rust-toolchain.toml's; any recent nightly works:
#   rustup toolchain install nightly && rustup component add rust-src --toolchain nightly
#   rustup target add riscv32imc-unknown-none-elf --toolchain nightly
#   rustup override set nightly          # in this directory
# Release, because ESP32-C3 DRAM is tight: anything after `--` goes to cargo
# verbatim.
nros build -- --release

# pack the ELF into the flash image QEMU boots (espflash comes from
# `nros setup esp32-c3-baremetal`, on PATH via activate):
espflash save-image --chip esp32c3 --flash-size 4mb --merge \
    build/esp32-c3-baremetal/target/riscv32imc-unknown-none-elf/release/esp32_qemu_talker \
    talker.bin
```

**Driving cargo yourself** — for an IDE or a CI step — run `nros sync` first,
then point cargo at the same generated file. Run it from the directory *above*
the package, so the package's own `.cargo/` is not read a second time;
phase-445 W6 deletes that directory, after which the working directory stops
mattering:

```bash
cd examples/esp32-c3-baremetal/rust
cargo +nightly build --release \
      --manifest-path talker/Cargo.toml \
      --config talker/build/esp32-c3-baremetal/nros-cargo.toml
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
#    dials (9800 — the image's `locator` above):
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
2. Confirm the image really is the C3 one — `[image.<id>] board` should
   read `esp32-c3-baremetal`, and the `[build] target` in the generated
   `build/esp32-c3-baremetal/nros-cargo.toml` should read
   `riscv32imc-unknown-none-elf`. The tutorial does not support ESP32-S3
   (Xtensa) yet.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- QEMU ESP32 talker:
  [`examples/esp32-c3-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/esp32-c3-baremetal/rust/talker)
- Board crate:
  [`packages/boards/nros-board-esp32-qemu/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-esp32-qemu)

## Next

- Subscriber + service + action peer directories under the same
  `examples/esp32-c3-baremetal/rust/`.
- ESP-IDF component path for C / C++ apps:
  [ESP32 (ESP-IDF component)](./integration-esp-idf.md).
- ESP32-S3 (Xtensa) — not supported today. The Xtensa toolchain
  does not ship via `rustup` (it requires
  [`espup`](https://github.com/esp-rs/espup)), and there is no
  in-tree Xtensa board crate. Stick with ESP32-C3 (RISC-V) for now.
