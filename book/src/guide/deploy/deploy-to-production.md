# Deployment Workflow

<!-- type: how-to · level: 3 -->

Deployment means different things per target, but the order is stable:
prepare toolchain, build package, move binary/firmware to target, then
verify ROS 2 communication.

## Native (host)

One command, whatever the workspace holds:

```bash
# One node (`nros new host_ws --board linux --template talker`):
cd host_ws
nros run host_node

# A bigger workspace — one entry generated per [image.*]:
cd my_robot
nros build                     # every default image
nros run linux                 # or run the binary `nros build` printed
```

`nros plan` / `nros check` are the inspection path, not
the build path — they produce and validate a plan you can read with
`nros explain`. You do not need them to build.

For interop with stock ROS 2 over Zenoh, run **the router ROS ships** and point
ROS 2 at it:

```bash
ros2 run rmw_zenoh_cpp rmw_zenohd
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
```

nano-ros does not bundle a router. `rmw_zenohd` links
the same `libzenohc.so` that `rmw_zenoh_cpp` does, so it cannot drift from the
RMW you are talking to — and it is what a ROS 2 deployment actually runs.

It takes no command-line configuration. To move it off the default `tcp/[::]:7447`:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"]' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

See [Native host build](../platforms/linux-host.md).

## RTOS and Bare-Metal

RTOS targets usually produce firmware images or simulator binaries. The
commands are the same as on the host — the board in `system.toml` is what
makes them cross-compile:

```bash
nros build                     # or: nros build <image-id>, in any language
```

After a successful build `nros build` prints the artifact and the board's own
next command (a QEMU line, `west build -d … -t run`, `west flash -d …`); each
platform chapter's **Run on QEMU / simulator** and **Flash a real board**
sections say the same. Driving CMake yourself is the escape hatch —
`nros build --dry-run` prints the line it would run, and
[Build as a CMake subdirectory](../build/cmake-subdirectory.md)
covers a tree with no `system.toml`.

Building and testing the in-tree fixtures: see
[Internals → Per-Platform Contributor Lanes](../../internals/platform/platform-lanes.html#freertos).

For real hardware, deployment step becomes flash/load/monitor. For QEMU,
deployment is launching simulator with correct network setup.

For a host image or an emulator, `nros run` builds the image if stale, starts
the router its link needs and streams the console; `nros run --expect
'<line>'` is the one-line smoke test. Real hardware is flashed:
`nros flash --monitor --expect '<line>'` builds, runs the board's human steps,
flashes through the board's flash method and watches the console on the same
port, and `nros monitor` attaches to a board that is already running. A board
with no flash method stays on the **vendor's native tools**. See
[What `nros build` Produces](../../reference/build-outputs.md) for where each target family's
artifact lands and which of the vendor's commands takes it from there.

(`nros build` itself *does* exist. It generates the root build file and the
entry and hands off; it does not wrap running or flashing.)

The embedded deploy contract is a documented three-step sequence:

1. **Configure** — `nros build` first reads the bringup's `system.toml` +
   `[image.<id>]` + its launch file (authored or generated) and emits, per
   image, the generated message crates, the resolved system model, the entry
   and the image's build settings under `build/`. Keyed on the image, not on
   the board: several images can name one board and still differ in what
   they contain. `nros configure` runs this step alone.
2. **Build** — `nros build` then hands off to the vendor tool — `cargo` /
   `cmake` / `west` — which you can equally drive yourself against the
   generated settings.
3. **Flash + monitor** — the vendor tool again: `probe-rs run` /
   `west flash`, or the platform's QEMU runner.

Platform guides should show:

- package layout,
- setup command,
- toolchain requirements,
- build command,
- run/flash command,
- ROS 2 interop or smoke-test command.

## Zephyr

Zephyr builds with `west`, and `nros build` runs it for you — the board comes
from `system.toml` like everywhere else; the descriptor states the `-b` west
receives:

```bash
nros setup zephyr --rmw zenoh       # Zephyr SDK + a west workspace in the store
cd my_robot
nros build demo_bringup:zephyr
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run   # native_sim / QEMU
west flash -d build/zephyr-zenoh/demo_bringup__zephyr_west          # a board with a flash runner
```

`nros build` generates the west application around the entry (Rust workspaces
included), passes the image's overlays, and builds
into the image's own directory, which is why the follow-up `west` commands
name it with `-d`. When it finishes it prints whichever of the two lines
the board supports. Bringing your own west workspace, or driving `west build`
by hand: [Zephyr Integration](../platforms/zephyr.md).

## Verify

After deployment, verify from ROS 2 side:

```bash
ros2 topic list
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

If discovery works but samples do not arrive, check domain ID, router
mode, QoS reliability, and platform network setup.

## Design background

- [RFC-0003: RTOS integration pattern](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md) — the three-step embedded deploy contract (§4)
- [RFC-0065: colcon-like workspace builder](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0065-colcon-like-workspace-builder.md) — `nros build`
- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md) — why the router comes from ROS 2
