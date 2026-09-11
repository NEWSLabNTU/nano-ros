# Deployment Workflow

Deployment means different things per target, but the order is stable:
prepare toolchain, build package, move binary/firmware to target, then
verify ROS 2 communication.

## Native (host)

Two commands, whatever the workspace shape:

```bash
# A single Rust package:
cd examples/native/rust/talker
nros sync
nros build
./build/native/target/debug/talker

# A workspace — same two commands, one entry generated per [image.*]:
cd my_robot
nros sync
nros build
./build/posix-native/cmake/native_entry
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

# C / C++ single package — its own CMake, no sync needed:
cmake -B build && cmake --build build
```

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

Zephyr deployment uses `west`:

```bash
nros setup zephyr --rmw zenoh
# **Contributors (in-tree checkout):** `just setup zephyr` creates
# zephyr-workspace/ (west init + SDK) — then:
source zephyr-workspace/env.sh      # in-tree workspace layout
nros sync
west build -b native_sim/native/64 nros/examples/zephyr/rust/talker
./build/zephyr/zephyr.exe
```

Zephyr is the one target that still names its board on the command line.
Everywhere else the board comes from `system.toml` and `nros build` hands
it to the tool; here the west *application* around the entry is not yet
generated
([issue 1288](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1288-zephyr-rust-workspace-entries-not-generated.md)),
so west's own `-b` is what selects the board. `nros sync` still runs
first, for the message crates and the resolved model.

Bringing your own west workspace instead? Follow
[Zephyr Integration](../getting-started/integration-zephyr.md) — the
`zephyr-workspace/env.sh` line above is specific to the in-tree
workspace layout.

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
