# ThreadX (Linux sim / RISC-V64 QEMU)

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours ship in-tree:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust and C are supported on both flavours; nros-cpp does not target
ThreadX (not in the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)).

> **Prereqs.** Clone with `just setup` already run. For
> threadx-riscv64 also need a `riscv64-unknown-linux-gnu-gcc` cross
> toolchain on `PATH` plus `qemu-system-riscv64`.

## Project layout

Each example is a standalone Cargo or CMake project under
`examples/threadx-{linux,riscv64}/<lang>/zenoh/<example>/`:

```text
examples/threadx-linux/
├── rust/zenoh/talker/                 # Cargo, target = x86_64-unknown-linux-gnu
│   ├── Cargo.toml
│   ├── package.xml
│   ├── generated/
│   └── src/main.rs
└── c/zenoh/talker/                    # CMake, add_subdirectory
    ├── CMakeLists.txt
    ├── package.xml
    └── src/main.c

examples/threadx-riscv64/
├── rust/zenoh/talker/                 # Cargo, target = riscv64gc-unknown-linux-gnu
│   └── ...
└── c/zenoh/talker/
    └── ...
```

ThreadX-linux runs as a regular host process — no QEMU. NetX Duo
uses the `nx_bsd_*` BSD socket shim layered on the host TCP stack
(threadx-linux variant) or on its own NetX Duo TCP/IP stack
(riscv64 variant).

## Configure

```toml
# threadx-linux talker config.toml
[zenoh]
locator   = "tcp/127.0.0.1:7447"
domain_id = 0
```

```toml
# threadx-riscv64 talker config.toml — QEMU Slirp
[zenoh]
locator   = "tcp/10.0.2.2:7447"
domain_id = 0
```

The ThreadX-Linux fixture intentionally pins zenohd to `0.0.0.0`
(not `127.0.0.1`) because the veth bridge needs an externally-
reachable bind. The QEMU-RISC-V64 fixture uses Slirp's default
`10.0.2.2` gateway just like the FreeRTOS QEMU flow.

## Build

```bash
# threadx-linux:
just threadx_linux setup            # build ThreadX + NetX Duo + NSOS shim
just threadx_linux build-fixtures   # build all rust + c examples

# Single example:
cd examples/threadx-linux/rust/zenoh/talker
cargo build --release

# threadx-riscv64:
just threadx_riscv64 setup
just threadx_riscv64 build-fixtures
```

First setup builds ThreadX + NetX Duo (~3 min). Subsequent example
builds finish in seconds.

## Run

```bash
# threadx-linux (no QEMU):
just zenohd setup && just zenohd run        # bring up router
cd examples/threadx-linux/rust/zenoh/talker
cargo run --release
# Expected:
#   nros ThreadX-Linux Talker
#   Published: 1
#   Published: 2
#   ...

# threadx-riscv64 (QEMU virt):
qemu-system-riscv64 -machine virt -cpu rv64 -smp 1 -m 256M \
                    -nographic \
                    -netdev user,id=net0 \
                    -device virtio-net-device,netdev=net0 \
                    -kernel ./build/talker.elf

# Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

For batch testing: `just threadx_linux test` runs every pubsub /
service / action against an in-test zenohd.

**Readiness signal.** threadx-linux: `Published: 1` within 3
seconds of `cargo run`. threadx-riscv64 (QEMU): within ~15
seconds of QEMU boot. If no `Published:` line:

1. Confirm `zenohd` reachable on the locator from `config.toml`
   (threadx-linux uses `127.0.0.1`; riscv64 QEMU uses `10.0.2.2`).
2. threadx-linux: confirm the veth bridge came up via
   `just threadx_linux setup`.
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- ThreadX-Linux Rust:
  [`examples/threadx-linux/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/rust/zenoh/talker)
- ThreadX-Linux C:
  [`examples/threadx-linux/c/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/c/zenoh/talker)
- ThreadX-RISC-V64 Rust:
  [`examples/threadx-riscv64/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-riscv64/rust/zenoh/talker)
- Board crates:
  [`packages/boards/nros-board-threadx-linux/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-threadx-linux),
  [`packages/boards/nros-board-riscv64-qemu/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-riscv64-qemu)

## Next

- Subscriber + service + action peers in the same example tree.
- DDS on ThreadX: dust-DDS works in Rust, gated on the `rmw-dds-cffi`
  feature axis; see
  [Choosing an RMW Backend](../user-guide/rmw-backends.md).
- Real hardware: same code runs against ThreadX vendor BSPs (Renesas
  Synergy, MIMXRT, etc.); replace the QEMU board crate with a vendor
  board crate.
