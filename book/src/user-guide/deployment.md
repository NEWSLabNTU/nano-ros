# Deployment Workflow

Deployment means different things per target, but the order is stable:
prepare toolchain, build package, move binary/firmware to target, then
verify ROS 2 communication.

## Native (host)

Two commands, whatever the workspace shape:

```bash
# A single Rust package (`nros new my_talker --platform linux --lang rust`):
cd my_talker
nros sync
nros build
./build/linux/target/debug/my_talker

# A workspace — same two commands, one entry generated per [image.*]:
cd my_robot
nros sync
nros build
./build/posix-cyclonedds-linux/cmake/linux_entry
```

`nros metadata` / `nros plan` / `nros check` are the inspection path, not
the build path — they produce and validate a plan you can read with
`nros explain`. You do not need them to build.

For interop with stock ROS 2 over Zenoh, run **the router ROS ships** and point
ROS 2 at it:

```bash
ros2 run rmw_zenoh_cpp rmw_zenohd
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
```

nano-ros no longer bundles a router (phase-362 / RFC-0075). `rmw_zenohd` links
the same `libzenohc.so` that `rmw_zenoh_cpp` does, so it cannot drift from the
RMW you are talking to — and it is what a ROS 2 deployment actually runs.

It takes no command-line configuration. To move it off the default `tcp/[::]:7447`:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"]' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

See [Native host build](../platform-guides/native-host.md).

## RTOS and Bare-Metal

RTOS targets usually produce firmware images or simulator binaries. The
commands are the same as on the host — the board in `system.toml` is what
makes them cross-compile:

```bash
# Rust leaf or workspace:
nros sync
nros build                     # or: nros build <image-id>

# C / C++ single package — no sync needed; `nros build` drives its CMake:
nros build
```

After a successful build `nros build` prints the artifact and the board's own
next command (a QEMU line, `west build -d … -t run`, `west flash -d …`); each
platform chapter's **Run on QEMU / simulator** and **Flash a real board**
sections say the same. Driving CMake yourself is the escape hatch —
`nros build --dry-run` prints the line it would run, and
[Build as a CMake subdirectory](../getting-started/build-as-subdirectory.md)
covers a tree with no `system.toml`.

> **Contributors:** the in-tree fixture build/test lanes are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

For real hardware, deployment step becomes flash/load/monitor. For QEMU,
deployment is launching simulator with correct network setup.

There is no `nros run`, no `nros flash` and no `nros deploy` verb, and none is
planned: `nros build` ends at an artifact, and how that artifact is flashed or
started is a property of the board and of your bench. Deployment runs on the
**vendor's native tools**. See
[What `nros build` Produces](build-artifacts.md) for where each target family's
artifact lands and which of the vendor's commands takes it from there.

(`nros build` itself *does* exist — RFC-0065 introduced it after Phase 222 had
removed an earlier set of wrappers. It generates the root build file and the
entry and hands off; it does not wrap running or flashing.)

The embedded deploy contract is a documented three-step sequence (per
[RFC-0003 §4](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md)):

1. **Sync** — `nros sync` reads `system.toml` + `[image.<id>]` +
   `launch/*.xml` and emits, per image, the generated message crates, the
   resolved system model and the image's build settings under
   `build/<image-id>/`. Keyed on the image, not on the board: several
   images can name one board and still differ in what they contain.
2. **Build** — `nros build` generates that image's entry and hands off to
   the vendor tool — `cargo` / `cmake` / `west` / `idf.py` — which you
   can equally drive yourself against the generated settings.
3. **Flash + monitor** — the vendor tool again: `probe-rs run` /
   `west flash` / `idf.py flash monitor`, or the platform's QEMU runner.

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
nros sync
nros build demo_bringup:zephyr
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run   # native_sim / QEMU
west flash -d build/zephyr-zenoh/demo_bringup__zephyr_west          # a board with a flash runner
```

`nros build` generates the west application around the entry (issue 1288 made
that the case for Rust workspaces too), passes the image's overlays, and builds
into the image's own directory, which is why the follow-up `west` commands
name it with `-d`. When it finishes it prints whichever of the two lines
the board supports. Bringing your own west workspace, or driving `west build`
by hand: [Zephyr Integration](../getting-started/integration-zephyr.md).

## ESP32

ESP32 deployment uses the Espressif toolchain and flash tool.

> **Contributors:** the in-tree ESP32 build/run lanes are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#esp32).

For physical boards: `espflash flash --monitor <elf>` (the QEMU
chapter's `espflash save-image` packs an emulator image instead —
real-hardware bring-up is not yet a documented end-to-end flow).

## Verify

After deployment, verify from ROS 2 side:

```bash
ros2 topic list
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

If discovery works but samples do not arrive, check domain ID, router
mode, QoS reliability, and platform network setup.
