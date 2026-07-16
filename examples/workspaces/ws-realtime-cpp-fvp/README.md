# ws-realtime-cpp-fvp — the two-tier C++ demo on the ARM FVP AEMv8-R over Cyclone DDS

The FVP variant of [`ws-realtime-cpp`](../ws-realtime-cpp/) (phase-292 W1.a):
ctrl (high tier, 10 ms) + telem (low tier, 100 ms) typed `std_msgs/Int32` C++
nodes over ONE shared Cyclone DDS session, deployed on the ARM Fast Models
`FVP_BaseR_AEMv8R` Cortex-A SMP target under Zephyr, one k_thread per tier
(`ZephyrBoard::run_tiers`, RFC-0015 Model 1).

This is the **Autoware Safety Island reference-consumer Entry shape** proven
in-tree (ASI phase-3 W2.b adopts exactly this):

- `src/fvp_entry/` — LAUNCH-only Entry. `nano_ros_use_board(fvp-aemv8r-smp)`
  BEFORE `find_package(Zephyr)` supplies BOARD id / base prj.conf / per-board
  Kconfig + DTS overlay / default RMW / the `armfvp` runner from the board
  crate; `find_package(nano_ros)` (287-W6) supplies the ament verbs; a single
  `nano_ros_add_executable(fvp_entry BOARD zephyr LAUNCH
  "demo_bringup:system.launch.xml" TYPED DEPLOY zephyr)` generates the whole
  entry. No hand-wired Zephyr compile context, no `-b` flag.
- `src/{ctrl_pkg,telem_pkg}` — component-only node pkgs (identical to
  ws-realtime-cpp's).
- `src/demo_bringup` — `system.toml` with `rmw = "cyclonedds"` +
  `[tiers.*.zephyr]` raw priorities.

## Build

```bash
just zephyr build-fvp-ws-entry     # or: part of `just zephyr build-fvp-all`
```

which is:

```bash
cd zephyr-workspace
west build -d build-fvp-ws-entry \
  <repo>/examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry \
  -- -D_NANO_ROS_CODEGEN_TOOL=<nros>
```

## Run

License-gated on the ARM FVP (`nros doctor --board fvp-aemv8r-smp`):

```bash
cd zephyr-workspace && west fvp run -d build-fvp-ws-entry
```
