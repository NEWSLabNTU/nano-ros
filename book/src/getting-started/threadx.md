# ThreadX (Linux sim / RISC-V64 QEMU)

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours ship in-tree:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust, C, and C++ are supported on both flavours — `just <flavour>
build-fixtures` produces `threadx_cpp_*` and `riscv64_threadx_cpp_*`
binaries alongside the Rust + C ones. See the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
for the per-RMW cell status.

> **Prereqs.** Install the `nros` CLI once, then run
> `nros setup <board> --rmw <rmw>` for the flavour you need (see
> [Setup](#setup)). It provisions the cross-compiler, emulator, RMW host
> daemon, and ThreadX/NetX sources — no hand-installed `riscv64` cross
> toolchain, `qemu-system-riscv64`, or ROS 2 required.

## Setup

`nros setup` is the single canonical command to prepare a machine to build
nano-ros for a board. It ships prebuilt toolchains per platform per RMW — the
cross-compiler, emulator, RMW host daemon, and SDK sources (the ThreadX/NetX
sources, and for threadx-linux the POSIX-sim sources) are fetched from a pinned
index into a shared store at `${NROS_HOME:-~/.nros}/sdk`. You do not need ROS 2 installed.

Build the in-tree `nros` CLI (Phase 218):

```bash
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros
```

Provision the ThreadX flavour you need (+ the RMW):

```bash
nros setup threadx-linux --rmw zenoh          # POSIX-sim flavour; --rmw defaults to zenoh
nros setup qemu-riscv64-threadx --rmw zenoh   # only if you need the RISC-V64 QEMU flow
source ./activate.sh
```

The RMW host daemon must be **running** before any example: `zenohd` for zenoh,
the Micro-XRCE-DDS agent for xrce. `nros setup … --rmw <rmw>` installs it.

## Project layout

Each example is a standalone Cargo or CMake project under
`examples/threadx-linux/` and `examples/qemu-riscv64-threadx/`
(`<lang>/<example>/` under each):

```text
examples/threadx-linux/
├── rust/talker/                 # Cargo, target = x86_64-unknown-linux-gnu
│   ├── Cargo.toml                # deps + [package.metadata.nros.deploy.threadx-linux]
│   ├── package.xml
│   ├── generated/                # codegen output — build.rs runs
│   │                             #   `nros generate-rust` on first
│   │                             #   `cargo build`; gitignored.
│   └── src/lib.rs                # the component class; nros::main! generates the entry
└── c/talker/                    # CMake, add_subdirectory
    ├── CMakeLists.txt            # targets + nano_ros_deploy(...)
    ├── package.xml
    └── src/Talker.c

examples/qemu-riscv64-threadx/
├── rust/talker/                 # Cargo, target = riscv64gc-unknown-linux-gnu
│   └── ...
└── c/talker/
    └── ...
```

ThreadX-linux runs as a regular host process — no QEMU. NetX Duo
uses the `nx_bsd_*` BSD socket shim layered on the host TCP stack
(threadx-linux variant) or on its own NetX Duo TCP/IP stack
(riscv64 variant).

## Configure

Deploy config is declared per flavour in the build manifest and baked at
compile time. Both shipped shapes, verbatim:

threadx-linux —
[`examples/threadx-linux/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/threadx-linux/rust/talker/Cargo.toml):

```toml
[package.metadata.nros.deploy.threadx-linux]
board     = "threadx-linux"
rmw       = "zenoh"
domain_id = 0
# locator/ip default to the board's loopback shape (dial 127.0.0.1)
```

threadx-riscv64 —
[`examples/qemu-riscv64-threadx/c/talker/CMakeLists.txt`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-riscv64-threadx/c/talker/CMakeLists.txt):

```cmake
nano_ros_deploy(TARGET riscv64-qemu RMW ${NROS_RMW} DOMAIN_ID 0)
```

Network shape (guest IP, gateway, router locator) beyond these fields
comes from the board crate's defaults — see the
[Configuration Guide](../user-guide/configuration.md).

ThreadX-Linux normally uses a veth pair (`tap-tx0`) for an isolated
host link, but `nros setup threadx-linux` does **not** create the
interface — the test fixtures fall back to a loopback path when
`tap-tx0` is absent, which is fine for the happy-path tutorial.
Bring up `tap-tx0` by hand (`ip link add … type veth …`) only when
you need real-network bridging. The QEMU-RISC-V64 fixture uses
Slirp's default `10.0.2.2` gateway just like the FreeRTOS QEMU flow.

## Build

```bash
# threadx-linux:
just threadx_linux build-fixtures   # build all rust + c examples

# Single example:
cd examples/threadx-linux/rust/talker
cargo build --release

# threadx-riscv64:
just threadx_riscv64 build-fixtures
```

First setup builds ThreadX + NetX Duo (~3 min). Subsequent example
builds finish in seconds.

## Run

```bash
# threadx-linux (no QEMU). Step 1 brings up the in-tree zenohd on
# the threadx-linux port (7455). Step 2 runs the talker via the
# matching just recipe — same binary the example dir builds.
just threadx_linux zenohd &
just threadx_linux talker
# Expected (per src/lib.rs structured logs):
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   ...

# threadx-riscv64 (QEMU virt). Same shape — zenohd on 7453 first,
# then the talker recipe boots `qemu-system-riscv64` with the
# virtio-net + Slirp wiring baked in:
just threadx_riscv64 zenohd &
just threadx_riscv64 talker

# Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

For batch testing: `just threadx_linux test` runs every pubsub /
service / action against an in-test zenohd.

**Readiness signal.** threadx-linux: `Publishing: 'Hello World: 1'`
within a few seconds of `just threadx_linux talker` **on a warm
cache**; a cold first run rebuilds the Rust example (~80 s on a
fresh checkout) before the first publish lands. threadx-riscv64
(QEMU): within ~15 seconds of QEMU boot. If no `Publishing:` line:

1. Confirm `zenohd` reachable on the deploy locator
   (threadx-linux uses `127.0.0.1`; riscv64 QEMU uses `10.0.2.2`).
2. threadx-linux: confirm the veth bridge came up via
   `nros setup threadx-linux`.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- ThreadX-Linux Rust:
  [`examples/threadx-linux/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/rust/talker)
- ThreadX-Linux C:
  [`examples/threadx-linux/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/c/talker)
- ThreadX-RISC-V64 Rust:
  [`examples/qemu-riscv64-threadx/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-riscv64-threadx/rust/talker)
- Board crates:
  [`packages/boards/nros-board-threadx-linux/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-threadx-linux),
  [`packages/boards/nros-board-threadx-qemu-riscv64/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-threadx-qemu-riscv64)

## Next

- Subscriber + service + action peers in the same example tree.
- DDS on ThreadX: Cyclone DDS is the surviving DDS backend
  (`nros-rmw-cyclonedds`, selected via `-DNANO_ROS_RMW=cyclonedds`); see
  [Choosing an RMW Backend](../user-guide/rmw-backends.md).
- Real hardware: same code runs against ThreadX vendor BSPs (Renesas
  Synergy, MIMXRT, etc.); replace the QEMU board crate with a vendor
  board crate.
