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

> **Prereqs.** NuttX ≥ nuttx-12 checkout with `apps/` sibling, a
> NuttX cross-toolchain (e.g. `gcc-arm-none-eabi` for ARM
> configurations), and Python 3.10+ for the NuttX configure
> scripts.

## Project layout

NuttX's "external apps" pattern places the app shim under
`$NUTTX_APPS/external/<name>/`:

```text
$NUTTX_DIR/                              # NuttX kernel checkout
$NUTTX_APPS/                             # sibling: apps tree
└── external/
    └── nano-ros/                        # symlink or submodule of
        ├── Make.defs                    #   integrations/nuttx/
        ├── Makefile
        ├── CMakeLists.txt               #   (cmake-driven NuttX builds)
        └── Kconfig
my_app/                                  # your application
├── package.xml
├── Cargo.toml | CMakeLists.txt
└── src/main.{rs,c,cpp}
```

Wire the shell once per workspace via symlink:

```bash
ln -s /path/to/nano-ros/integrations/nuttx \
      $NUTTX_APPS/external/nano-ros
```

Or attach as a git submodule under `$NUTTX_APPS/external/`. NuttX's
app discovery picks up the `Make.defs` automatically.

## Configure

NuttX uses Kconfig as its single source of truth. After the symlink
above:

```bash
cd $NUTTX_DIR
make menuconfig
# Navigate to:
#   Application Configuration → External Modules → nano-ros
#   [*] nano-ros ROS 2 client
#       RMW backend  →  zenoh           # zenoh | xrce | dds
#       ROS 2 edition →  humble
```

Networking Kconfig requirements live under
`Networking Support` — enable `CONFIG_NET`, `CONFIG_NET_TCP`,
`CONFIG_NET_IPv4`. For QEMU `nsh_smp` configurations the defaults
already include these.

Runtime config (locator / domain id) is read from either the
companion `config.toml` or from `nros_init()` arguments — see the
example source.

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
# QEMU NuttX (ARM):
qemu-system-arm -cpu cortex-a8 -machine virt -nographic \
                -kernel $NUTTX_DIR/nuttx
nsh> nros_talker        # or whatever your app's NSH command is

# Real hardware: standard NuttX flash flow (openocd / J-Link / etc.)

# Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

**Readiness signal.** After typing the app's NSH command (e.g.
`nros_talker`), expect `Published: 1` on the NSH console within
5 seconds. If no `Published:` line:

1. Confirm the app actually ran — `ps` should show your task.
2. Confirm networking — `ifconfig` shows a configured interface.
   For QEMU `nsh_smp`, Slirp defaults apply: `eth0` at `10.0.2.15`.
3. Confirm `zenohd` reachable; the locator in `config.toml` /
   `nros_init` arguments must match.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- NuttX integration shell:
  [`integrations/nuttx/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nuttx)
- Worked NuttX QEMU examples:
  [`examples/nuttx/rust/zenoh/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/nuttx/rust/zenoh),
  [`examples/nuttx/c/zenoh/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/nuttx/c/zenoh)
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
