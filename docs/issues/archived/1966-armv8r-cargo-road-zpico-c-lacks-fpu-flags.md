---
id: 1966
title: "On the cargo road, zpico-sys's C for `armv8r-none-eabihf` gets `-mfloat-abi=hard` with no `-mfpu`, so the two Cortex-R52 boards cannot build their default `rmw-zenoh`"
status: resolved
resolved: 2026-10-20
resolved_in: "branch fix/issue-1966 (nros_zpico_build::platform_arch)"
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

## Resolution (2026-10-20)

The library compile was never the problem. `build_zenoh_pico_unified` already
applied the platform's `[arch.cortex-r52]` profile, because phase-385 W1 put
`cortex-r52` on the FreeRTOS descriptor's `arch_profiles` list. The compile that
failed was `probe_net_type_sizes`, the size probe zpico-sys runs before the
library build. It never read the profile. It carried per-triple flags of its
own: four arms that copied four `[arch.*]` profiles by hand (cortex-m3,
cortex-m4, ARMv7-R, rv32imc and rv64gc), and no arm at all for FreeRTOS. So
`armv8r-none-eabihf` probed with cc-rs's `-mfloat-abi=hard` and no `-mfpu`.

The lookup itself was spelled three times: the library and the two alias TUs
each walked `arch_profiles` on their own. `nros_zpico_build::platform_arch` is
now the one lookup. The build resolves the profile once per image and hands the
same entry to all four compiles: the library, both alias TUs and the probe. The
probe's hand-written arms are gone. Where the descriptor lists no profile for a
triple, the probe now gets cc-rs's defaults, exactly as the library does. Before
this change the probe could disagree with the library there.

The cmake road's `nros_armv8r_cflags_env` stays. It also carries the flags to
the board crates' own C on that road, and this issue is about the cargo road.

Measured:

- `check-feature-sweep.py --half cross --crate <board>` with the defaults (the
  board's `rmw-zenoh`) is `ok` for both `nros-board-mps3-an536-freertos` and
  `nros-board-s32z270-freertos`, so `STANDALONE_NO_DEFAULT` is empty.
- Mutation: with the probe's `apply_arch` removed, the mps3-an536 run fails in
  zpico-sys's build script again. Restored and compared with `cmp`.
- The full cross half: 58 of 65 runs are `ok` on this host, and the other 7 are
  the NuttX rows, which need a configured NuttX tree. With one supplied, the 6
  rows that build zpico-sys (`nros-platform`, `zpico-sys`, `nros-rmw-zenoh`,
  `nros-rmw-zenoh-staticlib`, `nros-c`, `nros-cpp`) are `ok`. That matters
  because the NuttX probe now gets `[arch.cortex-a7]`'s `-march=armv7-a`,
  where it used to get no arch flags. The board row,
  `nros-board-nuttx-qemu`, fails in its own build script, and it fails the same
  way with main's zpico build sources, so the cause is this host's setup.
- `just ci gate` results are in the PR.
