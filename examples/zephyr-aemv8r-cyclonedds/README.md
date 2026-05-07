# zephyr-aemv8r-cyclonedds

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
    nano-ros/examples/zephyr-aemv8r-cyclonedds \
    -- -DCMAKE_PREFIX_PATH=$NROS_ROOT/build/install
```

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

The example builds against the Zephyr nros module's existing DDS
backend (`CONFIG_NROS_RMW_DDS=y`, dust-dds). The Cyclone DDS RMW
in `packages/dds/nros-rmw-cyclonedds/` lands on POSIX today (Phase
117.12 stock-RMW interop validated end-to-end); extending the
Zephyr nros module to ship a Cyclone build path is tracked
separately. Once it lands, swap `CONFIG_NROS_RMW_DDS=y` →
`CONFIG_NROS_RMW_CYCLONEDDS=y` (or whatever Kconfig symbol the
glue exposes) in `prj.conf` and the wire format on this binary
becomes byte-equal with stock `rmw_cyclonedds_cpp` peers.
