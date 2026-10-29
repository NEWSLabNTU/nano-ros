# NuttX (apps/external)

Single-node starter on NuttX. nano-ros plugs into the standard NuttX
app discovery as an external app under `apps/external/nano-ros/`,
exposing Kconfig knobs under `Application Configuration → External
Modules → nano-ros`. Use this entry when your NuttX board ships its
own kernel build and you want to add ROS 2 communication.

> **Contributor path?** Building nano-ros's own NuttX QEMU examples
> straight from this repository (no NuttX-managed workspace) is
> covered at [NuttX in-tree examples (contributor)](../internals/nuttx-in-tree.md). The page below is
> the canonical user entry.

> **Prereqs.** Install the `nros` CLI once per machine, then run
> `nros setup qemu-armv7a-nuttx --rmw <zenoh|xrce|cyclonedds>` (`--rmw`
> defaults to `zenoh`). This fetches a mostly-prebuilt toolchain set into
> `${NROS_HOME:-~/.nros}/sdk` — the NuttX cross-compiler, the emulator and the
> NuttX sources — so you do **not** hand-install a cross-toolchain. The zenoh
> router is ROS's `rmw_zenohd` and comes from a ROS 2 install (RFC-0075); the
> xrce path needs only the Micro-XRCE-DDS agent, which `nros setup … --rmw
> xrce` installs:
>
> ```bash
> nros setup qemu-armv7a-nuttx --rmw zenoh
> ```
>
> (`nros` is the installed CLI — [Installation](./installation.md).)
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
my_app/                                  # your application
├── system.toml                          # WHAT this deploys to — the one file you edit
├── package.xml
├── Cargo.toml | CMakeLists.txt          # language-toolchain facts only
├── generated/                           # generated message bindings (gitignored)
└── src/main.{rs,c,cpp}
```

Wire the shell into your NuttX apps tree. Two commands, both run once:

```bash
nros setup qemu-armv7a-nuttx --rmw zenoh    # toolchain + NuttX sources + SDK
"$(nros sdk-root)/scripts/nuttx/stage-external-apps.sh" "$NUTTX_APPS_DIR"
```

The staging script writes `$NUTTX_APPS_DIR/external/Make.defs` + `Kconfig`
and symlinks the integration shell in as `external/nano-ros`. (The SDK
root carries `integrations/` in releases cut after 0.6.0-nros4.) Menuconfig
surfaces it under `Application Configuration → External Modules`.

The script stages the integration shell only. A NuttX **workspace image** (a
bringup with several components) is not built through the apps tree: add an
`[image.<id>]` with `board = "qemu-armv7a-nuttx"` to the bringup's
`system.toml` and run `nros build` — see `[image.nuttx]` in
[`examples/workspaces/rust`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/workspaces/rust/src/demo_bringup/system.toml). The script's old `--bringup <dir>` / `--image <id>` flags
staged a per-bringup app that could not build an image, and now refuse
(issue 1883).

If you'd rather wire it yourself (e.g. into a vendored apps tree):

```bash
ln -s "$(nros sdk-root)/integrations/nuttx" \
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
`locator` — is declared in `system.toml` beside the package and baked at
compile time. Verbatim from the in-tree
[`examples/qemu-armv7a-nuttx/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-armv7a-nuttx/rust/talker/system.toml):

```toml
[system]
name = "nuttx_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "nuttx_talker"
class = "nuttx_rs_talker::Talker"
name = "talker"

[image.qemu-armv7a-nuttx]
board = "nuttx"
locator = "tcp/10.0.2.2:8200"
```

The C / C++ variants carry the same file, in the same schema — their
`package.xml` has no `<nano_ros …/>` tuple and their `CMakeLists.txt` names no
board. Nor does anything here name a target triple: `armv7a-nuttx-eabihf` is a
fact about the board, and it lives in the board descriptor. Run `nros sync` in
the application directory after editing this file; it writes the generated
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

**`nros build` — the path for a nano-ros package or workspace.** The board in
`system.toml` (`qemu-armv7a-nuttx` for Arm `virt`, `rv-virt-nuttx` for RISC-V
`rv-virt`) picks the cross toolchain and the NuttX board config; `nros build`
links your application into a bootable NuttX kernel ELF. Provision the board
(no NuttX checkout of your own is needed on this road — `nros setup` fetches
the kernel and apps sources), then scaffold a C / C++ package — `nros new`
writes `board = "qemu-armv7a-nuttx"`, the platform's default board — and build
it:

```bash probe=30 variant=arm
nros setup qemu-armv7a-nuttx --rmw zenoh --sudo
```

The NuttX kernel build needs two host packages a fresh Debian/Ubuntu host may
lack — the Python venv module and `unzip`. `nros setup` names any that are
missing before fetching anything; `--sudo` lets it run the one install command
it composed for your package manager (`sudo apt-get install -y python3-venv
unzip` here) before the download starts. Leave `--sudo` off and it only prints
that command, and never runs `sudo` for you.

```bash probe=40 variant=arm
nros new my_app --platform nuttx --lang c
cd my_app
nros build
```

The kernel ELF lands at `build/nuttx/cmake/my_app` (`build/<image>/cmake/`
plus the name `nano_ros_add_executable` gives it), and `nros build` prints it
with the QEMU line that boots it. Rust starts as a workspace image
(`nros new --image nuttx --board qemu-armv7a-nuttx`, then `nros sync` and
`nros build nuttx`); its ELF is under the entry's target dir, which `nros
build` prints. `nros build <image> --dry-run` shows the native commands.

**NuttX's own build — the integration shell in a NuttX-managed tree.** NuttX
owns the build verb there, `make` from the configured kernel, the way Zephyr
owns `west build`. `nros sync` still runs first, in the application
directory, because the generated message bindings must exist before anything
compiles against them:

```bash
cd my_app
nros sync                           # bindings + settings from system.toml

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
app bakes — 7447 when it states none, as a scaffolded package; 8200 in the
example above:

```bash probe=60 probe-run=60 probe-keep
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60
Started Zenoh router with id
```

**2. The board** — boot the kernel ELF, the one `nros build` wrote (or
`$NUTTX_DIR/nuttx` from the make build), from the package directory:

```bash probe=70 probe-run=120 variant=arm
"$(nros sdk-path qemu)/bin/qemu-system-arm" -M virt -cpu cortex-a7 -nographic \
    -icount shift=auto \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0 \
    -kernel build/nuttx/cmake/my_app
```

An image `nros build` produced starts its application itself. The scaffolded
C talker is the in-tree
[example's `main.c`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-armv7a-nuttx/c/talker/src/main.c),
so it publishes `Hello World: N` once a second:

```text probe-expect=70 variant=arm
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

QEMU exits via Ctrl-A x.

> **Contributors:** the in-tree fixture/test lanes for nano-ros's own
> QEMU examples are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#nuttx).

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
3. Confirm `zenohd` reachable; the image's `locator` (or the
   `rclc_support_init` arguments) must match the router's listen port.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Run on QEMU (RISC-V rv-virt)

The `rv-virt-nuttx` board (`rv32imac`) boots on QEMU's RISC-V `virt` machine.
**The store's QEMU is built for Arm and RISC-V 64 only**, so this board needs
a `qemu-system-riscv32` from your distribution (Debian/Ubuntu:
`qemu-system-misc`):

```bash probe=30 variant=riscv
nros setup rv-virt-nuttx --rmw zenoh --sudo
sudo apt-get install -y qemu-system-misc
```

Name it with `QEMU_SYSTEM_RISCV32` and `nros build` prints a line that uses
it; without it the printed line asks `nros locate tool qemu-system-riscv32`,
which then has nothing to answer. Scaffold for this board (`nuttx` names the
Arm one by default) and build:

```bash probe=40 variant=riscv
nros new my_app --platform nuttx --lang c --board rv-virt-nuttx
cd my_app
export QEMU_SYSTEM_RISCV32="$(command -v qemu-system-riscv32)"
nros build
```

Start the router as above (on the port your image's `locator` names), then
boot it:

```bash probe=70 probe-run=120 variant=riscv
"$QEMU_SYSTEM_RISCV32" -M virt -bios none -nographic \
    -icount shift=auto \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0 \
    -kernel build/nuttx/cmake/my_app
```

```text probe-expect=70 variant=riscv
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

(OpenOCD is an example; NuttX boards also flash with J-Link, `dfu-util`,
`esptool` or a vendor tool, as the board's page says.)

## GitHub source

- NuttX integration shell:
  [`integrations/nuttx/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nuttx)
- Worked NuttX QEMU examples:
  [`examples/qemu-armv7a-nuttx/rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/rust),
  [`examples/qemu-armv7a-nuttx/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/c),
  [`examples/qemu-armv7a-nuttx/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-armv7a-nuttx/cpp)
- Kconfig schema:
  [`integrations/nuttx/Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/nuttx/Kconfig)

## Next

- Multiple apps: each app declares its own `progname` in
  `Application Configuration → External Modules`; they share the
  one `libnros_c.a` build via the external-app shell.
- DDS on NuttX: bump the netbuffer Kconfig knobs (similar to the
  Zephyr DDS profile under
  [Choosing an RMW Backend](../user-guide/rmw-backends.md)).
- Build nano-ros's own NuttX QEMU tests without a NuttX-managed
  workspace: [NuttX in-tree examples (contributor)](../internals/nuttx-in-tree.md).
