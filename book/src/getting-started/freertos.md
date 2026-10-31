# FreeRTOS (QEMU MPS2-AN385)

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

With the `nros` CLI on PATH ([Installation](./installation.md) —
`install.sh` puts it in `~/.nros/bin`), provision the board (`--rmw` defaults to `zenoh`; pick `xrce`
or `cyclonedds` to match the example you intend to run):

```bash probe=30
nros setup mps2-an385-freertos --rmw zenoh
```

This fetches the cross-compiler, the patched `qemu-system-arm`, and the
FreeRTOS + lwIP sources into `${NROS_HOME:-~/.nros}/sdk` (plus the
Micro-XRCE-DDS agent when you pick `--rmw xrce`). The zenoh router is
NOT installed — it comes from your ROS 2 install (RFC-0075).

## Project layout

**C / C++** is a single package — `nros new` scaffolds it:

```bash probe=40 variant=c
nros new fr_talker --platform freertos --lang c      # or --lang cpp
cd fr_talker
```

```text
fr_talker/
├── system.toml          # WHAT this deploys to — the one file you edit
├── CMakeLists.txt       # find_package(nano_ros) + targets; no board facts
├── package.xml
└── src/main.c
```

The scaffold's image is `[image.freertos]` with `board =
"mps2-an385-freertos"` — the platform's one board, which `nros new` picks
for you (`--board` names another).

**Rust** on FreeRTOS is a node package plus a generated entry, so it
starts as a workspace (single-package Rust scaffolding is not offered
for FreeRTOS — `nros new` says so and names the alternative):

```bash probe=40 variant=rust
nros new fr_ws --workspace --lang rust --rmw zenoh
cd fr_ws
nros new --image freertos --board mps2-an385-freertos   # adds [image.freertos]
```

Either way the deployment is stated in a `system.toml` (the single
package's own, or the workspace bringup's), and nothing else does. The
Rust side has no `.cargo/` to edit — the `thumbv7m-none-eabi` triple,
the `-Tmps2_an385.ld --nmagic --gc-sections` link group, the QEMU
runner and the `[patch.crates-io]` rows all come from the board and are
written by `nros sync` / `nros build` into the image's
`nros-cargo.toml` under `build/` (RFC-0098). The C / C++ leaves have no
`<nano_ros deploy=… board=… rmw=…/>` tuple in their `package.xml`:
`find_package(nano_ros)` reads `system.toml` instead, and derives the
platform from the board.

The board crate (`nros-board-mps2-an385-freertos`) wraps the kernel +
lwIP + LAN9118 driver build; the generated entry depends on it, so
nothing you write names it.

## Configure

Board, RMW, domain and network identity are declared in `system.toml` and
**baked at compile time** — there is no config file on the device. For
example,
[`examples/mps2-an385-freertos/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/rust/talker/system.toml)
on GitHub:

```toml
[system]
name = "freertos_rs_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "freertos_rs_talker"
class = "freertos_rs_talker::Talker"
name = "talker"

[image.mps2-an385-freertos]
board = "freertos"
locator = "tcp/10.0.2.2:7800"
os_config = ["board-net.toml"]    # [eth0] network = "qemu", host = 15
```

Your image takes the same keys: add `locator` (the QEMU guest reaches the
host's router through the Slirp gateway `10.0.2.2`) and `os_config` (the
board network file) to the `[image.freertos]` table `nros new` wrote.
The C and C++ examples carry the *same* file, in the same schema — only
the names and the locator differ. Nothing about the deployment lives in
`Cargo.toml`, in `CMakeLists.txt`, or in the `package.xml` `<export>`
block.

Switching board is editing `[image.<id>] board` and rebuilding: `nros
build` picks the cross-toolchain file the board names, and for Rust it
regenerates the `<pkg>_nros_selection` package that carries the board
crate, its RMW and `nros-platform`'s FreeRTOS feature
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

Task stacks / priorities come from the board crate's defaults (Cargo
features), not a config file — see the
[Configuration Guide](../user-guide/configuration.md).

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

