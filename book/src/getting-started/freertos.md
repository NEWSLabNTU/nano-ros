# FreeRTOS (QEMU MPS2-AN385)

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

```bash
nros setup mps2-an385-freertos --rmw zenoh
```

This fetches the cross-compiler, the patched `qemu-system-arm`, and the
FreeRTOS + lwIP sources into `${NROS_HOME:-~/.nros}/sdk` (plus the
Micro-XRCE-DDS agent when you pick `--rmw xrce`). The zenoh router is
NOT installed — it comes from your ROS 2 install (RFC-0075).

## Project layout

**C / C++** is a single package — `nros new` scaffolds it:

```bash
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

The scaffold writes `board = "freertos"`, a board *alias*; the C/C++
build road needs the board's full name, so edit `system.toml` to:

```toml
[image.freertos]
board = "mps2-an385-freertos"
```

**Rust** on FreeRTOS is a node package plus a generated entry, so it
starts as a workspace (single-package Rust scaffolding is not offered
for FreeRTOS — `nros new` says so and names the alternative):

```bash
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

C / C++, in the scaffolded package. No `nros sync`: a C or C++ leaf's
message bindings are a CMake-time output. `nros build` passes the
board's cross-toolchain file and the SDK root:

```bash
cd fr_talker
nros build
```

Rust, in the workspace. `nros sync` first: it writes the generated
message bindings and the resolved launch model the entry is built from.
Without it the build stops at its preflight naming sync:

```bash
cd fr_ws
nros sync
nros build freertos
```

First Rust build pulls + cross-compiles deps (~5 min); the ELF lands at
`build/freertos-zenoh/freertos_entry/target/thumbv7m-none-eabi/debug/freertos_entry`
(`nros build freertos --dry-run` prints the paths without building). The
C / C++ build lands under `build/freertos/cmake/` and also compiles the
FreeRTOS kernel + lwIP — first run ~3 min.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is
supported: run `nros sync` first, then point cargo at the generated
settings file — `nros build freertos --dry-run` prints the exact
`cargo build --manifest-path … --config …/nros-cargo.toml` line.

See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md)
for why the sync requirement is per language rather than per platform.

## Run

```bash
# 1. Start the router (ROS's `rmw_zenohd`) on the host, on the port
#    your image's `locator` names (7447 by default; 7800 in the example
#    shown above). It must listen on 0.0.0.0 for the Slirp gateway:
#    Slirp forwards guest 10.0.2.2:<p> → host:<p>.
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. Boot the talker in QEMU. Invoke qemu-system-arm directly, with the
#    LAN9118 + Slirp wiring the example expects — the board's own cargo
#    runner is bare `-kernel`, so booting through it gives you QEMU
#    without networking:
cd fr_ws
"$(nros sdk-path qemu)/bin/qemu-system-arm" -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/freertos-zenoh/freertos_entry/target/thumbv7m-none-eabi/debug/freertos_entry \
    -nic user,model=lan9118

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

QEMU exits via Ctrl-A x.

> **Contributors:** the in-tree fixture run/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

**Readiness signal.** Within ~20 seconds of QEMU boot, the talker
should print `Publishing: 'Hello World: 1'` on its semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. QEMU cold-boot through FreeRTOS init + lwIP DHCP + zenoh
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

## GitHub source

The full examples, to read:

- Rust: [`examples/mps2-an385-freertos/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/rust/talker)
- C: [`examples/mps2-an385-freertos/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/c/talker)
- C++: [`examples/mps2-an385-freertos/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/cpp/talker)

## Next

- Subscriber: peer `listener/` directory next to each talker.
- Services + actions: peer `service-*/` and `action-*/` directories.
- Real hardware: same code runs on STM32F4-Discovery /
  NXP-LPC55S69 / TI-MSP432 with a different board crate + linker
  script; see the [Bare-metal Cortex-M3](./bare-metal.md) page for
  the no-RTOS variant.
- Your own board / RTOS: the
  [Board Integration matrix](../concepts/board-integration.md) maps each
  user profile (Cargo-first, vendor-IDE, Zephyr, NuttX, ESP32-C3, niche fork)
  to the shortest bring-up path.
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/freertos-lan9118-debugging.md).
