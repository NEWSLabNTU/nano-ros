# ThreadX (Linux sim / RISC-V64 QEMU)

<!-- type: how-to · level: 2 -->

> **Support level:** [`threadx-linux`](../reference/boards/threadx-linux.md), [`rv-virt-threadx`](../reference/boards/threadx-qemu-riscv64.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust, C, and C++ are supported on both flavours.

> Changing this in the tree: see [Internals → per-platform lanes](../internals/platform/platform-lanes.html#threadx).

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
NOT among them — it comes from a ROS 2 install; only the xrce
agent is provisioned by `nros setup`.

Packages are prebuilt where the index has a binary for your host and built from
source otherwise. `--dry-run` prints the plan for your host. See
[Installation Details](../get-started/installation-details.md#provisioning-with-nros-setup) for the full explanation.

With the `nros` CLI on PATH ([Installation](../get-started/installation.md) —
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

`nros new` scaffolds a workspace for one board, in any of the three languages.
`threadx` names two boards, so name the one you want with `--board`:

```bash probe=40 variant=linux-c
nros new tx_ws --board threadx-linux --template talker --lang c   # or rv-virt-threadx; --lang cpp
cd tx_ws
```

```bash probe=40 variant=linux
nros new tx_ws --board threadx-linux --template talker --lang rust
cd tx_ws
```

```bash probe=40 variant=riscv64
nros new tx_ws --board rv-virt-threadx --template talker --lang rust
cd tx_ws
```

```text
tx_ws/
└── src/
    ├── tx_node/                 # node package: package.xml + CMakeLists.txt (or Cargo.toml), the talker
    └── tx_node_bringup/         # package.xml and system.toml: the board, the RMW, the node
```

`tx_node` is a library with no `main()` that names no board; the ThreadX
startup that runs it is generated from the bringup's image at build time.

The two flavours differ in the `board =` line of their image — nothing
else. Everything else follows the board. `threadx-linux` builds for the
host triple (its Rust entry is still `#![no_std]`), so its
board contributes no target triple; `rv-virt-threadx` cross-compiles,
and its triple, link group and emulator all come from the board
descriptor — never from your package. `nros build` resolves the choice
into the generated entry and its settings under `build/`, and the entry is
what depends on the board crate, so nothing you write names it.

ThreadX-linux runs as a regular host process — no QEMU. NetX Duo
uses the `nx_bsd_*` BSD socket shim layered on the host TCP stack
(threadx-linux variant) or on its own NetX Duo TCP/IP stack
(riscv64 variant).

## Why there are two board crates and one arch port

The two flavours look like one board with two targets, and they are not.
nano-ros ships **three** ThreadX packages, in three layers:

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
[Custom boards](../extending/custom-board.md).

One rule matters if you build ThreadX outside the shipped recipes: the
arch port's `inc/` **must** be searched before
`$THREADX_DIR/ports/risc-v64/gnu/inc`. Get that wrong and the build
succeeds against upstream's 8-byte `ULONG`, and the symptom is corrupted
packets at runtime rather than a compiler error. A
`_Static_assert(sizeof(ULONG) == 4, …)` in
`packages/boards/nros-board-common/c/threadx_hooks.c` turns that mistake
into a build failure with a name on it.

## Configure

Deployment is declared in the bringup's `system.toml`, one image per flavour,
and baked at compile time. The scaffold names its image after the platform,
`[image.threadx]`. For the hosted flavour, with a `locator` added:

```toml
[system]
name = "tx_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "tx_node"
class = "tx_node::Talker"
name = "tx_node"

[image.threadx]
board = "threadx-linux"
locator = "tcp/127.0.0.1:9000"
```

The QEMU flavour adds the guest's network identity, because it boots behind
QEMU Slirp. Added beside the first, it takes a name of its own:

```toml
[image.threadx_rv]
board = "rv-virt-threadx"
locator = "tcp/10.0.2.2:9400"
os_config = ["board-net.toml"]    # [eth0] network = "qemu", host = 15
```

Both blocks can live in one bringup; the node package does not change. With
two images, `nros run tx_node` no longer picks one, so name it
(`nros run threadx`, `nros run threadx_rv`). Whatever the node's language,
the bringup carries the same keys; a C node's `CMakeLists.txt` declares targets
and nothing about the deployment, and its `package.xml` carries no
`<nano_ros …/>` tuple. Anything the image does not state — the rest of the
network shape — comes from the board crate's defaults; see the
[Configuration Guide](../build/configuration.md).

ThreadX-Linux normally uses a veth pair (`tap-tx0`) for an isolated
host link, but `nros setup threadx-linux` does **not** create the
interface — the test fixtures fall back to a loopback path when
`tap-tx0` is absent, which is fine for the happy-path tutorial.
Bring up `tap-tx0` by hand (`ip link add … type veth …`) only when
you need real-network bridging. The QEMU-RISC-V64 fixture uses
Slirp's default `10.0.2.2` gateway just like the FreeRTOS QEMU flow.

## Build

The build generates the message bindings from your ROS 2 install, so source
it in this shell first. This first build records the ROS 2 prefix in the
workspace, so later builds and runs of it need no sourced shell.

In the workspace root (`tx_ws/`, where `nros new` left you), whatever the
language and flavour:

```bash probe=50
source /opt/ros/humble/setup.bash
nros build
```

`nros build` prepares the build first (message bindings, the generated entry
for the image, its cargo or CMake settings), then hands off. There is no step
to run before it.

First setup builds ThreadX + NetX Duo (~3 min). Subsequent builds finish
in seconds. The binary lands under `build/`; `nros build --dry-run` prints the
exact paths and the `cargo` or `cmake` lines, for driving the tool yourself
(run `nros configure` first to write the generated entry without
building). See
[When `nros build` drives the build](how-integration-works.md#when-nros-build-drives-the-build).

After a successful build, `nros build` prints the artifact and the command
that runs it.

## Run on the Linux simulator

`threadx-linux` is an ordinary host process, with no QEMU. Run it from the
workspace root, with ROS 2 sourced: the image dials a router on this machine
(port 7447 when its `locator` names none, as a scaffolded image does; 9000 in
the example above), so `nros run` starts that router, ROS's `rmw_zenohd`, and
stops it with the image. Don't start a router of your own on the same port;
to use one you already have, pass `--router <its locator>`.

```bash probe=70 probe-run=120 variant=linux,linux-c
source /opt/ros/humble/setup.bash
nros run tx_node
```

The scaffolded talker publishes `std_msgs/String` once a second, in every
language:

```text probe-expect=70 variant=linux,linux-c
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

**2. The board** — from the workspace root:

```bash probe=70 probe-run=150 probe-keep variant=riscv64
nros run tx_node
```

`nros run` boots the image with the board's runner: the store's
`qemu-system-riscv64` (`nros setup rv-virt-threadx` installed it) on the
`virt` machine, with virtio-net on Slirp. A C or C++ node on this board runs
the same way.

```text probe-expect=70 variant=riscv64
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

**3. A stock ROS 2 peer** (either flavour). The talker publishes
best-effort; stock `ros2 topic echo` defaults to RELIABLE, so the
QoS-mismatched echo silently delivers nothing. Force best-effort to receive:

```bash probe=80 probe-run=90 variant=riscv64
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

```text probe-expect=80 variant=riscv64
data:
```

Ctrl-C stops `nros run` and QEMU with it.

**Readiness signal.** threadx-linux: `Publishing: 'Hello World: 1'`
within a few seconds of the program starting **on a warm
cache**; a cold first `nros build` compiles the Rust image (~80 s)
before the first publish lands. threadx-riscv64
(QEMU): within ~15 seconds of QEMU boot. If no `Publishing:` line:

1. Confirm the router is reachable on the image's `locator`
   (threadx-linux uses `127.0.0.1`; riscv64 QEMU uses `10.0.2.2`).
2. threadx-linux: if you brought up `tap-tx0` by hand, confirm it is
   up; without it the fixtures use the loopback fallback, which is
   fine for this tutorial (`nros setup threadx-linux` does not create
   the interface).
3. See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

## Flash a real board

No real ThreadX board is supported. Both boards here are simulated — a
Linux process and a QEMU machine — and nano-ros ships no ThreadX board
package for physical hardware. A board of your own starts from the
[porting guide](../extending/custom-board.md), with the
`nros-board-threadx` family crate and the riscv64 arch port as the
pieces to reuse; it is then flashed with that board vendor's tooling.

## Multi-sched-task scheduling

ThreadX supports the multi-sched-task workspace scheduling model: declare per-sched-task ThreadX priorities in the bringup's
`system.toml` with `[tasks.<name>.threadx]` priority tables, and
the generated entry routes the app onto `run_sched_tasks` — one executor per
sched task, all sharing one session, with per-sched-task stacks carved from the
ThreadX byte pool. ThreadX priorities follow the native convention:
**lower number = higher priority**. Sched tasks are sorted descending by
raw priority number, so `sched_tasks[0]` (the numerically-largest, i.e.
lowest-priority sched task) boots first and then adopts its declared
priority. See [Scheduling Models](../realtime/scheduling-models.md)
for the mechanics.

## GitHub source

The copy-out talkers, each the same project as the one this page scaffolds:

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

- Subscriber, service and action: the `threadx_*` and `riscv_threadx_*`
  image rows of the language workspaces, `examples/workspaces/{rust,c,cpp}`,
  under `pubsub_bringup`, `service_bringup` and `action_bringup`
  (`nros run pubsub_bringup:threadx_listener`).
- DDS on ThreadX: Cyclone DDS is the surviving DDS backend
  (`nros-rmw-cyclonedds`) — select it with `[system] rmw = "cyclonedds"` in
  the bringup's `system.toml` and build again; see
  [Choosing an RMW Backend](../communicate/choosing-an-rmw.md).
- Real hardware: same code runs against ThreadX vendor BSPs (Renesas
  Synergy, MIMXRT, etc.); replace the QEMU board crate with a vendor
  board crate.

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples as
  image rows of behaviour bringups.
