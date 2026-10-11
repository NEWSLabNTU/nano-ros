# nros-board-fvp-aemv8r-smp

ARM FVP `Base_RevC AEMv8-R` (Cortex-A AArch64 SMP) board crate for
nano-ros under Zephyr. Phase-117 reference platform for the Cyclone
DDS RMW backend on the Autoware safety-island stack.

## Build

```sh
just setup zephyr       # one-time: workspace + SDK + zephyr-lang-rust
just zephyr build-fvp-ws-entry   # nros build demo_bringup:fvp in examples/workspaces/realtime-cpp
```

This package's `prj.conf`, `boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf`
and `.overlay` reach an application two ways, derived by one rule
(`board_projection::board_files`): `nros build` hands them to a GENERATED
west application as the first `EXTRA_CONF_FILE` / `EXTRA_DTC_OVERLAY_FILE`
entries, and a hand-written one gets them from `nano_ros_use_board(fvp-aemv8r-smp)`.

## Runtime

The FVP itself is license-gated (Arm Development Studio or the
standalone FVP package). Set `FVP_BIN=/path/to/FVP_BaseR_AEMv8R`
and the example's `west flash` target will spawn the runner with
the stock FVP options. CI build-only smoke is tracked in Phase
117.13.

## Status

- Phase 117.10 — config + skeleton landed.
- Phase 117.13 — `just zephyr build` smoke against this board, gated
  on `aarch64-zephyr-elf` toolchain in the SDK install.
- Runtime / interop validation against `rmw_cyclonedds_cpp` peers —
  blocked on FVP runner availability.
