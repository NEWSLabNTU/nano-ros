# ThreadX (Linux sim / RISC-V64 QEMU)

<!-- type: how-to · level: 2 -->

**Needs first:** [Get started on the ThreadX Linux simulator](../../starters/threadx-linux.md) or [ThreadX in QEMU (RISC-V 64)](../../starters/threadx-riscv64.md)

> **Support level:** [`threadx-linux`](../../reference/boards/threadx-linux.md), [`rv-virt-threadx`](../../reference/boards/threadx-qemu-riscv64.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../../reference/board-support-levels.md) explains the words.

Single-node starter on Microsoft Azure RTOS ThreadX + NetX Duo (BSD
socket layer). Two flavours:

- **threadx-linux** — ThreadX user-space simulator on Linux. Fast
  build, host network stack, ideal for development.
- **threadx-riscv64** — QEMU `virt` machine with the RISC-V64 GCC
  toolchain. Full kernel + NetX Duo TCP/IP stack.

Rust, C, and C++ are supported on both flavours.

> Changing this in the tree: see [Internals → per-platform lanes](../../internals/platform/platform-lanes.html#threadx).

See the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)
for the per-RMW cell status.

Each flavour has a starter that provisions it, scaffolds a talker and runs
it: [the Linux simulator](../../starters/threadx-linux.md) and
[QEMU RISC-V 64](../../starters/threadx-riscv64.md). This page covers what
comes after them: the two flavours' board crates, the configuration, and the
scheduling tiers.

`nros setup <board>` fetches most components prebuilt per platform per RMW
(the cross-compiler, the emulator, and the ThreadX/NetX sources, and for
threadx-linux the POSIX-sim sources) from a pinned index into a shared store
at `${NROS_HOME:-~/.nros}/sdk`, and builds from source where the index has no
binary for your host. `--dry-run` prints the plan. See
[Installation Details](../build/install-options.md#provisioning-with-nros-setup).
For xrce, `nros setup … --rmw xrce` also installs the Micro-XRCE-DDS agent.

## Project layout

The starters' scaffold, `nros new tx_ws --board <board> --template talker`,
writes this workspace in any of the three languages. `threadx` names two
boards, so `--board` names the one you want:

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
[Add a board](../../extending/add-a-board.md).

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
[Configuration Guide](../build/configure-an-image.md).

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

```bash
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
[When `nros build` drives the build](../../background/how-a-build-becomes-an-image.md#when-nros-build-drives-the-build).

After a successful build, `nros build` prints the artifact and the command
that runs it.

## Run on the Linux simulator

`threadx-linux` is an ordinary host process, with no QEMU.
[Its starter](../../starters/threadx-linux.md#run-it-on-this-machine) runs it:
the image dials a router on this machine (port 7447 when its `locator` names
none, as a scaffolded image does; 9000 in the example above), so `nros run`
starts that router, ROS's `rmw_zenohd`, and stops it with the image. To use a
router you already have, pass `--router <its locator>`. If you brought up a
`tap-tx0` interface by hand, confirm it is up; without it the image uses the
loopback fallback (`nros setup threadx-linux` does not create the interface).

## Run on QEMU (RISC-V 64)

`rv-virt-threadx` boots on QEMU's RISC-V `virt` machine, UART on stdio,
virtio-net on Slirp. [Its starter](../../starters/threadx-riscv64.md#run-it-in-qemu)
runs it against a router on the host's `0.0.0.0` (port 7447 for a scaffolded
image; 9400 in the example above), which the guest dials as `10.0.2.2`. A C or
C++ node on this board runs the same way.

## Flash a real board

No real ThreadX board is supported. Both boards here are simulated — a
Linux process and a QEMU machine — and nano-ros ships no ThreadX board
package for physical hardware. A board of your own starts from the
[Add a board](../../extending/add-a-board.md), with the
`nros-board-threadx` family crate and the riscv64 arch port as the
pieces to reuse; it is then flashed with that board vendor's tooling.

## Multi-tier scheduling

ThreadX supports the multi-tier workspace scheduling model: declare per-tier ThreadX priorities in the bringup's
`system.toml` with `[tasks.<name>.threadx]` priority tables, and
the generated entry routes the app onto `run_sched_tasks` — one executor per
tier, all sharing one session, with per-tier stacks carved from the
ThreadX byte pool. ThreadX priorities follow the native convention:
**lower number = higher priority**. Tiers are sorted descending by
raw priority number, so `tiers[0]` (the numerically-largest, i.e.
lowest-priority tier) boots first and then adopts its declared
priority. See [Scheduling Models](../realtime/pick-a-scheduling-model.md)
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
  [Choosing an RMW Backend](../../tutorial/pick-the-rmw.md).
- Real hardware: same code runs against ThreadX vendor BSPs (Renesas
  Synergy, MIMXRT, etc.); replace the QEMU board crate with a vendor
  board crate.

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples as
  image rows of behaviour bringups.
