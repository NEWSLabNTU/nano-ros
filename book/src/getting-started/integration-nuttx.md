# NuttX (apps/external)

Single-node starter on NuttX. nano-ros plugs into the standard NuttX
app discovery as an external app under `apps/external/nano-ros/`,
exposing Kconfig knobs under `Application Configuration → External
Modules → nano-ros`. Use this entry when your NuttX board ships its
own kernel build and you want to add ROS 2 communication.

> **Contributor path?** Building nano-ros's own NuttX QEMU examples
> straight from this repository (no NuttX-managed workspace) is
> covered at [NuttX (contributor)](./nuttx.md). The page below is
> the canonical user entry.

> **Prereqs.** Install the `nros` CLI once per machine, then run
> `nros setup qemu-arm-nuttx --rmw <zenoh|xrce|cyclonedds>` (`--rmw`
> defaults to `zenoh`). This fetches a mostly-prebuilt toolchain set into
> `${NROS_HOME:-~/.nros}/sdk` — the NuttX cross-compiler, the emulator, the NuttX
> sources, and the RMW host daemon — so you do **not** hand-install
> a cross-toolchain and do **not** need a ROS 2 install:
>
> ```bash
> ./scripts/bootstrap.sh      # builds packages/cli/target/release/nros (Phase 218)
> source ./activate.sh        # OR: direnv allow / source ./activate.fish
> nros setup qemu-arm-nuttx --rmw zenoh
> ```
>
> You still need a NuttX ≥ nuttx-12 checkout with an `apps/` sibling
> and Python 3.10+ for the NuttX configure scripts.

## Project layout

NuttX's "external apps" pattern places the app shim under
`$NUTTX_APPS_DIR/external/<name>/`:

```text
$NUTTX_DIR/                              # NuttX kernel checkout
$NUTTX_APPS_DIR/                             # sibling: apps tree
└── external/
    └── nano-ros/                        # symlink or submodule of
        ├── Make.defs                    #   integrations/nuttx/
        ├── Makefile
        ├── CMakeLists.txt               #   (cmake-driven NuttX builds)
        └── Kconfig
my_app/                                  # your application
├── package.xml
├── Cargo.toml | CMakeLists.txt
├── generated/                           # Rust codegen — build.rs runs
│                                        #   `nros generate-rust` on first
│                                        #   `cargo build`; gitignored.
└── src/main.{rs,c,cpp}
```

Wire the shell into your NuttX apps tree. Easiest path:

```bash
just setup nuttx        # contributor helper: stages the shell +
                        # example apps into $NUTTX_APPS_DIR/external/
                        # (delegates to `nros setup qemu-arm-nuttx`
                        # for the toolchain/SDK provisioning)
```

This runs `scripts/nuttx/stage-external-apps.sh`, which writes
`$NUTTX_APPS_DIR/external/Make.defs` + `Kconfig` and symlinks the
integration shell in as `external/nano-ros`. Menuconfig surfaces it
under `Application Configuration → External Modules`.

Passing `--bringup <dir>` additionally stages a per-bringup app
(`external/<bringup>/`, with the bringup tree symlinked alongside as
`external/<bringup>-source`) — the Phase 212 multi-package workspace
shape. There are no per-EXAMPLE symlinks: that staging loop was
retired in phase-212.M-F.12 when the bringup path superseded it.

If you'd rather wire it yourself (e.g. into a vendored apps tree):

```bash
ln -s /path/to/nano-ros/integrations/nuttx \
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

Deploy config (RMW / domain id, plus an optional `locator` override) is
declared in the build manifest and baked at compile time. Verbatim from
the in-tree
[`examples/qemu-arm-nuttx/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-nuttx/rust/talker/Cargo.toml):

```toml
[package.metadata.nros.deploy.nuttx]
board     = "qemu-armv7a-nsh"
target    = "armv7a-nuttx-eabihf"
rmw       = "zenoh"
domain_id = 0
```

The C / C++ variants declare the same in their `package.xml` `<export>`
tuple (RFC-0048 §4):

```xml
<export>
  <build_type>ament_cmake</build_type>
  <nano_ros deploy="nuttx" board="nuttx-qemu-arm" rmw="zenoh"/>
</export>
```

