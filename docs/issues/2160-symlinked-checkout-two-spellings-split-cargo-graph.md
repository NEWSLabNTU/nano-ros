---
id: 2160
title: "A checkout reached through a symlink builds with BOTH path spellings, so cargo sees two `nros-rmw` packages and refuses the graph on `links = \"nros_rmw\"`"
status: open
type: bug
area: [build, testing]
severity: low
found: 2026-10-30
related: [0375, 0849, 0616]
---

## What

Here `<real>` is the checkout's physical path and `<link>` the symlink a shell enters through.

On 2026-10-30 the main checkout moved to another disk (`<real>`), and
a symlink `<link>` points at it. A shell started at
`<link>` has `PWD=<link>`. From that shell,
`just ci gate` failed at `check::probe-workspace-caps`, 2 of 5 checks:

```
error: failed to select a version for `nros-rmw`.
    ... required by package `nros-rmw-zenoh v0.6.0 (<real>/packages/rmw/zenoh/nros-rmw-zenoh)`
package `nros-rmw` links to the native library `nros_rmw`, but it conflicts with a previous package which links to `nros_rmw` as well:
package `nros-rmw v0.6.0 (<link>/packages/core/nros-rmw)`
    ... which satisfies path dependency `nros-rmw` of package `nros-c v0.6.0 (<link>/packages/api/nros-c)`
```

One cargo graph reached the same crate through two directory spellings. Cargo
treats them as two packages, and both declare `links = "nros_rmw"`, so it
refuses to resolve. This is issue 0616's identity rule (a path SPELLING is part
of a crate's identity), reached through a symlink instead of a shared target
dir.

## Where the two spellings come from

- **Logical (the symlink):** `tests/cmake-probe-workspace-caps-tests.sh`
  computes `PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"`. `pwd` without `-P`
  keeps the caller's spelling, so `-DCMAKE_PREFIX_PATH`, `nano_ros_DIR` and
  `NanoRos_SOURCE_DIR` in the probe's `CMakeCache.txt` all read
  `<link>`.
- **Physical:** the nested image's own path dependency resolves to
  `<real>/...`, so the generated `nros-c-image` manifest
  reaches `nros-rmw-zenoh` by the real path.

Issue 0375 fixed the activation files (`activate.sh` uses `cd -P` / `pwd -P`)
and wrote down exactly this gap: "paths derived from the shell's cwd rather
than from the activation files ... still report whatever spelling the user
`cd`'d in with."

Sweep size, measured with `git grep -nE '&& pwd\)"' -- '*.sh' '*.just'`
excluding `pwd -P`: **257** logical sites, against 45 physical ones. Not every
site is a defect. One only fails when its value is joined with a physical path
in the same build graph.

## Workaround (measured)

Start from the physical path and strip the logical spelling from the
environment:

```bash
cd -P <real> && export PWD="$(pwd -P)" && unset OLDPWD DIRENV_DIR
export PATH="${PATH//<link>/<real>}"   # escape the slashes in real paths
source ./activate.sh
```

With that, `just check probe-workspace-caps` passes 5 of 5. CI checkouts are
never symlinked, so no CI lane sees this.

## What would close it

Resolve the repo root physically at the entry points the 0375 note names:
the `just` entry (`justfile_directory()` and its consumers), and the
`SCRIPT_DIR`/`PROJECT_ROOT` idiom in `tests/` and `scripts/`. Prefer one
shared helper over 257 hand edits (CLAUDE.md "fix the class"). Then re-run
`just ci gate` from a shell whose `PWD` is the symlink.
