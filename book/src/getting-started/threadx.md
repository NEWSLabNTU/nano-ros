# ThreadX (Linux sim / RISC-V64 QEMU)

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours:

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

With the `nros` CLI on PATH ([Installation](./installation.md) —
`install.sh` puts it in `~/.nros/bin`), provision the ThreadX flavour you
need (+ the RMW; `--rmw` defaults to zenoh). The POSIX-sim flavour:

```bash probe=30 variant=linux,linux-c
nros setup threadx-linux --rmw zenoh
```

and, if you need the RISC-V64 QEMU flow:

```bash probe=30 variant=riscv64
nros setup rv-virt-threadx --rmw zenoh
```

The RMW host daemon must be **running** before any example: for zenoh the
ROS 2 router (`ros2 run rmw_zenoh_cpp rmw_zenohd`), for xrce the
Micro-XRCE-DDS agent (installed by `nros setup … --rmw xrce`).

## Project layout

**C / C++** is a single package — `nros new` scaffolds it. `threadx`
names two boards, so `nros new` asks which one (`--board`) and refuses
without it:

```bash probe=40 variant=linux-c
nros new tx_talker --platform threadx --lang c --board threadx-linux   # or rv-virt-threadx; --lang cpp
cd tx_talker
```

```text
tx_talker/
├── system.toml          # WHAT this deploys to — the one file you edit
├── CMakeLists.txt       # find_package(nano_ros) + targets; no board facts
├── package.xml
└── src/main.c
```

**Rust** on ThreadX is a node package plus a generated entry, so it
starts as a workspace (single-package Rust scaffolding is not offered
for ThreadX — `nros new` says so and names the alternative):

```bash probe=40 variant=linux,riscv64
nros new tx_ws --workspace --lang rust --rmw zenoh
cd tx_ws
nros new --image tx_linux --board threadx-linux       # hosted flavour
nros new --image tx_riscv --board rv-virt-threadx     # QEMU flavour (optional)
```

The two flavours differ in the `board =` line of their image — nothing
else. Everything else follows the board. `threadx-linux` builds for the
host triple (its Rust entry is still `#![no_std]`, issue 1759), so its
board contributes no target triple; `rv-virt-threadx` cross-compiles,
and its triple, link group and emulator all come from the board
descriptor — never from your package. `nros sync` / `nros build`
resolve the choice into the image's `nros-cargo.toml` under `build/`,
which is what the build reads (RFC-0098). The board crate is reached
through a generated `<pkg>_nros_selection` package
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)),
so nothing you write names it.

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
time. The two examples on GitHub show the keys an image takes:

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
os_config = ["board-net.toml"]    # [eth0] network = "qemu", host = 15
```

Add the same keys (`locator`, and `os_config` for the QEMU flavour) to
the image `nros new` wrote. The C leaves carry the same file with the
same keys; their `CMakeLists.txt`
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

C / C++, in the scaffolded package (`tx_talker/`, where `nros new` left
you; no `nros sync` — its message bindings are a CMake-time output):

```bash probe=50 variant=linux-c
nros build
```

Rust, in the workspace (`tx_ws/`) — `nros sync` first, then the image.
The hosted flavour:

```bash probe=50 variant=linux
nros sync
nros build tx_linux
```

or the QEMU flavour:

```bash probe=50 variant=riscv64
nros sync
nros build tx_riscv
```

First setup builds ThreadX + NetX Duo (~3 min). Subsequent builds finish
in seconds. A Rust image lands under its entry's directory —
`build/threadx-linux-zenoh/tx_linux_entry/target/debug/tx_linux_entry`
for the hosted flavour, with the cross triple as an extra path segment
for `rv-virt-threadx` (`build/threadx-riscv64-zenoh/tx_riscv_entry/target/riscv64gc-unknown-none-elf/debug/tx_riscv_entry`).
`nros build <image> --dry-run` prints the exact paths and the `cargo`
line, for driving cargo yourself.

Skipping `nros sync` is not a mysterious failure: `nros build` stops at its
preflight with one line naming what is missing and telling you to run sync in
that directory. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).

After a successful build, `nros build` prints the artifact and the command
that runs it: the binary itself for `threadx-linux`, and for
`rv-virt-threadx` the QEMU line below with the store's
`qemu-system-riscv64` filled in. The C / C++ package's image is
`build/threadx/cmake/tx_talker` (`build/<image>/cmake/` plus the name
`nano_ros_add_executable` gives it).

## Run on the Linux simulator

`threadx-linux` is an ordinary host process — no QEMU. First the router
(ROS's `rmw_zenohd`), on the port your image's `locator` names — 7447 when
it names none, as a scaffolded image; 9000 in the example above:

```bash probe=60 probe-run=60 probe-keep variant=linux,linux-c
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=linux,linux-c
Started Zenoh router with id
```

Then the image, from the workspace (`tx_ws/`). It runs the scaffold's talker
and listener together, so it prints both sides of `/chatter` (a
`std_msgs/Int32` counter from 0):

```bash probe=70 probe-run=120 variant=linux
./build/threadx-linux-zenoh/tx_linux_entry/target/debug/tx_linux_entry
```

```text probe-expect=70 variant=linux
Publishing: 0
I heard: 0
...
```

The C / C++ package's image, from `tx_talker/` (board `threadx-linux`):

```bash probe=70 probe-run=120 variant=linux-c
./build/threadx/cmake/tx_talker
```

The scaffolded C talker is the in-tree example's, publishing `std_msgs/String`
once a second:

```text probe-expect=70 variant=linux-c
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

