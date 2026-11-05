# FreeRTOS (QEMU MPS2-AN385)

<!-- type: how-to · level: 2 -->

> **Support level:** [`mps2-an385-freertos`](../reference/boards/mps2-an385-freertos.md), [`freertos-posix`](../reference/boards/freertos-posix.md), [`s32z270-freertos`](../reference/boards/s32z270-freertos.md), [`mps3-an536-freertos`](../reference/boards/mps3-an536-freertos.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

Single-node starter on FreeRTOS + lwIP, cross-compiled for Cortex-M3
and booted in QEMU MPS2-AN385. Slirp networking; no host TAP /
bridge / sudo. Rust, C, and C++ talkers are on
[GitHub](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos)
as reading material; this page scaffolds your own.

> **Prereqs.** `nros setup mps2-an385-freertos` is the single command
> that prepares your machine for this board. It fetches a prebuilt
> toolchain set into the shared store at `~/.nros/sdk` — the
> `arm-none-eabi-gcc` cross-compiler, the patched
> `qemu-system-arm` emulator, and the FreeRTOS kernel + lwIP sources.
> You do **not** hand-install a cross-toolchain. The zenoh path DOES
> need a ROS 2 install for the router (`ros2 run rmw_zenoh_cpp
> rmw_zenohd`); the xrce path needs only the Micro-XRCE-DDS agent,
> which `nros setup` installs.

## Setup

With the `nros` CLI on PATH ([Installation](../get-started/installation.md) —
`install.sh` puts it in `~/.nros/bin`), provision the board (`--rmw` defaults to `zenoh`; pick `xrce`
or `cyclonedds` to match the example you intend to run):

```bash probe=30
nros setup mps2-an385-freertos --rmw zenoh
```

This fetches the cross-compiler, the patched `qemu-system-arm`, and the
FreeRTOS + lwIP sources into `${NROS_HOME:-~/.nros}/sdk` (plus the
Micro-XRCE-DDS agent when you pick `--rmw xrce`). The zenoh router is
NOT installed — it comes from your ROS 2 install.

## Project layout

`nros new` scaffolds a workspace for the board, in any of the three languages:

```bash probe=40 variant=c
nros new fr_ws --board mps2-an385-freertos --template talker --lang c      # or --lang cpp
cd fr_ws
```

```bash probe=40 variant=rust
nros new fr_ws --board mps2-an385-freertos --template talker --lang rust
cd fr_ws
```

```text
fr_ws/
└── src/
    ├── fr_node/                 # node package: package.xml + CMakeLists.txt (or Cargo.toml), the talker
    └── fr_node_bringup/         # package.xml and system.toml: the board, the RMW, the node
```

`fr_node` is a library: no `main()`, no board, no RMW. The bringup's
`system.toml` holds one image, `[image.freertos]`, with `board =
"mps2-an385-freertos"`. The program's entry, the FreeRTOS startup task that
runs your node, is generated from that image at build time.

Nothing else states the deployment. The Rust side has no `.cargo/` to edit —
the `thumbv7m-none-eabi` triple, the `-Tmps2_an385.ld --nmagic --gc-sections`
link group, the QEMU runner and the `[patch.crates-io]` rows all come from the
board and are written by `nros build` under `build/`. A C or C++ node package
has no `<nano_ros deploy=… board=… rmw=…/>` tuple in its `package.xml` and
names no board in its `CMakeLists.txt`.

The board crate (`nros-board-mps2-an385-freertos`) wraps the kernel +
lwIP + LAN9118 driver build; the generated entry depends on it, so
nothing you write names it.

## Configure

Board, RMW, domain and network identity are declared in the bringup's
`system.toml` and **baked at compile time** — there is no config file on the
device. With a `locator` and an address added,
`src/fr_node_bringup/system.toml` reads:

```toml
[system]
name = "fr_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "fr_node"
class = "fr_node::Talker"
name = "fr_node"

[image.freertos]
board = "mps2-an385-freertos"
locator = "tcp/10.0.2.2:7800"
os_config = ["board-net.toml"]    # [eth0] network = "qemu", host = 15
```

`locator` is where the router is (the QEMU guest reaches the host's router
through the Slirp gateway `10.0.2.2`); `os_config` names the board network
file, `board-net.toml` beside `system.toml` in the bringup. The file is the
same whatever language the node is written in. Nothing about the deployment
lives in `Cargo.toml`, in `CMakeLists.txt`, or in the `package.xml`
`<export>` block.

Switching board is editing `[image.<id>] board` and rebuilding: `nros
build` picks the cross-toolchain file the board names, and the generated entry
brings the board crate, its RMW and `nros-platform`'s FreeRTOS feature.

Task stacks / priorities come from the board crate's defaults (Cargo
features), not a config file — see the
[Configuration Guide](../build/configuration.md).

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

