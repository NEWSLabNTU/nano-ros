# ThreadX (Linux sim / RISC-V64 QEMU)

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours ship in-tree:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust, C, and C++ are supported on both flavours.

> **Contributors:** the in-tree fixture/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#threadx).

See the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
for the per-RMW cell status.

> **Prereqs.** Install the `nros` CLI once, then run
> `nros setup <board> --rmw <rmw>` for the flavour you need (see
> [Setup](#setup)). It provisions the cross-compiler, emulator, and
> ThreadX/NetX sources — no hand-installed `riscv64` cross toolchain or
> `qemu-system-riscv64`. The zenoh path additionally needs a ROS 2
> install for the router (`ros2 run rmw_zenoh_cpp rmw_zenohd`); xrce
> needs only the Micro-XRCE-DDS agent, which `nros setup` installs.

## Setup

`nros setup` is the single canonical command to prepare a machine to build
nano-ros for a board. Most components are prebuilt per platform per RMW — the
cross-compiler, emulator, and SDK sources (the ThreadX/NetX
sources, and for threadx-linux the POSIX-sim sources) are fetched from a pinned
index into a shared store at `${NROS_HOME:-~/.nros}/sdk`. The zenoh router is
NOT among them — it comes from a ROS 2 install (RFC-0075); only the xrce
agent is provisioned by `nros setup`.

Packages are prebuilt where the index has a binary for your host and built from
source otherwise. `--dry-run` prints the plan for your host. See
[Installation](installation.md#provision-your-toolchain-with-nros-setup) for the full explanation.

Build the in-tree `nros` CLI (Phase 218):

```bash
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish
```

Provision the ThreadX flavour you need (+ the RMW):

```bash
nros setup threadx-linux --rmw zenoh          # POSIX-sim flavour; --rmw defaults to zenoh
nros setup rv-virt-threadx --rmw zenoh   # only if you need the RISC-V64 QEMU flow
source ./activate.sh
```

The RMW host daemon must be **running** before any example: for zenoh the
ROS 2 router (`ros2 run rmw_zenoh_cpp rmw_zenohd`), for xrce the
Micro-XRCE-DDS agent (installed by `nros setup … --rmw xrce`).

## Project layout

Each example is a standalone Cargo or CMake project under
`examples/threadx-linux/` and `examples/rv-virt-threadx/`
(`<lang>/<example>/` under each):

```text
examples/threadx-linux/
├── rust/talker/                 # Cargo package
│   ├── system.toml               # WHAT this deploys to — the one file you edit
│   ├── Cargo.toml                # Rust deps only; nothing here names a board
│   ├── package.xml
│   ├── generated/                # generated message bindings (gitignored)
│   ├── build/                    # everything `nros sync` generates (gitignored)
│   └── src/lib.rs                # the component class; nros::main! generates the entry
└── c/talker/                    # CMake, add_subdirectory
    ├── system.toml               # the same file, same schema
    ├── CMakeLists.txt            # targets only
    ├── package.xml
    └── src/Talker.c

examples/rv-virt-threadx/
├── rust/talker/                 # identical shape; `board = "rv-virt-threadx"`
│   └── ...
└── c/talker/
    └── ...
```

The two flavours differ in the `board =` line of their `system.toml` — plus,
on the Rust side, the board crate in `[dependencies]`
(`nros-board-threadx-linux` against `nros-board-threadx-qemu-riscv64`). A
single-package leaf is its own entry, so it still names that crate by hand and
`nros sync` reads the patch rows from the manifest;
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md)
is why it is two lines rather than one. The C leaves have no board crate and
really do switch on the one line.

Everything else follows the board. `threadx-linux` is a hosted build, so its
board contributes no target triple; `rv-virt-threadx` cross-compiles, and its
triple, link group and emulator all come from the board descriptor — never
from a leaf. `nros sync` resolves the choice into
`build/<image-id>/nros-cargo.toml`, which is what the build reads (RFC-0098).

ThreadX-linux runs as a regular host process — no QEMU. NetX Duo
uses the `nx_bsd_*` BSD socket shim layered on the host TCP stack
(threadx-linux variant) or on its own NetX Duo TCP/IP stack
(riscv64 variant).

## Why there are two board crates and one arch port

The two flavours look like one board with two targets, and they are not.
nano-ros ships **three** ThreadX packages, in the three layers RFC-0064
describes:

| Package | Layer | What it owns |
|---|---|---|
| `nros-board-threadx` | family driver | the kernel/NetX build and the generic `run_entry` boot path both boards call |
| `nros-board-threadx-port-riscv64` | **arch port** | a fork of upstream's `ports/risc-v64/gnu`: `tx_port.h` + five context-switch `.S` files |
| `nros-board-threadx-{linux,qemu-riscv64}` | board overlay | defaults, console/exit/panic, and the network bring-up |

The arch port is the reason the two boards did not merge. Upstream's
RISC-V64 port types `ULONG` as `unsigned long` — 8 bytes on rv64 — but
NetX Duo's packet code does `ULONG *` arithmetic assuming 4-byte words,
so nano-ros retypes it to `unsigned int`, matching upstream's own Linux
and AArch64 ports. That retype shifts every field offset inside
`TX_THREAD`, and the port's context-switch assembly loads those fields at
hard-coded offsets — so the header change forces the assembly to be
forked with it. `threadx-linux` needs none of this, because upstream's
Linux port already types `ULONG` as 4 bytes.

That is architecture code, not board code: a second RISC-V64 ThreadX
board would need every line of it unchanged, and folding it into a crate
that also serves Linux would mean `cfg`-gating RISC-V assembly there. If
you are porting to another RISC-V64 ThreadX board, depend on the arch
port and write only the overlay — see
[Custom boards](../porting/custom-board.md).

One rule matters if you build ThreadX outside the shipped recipes: the
arch port's `inc/` **must** be searched before
`$THREADX_DIR/ports/risc-v64/gnu/inc`. Get that wrong and the build
succeeds against upstream's 8-byte `ULONG`, and the symptom is corrupted
packets at runtime rather than a compiler error. A
`_Static_assert(sizeof(ULONG) == 4, …)` in
`packages/boards/nros-board-common/c/threadx_hooks.c` turns that mistake
into a build failure with a name on it.

## Configure

Deployment is declared per flavour in `system.toml` and baked at compile
time. Both shipped shapes, verbatim:

threadx-linux —
[`examples/threadx-linux/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/threadx-linux/rust/talker/system.toml):

```toml
[system]
name = "threadx_linux_rs_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "threadx_linux_rs_talker"
class = "threadx_linux_rs_talker::Talker"
name = "talker"

[image.threadx-linux]
board = "threadx-linux"
locator = "tcp/127.0.0.1:9000"
```

threadx-riscv64 —
[`examples/rv-virt-threadx/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/rv-virt-threadx/rust/talker/system.toml)
adds the guest's network identity, because it boots behind QEMU Slirp:

```toml
[image.rv-virt-threadx]
board = "rv-virt-threadx"
locator = "tcp/10.0.2.2:9400"
ip = "10.0.2.15"
netmask = "255.255.255.0"
gateway = "10.0.2.2"
```

The C leaves carry the same file with the same keys; their `CMakeLists.txt`
declares targets and nothing about the deployment, and their `package.xml`
carries no `<nano_ros …/>` tuple. Anything the image does not state — the rest
of the network shape — comes from the board crate's defaults; see the
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
# Single example — `nros sync` writes the generated message bindings and
# build/threadx-linux/nros-cargo.toml from system.toml; `nros build` builds
# the image that file declares.
cd examples/threadx-linux/rust/talker
nros sync
nros build
```

> **Contributors:** the in-tree fixture build lanes for both flavours are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#threadx).

First setup builds ThreadX + NetX Duo (~3 min). Subsequent example
builds finish in seconds. The binary lands under the image's own directory —
`build/threadx-linux/target/debug/talker` for the hosted flavour, with the
cross triple as an extra path segment for `rv-virt-threadx`.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is supported: run
`nros sync` first, then point cargo at the generated settings file. Run it from
the directory *above* the package, so the package's own `.cargo/` is not read
a second time — phase-445 W6 deletes that directory, after which the working
directory stops mattering:

```bash
cd examples/threadx-linux/rust
cargo build --manifest-path talker/Cargo.toml \
            --config talker/build/threadx-linux/nros-cargo.toml
```

Skipping `nros sync` is not a mysterious failure: `nros build` stops at its
preflight with one line naming what is missing and telling you to run sync in
that directory. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).

## Run

```bash
# threadx-linux (no QEMU). Step 1 brings up the router (ROS's
# `rmw_zenohd`) on port 9000 — the `locator` the talker bakes from
# `[image.threadx-linux]`. Step 2 builds + runs the talker:
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:9000"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &
cd examples/threadx-linux/rust/talker && nros sync && nros build
./build/threadx-linux/target/debug/talker
# Expected (per src/lib.rs structured logs):
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   ...

# threadx-riscv64 (QEMU virt). Same shape — the router on 9400 first
# (the riscv64 talker dials tcp/10.0.2.2:9400 through QEMU Slirp):
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:9400"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# Build it the same way (`system.toml` names `board = "rv-virt-threadx"`,
# so the riscv64 triple and link group come from the board):
cd examples/rv-virt-threadx/rust/talker && nros sync && nros build

# Then boot the built image in QEMU (UART on stdio, Slirp networking):
qemu-system-riscv64 -M virt -m 256M -bios none -nographic \
    -global virtio-mmio.force-legacy=false \
    -kernel build/rv-virt-threadx/target/riscv64gc-unknown-none-elf/debug/talker \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0
# **Contributors (in-tree checkout):** `just threadx_riscv64 talker`
# builds and boots this exact shape in one step.

# Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

> **Contributors:** the in-tree fixture run/test lanes are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#threadx).

**Readiness signal.** threadx-linux: `Publishing: 'Hello World: 1'`
within a few seconds of starting the binary **on a warm
cache**; a cold first `nros build` compiles the Rust example (~80 s on a
fresh checkout) before the first publish lands. threadx-riscv64
(QEMU): within ~15 seconds of QEMU boot. If no `Publishing:` line:

1. Confirm the router is reachable on the image's `locator`
   (threadx-linux uses `127.0.0.1`; riscv64 QEMU uses `10.0.2.2`).
2. threadx-linux: if you brought up `tap-tx0` by hand, confirm it is
   up; without it the fixtures use the loopback fallback, which is
   fine for this tutorial (`nros setup threadx-linux` does not create
   the interface).
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Multi-tier scheduling (phase-297)

ThreadX supports the multi-tier workspace scheduling model
(RFC-0053): declare per-tier ThreadX priorities in the workspace
manifest with `[tiers.<name>.threadx]` priority tables, and
`nros::main!` routes the app onto `run_tiers` — one executor per
tier, all sharing one session, with per-tier stacks carved from the
ThreadX byte pool. ThreadX priorities follow the native convention:
**lower number = higher priority**. Tiers are sorted descending by
raw priority number, so `tiers[0]` (the numerically-largest, i.e.
lowest-priority tier) boots first and then adopts its declared
priority. See [Scheduling Models](../internals/scheduling-models.md)
for the mechanics.

## GitHub source

- ThreadX-Linux Rust:
  [`examples/threadx-linux/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/rust/talker)
- ThreadX-Linux C:
  [`examples/threadx-linux/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux/c/talker)
- ThreadX-RISC-V64 Rust:
  [`examples/rv-virt-threadx/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/rv-virt-threadx/rust/talker)
- Board crates:
  [`packages/boards/nros-board-threadx-linux/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-threadx-linux),
  [`packages/boards/nros-board-threadx-qemu-riscv64/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-threadx-qemu-riscv64)

## Next

- Subscriber + service + action peers in the same example tree.
- DDS on ThreadX: Cyclone DDS is the surviving DDS backend
  (`nros-rmw-cyclonedds`) — select it with `[system] rmw = "cyclonedds"` in
  `system.toml` and re-run `nros sync`; see
  [Choosing an RMW Backend](../user-guide/rmw-backends.md).
- Real hardware: same code runs against ThreadX vendor BSPs (Renesas
  Synergy, MIMXRT, etc.); replace the QEMU board crate with a vendor
  board crate.
