---
id: 2421
title: "The `just` fan-out pool is always a FIFO, so a unit whose build dir cached make 4.3 would stop on `invalid --jobserver-auth`"
status: open
type: bug
severity: low
area: [build, testing]
found: 2026-11-10
related: [issue-2241, issue-2119, issue-2058]
---

Split out of [issue 2241](archived/2241-live-peer-fifo-jobserver-without-pinned-make.md),
whose fix covers `nros build` only. No sighting yet; this is the same class,
found by reading the code.

## The reach

`scripts/build/jobserver-pool.sh` runs the pinned make 4.4 with
`--jobserver-style=fifo`, and every unit inherits
`--jobserver-auth=fifo:`. Issue 2119 put the pinned make first on the
children's `PATH`, so a NEW configure caches make 4.4. A build dir configured
earlier keeps the make its `CMakeCache.txt` recorded (`CMAKE_MAKE_PROGRAM`,
or a `find_program(MAKE …)` result such as Zephyr native_sim's), and that
make runs whatever is on `PATH` now. If it is make 4.3, the unit stops with
`invalid --jobserver-auth string 'fifo:…'`, the third sighting of 2241.
Re-running `cmake <dir>` does not refresh a cached `find_program` entry.

## Why 2241's fix does not carry over as is

`nros build` knows the build dirs its plans drive before it creates the
pool, so it reads their caches and falls back to the pipe form. A pool unit
is an opaque command line, and the pool does not know its build dirs.
The pipe form also costs the pool its main purpose here: ninja (≥ 1.13)
joins only the FIFO form, so every Ninja unit would run at its own width.

## What would close it

The likely answer is to repair the stale cache rather than downgrade the
pool. A fixture driver that owns a build dir would ask the same helper
`nros build` now uses (`builder::jobserver::cached_makes`, exposed as one
`nros` subcommand so the Rust and shell sides cannot drift). If a cached
make is older than 4.4, the driver would clear that one cache entry before
it builds. Acceptance: a fixture dir configured with `MAKE:FILEPATH` set to
a make 4.3 builds under the pool.

## Fix (2026-11-11)

The issue's suggested shape: repair the stale cache rather than downgrade the
pool.

- **One helper, in the CLI.** `builder::jobserver::repair_cached_makes`
  reads the same caches `cached_makes` does (the dir's `CMakeCache.txt` and
  each immediate subdirectory's) and rewrites every `CACHED_MAKE_KEYS` entry
  whose make is older than `MAKE_FIFO` or no longer answers `--version`, to
  the make first on `PATH`. It rewrites rather than deletes: `CMakeCache.txt`
  is an input of ninja's `RERUN_CMAKE` edge (measured), so the next build
  regenerates with the new path, and a Makefiles generator's `cmake --build`
  still finds its `CMAKE_MAKE_PROGRAM`. Exposed as
  `nros dev jobserver repair-makes <dir>…`, which does nothing when the make
  on `PATH` is itself below 4.4.
- **Rule 5 of `cmake-cache-guard.sh`**, `nros_cmake_guard_stale_make <dir>`,
  calls it only when `MAKEFLAGS` carries `--jobserver-auth=fifo:`, so an old
  make outside a pool is left alone. It runs from `nros_cmake_guard_build_dir`
  (cmake-incremental, fixture-matrix, fixtures-build's workers, which
  `export -f` it), and before the build in `west-fixtures.sh`,
  `workspace-fixtures-build.sh` (whose `nros build` joins the sweep's pool
  rather than choosing its own style), `compile-check-fixtures.sh`'s
  persistent Makefiles dirs, and `zephyr-fixture-run-one.sh` (on the build's
  own `PATH`). A CLI that cannot answer is reported by name, never silently.

Tested:

- Rust: a 4.3 make and a vanished one are rewritten in the dir's cache and a
  sysbuild domain's, while ninja, a 4.4 make and every other line are left
  byte for byte; a second pass finds nothing. End to end over real
  `--version` probes with fake makes on a constructed `PATH`.
- Shell: `tests/cmake-guard-stale-make-tests.sh` (gate
  `cmake-guard-stale-make`), against a stub CLI. Outside a FIFO pool the CLI
  is not asked. Inside one it is asked with the dir, also from a shell that
  sourced only the guard and from an `export -f` copy in a fresh bash. A
  failing CLI is reported.
- The acceptance, locally: a Ninja dir configured with `MAKE:FILEPATH` set
  to a make answering 4.3 (and refusing `--jobserver-auth`), built under the
  pinned make's `-j4 --jobserver-style=fifo`. Without the guard the build
  stopped (`subcommand failed`); with it, the target ran the pinned 4.4.1.

Sweep: `git grep -n 'nros_cmake_guard_retired_names\|cmake -S' -- scripts/build`.
Every persistent build dir a pool unit configures or builds is reached. The
census pre-pass and `nros build`'s own pool are covered by issue 2241.
