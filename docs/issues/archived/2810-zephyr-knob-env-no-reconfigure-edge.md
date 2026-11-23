---
id: 2810
title: "A knob's environment rung reaches a Zephyr build only on its first configure: an already-configured dir keeps the old value"
status: resolved
type: bug
area: [zephyr, build, knobs]
severity: medium
found: 2026-11-21
resolved: 2026-11-23
resolved_in: \"825231f733 (the Zephyr knob refusal); the residue: b61082063c (#4656, issue 2869)"
related: [2780, 0460, 0491, 0991, 2869]
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

## Fix directions

1. Fold the resolved environment rungs into whatever decides "re-configure":
   the fixture signature, and `nros build`'s own west driver.
2. A build-time check that compares the live environment against what the
   configure read, and refuses (naming the knob and the re-configure command)
   instead of building with the stale value.
3. Retire the environment rung for build sizing in favour of the image rung.

## Ruling (2026-11-21)

The maintainer ruled for option 2, in the one `_nros_knob_env` helper, plus
moving the 2 bare `$ENV{}` sites onto it. There is no automatic re-configure, and
the environment rung stays. Requirements:

- The always-run step compares only registry knob names, so it cannot refuse
  on unrelated environment.
- The refusal names the knob, its cached value and source, the live value, and
  the exact `cmake <build-dir>` command.
- A negative control: configure, change the environment, build, and it must
  refuse; re-configure and it passes.
- A build with nothing changed stays a no-op apart from the check itself.

## Fix

- `_nros_knob_env` records the FIRST read of each knob. Only the first, because
  `nros_set_cargo_env_from_kconfig` publishes every resolved knob back into the
  configure's own environment, so later reads see the configure's answer.
- `nros_knob_env_check_target` (`cmake/NanoRosKnobEnv.cmake`) defers writing
  `<build>/nros/knob-env-at-configure.cmake` to the end of the configure. It
  rewrites the file only when its content changes.
- The same function defines the always-run `nros_knob_env_check` target. It runs
  `cmake/NanoRosKnobEnvCheck.cmake` and has no outputs, so nothing depends on it
  by file.
- `zephyr/CMakeLists.txt` installs it right after `nros_resolve_knobs()`. Each
  `nros_cargo_build()` cargo step is ordered after it, so a refused build stops
  before compiling. The Rust lane gets it through `ALL`.
- The `NROS_RMW_LOCAL_QUERYABLE` and `NROS_PARAM_SERVICE_BUFFER_SIZE` guards
  read through the helper.

Only names the helper read are compared, and it is only called with registry
knobs, so an unrelated variable cannot trip it.

Neither `nros build` nor the Zephyr fixture road sets a registry knob in the
environment of the native build: the CLI's handoff env is `ZEPHYR_*`, the entity
record and the toolchain variables, and no Zephyr fixture row has an `env`
table. So a build that `nros build` or the fixture road configured and built in
one go never refuses.

Gate: `just check knob-env-refusal`. Rule 1 refuses a bare `$ENV{}` read of
any of the 88 resolved knobs under `cmake/` or `zephyr/`; rule 2 refuses a
Zephyr module that stops installing the check. Rule 3, the selftest, drives the
real helper through cmake + ninja:
- a knob changed, unset, or exported after the configure refuses, with every
  field named, and the refused build regenerates nothing;
- an unrelated variable passes, and so does a re-configured dir;
- a second build with nothing changed runs only the check.

Mutation-tested:
- restoring a bare read, removing the install, a check that never refuses, and
  a record that keeps the last read each turn it red;
- giving the check a byproduct nothing depends on stays green, an equivalent
  mutant.

Still open after this fix:

- The plain cmake road. `cmake/NanoRosSharedCargoDir.cmake` also records its
  reads there, but no check is installed. Its env reaches cargo at build time,
  and the shared cargo dir key is its only configure-time use.
- The non-knob reads listed under Reach.

## Resolved (2026-11-23)

Both residue items are answered by issue 2869 (#4656, merged as b61082063c):

- **The non-knob reads** (`NROS_CARGO_PROFILE`, `NROS_INTERFACE_SEARCH_PATH`,
  `NROS_PLATFORM_CFFI_INCLUDE`, `NROS_STACK_FLOOR`) are refused through
  `nros_configure_env_watch`, on the host road and the Zephyr road. Every
  other configure-time read is classified there, with the reason it is not
  refused.
- **The plain cmake road's registry knobs** are deliberately not compared.
  Their environment reaches cargo at build time, where each build script's
  `rerun-if-env-changed` sees a change, so a stale configure value cannot
  reach the image. The host record holds the watched names only; comparing
  the knobs there would refuse on ~90 names for no wrong output.
