# zephyr/cpp/cyclonedds/talker-aemv8r

Phase 117.14 — nros C++ pub/sub demo on the ARM FVP `Base_RevC AEMv8-R`
Cortex-A SMP target under Zephyr. Copy-out template per CLAUDE.md
examples convention.

## Build

```sh
just zephyr setup       # one-time: workspace + SDK + zephyr-lang-rust
just zephyr build-fvp-aemv8r-cyclonedds
```

Or manually:

```sh
cd zephyr-workspace
west build \
    -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp \
    -d build-aemv8r-cyclonedds \
    nano-ros/examples/zephyr/cpp/cyclonedds/talker-aemv8r
```

(Phase 140 — no `-DCMAKE_PREFIX_PATH` needed; the example's
`CMakeLists.txt` consumes nano-ros via the Phase 139 Zephyr
integration shell at `integrations/zephyr/`.)

The `boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf` overlay
is auto-picked when the board target matches; layer additional
fragments via `-DEXTRA_CONF_FILE=...`.

## Runtime

ARM FVP `Base_RevC AEMv8R` is license-gated (Arm Development
Studio). Pair with:

- A second `native_sim` instance running
  `examples/zephyr/cpp/dds/listener` for an in-tree round-trip.
- Stock ROS 2 `ros2 topic echo /chatter std_msgs/msg/Int32` for
  cross-stack interop.

## Wire backend

The example builds against the Zephyr nros module's Cyclone DDS
backend (`CONFIG_NROS_RMW_CYCLONEDDS=y`). The Cyclone DDS RMW in
`packages/dds/nros-rmw-cyclonedds/` is validated on POSIX (Phase
117.12 stock-RMW interop end-to-end) and on Zephyr `native_sim`
(Phase 11W / 171.0 — pub/sub + services); this FVP Cortex-A/R
target reuses that build glue. The wire format on this binary is
byte-equal with stock `rmw_cyclonedds_cpp` peers. (FVP build
verification for this board is tracked as Phase 171.0.c.)
