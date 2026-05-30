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
> defaults to `zenoh`). This fetches a prebuilt toolchain set into
> `~/.nros/sdk` — the NuttX cross-compiler, the emulator, the NuttX
> sources, and the RMW host daemon — so you do **not** hand-install
> a cross-toolchain and do **not** need a ROS 2 install:
>
> ```bash
> curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
> export PATH="$HOME/.nros/bin:$PATH"
> nros setup qemu-arm-nuttx --rmw zenoh
> ```
>
> You still need a NuttX ≥ nuttx-12 checkout with an `apps/` sibling
> and Python 3.10+ for the NuttX configure scripts.

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

Wire the shell into your NuttX apps tree. Easiest path:

```bash
just nuttx setup        # contributor helper: stages the shell +
                        # example apps into $NUTTX_APPS_DIR/external/
                        # (delegates to `nros setup qemu-arm-nuttx`
                        # for the toolchain/SDK provisioning)
```

This runs `scripts/nuttx/stage-external-apps.sh`, which writes
`$NUTTX_APPS_DIR/external/Make.defs` + `Kconfig` and symlinks the
integration shell (`external/nano-ros`) plus every example app
(`external/nano-ros-<example>-<lang>`). Menuconfig surfaces them
under `Application Configuration → External Modules`.

If you'd rather wire it yourself (e.g. into a vendored apps tree):

```bash
ln -s /path/to/nano-ros/integrations/nuttx \
      $NUTTX_APPS/external/nano-ros
# then copy integrations/nuttx/external-Make.defs.in →
# $NUTTX_APPS/external/Make.defs and add a matching
# $NUTTX_APPS/external/Kconfig that `source`s the shell.
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

Runtime config (locator / domain id) is read from either the
companion `config.toml` or from `nros_init()` arguments — see the
example source.

## Build

A bare `cd $NUTTX_DIR && make` will not produce a working nano-ros
kernel — NuttX needs the nano-ros board defconfig and a fistful of
`kconfig-tweak --enable / --set-val` toggles before its build
graph wires nano-ros in. The canonical builder does all of that:

```bash
just nuttx build-fixtures      # builds the kernel + every staged example
```

Internally it copies
`packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/defconfig`
into `$NUTTX_DIR/.config`, runs two `make olddefconfig` passes,
flips `CONFIG_NROS` / `CONFIG_NROS_C_API` / `CONFIG_NROS_CPP_API`
/ `CONFIG_NROS_EXAMPLE_*_{C,CPP}` on, sets `CONFIG_TLS_NELEM=8`
(Rust std TLS), switches to `LIBCXXTOOLCHAIN` (nros-cpp needs
`<utility>`), disables `ALLSYMS` (first-link chicken-egg), and
runs the NuttX `make`. The Cargo build of nano-ros's Rust
staticlibs runs as a sub-step; `libnros_c.a` (and the per-example
auxiliary staticlibs) link in at the final app link stage.

For a custom workspace where you maintain your own kernel
configuration, re-create that toggle set against your own
`.config` (cite `just/nuttx.just::_nuttx-configure-fixtures` as
the source of truth). The Phase 208.A.1 sweep retired the
`cmake -B build -DBOARD=… -DCONFIG=…` block that previously
appeared here — those flags aren't supported by upstream NuttX
(it uses the combined `-DBOARD_CONFIG=<board>:<config>` form, and
even that requires NuttX's CMake-build feature which isn't part
of nano-ros's pinned line).

## Run

```bash
# Terminal 1 — zenohd on the NuttX test-fixture port (7452):
just nuttx zenohd

# Terminal 2 — QEMU NuttX (ARM cortex-a7 virt board, virtio-net
# for Slirp). `just nuttx talker` wires every flag from
# `packages/testing/nros-tests/src/qemu.rs::NUTTX`:
just nuttx talker
nsh> nuttx_c_talker         # or nuttx_cpp_talker / nuttx_c_listener / …

# Real hardware: standard NuttX flash flow (openocd / J-Link / etc.)

# Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

The NSH command is the staged example's PROGNAME (see each
`examples/qemu-arm-nuttx/<lang>/<example>/Makefile`): `nuttx_c_talker`,
`nuttx_cpp_talker`, `nuttx_c_listener`, `nuttx_cpp_service-server`,
etc. `nros_talker` is **not** a registered name.

A direct `qemu-system-arm` invocation, if you need one, MUST
include `-cpu cortex-a7` (not `cortex-a8`) and the virtio-net pair
`-netdev user,id=net0 -device virtio-net-device,netdev=net0` — the
nano-ros board defconfig enables `CONFIG_DRIVERS_VIRTIO_MMIO`, so
the default `-nic user` (PCI virtio-net) gets ignored by NuttX's
NIC driver. Grep the exact flag set from
`packages/testing/nros-tests/src/qemu.rs::qemu_args_nuttx_arm`.

**Readiness signal.** After typing the app's NSH command, expect
`Published: 0` on the NSH console within 5 seconds. If no
`Published:` line:

1. Confirm the app actually ran — `ps` should show your task.
2. Confirm networking — `ifconfig` shows `eth0` at `10.0.2.30`
   (the nano-ros board defconfig sets `CONFIG_NETINIT_IPADDR=
   0x0a00021e`, NOT the stock Slirp default 10.0.2.15).
3. Confirm `zenohd` reachable on the locator from `nros.toml`
   (default 7452 for the NuttX QEMU fixture).
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- NuttX integration shell:
  [`integrations/nuttx/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/nuttx)
- Worked NuttX QEMU examples:
  [`examples/qemu-arm-nuttx/rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-nuttx/rust),
  [`examples/qemu-arm-nuttx/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-nuttx/c)
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
