# PX4 Autopilot (external module)

Single-node starter on PX4 Autopilot via the **external-module
copy-out template**. PX4's `EXTERNAL_MODULES_LOCATION` pattern lets
downstream firmware drop in nano-ros without forking PX4 itself.
C++ only — PX4's uORB binding is C++-only (Rust + C not in the
[coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md)).

> **Prereqs.** A PX4-Autopilot ≥ v1.16 clone, the matching cross
> toolchain (e.g. `gcc-arm-none-eabi` for Pixhawk targets), and
> `python3` with the PX4 development requirements installed
> (`bash ./Tools/setup/ubuntu.sh` once).

## Project layout

PX4 external modules live **outside** the PX4 source tree and are
hooked in at configure time. The template at
`integrations/px4/module-template/` has the PX4-required
`src/modules/<name>/` shape:

```text
my_drone_firmware/
├── PX4-Autopilot/                          # PX4 source tree (submodule)
└── px4-modules/                            # passed via EXTERNAL_MODULES_LOCATION
    └── nano-ros/                           # copy-out from integrations/px4/module-template/
        └── src/
            ├── CMakeLists.txt              # populates config_module_list_external
            └── modules/
                └── nano_ros_app/
                    ├── CMakeLists.txt      # px4_add_module(... MAIN nano_ros_app)
                    └── nano_ros_app.cpp    # the user-editable app
```

> **Prereq.** PX4 is a full-tier dependency. Run
> `just setup px4` first to populate `third-party/px4/PX4-Autopilot`
> and `third-party/px4/px4-rs`. `just px4 doctor` reports the gap
> on a fresh clone.

```bash
just setup px4              # equivalent to: just px4 setup
just px4 doctor
```

Vendor the template into your firmware repo, then point PX4 at its
parent directory + tell the template where nano-ros lives via
`NANO_ROS_DIR`:

```bash
cmake -B build -S PX4-Autopilot \
      -DCONFIG=px4_fmu-v5_default \
      -DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules \
      -DNANO_ROS_DIR=$PWD/../nano-ros            # point at your nano-ros clone
```

Inside the module, the canonical pattern bridges uORB → nano-ros.
The module is a `PX4Module` subclass that runs in its own work
queue, opens an nros executor, and forwards uORB messages onto a
zenoh / DDS topic (or vice versa). Edit `nano_ros_app.cpp` to add
your topic bindings.

## Configure

The template does **not** ship a Kconfig overlay (no `Kconfig.projbuild`
files). Module enablement is implicit once `EXTERNAL_MODULES_LOCATION`
points at the template's parent. Pass RMW + ROS-edition selection
via CMake cache vars rather than menuconfig:

```bash
cmake -B build -S PX4-Autopilot \
      -DCONFIG=px4_fmu-v5_default \
      -DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules \
      -DNANO_ROS_DIR=$PWD/../nano-ros \
      -DNANO_ROS_RMW=zenoh
```

(Adding a Kconfig overlay so the module appears under
`menuconfig → External modules` is a follow-up task; for now the
template is always enabled.)

## Build

```bash
cd PX4-Autopilot
make px4_fmu-v5_default
# Or for the SITL simulator (POSIX target — easier to develop against):
make px4_sitl_default gazebo
```

The first build cross-compiles nano-ros's Rust staticlibs alongside
PX4's NuttX kernel + apps (~10 min on a fresh checkout).

## Run

```bash
# SITL: PX4 boots Gazebo + the autopilot binary
cd PX4-Autopilot
make px4_sitl_default gazebo
# In the PX4 console:
pxh> nano_ros_app start

# Real hardware (Pixhawk): flash via QGroundControl or
#     `make px4_fmu-v5_default upload` over the bootloader USB

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /vehicle_local_position px4_msgs/msg/VehicleLocalPosition
```

**Readiness signal.** After `nano_ros_app start` in the PX4
console, expect `INFO  [nano-ros] bridge started` plus messages
flowing within 5 seconds. If no bridge log:

1. uORB topic not advertised yet — start the upstream PX4 module
   that publishes it (`commander start` etc.) first.
2. `zenohd` unreachable — Pixhawk's network config (set via
   QGroundControl or `param set`) needs to route to the host
   running zenohd.
3. Module didn't register — check the PX4 boot log for
   `nano-ros: register failed`.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- PX4 external-module template:
  [`integrations/px4/module-template/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/px4/module-template)
- Worked PX4 example:
  [`examples/px4/cpp/uorb/nros-register-check/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/px4/cpp/uorb/nros-register-check)
- PX4 integration roadmap notes:
  [`integrations/px4/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/px4/README.md)

## Constraints to be aware of

- **uORB binding is C++-only.** The PX4 example collapses uORB
  registration to a C++ port; Rust / C variants exist for non-PX4
  RTOSes but not for PX4.
- **PX4's NuttX kernel.** Underneath, PX4 runs NuttX; if you need
  to debug at the kernel layer, the
  [NuttX starter](./integration-nuttx.md) page applies too.
- **uORB throughput vs zenoh hops.** uORB is in-process pub/sub at
  ~µs latency; zenoh adds network-RTT. Plan accordingly when
  bridging high-rate streams.

## Next

- Add your own uORB topics to the bridge: see the
  `nano_ros_app.cpp` template's topic-table section.
- Multi-vehicle: PX4-XRCE-Agent → nano-ros XRCE backend gives you
  the standard PX4-ROS bridge with nano-ros on the autopilot side.
- For pure-NuttX (no PX4) firmware: see the
  [NuttX starter](./integration-nuttx.md).