The guest network shape (eth0 `10.0.2.30`, Slirp gateway `10.0.2.2`)
comes from the board crate; add a `locator = "tcp/10.0.2.2:<port>"`
field to dial a non-default router port. The prebuilt *test fixtures*
bake distinct per-language allocator ports so parallel suites don't
collide on one router. Start the router (ROS's `rmw_zenohd`) on the
port your app dials — it must listen on `0.0.0.0`, not loopback,
because the guest reaches it through QEMU's Slirp gateway:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:8200"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

## Build

```bash
cd $NUTTX_DIR
make                                # full kernel + apps build
```

The Cargo build of nano-ros's Rust staticlibs runs as a sub-step of
the NuttX app build; `libnros_c.a` is linked at the final app
link stage.

For CMake-driven NuttX builds:

```bash
cmake -B build -DBOARD=qemu-armv7a \
              -DCONFIG=nsh_smp
cmake --build build
```

## Run

```bash
# 1. Start the router on the host (Slirp forwards 10.0.2.2:<port> →
#    host). It must listen on 0.0.0.0 for the Slirp gateway; 8200 is
#    the port the in-tree lanes use — match whatever `locator` your
#    app bakes:
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:8200"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. QEMU NuttX (ARM). For a NuttX-managed workspace where you've
#    staged the integration shell + your own app, boot QEMU directly:
qemu-system-arm -M virt -cpu cortex-a7 -nographic \
                -icount shift=auto \
                -kernel $NUTTX_DIR/nuttx \
                -netdev user,id=net0 \
                -device virtio-net-device,netdev=net0
# `$NUTTX_DIR/nuttx` is the linked NuttX ELF produced by `make`
# at the NuttX source root — adjust if your workspace puts it
# elsewhere (e.g. an out-of-tree build dir).
# At the NSH prompt, run the example's PROGNAME — the
# `make`-driven build registers every nano-ros example as a
# built-in command via Application.mk's `-Dmain=<PROGNAME>_main`
# rename. Real PROGNAMEs (from
# `packages/testing/nros-tests/tests/nuttx_make_e2e.rs::EXPECTED_PROGNAMES`):
nsh> nuttx_c_talker            # C talker
# nsh> nuttx_cpp_talker        # C++ talker
# nsh> nuttx_c_listener        # ...and listener / service / action variants

# Real hardware: standard NuttX flash flow (openocd / J-Link / etc.)

# 3. Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

> **Contributors:** the in-tree fixture/test lanes for nano-ros's own
> QEMU examples are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#nuttx).

**Readiness signal.** After typing the app's NSH command (e.g.
`nuttx_c_talker`), expect `Publishing: 'Hello World: 1'` on the NSH
console within ~6 seconds — Rust + C + C++ talkers all start the
count at 1, matching the official ROS 2 demo talker. If no
`Publishing:` line:

1. Confirm the app actually ran — `ps` should show your task.
2. Confirm networking — `ifconfig` shows a configured interface.
   With the virtio-net + Slirp wiring above, `eth0` comes up at
   `10.0.2.30` (the board crate's default for the qemu-arm-nuttx examples).
3. Confirm `zenohd` reachable; the deploy locator (or the
   `nros_init` arguments) must match the router's listen port.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

### Auto-configure glue (NSH built-in registration)

The `make`-driven build above relies on a host-side glue layer. In-tree
that is `scripts/nuttx/stage-external-apps.sh --bringup <dir>`, the only
supported NuttX make path since phase-212 M-F.16 retired the per-example
`build-fixtures-make` recipe. If you wire a NuttX workspace by hand,
reproduce the same three steps:

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
   `integrations/nuttx/` into `$NUTTX_APPS_DIR/external/` (add
   `--bringup <dir>` for a workspace app). Remove `$NUTTX_APPS_DIR/Kconfig` so
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

## GitHub source

- NuttX integration shell:
  [`integrations/nuttx/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nuttx)
- Worked NuttX QEMU examples:
  [`examples/qemu-arm-nuttx/rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-nuttx/rust),
  [`examples/qemu-arm-nuttx/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-nuttx/c),
  [`examples/qemu-arm-nuttx/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-nuttx/cpp)
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
  workspace: [NuttX (contributor)](./nuttx.md).
