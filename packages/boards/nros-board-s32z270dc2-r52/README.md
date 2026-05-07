# nros-board-s32z270dc2-r52

NXP X-S32Z270-DC (DC2) evaluation board, RTU0 Cortex-R52 cores under
Zephyr. Phase-117 production-silicon target for the Cyclone DDS RMW
backend on the Autoware safety-island stack.

## Build

```sh
just zephyr setup        # one-time: workspace + SDK + zephyr-lang-rust
just zephyr build-s32z   # Phase 117.13 build smoke
```

Or manually:

```sh
west build \
    -b s32z2xxdc2/s32z270/rtu0/D \
    -d build-s32z \
    examples/zephyr/rust/dds/talker \
    -- -DEXTRA_CONF_FILE=$NROS_ROOT/packages/boards/nros-board-s32z270dc2-r52/boards/s32z2xxdc2_s32z270_rtu0_D.conf
```

## Runtime

Hardware-gated:
- NXP X-S32Z270-DC (DC2) evaluation board.
- NXP S32 Design Studio + S32 Debug Probe (or Lauterbach Trace32) for
  flashing. Configure via `west flash --runner nxp_s32dbg` or
  `--runner trace32`.

CI build smoke is the gating check; runtime / interop validation
against `rmw_cyclonedds_cpp` peers is tracked separately and depends
on board availability.

## Status

- Phase 117.11 — config + skeleton landed.
- Phase 117.13 — `just zephyr build-s32z` smoke shares the same
  toolchain plumbing as the FVP target (Cortex-R AArch32 Rust patch).
