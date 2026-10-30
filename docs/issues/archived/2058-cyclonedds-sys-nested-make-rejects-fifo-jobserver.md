---
id: 2058
title: "cyclonedds-sys's nested build runs the system make 4.3, which rejects the
  fixture driver's fifo jobserver — `build-test-fixtures lane=linux` stops"
status: resolved
type: bug
area: rmw, build
related: [2119]
resolved: 2026-10-30
---

## Symptom

`just build-test-fixtures lane=linux` (2026-10-25, worktree off main
`5cbd55a6e0` plus phase-515 C1) fails in the Rust stage, before any C row runs:

```
running: ... MAKEFLAGS="-j --jobserver-fds=fifo:/tmp/GMfifo2985646 --jobserver-auth=fifo:/tmp/GMfifo2985646" "cmake" "--build" ".../cyclonedds-sys-<hash>/out/build" "--target" "install" ...
gmake[2]: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/GMfifo2985646'.  Stop.
thread 'main' panicked at .../cmake-0.1.58/src/lib.rs:1132:5
make[1]: *** [build/fixtures-build-make/linux-rust-cargo-all-*.mk:100: fixture-0030] Error 1
```

## Cause (read from the tree, not bisected)

* `scripts/build/fixture-make-driver.sh` runs the PINNED make
  (`nros sdk-path make` → 4.4.1) with `--jobserver-style=fifo`, so every child
  inherits `MAKEFLAGS` naming a fifo.
* `packages/rmw/cyclonedds/cyclonedds-sys/build.rs` builds Cyclone with the
  `cmake` crate's default generator, Unix Makefiles. CMake resolves
  `CMAKE_MAKE_PROGRAM` itself and finds `/usr/bin/gmake`, GNU Make 4.3 on this
  host.
* make 4.3 understands only the pipe-FD jobserver. Handed a fifo auth string it
  stops with `internal error`.

`scripts/build/jobserver-pool.sh` already states the constraint: "pinned make
4.4 is required for `--jobserver-style=fifo`; the system make may be 4.3, whose
pipe-based jobserver child tools cannot join". This build script is a child the
pool does not reach. It is the only `cmake::Config::new` in the tree
(`git grep -l 'cmake::Config::new' -- '*build.rs'`).

## Direction

Make the nested build JOIN the pool rather than strip it: name the pinned make
(or the pinned ninja, which joins make 4.4's fifo since 1.13) as the generator's
program, resolved the way `nros_pinned_make` / `nros_pinned_ninja` resolve it,
never from PATH. Stripping `MAKEFLAGS` would make it build, but unbounded
(`-j` with no jobserver) inside a 32-way pool.

Acceptance: a BUILD — `just build-test-fixtures lane=linux` gets past
`fixture-0030` on a host whose system make is 4.3.

## Resolution (2026-10-30)

Issue 2119 (`fd4d6008eb`, 2026-10-29) fixed the drivers' half: every script
that runs the pinned make as a jobserver now puts its directory first on
`PATH`, so a child's `gmake` search finds 4.4.1. That fix keeps working only as
long as every road into the build script prepends the directory. This change
is the build script's own half, which is the direction above: the nested build
names its make itself.

- `nros_build_paths::jobserver` (`packages/tooling/nros-build-paths/src/jobserver.rs`):
  `pinned_make()` builds `<store>/sdk/make/<pin>/bin/make` from the index pin
  (`[tool.make]`). It never searches `PATH` and never lists the store,
  the same construction `nros_pinned_make` uses. `choose()` is the decision as a
  pure function. If the pin is provisioned, use it, with or without a jobserver,
  because `CMAKE_MAKE_PROGRAM` is cached. If there is no pin and a FIFO
  jobserver is inherited (`CARGO_MAKEFLAGS` naming `--jobserver-auth=fifo:`
  or `--jobserver-fds=fifo:`), print a `cargo:warning` naming
  `nros setup --tool make`. Otherwise do nothing.
- `cyclonedds-sys/build.rs` passes the pinned make as `CMAKE_MAKE_PROGRAM`, so
  the nested `cmake --build` JOINS the pool. `MAKEFLAGS` is not stripped.
- Sweep: `git grep -l 'cmake::Config::new' -- '*build.rs'` still finds only
  `cyclonedds-sys`. The CLI's own cmake runs (metadata probe, entity census)
  are not build scripts. They use Ninja or the drivers' `PATH` from 2119.

Measured in the ros2 distrobox (system `/usr/bin/gmake` 4.3), with a pinned
make 4.4.1 `--jobserver-style=fifo -j4` pool whose recipe is
`cargo build -p cyclonedds-sys`. The pinned make's directory was removed
from `PATH`, which is the state 2119 found in a linked worktree:

- before (build.rs at `origin/main`): `gmake[1]: *** internal error: invalid
  --jobserver-auth string 'fifo:/tmp/GMfifo726137'.  Stop.`, pool rc 2,
  cache `CMAKE_MAKE_PROGRAM:FILEPATH=/usr/bin/gmake`. This reproduces the
  symptom above exactly.
- after: `Finished`, pool rc 0, cache
  `CMAKE_MAKE_PROGRAM=<store>/sdk/make/4.4.1/bin/make`.

Unit tests `jobserver::tests` (4). Negative control: dropping the
`--jobserver-fds=` spelling, or using the pin only under a FIFO jobserver, fails
2 of the 4.