Ports: read your image's own `locator`. The example Rust talker dials
host port **7800**; with no `locator` an image dials the board's default
(`tcp/10.0.2.2:7447`).

> **Contributors:** the prebuilt test fixtures bake different ports —
> see [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

## Build

C / C++, in the scaffolded package (`fr_talker/`, where `nros new` left
you). No `nros sync`: a C or C++ leaf's message bindings are a CMake-time
output. `nros build` passes the board's cross-toolchain file and the SDK
root:

```bash probe=50 variant=c
nros build
```

Rust, in the workspace (`fr_ws/`). `nros sync` first: it writes the
generated message bindings and the resolved launch model the entry is
built from. Without it the build stops at its preflight naming sync:

```bash probe=50 variant=rust
nros sync
nros build freertos
```

First Rust build pulls + cross-compiles deps (~5 min); the ELF lands at
`build/freertos-zenoh/freertos_entry/target/thumbv7m-none-eabi/debug/freertos_entry`.
The C / C++ build lands at `build/freertos/cmake/fr_talker` (`build/<image>/cmake/`
plus the name `nano_ros_add_executable` gives it) and also compiles the
FreeRTOS kernel + lwIP — first run ~3 min. `nros build <image> --dry-run`
prints the native commands without building.

After a successful build `nros build` prints what it built and the
command that runs it — a block like

```text
nros build: next for `fr_talker:freertos`:
  artifact: /home/you/fr_talker/build/freertos/cmake/fr_talker
  run:
    /home/you/.nros/sdk/qemu/<version>/bin/qemu-system-arm -cpu cortex-m3 -machine mps2-an385 …
```

That `run` line is the board's own QEMU runner with the store's
`qemu-system-arm` filled in; QEMU's default user-mode network lands on the
board's LAN9118, so it reaches a router on the host. The line under
[Run on QEMU](#run-on-qemu) below is the one the test harness boots: it adds
`-icount shift=auto`, which keeps the board's timer in step with Slirp's
I/O, and names the NIC explicitly.

**Driving the native tool yourself** is the escape hatch, not the path.
For cargo (an IDE, a CI step, `--release`): run `nros sync` first, then
point cargo at the generated settings file — `nros build freertos
--dry-run` prints the exact `cargo build --manifest-path … --config
…/nros-cargo.toml` line. For CMake, `nros build --dry-run` in the C /
C++ package prints the configure line it would run; its core is

```bash
cmake -S . -B build/freertos/cmake -DNROS_IMAGE=freertos \
    -DCMAKE_TOOLCHAIN_FILE="$(nros sdk-root)/cmake/toolchain/arm-freertos-armcm3.cmake" \
    -Dnano_ros_ROOT="$(nros sdk-root)"
cmake --build build/freertos/cmake
```

plus the board's RX-timing knobs, which the dry-run spells out. Without
a `system.toml` at all, see
[Build as a CMake subdirectory](./build-as-subdirectory.md).

See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md)
for why the sync requirement is per language rather than per platform.

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

**2. The board** — the store's patched `qemu-system-arm` (`nros setup
mps2-an385-freertos` installed it), with the LAN9118 on Slirp and the
`-icount` the test harness uses. From the workspace (`fr_ws/`):

```bash probe=70 probe-run=120 probe-keep variant=rust
"$(nros sdk-path qemu)/bin/qemu-system-arm" -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/freertos-zenoh/freertos_entry/target/thumbv7m-none-eabi/debug/freertos_entry \
    -nic user,model=lan9118
```

The workspace image runs the scaffold's talker and listener in one board,
so it prints both sides of `/chatter` (a `std_msgs/Int32` counter from 0):

```text probe-expect=70 variant=rust
Network ready.
[INFO] Publishing: 0
[INFO] I heard: 0
[INFO] Publishing: 1
[INFO] I heard: 1
...
```

The C / C++ package's image boots with the same flags and its own ELF, from
`fr_talker/`:

```bash probe=70 probe-run=120 probe-keep variant=c
"$(nros sdk-path qemu)/bin/qemu-system-arm" -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/freertos/cmake/fr_talker \
    -nic user,model=lan9118
```

The scaffolded C talker is the in-tree
[example's `main.c`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/c/talker/src/main.c),
publishing `std_msgs/String` once a second:

```text probe-expect=70 variant=c
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

**3. A stock ROS 2 peer.** The talker publishes best-effort; stock `ros2 topic
echo` defaults to RELIABLE, so the QoS-mismatched echo silently delivers
nothing. Force best-effort to receive:

```bash probe=80 probe-run=90 variant=rust
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

The C talker publishes `std_msgs/String`:

```bash probe=80 probe-run=90 variant=c
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

Either prints a `data:` line per message:

```text probe-expect=80
data:
```

QEMU exits via Ctrl-A x.

> **Contributors:** the in-tree fixture run/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

**Readiness signal.** Within ~20 seconds of QEMU boot, the workspace
image prints `[INFO] Publishing: 0` on its semihosting stdout (the
[GitHub examples](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos)
print `Publishing: 'Hello World: 1'` instead — a `std_msgs/String`
counted from 1, like the official ROS 2 demo talker). QEMU cold-boot through FreeRTOS init + lwIP DHCP + zenoh
session open typically takes 10–15 s. If no `Publishing:` line in
30 seconds:

1. Confirm the router is running on the host on the port the image
   dials — the `locator` in its `system.toml` (Slirp forwards
   `10.0.2.2:<p>` → host:<p>). Without it the talker retries the
   zenoh handshake until QEMU is killed.
2. Check the talker's early log for `lwIP DHCP timeout` or
   `Failed to open session`.
3. Bridge tip: `ros2 topic echo /chatter` from a stock ROS 2
   install (with `RMW_IMPLEMENTATION=rmw_zenoh_cpp`) confirms
   end-to-end interop.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Run on the POSIX simulator

The `freertos-posix` board runs the FreeRTOS kernel as an ordinary
Linux process (FreeRTOS's POSIX port) — no QEMU, host networking. It is
a C / C++ board (it has no Rust entry shape). Provision it, point the
package's image at it, build, and run the binary:

```bash
nros setup freertos-posix --rmw zenoh
# in system.toml:  [image.freertos] board = "freertos-posix"
cd fr_talker
nros build
./build/freertos/cmake/fr_talker
```

The image dials the router on the host directly (no Slirp gateway), so
start `rmw_zenohd` on the port its `locator` names, as in
[Run on QEMU](#run-on-qemu) step 1.

## Flash a real board

No in-tree FreeRTOS board is flashed by a documented flow, and none is
claimed: the MPS2-AN385 boards above are QEMU machines.

* **NXP S32Z270 (`s32z270-freertos`, [Builds](../reference/boards/s32z270-freertos.md)) is build-only.** No
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
  # in system.toml:  [image.s32z] board = "s32z270-freertos"
  nros build s32z
  # → build/s32z/cmake/<your executable>   (flash it with your S32 tooling)
  ```

* **Your own FreeRTOS board** is a board package of your own — see
  [Adding a FreeRTOS Board](../porting/freertos-board.md) — and is
  flashed with whatever that board's vendor uses (probe-rs, OpenOCD, a
  vendor IDE).

Neither flash path is run by any probe (a CI runner has no board): both
stand unverified until run against a named board. The QEMU line above is
what the weekly probe tracks boot.

## GitHub source

The full examples, to read:

- Rust: [`examples/mps2-an385-freertos/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/rust/talker)
- C: [`examples/mps2-an385-freertos/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/c/talker)
- C++: [`examples/mps2-an385-freertos/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/cpp/talker)

## Next

- Subscriber: peer `listener/` directory next to each talker.
- Services + actions: peer `service-*/` and `action-*/` directories.
- No RTOS: the [Bare-metal Cortex-M3](./bare-metal.md) page is the
  same board without FreeRTOS.
- Your own board / RTOS: the
  [Board Integration matrix](../concepts/board-integration.md) maps each
  user profile (Cargo-first, vendor-IDE, Zephyr, NuttX, ESP32-C3, niche fork)
  to the shortest bring-up path.
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/freertos-lan9118-debugging.md).
