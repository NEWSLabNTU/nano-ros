---
id: 1966
title: "On the cargo road, zpico-sys's C for `armv8r-none-eabihf` gets `-mfloat-abi=hard` with no `-mfpu`, so the two Cortex-R52 boards cannot build their default `rmw-zenoh`"
status: open
type: bug
area: [boards, build]
severity: low
found: 2026-10-19
related: [1960, 1561, phase-372, phase-471]
---

## What was measured

Found while closing issue 1960, whose feature-sweep `STANDALONE` run lints
each excluded board crate with the environment its `nros-board.toml` states.
`cargo clippy --locked --target armv8r-none-eabihf` in
`packages/boards/nros-board-mps3-an536-freertos` (and the same in
`nros-board-s32z270-freertos`), with the board's `[env]` (`FREERTOS_PORT`,
`FREERTOS_CONFIG_DIR`), fails in zpico-sys's build script:

```
cc1: error: '-mfloat-abi=hard': selected architecture lacks an FPU
zpico-sys size_probe failed for cross target armv8r-none-eabihf
```

cc-rs derives the flags from the Rust triple. For `armv8r-none-eabihf` it adds
`-mfloat-abi=hard` and no `-mfpu`, which gcc refuses. The board's own C is
fine, because `nros-board-common`'s family builder reads the `[arch.cortex-r52]`
profile in `nros-platform-freertos/nros-platform.toml`. zpico-sys's C build
does not read that profile.

## Why it has not bitten

The only images built for these boards use the cmake road (the
`workspace-cpp-{s32z270,mps3-an536}-freertos` fixture rows, Cyclone DDS).
There, `nros_armv8r_cflags_env` in `cmake/NanoRosCargoProfile.cmake` hands
cargo `CFLAGS_armv8r_none_eabihf=-mcpu=cortex-r52 -mfpu=neon-fp-armv8
-mfloat-abi=hard`. The cargo road has no carrier for that fact, so a cargo
image for either board with zenoh would fail the same way. This is issue
1561's class: a compiler fact stated in one build system and not the other.

## Direction

zpico-sys's C build (`nros-zpico-build`) should take its arch flags from the
same `arch_flags::cflags_for_target` profile the board builders read, so the
flags have one source on both roads. Until then the feature sweep lints these
two boards `--no-default-features` (`STANDALONE_NO_DEFAULT` in
`scripts/check-feature-sweep.py`); drop those entries once the defaults build.
