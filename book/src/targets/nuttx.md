# NuttX (apps/external)

> **Support level:** [`qemu-armv7a-nuttx`](../reference/boards/nuttx-arm.md), [`rv-virt-nuttx`](../reference/boards/nuttx-riscv.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

Single-node starter on NuttX. nano-ros plugs into the standard NuttX
app discovery as an external app under `apps/external/nano-ros/`,
exposing Kconfig knobs under `Application Configuration → External
Modules → nano-ros`. Use this entry when your NuttX board ships its
own kernel build and you want to add ROS 2 communication.

> Changing this in the tree: see [Internals → in-tree examples](../internals/platform/nuttx-in-tree.html).

> **Prereqs.** Install the `nros` CLI once per machine, then run
> `nros setup qemu-armv7a-nuttx --rmw <zenoh|xrce|cyclonedds>` (`--rmw`
> defaults to `zenoh`). This fetches a mostly-prebuilt toolchain set into
> `${NROS_HOME:-~/.nros}/sdk` — the NuttX cross-compiler, the emulator and the
> NuttX sources — so you do **not** hand-install a cross-toolchain. The zenoh
> router is ROS's `rmw_zenohd` and comes from a ROS 2 install; the
> xrce path needs only the Micro-XRCE-DDS agent, which `nros setup … --rmw
> xrce` installs:
>
> ```bash
> nros setup qemu-armv7a-nuttx --rmw zenoh
> ```
>
> (`nros` is the installed CLI — [Installation](../get-started/installation.md).)
>
> The NuttX-managed road below (`apps/external`, NuttX's own `make`) also
> needs a NuttX ≥ nuttx-12 checkout of your own with an `apps/` sibling;
> the `nros build` road does not.

## Project layout

NuttX's "external apps" pattern places the app shim under
`$NUTTX_APPS_DIR/external/<name>/`:

```text
$NUTTX_DIR/                              # NuttX kernel checkout
$NUTTX_APPS_DIR/                             # sibling: apps tree
└── external/
    └── nano-ros/                        # symlink to the SDK root's
        ├── Make.defs                    #   integrations/nuttx/
        ├── Makefile
        ├── CMakeLists.txt               #   (cmake-driven NuttX builds)
        └── Kconfig
nx_ws/                                   # your application: a workspace
└── src/
    ├── nx_node/                         # node package: package.xml, Cargo.toml | CMakeLists.txt, the node
    └── nx_node_bringup/                 # package.xml and system.toml: the board, the RMW, the node
```

Wire the shell into your NuttX apps tree. Two commands, both run once:

```bash
nros setup qemu-armv7a-nuttx --rmw zenoh    # toolchain + NuttX sources + SDK
sdk="$(nros locate nano-ros)"   # this project's nano-ros SDK root
"$sdk/scripts/nuttx/stage-external-apps.sh" "$NUTTX_APPS_DIR"
```

The staging script writes `$NUTTX_APPS_DIR/external/Make.defs` + `Kconfig`
and symlinks the integration shell in as `external/nano-ros`. (The SDK
root carries `integrations/` in releases cut after 0.6.0-nros4.) Menuconfig
surfaces it under `Application Configuration → External Modules`.

The script stages the integration shell only. A bringup with several
components is not built through the apps tree: give it an `[image.<id>]` with
`board = "qemu-armv7a-nuttx"` and run `nros build` — see `[image.nuttx]` in
[`examples/workspaces/rust`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/workspaces/rust/src/demo_bringup/system.toml). The script's old `--bringup <dir>` / `--image <id>` flags
staged a per-bringup app that could not build an image, and now refuse.

If you'd rather wire it yourself (e.g. into a vendored apps tree):

```bash
ln -s "$sdk/integrations/nuttx" \
      $NUTTX_APPS_DIR/external/nano-ros
# then copy integrations/nuttx/external-Make.defs.in →
# $NUTTX_APPS_DIR/external/Make.defs and add a matching
# $NUTTX_APPS_DIR/external/Kconfig that `source`s the shell.
```

## Configure

NuttX uses Kconfig as its single source of truth. After the symlink
above:

```bash
cd $NUTTX_DIR
make menuconfig
# Navigate to:
#   Application Configuration → External Modules → nano-ros
#   [*] nano-ros ROS 2 client
#       RMW backend  →  zenoh           # zenoh | xrce | cyclonedds
#       ROS 2 edition →  humble
```

Networking Kconfig requirements live under
`Networking Support` — enable `CONFIG_NET`, `CONFIG_NET_TCP`,
`CONFIG_NET_IPv4`. For QEMU `nsh_smp` configurations the defaults
already include these.

Those Kconfig knobs govern what the integration shell compiles into
`libnros_c.a` — NuttX's build owns that, the way Zephyr's Kconfig owns its
lane. They are not where *your application* states its deployment, and the two
must name the same RMW.

Your *application's* deployment — board, RMW, domain id, and an optional
`locator` — is declared in the bringup's `system.toml` and baked at compile
time. With a `locator` added, `src/nx_node_bringup/system.toml` reads:

```toml
[system]
name = "nx_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "nx_node"
class = "nx_node::Talker"
name = "nx_node"

[image.nuttx]
board = "qemu-armv7a-nuttx"
locator = "tcp/10.0.2.2:8200"
```

The file is the same whatever language the node is written in — a C or C++
node's `package.xml` has no `<nano_ros …/>` tuple and its `CMakeLists.txt`
names no board. Nor does anything here name a target triple:
`armv7a-nuttx-eabihf` is a fact about the board, and it lives in the board
descriptor. The next build picks an edit to this file up, regenerating the
message bindings and the build settings the board choice implies.

The guest network shape (eth0 `10.0.2.15`, Slirp gateway `10.0.2.2` — QEMU's
own network, which the board's `[board.net] network = "qemu"` names)
comes from the board; add a `locator = "tcp/10.0.2.2:<port>"`
field to `[image.<id>]` to dial a non-default router port. The prebuilt *test fixtures*
bake distinct per-language allocator ports so parallel suites don't
collide on one router. Start the router (ROS's `rmw_zenohd`) on the
port your app dials — it must listen on `0.0.0.0`, not loopback,
because the guest reaches it through QEMU's Slirp gateway:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:8200"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

## Build

Two roads, and they differ in who owns the NuttX kernel build.

**`nros build` — the path for a nano-ros workspace.** The board in the
bringup (`qemu-armv7a-nuttx` for Arm `virt`, `rv-virt-nuttx` for RISC-V
`rv-virt`) picks the cross toolchain and the NuttX board config; `nros build`
links your node into a bootable NuttX kernel ELF. Provision the board (no NuttX
checkout of your own is needed on this road — `nros setup` fetches the kernel
and apps sources), then scaffold a workspace for it and build:

```bash probe=30 variant=arm,arm-rust
nros setup qemu-armv7a-nuttx --rmw zenoh --sudo
```

The NuttX kernel build needs two host packages a fresh Debian/Ubuntu host may
lack — the Python venv module and `unzip`. `nros setup` names any that are
missing before fetching anything; `--sudo` lets it run the one install command
it composed for your package manager (`sudo apt-get install -y python3-venv
unzip` here) before the download starts. Leave `--sudo` off and it only prints
that command, and never runs `sudo` for you.

The build generates the message bindings from your ROS 2 install, so source
it in this shell first. This first build records the ROS 2 prefix in the
workspace, so later builds and runs of it need no sourced shell.

```bash probe=40 variant=arm
source /opt/ros/humble/setup.bash
nros new nx_ws --board qemu-armv7a-nuttx --template talker --lang c    # or --lang cpp
cd nx_ws
nros build
```

A Rust node is the same scaffold with `--lang rust`. For it, `nros build`
configures and exports the NuttX kernel from the board's defconfig first (the
same step the C/C++ road runs inside its configure; a current export is a
no-op), then builds the entry with the board's pinned nightly:

```bash probe=40 variant=arm-rust
source /opt/ros/humble/setup.bash
nros new nx_ws --board qemu-armv7a-nuttx --template talker --lang rust
cd nx_ws
nros build
```

Either way `nros build` prepares the build itself (message bindings, the
generated entry), and the kernel ELF lands under `build/`; `nros build` prints
its path with the QEMU line that boots it. `nros build --dry-run` shows the
native commands.

**NuttX's own build — the integration shell in a NuttX-managed tree.** NuttX
owns the build verb there, `make` from the configured kernel, the way Zephyr
owns `west build`. NuttX's `make` does not prepare a nano-ros workspace, so
prepare it first, in the workspace, because the generated message bindings must
exist before anything compiles against them:

```bash
cd nx_ws
nros configure                      # bindings + settings from the bringup

cd $NUTTX_DIR
make                                # full kernel + apps build
```

The Cargo build of nano-ros's Rust staticlibs runs as a sub-step of
the NuttX app build; `libnros_c.a` is linked at the final app
link stage. The kernel ELF is `$NUTTX_DIR/nuttx`.

For a CMake-driven NuttX build, NuttX's own CMake takes one
`BOARD_CONFIG=<board>:<config>` and finds `apps/` beside the kernel tree
(`-DNUTTX_APPS_DIR=<dir>` names another):

```bash
cmake -S "$NUTTX_DIR" -B build -DBOARD_CONFIG=qemu-armv7a:nsh -GNinja
cmake --build build                 # the ELF is build/nuttx
```

Stock `qemu-armv7a:nsh` has no networking; merge the nano-ros requirements
into it as described under
[Auto-configure glue](#auto-configure-glue-nsh-built-in-registration) below.

## Run on QEMU (Arm virt)

QEMU is the store's build (`nros setup qemu-armv7a-nuttx` installed it), on
QEMU's `virt` machine with virtio-net on Slirp — the line `nros build`
prints. Two terminals: the router, then the board.

**1. The router** on the host (Slirp forwards `10.0.2.2:<port>` → host). It
must listen on `0.0.0.0` for the Slirp gateway; match whatever `locator` your
app bakes — 7447 when it states none, as a scaffolded image; 8200 in the
example above:

```bash probe=60 probe-run=60 probe-keep
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60
Started Zenoh router with id
```

**2. The board** — from the workspace root:

```bash probe=70 probe-run=120 variant=arm,arm-rust
nros run nx_node
```

`nros run` boots the kernel ELF with the board's runner: the store's
`qemu-system-arm` on the `virt` machine, virtio-net on Slirp, and `-icount
shift=auto`. (A make-built kernel is `$NUTTX_DIR/nuttx`; boot it with the same
QEMU line, which `nros build --dry-run` prints.)

An image `nros build` produced starts its application itself. The scaffolded
talker publishes `Hello World: N` once a second, in every language:

```text probe-expect=70 variant=arm,arm-rust
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

A stock ROS 2 peer can echo it:

```bash
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

A make-built kernel boots to the NSH prompt instead; run your app's PROGNAME
there — the `make`-driven build registers every nano-ros example as a built-in
command via Application.mk's `-Dmain=<PROGNAME>_main` rename, e.g.
`nsh> nuttx_c_talker` (C) or `nsh> nuttx_cpp_talker` (C++).

Ctrl-C stops `nros run` and QEMU with it (Ctrl-A x for a QEMU you started yourself).

**Readiness signal.** Once the image boots (a make-built kernel: once
you type the app's NSH command, e.g. `nuttx_c_talker`), expect its first
line on the NSH console within ~6 seconds — `Publishing: 'Hello World: 1'`
from the scaffolded and in-tree Rust, C and C++ talkers, which start the
count at 1 like the official ROS 2 demo talker. If no
`Publishing:` line:

1. Confirm the app actually ran — `ps` should show your task.
2. Confirm networking — `ifconfig` shows a configured interface.
   With the virtio-net + Slirp wiring above, `eth0` comes up at
   `10.0.2.15` (the board's default on QEMU's network; an image's `net` address overrides it).
3. Confirm `zenohd` reachable; the image's `locator` must match the
   router's listen port.
4. See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

## Run on QEMU (RISC-V rv-virt)

The `rv-virt-nuttx` board (`rv32imac`) boots on QEMU's RISC-V `virt` machine.
**The store's QEMU is built for Arm and RISC-V 64 only**, so this board needs
a `qemu-system-riscv32` from your distribution (Debian/Ubuntu:
`qemu-system-misc`):

```bash probe=30 variant=riscv,riscv-rust
nros setup rv-virt-nuttx --rmw zenoh --sudo
sudo apt-get install -y qemu-system-misc
```

Name it with `QEMU_SYSTEM_RISCV32` and `nros build` prints a line that uses
it; without it the printed line asks `nros locate tool qemu-system-riscv32`,
which then has nothing to answer. Scaffold for this board and build:

```bash probe=40 variant=riscv
source /opt/ros/humble/setup.bash
nros new nx_ws --board rv-virt-nuttx --template talker --lang c
cd nx_ws
export QEMU_SYSTEM_RISCV32="$(command -v qemu-system-riscv32)"
nros build
```

A Rust node builds for this board the same way as for Arm. The kernel step
uses rv-virt's own defconfig, and the target is the board's
`riscv32imac-unknown-nuttx-elf` spec, built with `build-std`:

```bash probe=40 variant=riscv-rust
source /opt/ros/humble/setup.bash
export QEMU_SYSTEM_RISCV32="$(command -v qemu-system-riscv32)"
nros new nx_ws --board rv-virt-nuttx --template talker --lang rust
cd nx_ws
nros build
```

Start the router as above (on the port your image's `locator` names), then
boot it; `nros run` uses the `qemu-system-riscv32` that `QEMU_SYSTEM_RISCV32`
names:

```bash probe=70 probe-run=120 variant=riscv,riscv-rust
nros run nx_node
```

```text probe-expect=70 variant=riscv,riscv-rust
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

## Auto-configure glue (NSH built-in registration)

The `make`-driven build above relies on a host-side glue layer. In-tree
that is `scripts/nuttx/stage-external-apps.sh` plus `kconfig-tweak`. If you
wire the integration shell into your own NuttX tree by hand, reproduce the same
three steps:

1. **Bring the required symbols into your config.** Stock NuttX
   `qemu-armv7a/nsh` ships without `CONFIG_NET=y`, virtio-net, or
   `TLS_NELEM`. If you have **no config of your own**, copy the board
   defconfig
   `packages/boards/nros-board-nuttx-qemu/nuttx-config/arm/defconfig`
   to `$NUTTX_DIR/.config` and run `make olddefconfig`. If you already
   maintain a vendor defconfig, do NOT clobber it — merge the nano-ros
   requirements into it instead (`CONFIG_NET=y`, the virtio-net /
   netdev options, `CONFIG_TLS_NELEM=8`, `CONFIG_LIBCXXTOOLCHAIN=y`,
   `CONFIG_ALLSYMS=n`; diff the board defconfig against `nsh` for the
   authoritative list) and re-run `make olddefconfig`.
2. **Stage the integration shell.** Run
   `scripts/nuttx/stage-external-apps.sh "$NUTTX_APPS_DIR"` to symlink
   `integrations/nuttx/` into `$NUTTX_APPS_DIR/external/`. Remove `$NUTTX_APPS_DIR/Kconfig` so
   NuttX's `mkkconfig.sh` rediscovers the new
   `apps/external/Kconfig`.
3. **Flip the nano-ros Kconfig knobs via `kconfig-tweak`.** The
   recipe enables `NROS`, `NROS_C_API`, `NROS_CPP_API`, every
   `NROS_EXAMPLE_<EX>_<LANG>`, sets `TLS_NELEM=8`, disables
   `LIBCXXNONE` + enables `LIBCXXTOOLCHAIN`, and disables
   `ALLSYMS` for the bootstrap link. Re-run `make olddefconfig`
   so the newly-visible dependencies settle, then `make`.

`kconfig-tweak` ships in the `kconfig-frontends` package on most
distros. Without it the recipe skips (`"NuttX skip: kconfig-tweak
not on PATH"`); install it before retrying.

## Flash a real board

nano-ros has no NuttX hardware board of its own: both boards above are QEMU
machines. On real hardware the image is an ordinary NuttX application, so it
goes to the board the way NuttX's documentation for that board says — build
the board's own `BOARD_CONFIG` with the integration shell staged as above,
then flash `nuttx` (or `nuttx.bin` / `nuttx.hex`) with the board's tool:

```bash
cmake -S "$NUTTX_DIR" -B build -DBOARD_CONFIG=<board>:<config> -GNinja
cmake --build build
openocd -f board/<your-board>.cfg -c "program build/nuttx verify reset exit"
```

> **Not replayed.** No probe runs this flash line (a CI runner has no board), so it is written from the board's runner, not measured ([tracking issue](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

(OpenOCD is an example; NuttX boards also flash with J-Link, `dfu-util`,
`esptool` or a vendor tool, as the board's page says.)

## GitHub source

- NuttX integration shell:
  [`integrations/nuttx/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nuttx)
- Copy-out NuttX QEMU talkers, one per language, each the same project as
  the one this page scaffolds:
  [`examples/qemu-armv7a-nuttx/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/rust/talker),
  [`examples/qemu-armv7a-nuttx/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/c/talker),
  [`examples/qemu-armv7a-nuttx/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/cpp/talker)
- The listener, service and action examples: the `nuttx_*` and
  `riscv_nuttx_*` image rows of the language workspaces,
  `examples/workspaces/{rust,c,cpp}`, under `pubsub_bringup`,
  `service_bringup` and `action_bringup`
  (`nros run pubsub_bringup:nuttx_listener`)
- Kconfig schema:
  [`integrations/nuttx/Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/nuttx/Kconfig)

## Next

- Multiple apps: each app declares its own `progname` in
  `Application Configuration → External Modules`; they share the
  one `libnros_c.a` build via the external-app shell.
- DDS on NuttX: bump the netbuffer Kconfig knobs (similar to the
  Zephyr DDS profile under
  [Choosing an RMW Backend](../communicate/choosing-an-rmw.md)).
- Build nano-ros's own NuttX QEMU tests without a NuttX-managed
  workspace: [NuttX in-tree examples (contributor)](../internals/platform/nuttx-in-tree.html).

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples as
  image rows of behaviour bringups.