Ports: read your image's own `locator`. The image above dials host port
**7800**; with no `locator`, as scaffolded, an image dials the board's default
(`tcp/10.0.2.2:7447`).

> Changing this in the tree: see [Internals → per-platform lanes](../internals/platform/platform-lanes.html#freertos).

## Build

The build generates the message bindings from your ROS 2 install, so source
it in this shell first. This first build records the ROS 2 prefix in the
workspace, so later builds and runs of it need no sourced shell.

In the workspace root (`fr_ws/`, where `nros new` left you), whatever the
language:

```bash probe=50
source /opt/ros/humble/setup.bash
nros build
```

`nros build` prepares the build first (message bindings, the resolved launch
model, the generated entry for `[image.freertos]`), then hands off to cargo or
CMake with the board's cross-toolchain file and the SDK root. There is no step
to run before it.

The first build cross-compiles nano-ros's dependencies and the FreeRTOS kernel
+ lwIP (~3 min for C / C++, ~5 min for Rust). The ELF lands under `build/`.
`nros build --dry-run` prints the native commands without building.

After a successful build `nros build` prints what it built and the
command that runs it — a block like

```text
nros build: next for `fr_node_bringup:freertos`:
  artifact: /home/you/fr_ws/build/…/freertos_entry
  run:
    nros run fr_node
```

**Driving the native tool yourself** is the escape hatch, not the path.
`nros build --dry-run` prints the exact `cargo build …` or `cmake` lines, run
against the generated entry under `build/`. To write that entry without
building (an IDE, a CI cache step), run:

```bash
nros configure
```

For a CMake tree that is
not a nano-ros workspace at all, see
[Build as a CMake subdirectory](../build/cmake-subdirectory.md).

See
[When `nros build` drives the build](how-integration-works.md#when-nros-build-drives-the-build)
for which tool builds each image.

## Run on QEMU

Three terminals: the router, the board, and a stock ROS 2 peer.

**1. The router** — ROS's `rmw_zenohd`, on the port your image's `locator`
names (7447 when it names none, as a scaffolded image; 7800 in the
example shown above). It must listen on `0.0.0.0` for the Slirp gateway:
Slirp forwards the guest's `10.0.2.2:<p>` to the host's `<p>`.

```bash probe=60 probe-run=60 probe-keep
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60
Started Zenoh router with id
```

**2. The board** — from the workspace root:

```bash probe=70 probe-run=120 probe-keep
nros run fr_node
```

`nros run` rebuilds if anything changed, then boots the image in QEMU with the
board's runner: the store's patched `qemu-system-arm` (`nros setup
mps2-an385-freertos` installed it), the LAN9118 on Slirp, and `-icount
shift=auto`, which keeps the board's timer in step with Slirp's I/O. It streams
the semihosting console until you press Ctrl-C.

The scaffolded talker publishes `std_msgs/String` once a second, in every
language:

```text probe-expect=70
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

**3. A stock ROS 2 peer.** The talker publishes best-effort; stock `ros2 topic
echo` defaults to RELIABLE, so the QoS-mismatched echo silently delivers
nothing. Force best-effort to receive:

```bash probe=80 probe-run=90
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

It prints a `data:` line per message:

```text probe-expect=80
data:
```

Ctrl-C stops `nros run` and QEMU with it.

**Readiness signal.** Within ~20 seconds of QEMU boot, the image
prints `Publishing: 'Hello World: 1'` on its semihosting stdout — a
`std_msgs/String` counted from 1, like the official ROS 2 demo talker. QEMU cold-boot through FreeRTOS init + lwIP DHCP + zenoh
session open typically takes 10–15 s. If no `Publishing:` line in
30 seconds:

1. Confirm the router is running on the host on the port the image
   dials — the `locator` in its `system.toml` (Slirp forwards
   `10.0.2.2:<p>` → host:<p>). Without it the talker makes one
   connection attempt and stops with `Failed to open session`; a
   board that should wait for a late router sets
   `[knobs.zenoh.limits] connect_forever = true` (or
   `connect_timeout_ms` for a bounded wait).
2. Check the talker's early log for `lwIP DHCP timeout` or
   `Failed to open session`.
3. Bridge tip: `ros2 topic echo /chatter` from a stock ROS 2
   install (with `RMW_IMPLEMENTATION=rmw_zenoh_cpp`) confirms
   end-to-end interop.
4. See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

## Run on the POSIX simulator

The `freertos-posix` board runs the FreeRTOS kernel as an ordinary
Linux process (FreeRTOS's POSIX port) — no QEMU, host networking. It is
a C / C++ board (it has no Rust entry shape). Provision it, point the
image of a C or C++ workspace at it, and run:

```bash
nros setup freertos-posix --rmw zenoh
# in src/fr_node_bringup/system.toml:  [image.freertos] board = "freertos-posix"
nros run fr_node
```

The image dials the router on the host directly (no Slirp gateway), so
start `rmw_zenohd` on the port its `locator` names, as in
[Run on QEMU](#run-on-qemu) step 1.

## Flash a real board

No in-tree FreeRTOS board is flashed by a documented flow, and none is
claimed: the MPS2-AN385 boards above are QEMU machines.

* **NXP S32Z270 (`s32z270-freertos`; its status is on
  [its board page](../reference/boards/s32z270-freertos.md)).** No
  emulator models its Cortex-R52 RTU, so nothing here boots it. It
  builds like the other boards — the licensed pieces (NXP's
  `GCC/ARM_CR52_GIC` FreeRTOS port, the RTD NETC Ethernet driver, the
  S32 Config Tools `PBcfg`) are yours to supply, as its
  [descriptor](https://github.com/NEWSLabNTU/nano-ros/blob/main/packages/boards/nros-board-s32z270-freertos/nros-board.toml)
  explains — and the ELF is flashed with your own S32 tooling (S32
  Design Studio, a Lauterbach/PEmicro probe); nano-ros adds no flash
  step:

  ```bash
  nros setup s32z270-freertos --rmw zenoh
  # in src/fr_node_bringup/system.toml, a second image:
  #   [image.s32z] board = "s32z270-freertos"
  nros build s32z
  # → nros build prints the ELF path; flash it with your S32 tooling
  ```

* **Your own FreeRTOS board** is a board package of your own — see
  [Adding a FreeRTOS Board](../extending/freertos-board.md) — and is
  flashed with whatever that board's vendor uses (probe-rs, OpenOCD, a
  vendor IDE).

Neither flash path is run by any probe (a CI runner has no board): both
stand unverified until run against a named board. How often the QEMU line
above runs is on each board's page under
[Supported Boards](../reference/supported-boards.md).

## GitHub source

The copy-out talkers for this board, one per language. Each is the same
project this page scaffolds: one node package and a bringup with no launch
file of its own.

- Rust: [`examples/mps2-an385-freertos/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/rust/talker)
- C: [`examples/mps2-an385-freertos/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/c/talker)
- C++: [`examples/mps2-an385-freertos/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/cpp/talker)

## Next

- Subscriber, services and actions: the `freertos_*` image rows of the
  language workspaces, `examples/workspaces/{rust,c,cpp}`, one bringup per
  behaviour (`pubsub_bringup`, `service_bringup`, `action_bringup`).
  `nros run pubsub_bringup:freertos_listener` runs the listener;
  `freertos_listener_xrce` is the same role over XRCE.
- No RTOS: the [Bare-metal Cortex-M3](./bare-metal.md) page is the
  same board without FreeRTOS.
- Your own board / RTOS: the
  [Board Integration matrix](../extending/board-integration.md) maps each
  user profile (Cargo-first, vendor-IDE, Zephyr, NuttX, ESP32-C3, niche fork)
  to the shortest bring-up path.
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/testing/freertos-lan9118-debugging.html).

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples as
  image rows of behaviour bringups.
