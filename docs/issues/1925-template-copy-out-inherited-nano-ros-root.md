---
id: 1925
title: "`check-template-copy-out` and `check-scaffold-builds` configure a copied-out CMake project with no `-Dnano_ros_ROOT`: a clean shell cannot find nano_ros, a worktree shell builds the parent checkout"
status: open
type: bug
area: [build, testing, cmake]
severity: medium
found: 2026-10-16
related: [1280, 1840, 1108]
---

## Measured

Both gates copy a project OUT of the checkout (a template, or an `nros new`
scaffold in `$TMPDIR`) and then run a bare

```
cmake -S <copy> -B <copy>/build [-DCMAKE_TOOLCHAIN_FILE=…]
```

with no `-Dnano_ros_ROOT`. A copy outside the checkout has no walk-up, so
`find_package(nano_ros)` has exactly one input left: whatever `nano_ros_ROOT`
the CALLER's environment holds. Since RFC-0103 D6 (amended 2026-10-12)
`activate.sh` exports none, and since issue 1840 it UNSETS one that names
another checkout. Measured in a linked worktree
(`/mnt/mx500/aeon/nros-worktrees/ptco`, origin/main `b7e63a5b44`), invoking the
scripts directly after `source ./activate.sh`:

**(a) clean env** (`env -u nano_ros_ROOT`):

```
$ scripts/check-template-copy-out.sh cpp-port-minimal-publisher
  cpp-port-minimal-publisher/mps2-an385-freertos: FAIL — the copy does not build
      CMake Error at CMakeLists.txt:13 (find_package):
        … Could not find a package configuration file provided by "nano_ros" …
check-template-copy-out: FAILED — a template a user copies does not build.

$ scripts/check-scaffold-builds.sh c_project       # same class
  CMake Error at CMakeLists.txt:9 (find_package): … "nano_ros" …
  CMakeCache.txt: nano_ros_DIR:PATH=nano_ros_DIR-NOTFOUND
```

**(b) `nano_ros_ROOT=<main>` inherited** (the main checkout —
what a worktree spawned from a pre-D6 shell holds; a `cmake` shim recorded the
cache after each configure):

```
check-template-copy-out (mps2-an385-freertos copy):
  nano_ros_DIR:PATH=<main>
  -- nano-ros: Corrosion 0.6.1 via SDK store … (asked by <main>)
  CMake Error at <main>/cmake/NanoRosRmwDispatch.cmake:255 …
  <main>/packages/api/nros-c/CMakeLists.txt:60 (nros_feature_set)
  (the main checkout's cmake driving the worktree's `nros`:
   "unrecognized subcommand 'entity-facts'")

check-scaffold-builds c_project:
  nano_ros_DIR:PATH=<main>
  CMake Error at <main>/cmake/bootstrap.cmake:97 …
```

So the gate measured the PARENT checkout's CMake tree — issue 1280's class: an
inherited absolute path outranks the checkout being built.

## Why `just check …` hid it

`just/sdk-env.just` exports `nano_ros_ROOT := _NROS_HERE` to every recipe, so
`just check template-copy-out` and `just check scaffold-builds` happened to get
THIS checkout's root in both shapes (measured: `just --evaluate nano_ros_ROOT`
prints the worktree with the variable unset AND with it inherited from main).
The scripts themselves carry a `Usage:` line and are run directly to narrow to
one template, and on that road the correctness of the gate depended on an
implicit just-only export it never named. The road a user is told to take
(book `first-node-c.md`, `freertos.md`) passes `-Dnano_ros_ROOT=…` explicitly;
the gates that exist to reproduce that road did not.

## Fix direction

One helper both gates configure through, which names THIS checkout's root
explicitly (RFC-0103 D6 rung 1, `-Dnano_ros_ROOT`) and drops the inherited
`nano_ros_ROOT`/`NROS_REPO_DIR` from the configure's environment, plus a gate
that refuses a bare `cmake` configure in a `scripts/check-*.sh` gate.
