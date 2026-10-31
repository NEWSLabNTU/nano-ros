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
hooked in at configure time. The template — shipped in the SDK root at
`integrations/px4/module-template/` (`nros sdk-root` prints where) — has
the PX4-required
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

> **Prereq.** PX4 is your own checkout — the firmware tree you
> already build. Clone it (or use the one you have) and initialize its
> submodules; nano-ros is tested against PX4 v1.17.0.

```bash
git clone --branch v1.17.0 https://github.com/PX4/PX4-Autopilot.git
git -C PX4-Autopilot submodule update --init \
    --depth 1 --recommend-shallow --recursive
```

PX4's own Python dependencies come from its `Tools/setup` scripts
(e.g. `PX4-Autopilot/Tools/setup/ubuntu.sh`) — run those per PX4's
documentation if the firmware build complains about missing Python
modules.

Copy the template into your firmware repo; PX4 is pointed at the copy at
build time:

```bash
mkdir -p px4-modules
cp -r "$(nros sdk-root)/integrations/px4/module-template" px4-modules/nano-ros
```

(Releases after 0.6.0-nros4 ship the template; with an older one, copy
it from
[GitHub](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/px4/module-template).)

PX4's own `make <target>` is the build verb, and it configures and builds ONE
tree, `PX4-Autopilot/build/<target>/`. The module joins it through
`EXTERNAL_MODULES_LOCATION`, passed on that same `make` line — PX4's
`Makefile` forwards it to its CMake configure — and pointing at the dir that
contains the template's `src/` (that's `px4-modules/nano-ros`, not its
parent: PX4's root `CMakeLists.txt` does
`add_subdirectory("${EXTERNAL_MODULES_LOCATION}/src" …)`).

The template finds nano-ros itself: an explicit `-Dnano_ros_ROOT=<path>`
cache variable wins, otherwise it asks the `nros` on PATH (`nros sdk-root`),
which answers with the SDK that binary belongs to. No environment variable is
read (RFC-0103 D6). To name the root anyway, pass it through PX4's
`CMAKE_ARGS`:

```bash
make -C PX4-Autopilot px4_sitl_default \
    EXTERNAL_MODULES_LOCATION="$PWD/px4-modules/nano-ros" \
    CMAKE_ARGS="-Dnano_ros_ROOT=$(nros sdk-root)"    # optional
```

> **Not replayed.** No probe track runs the PX4 SITL build yet, so this line is unmeasured on the installed track ([issue 2176](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2176-unprobed-fvp-sitl-flash-lines.md)).

Inside the module, the canonical pattern bridges uORB → nano-ros.
The module is a `PX4Module` subclass that runs in its own work
queue, opens an nros executor, and forwards uORB messages onto a
zenoh / DDS topic (or vice versa). Edit `nano_ros_app.cpp` to add
your topic bindings.

## Configure

The template does **not** ship a Kconfig overlay (no `Kconfig.projbuild`
files). Module enablement is implicit once `EXTERNAL_MODULES_LOCATION`
points at the template. Which RMW backends the module registers is the
`BACKENDS` list of `nros_px4_add_module(...)` in
`src/modules/nano_ros_app/CMakeLists.txt` — the template ships `BACKENDS uorb`
— not a cache variable. (Adding a Kconfig overlay so the module appears under
`menuconfig → External modules` is a follow-up task; for now the template is
always enabled.)

## Build

One `make`, one tree. For the SITL simulator (POSIX target — easier to
develop against), or for a Pixhawk (`px4_fmu-v5_default` here; use your
board's PX4 target):

```bash
make -C PX4-Autopilot px4_sitl_default \
    EXTERNAL_MODULES_LOCATION="$PWD/px4-modules/nano-ros"
make -C PX4-Autopilot px4_fmu-v5_default \
    EXTERNAL_MODULES_LOCATION="$PWD/px4-modules/nano-ros"
```

The firmware lands in PX4's own tree, `PX4-Autopilot/build/<target>/`
(`px4_fmu-v5_default.px4` for the Pixhawk). The first build cross-compiles
nano-ros's Rust staticlibs alongside PX4's NuttX kernel + apps (~10 min on a
fresh checkout). Note that the module links prebuilt nano-ros archives
(`nros_px4_add_module` resolves them at configure time and, when one is
missing, stops naming the command that builds it).

## Run on the SITL simulator

PX4's SITL runs the autopilot as a host process against a simulator. With
Gazebo (Harmonic) installed per PX4's documentation:

```bash
make -C PX4-Autopilot px4_sitl_default gz_x500 \
    EXTERNAL_MODULES_LOCATION="$PWD/px4-modules/nano-ros"
# In the PX4 console:
pxh> nano_ros_app start

# Verify from stock ROS 2 on the same network (only after you have
# added uORB → nano-ros forwarders to nano_ros_app.cpp — the shipped
# template forwards nothing on its own):
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /vehicle_local_position px4_msgs/msg/VehicleLocalPosition
```

## Flash a real board

PX4's own upload target flashes over the bootloader USB, with the same
`EXTERNAL_MODULES_LOCATION` as the build (it rebuilds if anything changed):

```bash
make -C PX4-Autopilot px4_fmu-v5_default upload \
    EXTERNAL_MODULES_LOCATION="$PWD/px4-modules/nano-ros"
```

> **Not replayed.** No probe runs this flash line (a CI runner has no board), so it is written from the board's runner, not measured ([issue 2176](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2176-unprobed-fvp-sitl-flash-lines.md)).

QGroundControl can flash the same `build/px4_fmu-v5_default/px4_fmu-v5_default.px4`
instead. Start the module from the NSH console (`nano_ros_app start`) or the
board's startup script.

**Readiness signal.** After `nano_ros_app start` in the PX4
console, the shipped template logs `nano-ros uORB backend
registered` and returns immediately — it doesn't start a
publisher loop or print a "bridge started" line on its own.
You're expected to edit `nano_ros_app.cpp` to wire your
uORB → nano-ros forwards. If `nano_ros_app` reports `register
failed`, check:

1. Module didn't register — the template logs
   `nros_rmw_uorb_register() -> <rc>` on startup (search the PX4 boot
   log for `nros_rmw_uorb_register`). A non-zero `<rc>` usually means
   a NuttX kernel-config / feature-gate mismatch.
2. Once you've added forwarders, `zenohd` must be reachable from
   the autopilot's network (Pixhawk: configured via QGroundControl
   or `param set`).
3. uORB topic not advertised yet — start the upstream PX4 module
   that publishes it (`commander start` etc.) first.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- PX4 external-module template:
  [`integrations/px4/module-template/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/px4/module-template)
- Worked PX4 example:
  [`packages/testing/nros-px4-register-check/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/testing/nros-px4-register-check)
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
