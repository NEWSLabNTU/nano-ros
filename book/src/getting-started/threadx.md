# ThreadX (Linux sim / RISC-V64 QEMU)

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours ship in-tree:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust, C, and C++ are supported on threadx-linux; threadx-riscv64
ships Rust + C only. See the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
for the exact cells.

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
index into a shared store at `~/.nros/sdk`. You do not need ROS 2 installed.

Install the `nros` CLI once per machine:

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
export PATH="$HOME/.nros/bin:$PATH"
```

Provision the ThreadX flavour you need (+ the RMW):

```bash
nros setup threadx-linux --rmw zenoh          # POSIX-sim flavour; --rmw defaults to zenoh
nros setup qemu-riscv64-threadx --rmw zenoh   # only if you need the RISC-V64 QEMU flow
source ./setup.bash
```

The RMW host daemon must be **running** before any example: `zenohd` for zenoh,
the Micro-XRCE-DDS agent for xrce. `nros setup … --rmw <rmw>` installs it.

## Project layout

Each example is a standalone Cargo or CMake project under
`examples/threadx-{linux,riscv64}/<lang>/<example>/`:

```text
examples/threadx-linux/
├── rust/talker/                 # Cargo, target = x86_64-unknown-linux-gnu
│   ├── Cargo.toml
│   ├── package.xml
│   ├── generated/
│   └── src/main.rs
└── c/talker/                    # CMake, add_subdirectory
    ├── CMakeLists.txt
    ├── package.xml
    └── src/main.c

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

The board crate parses `nros.toml` at boot. Schema is `[node]` +
`[[transport]]` (direct-mode); the locator rides inside the
transport.

```toml
# threadx-linux — mirror of examples/threadx-linux/rust/talker/nros.toml
[node]
domain_id = 0

[[transport]]
kind      = "ethernet"
ip        = "192.0.3.10/24"     # CIDR; veth peer is 192.0.3.1
mac       = "02:00:00:00:00:00"
gateway   = "192.0.3.1"
interface = "tap-tx0"           # veth side of the pair (see below)
locator   = "tcp/127.0.0.1:7455"
```

```toml
# threadx-riscv64 — QEMU Slirp; mirror of
# examples/qemu-riscv64-threadx/rust/talker/nros.toml
[node]
domain_id = 0

[[transport]]
kind    = "ethernet"
ip      = "10.0.2.40/24"
mac     = "52:54:00:12:34:56"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7453"
```

`tap-tx0` is a veth pair brought up out-of-band by
`scripts/qemu/setup-network.sh` — **the script needs root** (it
calls `ip link add`/`ip addr add`) and so it does NOT run as part
of `nros setup threadx-linux`. Run it once per machine:

```bash
sudo scripts/qemu/setup-network.sh
```

The QEMU-RISC-V64 fixture uses Slirp's default `10.0.2.2` gateway
and needs no host-side network setup.

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
# threadx-linux (no QEMU):
just threadx_linux zenohd        # router on 127.0.0.1:7455
just threadx_linux talker        # boots the linux talker process
# Expected:
#   nros ThreadX-Linux Talker
#   Published: 0
#   Published: 1
#   ...

# threadx-riscv64 (QEMU virt):
just threadx_riscv64 zenohd      # router on 127.0.0.1:7453
just threadx_riscv64 talker      # boots the riscv64 kernel under qemu-system-riscv64

# Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

For batch testing: `just threadx_linux test` runs every pubsub /
service / action against an in-test zenohd.

**Readiness signal.** threadx-linux: `Published: 0` within 3
seconds of `just threadx_linux talker`. threadx-riscv64 (QEMU):
within ~15 seconds of QEMU boot. If no `Published:` line:

1. Confirm `zenohd` reachable on the locator from `nros.toml`
   (threadx-linux uses `127.0.0.1:7455`; riscv64 QEMU uses
   `tcp/10.0.2.2:7453`).
2. threadx-linux: confirm `tap-tx0` is up — `ip link show tap-tx0`
   should report the interface. If absent, re-run
   `sudo scripts/qemu/setup-network.sh`.
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
