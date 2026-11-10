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
