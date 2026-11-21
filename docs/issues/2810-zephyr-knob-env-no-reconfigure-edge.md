---
id: 2810
title: "A knob's environment rung reaches a Zephyr build only on its first configure: an already-configured dir keeps the old value"
status: open
type: bug
area: [zephyr, build, knobs]
severity: medium
found: 2026-11-21
related: [2780, 0460, 0491, 0991]
---

## What fails

The knob ladder says the environment wins: `_nros_resolve_knob`
(`zephyr/cmake/nros_cargo_build.cmake`) prints
`nros: <KNOB>=<v> from environment (Kconfig says <k>) — environment wins`, and
the registry documents the canonical `NROS_<GROUP>_<NAME>` env spelling as
rung 1. That holds only for a build directory being configured for the first
time.

Measured on host 241 for issue 2780: both Zephyr C++ fixture images
(`workspace-zephyr-cpp-derived-sched-tasks` and the `levels-cpp` zephyr row)
were rebuilt with `NROS_ZEPHYR_HEAP_SIZE=327680` exported to
`just zephyr build-fixtures`. Both rebuilt images still booted with
`arena 115200 bytes` (the Kconfig default 114,688 plus the slab) and panicked at
the same callers as before. Their build dirs had been configured before. The
same two images rebuilt in a FRESH worktree are the control (queued on 241).

## Why

The value is read once, at host configure, and nothing re-runs that configure
when it changes:

- `_nros_resolve_knob` reads the environment through `_nros_knob_env`
  (`cmake/NanoRosKnobEnv.cmake`) and stores the answer `CACHE INTERNAL` as
  `NROS_RESOLVED_<KNOB>`, with `NROS_KNOB_SOURCE_<KNOB>`. The C lane bakes the
  cached value into its `cmake -E env` cargo command, and `nros-platform`'s
  `build.rs` reads the forwarded value.
- cmake has no dependency on an environment variable, so ninja never
  regenerates for one. A `CMAKE_CONFIGURE_DEPENDS` file would not help either:
  issue 0991 measured that ninja re-runs cmake only for an input newer than
  `build.ninja`.
- The fixture road decides whether to run west from a per-row signature
  (`scripts/build/zephyr-fixture-leaves.sh`, compared in
  `zephyr-fixture-run-one.sh`). Ambient knob env is not in it. So a configured
  leaf with an unchanged signature takes the incremental path and keeps the
  cached value, silently.

The cargo road does not have this defect: `nros-platform/build.rs` declares
`rerun-if-env-changed=NROS_ZEPHYR_HEAP_SIZE`. That makes this the 0460 class
(a knob reaching one road and not another) by a new route: there it was the
carrier, here it is the freshness edge.

A Kconfig `.config` reading the default is expected either way. The env rung
never writes Kconfig; it overrides only what the resolver hands on.

## Reach (not yet swept)

`_nros_knob_env` / `_nros_resolve_knob` are shared by `cmake/NanoRosKnobRegistry.cmake`,
`cmake/NanoRosSharedCargoDir.cmake`, `zephyr/cmake/nros_image_kconfig.cmake` and
`zephyr/cmake/nros_rmw_zenoh.cmake`. A plain `$ENV{NROS_*}` read appears 33 times
across 17 files under `cmake/` and `zephyr/cmake/`. Each configure-time read has
the same missing edge unless something else re-runs the configure. Which of
those reads are knobs (rung 1) and which are tool locations is the first step of
the sweep.

## How to tell, on a build dir

- The configure log line `nros: <KNOB>=<v> from environment` is absent.
- `NROS_KNOB_SOURCE_<KNOB>` in `CMakeCache.txt` reads `kconfig`, not `environment`.
- `just check knob-delivery <build-dir>` answers whether the value arrived.

## Workaround

State the knob through a rung that is a file the build already watches:
`[image.<id>.knobs.<group>] <name> = <value>` in the image's `system.toml`
(the RFC-0049 APP rung). Changing it re-renders the image's Kconfig and
re-configures. To force one leaf down the west path without wiping it, move its
`.nros-zephyr-fixture.sig` aside.

## Fix directions (no ruling yet)

1. Fold the resolved environment rungs into whatever decides "re-configure":
   the fixture signature, and `nros build`'s own west driver. The registry
   already lists every canonical and retired spelling, so the set is derivable,
   not authored.
2. A build-time check that compares the live environment against
   `NROS_KNOB_SOURCE_*` / `NROS_RESOLVED_*` and refuses (naming the knob and the
   re-configure command) instead of building with the stale value.
3. Retire the environment rung for build sizing in favour of the image rung.
   That is the larger change, and it is a design question for RFC-0103.

Option 2 is the smallest change that turns the silent case into a loud one on
every road; option 1 alone covers only the roads that compute a signature.
