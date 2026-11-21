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

## Reach (swept 2026-11-21, by reading)

Sweep:

```
git grep -n '\$ENV{NROS_' -- 'cmake/*.cmake' 'zephyr/cmake/*.cmake'
git grep -n '^[[:space:]]*_nros_\(resolve_knob\|resolve_derivable_knob\|knob_env\)(' -- cmake zephyr/cmake
```

**Registry knobs: one funnel.** Every configure-time read of a registry knob's
environment rung goes through `_nros_knob_env` (`cmake/NanoRosKnobEnv.cmake:207`):

| Site | Calls |
| --- | --- |
| `zephyr/cmake/nros_cargo_build.cmake` `_nros_resolve_knob(` | 63 |
| same file, `_nros_resolve_derivable_knob(` (calls the two above) | 32 |
| same file, direct `_nros_knob_env(` | 3 |
| `cmake/NanoRosSharedCargoDir.cmake:158`, "road 2" of the shared cargo dir KEY | 1 |

Two more sites read a knob's env with a bare `$ENV{}`, as a GUARD around a
resolve: `NROS_RMW_LOCAL_QUERYABLE` (`nros_cargo_build.cmake:1353`) and
`NROS_PARAM_SERVICE_BUFFER_SIZE` (`:1777`). They bypass the helper's
retired-spelling refusal, and they belong in it.

So the Zephyr fix is ONE helper, not ~100 sites. Anything that records or checks
the environment rung belongs in `_nros_knob_env`; the two bare guards move onto
it. The shared cargo dir key has the same lag: a changed knob env leaves the
cached key.

**The plain cmake road has no ladder of its own.** Outside `zephyr/cmake`,
nothing resolves a registry knob at configure. The env reaches cargo at BUILD
time, where the 16 `build.rs` that watch knob env (63
`rerun-if-env-changed`) catch a change. This comes from reading, not from a
measurement.

**Not knobs, same missing edge (out of this issue's fix).** These configure-time
reads change what is built, yet are tool, path or handshake variables rather
than ladder rungs:

- `NROS_INTERFACE_SEARCH_PATH`: 6 reads, in `NanoRosBundledInterfaces`,
  `NanoRosCodegenCore`, `NanoRosGenerateInterfaces` and `_NrosFindRosMsgPackage`.
- `NROS_CARGO_PROFILE` (`NanoRosCargoProfile.cmake:66`).
- `NROS_PLATFORM_CFFI_INCLUDE`, baked into the cargo command
  (`nros_cargo_build.cmake:2619`).
- `NROS_STACK_FLOOR_RAISE` (`NanoRosBoardNet.cmake:257`).

`NROS_STACK_FLOOR_RAISE` and `NROS_RESOLVED_ENTITY_RECORD_SHA256` are handed in
by `nros build` for the configure it runs, so they are handshakes, not operator
overrides.

Location and mode switches change no image content: `NROS_CLI`, `NROS_HOME`,
`NROS_GEN_CACHE_DIR`, `NROS_VERBOSE`, `NROS_LEAF_CENSUS_HOST` (7 reads) and
`NROS_ALLOW_UNSYNCED_MODEL`.

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
