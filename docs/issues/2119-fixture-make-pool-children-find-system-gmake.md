---
id: 2119
title: "`fixtures-build.sh` runs the pinned make 4.4 but leaves the system gmake 4.3 first on PATH, so a cargo leaf's cmake child cannot join the fifo jobserver"
status: open
type: bug
area: [build, testing, multi-session]
severity: medium
found: 2026-10-28
related: [issue-1638, issue-1280, issue-1403]
---

# The cargo fixture pool's children find the system gmake

**Home:** [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W2.

## Problem

`scripts/build/fixtures-build.sh` picks the pinned make for its row pool:

```sh
make_bin="make"
if [ -x "$(nros sdk-path make)/bin/make" ] && \
   "$(nros sdk-path make)/bin/make" --version | head -1 | grep -q "4.4"; then
    make_bin="$(nros sdk-path make)/bin/make"
fi
... "$make_bin" -j"$jobs" -f "$makefile"
```

It runs that make by absolute path and never puts its directory on `PATH`. Make
4.4 hands its children `MAKEFLAGS=--jobserver-auth=fifo:…`. A cargo row whose
build script configures CMake with "Unix Makefiles" (`cyclonedds-sys`)
caches whatever `gmake` `PATH` resolves. Without the pinned directory first
that is `/usr/bin/gmake`, GNU Make 4.3, which cannot parse a fifo jobserver:

```
gmake[2]: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/GMfifo289909'.  Stop.
thread 'main' panicked at …/cmake-0.1.58/src/lib.rs:1132:5:
command did not execute successfully, got: exit status: 2
```

and the cached value in that build's `CMakeCache.txt`:

```
CMAKE_MAKE_PROGRAM:FILEPATH=/usr/bin/gmake
```

This is issue 1638's class in a driver its fix did not reach.
`workspace-fixtures-build.sh` says so in its own comment: it lists
`fixture-make-driver.sh` and `build-all-jobserver.sh` as the drivers that
already prepend the pinned tools, and itself as the third. `fixtures-build.sh`
is a fourth.

## Why it hides

It only fails in a linked worktree. The main checkout's shell inherits a `PATH`
whose first entry is `<main>/third-party/make` (a make 4.4 with a `gmake`
link), so the child finds a 4.4 by accident. Issue 1280's re-rooting drops
that entry in a worktree, as 1638 intends, and the accident goes with it.
Measured 2026-10-28 in a fresh worktree: `just build-test-fixtures lane=linux`
failed in `build-fixture-rust` (linux) on the first cyclonedds row.

**Prepending by hand does NOT fix it, and that is the open question.** The
same run started as `PATH=$(nros sdk-path make)/bin:$PATH just
build-test-fixtures lane=linux` failed identically, and a FRESH configure (its
`CMakeCache.txt` written in that run) still cached
`CMAKE_MAKE_PROGRAM=/usr/bin/gmake`. So an entry outside any checkout, put
first on the caller's `PATH`, does not survive to the build script's CMake.
Something between `just` and cargo reorders or rebuilds `PATH`; it was not
located. The re-rooting helper (`scripts/lib/reroot-checkout-pathlist.sh`)
is documented to KEEP out-of-checkout entries, so it is the first suspect to
measure, not the answer.

## Fix direction

First find where the caller's `PATH` is lost (print `PATH` and `command -v
gmake` from inside a build script on this road). Then prepend the pinned
make's directory at the point that survives, the way `fixture-make-driver.sh`
does (`export PATH="$(nros sdk-path make)/bin:$(nros sdk-path ninja)/bin:$PATH"`),
inside the arm that selects it, so pool and children share one make. Pinning
`CMAKE_MAKE_PROGRAM` for the row's cmake would also work, but it treats the
symptom on one generator. Then
check the sweep:
`git grep -n 'sdk-path make' -- scripts just`, for every site that RUNS a
pinned make as a jobserver, not one that only reports it (`just/workspace.just`
`doctor` lines). A gate fits better than a fifth comment: every script that
selects the pinned make for a `-j` run must also export its directory.
`check-inherited-checkout-paths` already drops the stray entry, so that half
is covered.
