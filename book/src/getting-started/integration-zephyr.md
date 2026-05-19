# Zephyr (west module)

Single-node starter on Zephyr via the in-tree `integrations/zephyr/`
west module. nano-ros ships as a Zephyr module — `west` discovers it
from your workspace's `west.yml`, drops in a `prj.conf` Kconfig
surface, and the standard `west build` / `west flash` flow takes
care of the rest.

> **Contributor path?** Building nano-ros's own Zephyr examples
> straight from this repository (no west-managed workspace) is
> covered at [Zephyr (contributor)](./zephyr.md). The page below is
> the canonical user entry.

> **Prereqs.** Zephyr SDK ≥ v0.16, `west` CLI (`pip install west`),
> Python 3.10+. Bootstrap a Zephyr workspace first
> (`west init -l <your-app>` or `just zephyr setup` to use the
> in-tree `zephyr-workspace/` layout). nano-ros's imported west
> fragment `integrations/zephyr/west.yml` is a manifest-only file —
> it does NOT pull Zephyr itself; that has to be in your parent
> manifest (`zephyrproject-rtos/zephyr`).

## Project layout

A Zephyr workspace using nano-ros looks like any other Zephyr
project — the **nano-ros module sits beside Zephyr**, your
application sits beside both:

```text
my_zephyr_ws/
├── .west/
├── zephyr/                            # cloned by `west init`
├── modules/
│   └── nano-ros/                      # imported via west.yml
└── apps/
    └── my_app/                        # your application
        ├── CMakeLists.txt
        ├── prj.conf                   # Kconfig — selects nros + RMW
        ├── west.yml                   # (optional) per-app manifest
        └── src/
            └── main.c                 # nros user code
```

The application `CMakeLists.txt` is a stock Zephyr app — `find_package(Zephyr)`
+ `target_sources`. **No `add_subdirectory(<nano-ros>)`** is needed;
the module shell handles it once `CONFIG_NROS=y` flips on.

## Configure

Add nano-ros to your workspace `west.yml`:

```yaml
manifest:
  remotes:
    - name: nano-ros
      url-base: https://github.com/NEWSLabNTU
  projects:
    - name: nano-ros
      remote: nano-ros
      path: modules/nano-ros
      import:
        file: integrations/zephyr/west.yml      # pulls Zephyr + nano-ros deps
```

Then per-application `prj.conf`:

```
CONFIG_NROS=y
CONFIG_NROS_RMW="zenoh"                 # zenoh | xrce | dds
CONFIG_NROS_ROS_EDITION="humble"        # humble | iron

# Required for any networked RMW on QEMU / native_sim:
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_TCP=y
```

`CONFIG_NROS=y` activates the shell, which maps Kconfig values to
`NANO_ROS_*` CMake cache vars and `add_subdirectory()`s the root
nano-ros CMake. `NanoRos::NanoRos` is linked into your `app`
library transparently.

## Build

```bash
west update                              # pull nano-ros + transitives
west build -b qemu_cortex_a9 apps/my_app
# native_sim alternative (POSIX, no QEMU):
west build -b native_sim/native/64 apps/my_app
```

For a quick sanity check that the module is wired correctly:

```bash
west build -t menuconfig                 # confirm CONFIG_NROS=y is visible
```

## Run

```bash
# QEMU Cortex-A9:
west build -t run

# native_sim:
./build/zephyr/zephyr.exe

# Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

The Zephyr boot banner runs first, then nano-ros prints
`Published: 1`, `Published: 2`, ... as the talker fires.

**Readiness signal.** On `native_sim`, expect `Published: 1`
within 5 seconds of `./build/zephyr/zephyr.exe`; on `qemu_cortex_a9`
expect it within ~15 seconds (QEMU cold boot + Zephyr init). If
no `Published:` line in 30 seconds:

1. Confirm `CONFIG_NROS=y` lit up via `west build -t menuconfig`;
   without it the module shell never `add_subdirectory`'s nano-ros.
2. Check `CONFIG_NETWORKING=y`, `CONFIG_NET_IPV4=y`, `CONFIG_NET_TCP=y`
   in `prj.conf` — Zephyr networking is opt-in.
3. Confirm `zenohd` reachable from the simulated network (Slirp
   needs `10.0.2.2:7447` on QEMU; native_sim uses host loopback).
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- Zephyr module shell:
  [`integrations/zephyr/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/zephyr)
- Worked examples:
  [`examples/zephyr/rust/zenoh/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/rust/zenoh),
  [`examples/zephyr/c/zenoh/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c/zenoh),
  [`examples/zephyr/cpp/zenoh/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/cpp/zenoh)
- Module manifest:
  [`integrations/zephyr/module.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/zephyr/module.yml)

## Next

- Pick a real board (Nordic, NXP, STM32, …): swap `-b <board>` and
  add a board-specific overlay to your `prj.conf`.
- Cyclone DDS on Cortex-A/R: see the DDS section of
  [Choosing an RMW Backend](../user-guide/rmw-backends.md) for the
  required Kconfig deltas.
- Build nano-ros's own Zephyr examples without west:
  [Zephyr (contributor)](./zephyr.md).
