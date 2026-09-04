# Deployment Workflow

Deployment means different things per target, but the order is stable:
prepare toolchain, build package, move binary/firmware to target, then
verify ROS 2 communication.

## Native (host)

Three equivalent entry points; pick by workspace shape:

```bash
# Per-example (Pattern B or any single binary):
cd examples/native/rust/talker
cargo run

# Multi-component system orchestration:
nros metadata my_system
nros plan my_system launch/my_system.launch.py
nros check
cargo run -p robot_entry

# Colcon consumer workspace (Pattern A):
colcon build && source install/setup.bash
ros2 run my_pkg my_node
```

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

RTOS targets usually produce firmware images or simulator binaries,
built with the platform's own tool from the example / package dir:

```bash
cargo build --release          # Rust leaves (after `nros sync`)
cmake -B build && cmake --build build   # C / C++ leaves
```

> **Contributors:** the in-tree fixture build/test lanes are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

For real hardware, deployment step becomes flash/load/monitor. For QEMU,
deployment is launching simulator with correct network setup.

There is no `nros deploy` / `nros build` / `nros run` verb — Phase 222
removed those wrappers. `nros` is provisioner + codegen + metadata only;
deployment runs on the **vendor's native tools**. The embedded deploy
contract is a documented three-step sequence (per
[RFC-0003 §4](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md)):

1. **Bake** — `nros codegen-system --bringup <pkg>` reads
   `system.toml` + `[image.<id>]` + `launch/*.xml` and emits the
   baked tree under `build/<board>/`.
2. **Build** — the vendor tool builds it: `cargo build` / `cmake --build`
   / `west build` / `idf.py build` (**contributors:** the in-tree
   `just <plat> build*` recipes wrap these with the right `-D` args
   derived from `[image.<id>]`).
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
west build -b native_sim/native/64 nros/examples/zephyr/rust/talker
./build/zephyr/zephyr.exe
```

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
