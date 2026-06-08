# Deployment Workflow

Deployment means different things per target, but the order is stable:
prepare toolchain, build package, move binary/firmware to target, then
verify ROS 2 communication.

## POSIX

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

For interop with stock ROS 2 over Zenoh, run the bundled router (built
by `just zenohd setup`) and point ROS 2 at it:

```bash
zenohd --listen tcp/127.0.0.1:7447
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
```

See [Native POSIX](../platform-guides/native-posix.md).

## RTOS and Bare-Metal

RTOS targets usually produce firmware images or simulator binaries:

```bash
just freertos build
just freertos test
```

For real hardware, deployment step becomes flash/load/monitor. For QEMU,
deployment is launching simulator with correct network setup.

There is no `nros deploy` / `nros build` / `nros run` verb — Phase 222
removed those wrappers. `nros` is provisioner + codegen + metadata only;
deployment runs on the **vendor's native tools**. The embedded deploy
contract is a documented three-step sequence (per
[RFC-0003 §4](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0003-rtos-integration-pattern.md)):

1. **Bake** — `nros codegen-system --bringup <pkg>` reads
   `system.toml` + `[deploy.<board>]` + `launch/*.xml` and emits the
   baked tree under `build/<board>/`.
2. **Build** — the vendor tool builds it: `cargo build` / `cmake --build`
   / `west build` / `idf.py build` (the `just <plat> build*` recipes wrap
   these with the right `-D` args derived from `[deploy.<board>]`).
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
just zephyr setup
source zephyr-workspace/env.sh
west build -b native_sim/native/64 nros/examples/zephyr/rust/talker
./build/zephyr/zephyr.exe
```

## ESP32

ESP32 deployment uses the Espressif toolchain and flash tool:

```bash
just esp32 build
just esp32 talker
```

For physical boards, use the platform guide's `espflash` path.

## Verify

After deployment, verify from ROS 2 side:

```bash
ros2 topic list
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

If discovery works but samples do not arrive, check domain ID, router
mode, QoS reliability, and platform network setup.
