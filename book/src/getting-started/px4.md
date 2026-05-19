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
hooked in at configure time:

```text
my_drone_firmware/
├── PX4-Autopilot/                       # PX4 source tree (submodule)
└── px4-modules/                         # passed via EXTERNAL_MODULES_LOCATION
    └── nano-ros/                        # copy-out from integrations/px4/module-template/
        ├── CMakeLists.txt
        ├── Kconfig
        ├── nros_uorb_bridge.cpp         # the actual nano-ros app
        └── ...
```

The template at `integrations/px4/module-template/` is the
canonical copy-out source — vendor it into your firmware repo, then
point PX4 at its parent directory:

```bash
cmake -B build -S PX4-Autopilot \
      -DCONFIG=px4_fmu-v5_default \
      -DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules
```

Inside the module, the canonical pattern bridges uORB → nano-ros.
The module is a `PX4Module` subclass that runs in its own work
queue, opens an nros executor, and forwards uORB messages onto a
zenoh / DDS topic (or vice versa).

## Configure

PX4 uses Kconfig for module enablement:

```bash
cd PX4-Autopilot
make px4_fmu-v5_default menuconfig
# Navigate to:
#   External modules → nano-ros
#       [*] Enable nano-ros uORB bridge
#           RMW backend       (zenoh)         zenoh | xrce | dds
#           ROS 2 edition     (humble)
#           Default locator   "tcp/10.41.0.1:7447"
```

Build-time CMake cache vars also work:

```bash
cmake -B build -S PX4-Autopilot \
      -DCONFIG=px4_fmu-v5_default \
      -DEXTERNAL_MODULES_LOCATION=$PWD/px4-modules \
      -DNANO_ROS_RMW=zenoh
```

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
pxh> nros_uorb_bridge start

# Real hardware (Pixhawk): flash via QGroundControl or
#     `make px4_fmu-v5_default upload` over the bootloader USB

# Verify from stock ROS 2 on the same network:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /vehicle_local_position px4_msgs/msg/VehicleLocalPosition
```

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
  `nros_uorb_bridge.cpp` template's topic-table section.
- Multi-vehicle: PX4-XRCE-Agent → nano-ros XRCE backend gives you
  the standard PX4-ROS bridge with nano-ros on the autopilot side.
- For pure-NuttX (no PX4) firmware: see the
  [NuttX starter](./integration-nuttx.md).
