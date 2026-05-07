# nros-board-fvp-aemv8r-smp

ARM FVP `Base_RevC AEMv8-R` (Cortex-A AArch64 SMP) board crate for
nano-ros under Zephyr. Phase-117 reference platform for the Cyclone
DDS RMW backend on the Autoware safety-island stack.

## Build

```sh
just zephyr setup       # one-time: workspace + SDK + zephyr-lang-rust
west build \
    -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp \
    -d build-fvp \
    examples/zephyr/rust/dds/talker
```

The crate's `boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf` and
`.overlay` are picked up automatically by the Zephyr build when the
example's `boards/` directory contains them, or they can be layered
via `-DEXTRA_CONF_FILE=...` / `-DEXTRA_DTC_OVERLAY_FILE=...`.

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