## Run on QEMU (RISC-V 64)

`rv-virt-threadx` boots on QEMU's RISC-V `virt` machine, UART on stdio,
virtio-net on Slirp. Three terminals: the router, the board, and a stock
ROS 2 peer.

**1. The router**, on the image's port (7447 for a scaffolded image; 9400 in
the example above). The guest dials `10.0.2.2:<p>` through QEMU Slirp, so it
listens on `0.0.0.0`:

```bash probe=60 probe-run=60 probe-keep variant=riscv64
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=riscv64
Started Zenoh router with id
```

**2. The board** — boot the built image with the store's
`qemu-system-riscv64` (`nros setup rv-virt-threadx` installed it), from the
workspace (`tx_ws/`):

```bash probe=70 probe-run=150 probe-keep variant=riscv64
"$(nros sdk-path qemu)/bin/qemu-system-riscv64" -M virt -m 256M -bios none -nographic \
    -global virtio-mmio.force-legacy=false \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=52:54:00:12:34:56 \
    -kernel build/threadx-riscv64-zenoh/tx_riscv_entry/target/riscv64gc-unknown-none-elf/debug/tx_riscv_entry
```

```text probe-expect=70 variant=riscv64
Publishing: 0
I heard: 0
...
```

The C / C++ package's image (board `rv-virt-threadx`) boots with the same
flags and `-kernel build/threadx/cmake/tx_talker`.

**3. A stock ROS 2 peer** (either flavour). The talker publishes
best-effort; stock `ros2 topic echo` defaults to RELIABLE, so the
QoS-mismatched echo silently delivers nothing. Force best-effort to receive:

```bash probe=80 probe-run=90 variant=riscv64
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

```text probe-expect=80 variant=riscv64
data:
```

QEMU exits via Ctrl-A x.

> **Contributors:** the in-tree fixture run/test lanes are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#threadx).

**Readiness signal.** threadx-linux: `Publishing: 0` (the
[GitHub examples](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/threadx-linux)
print `Publishing: 'Hello World: 1'`) within a few seconds of starting the binary **on a warm
cache**; a cold first `nros build` compiles the Rust image (~80 s)
before the first publish lands. threadx-riscv64
(QEMU): within ~15 seconds of QEMU boot. If no `Publishing:` line:

1. Confirm the router is reachable on the image's `locator`
   (threadx-linux uses `127.0.0.1`; riscv64 QEMU uses `10.0.2.2`).
2. threadx-linux: if you brought up `tap-tx0` by hand, confirm it is
   up; without it the fixtures use the loopback fallback, which is
   fine for this tutorial (`nros setup threadx-linux` does not create
   the interface).
3. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Flash a real board

No real ThreadX board is supported. Both boards here are simulated — a
Linux process and a QEMU machine — and nano-ros ships no ThreadX board
package for physical hardware. A board of your own starts from the
[porting guide](../porting/custom-board.md), with the
`nros-board-threadx` family crate and the riscv64 arch port as the
pieces to reuse; it is then flashed with that board vendor's tooling.

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
