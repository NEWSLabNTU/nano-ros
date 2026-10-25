---
id: 2058
title: "cyclonedds-sys's nested build runs the system make 4.3, which rejects the
  fixture driver's fifo jobserver — `build-test-fixtures lane=linux` stops"
status: open
type: bug
area: rmw, build
related: []
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
