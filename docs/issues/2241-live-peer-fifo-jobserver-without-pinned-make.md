---
id: 2241
title: "live-peer: the fixture build hands out a FIFO jobserver on a runner with no pinned make, so the system make 4.3 children stop"
status: open
type: bug
area: [build, ci, testing]
severity: medium
found: 2026-11-04
related: [2058, 2119]
---

## Measured

The scheduled `live-peer regression` run on main, 41771578704 (2026-11-04,
head `c23ed3b659`), failed both fixture jobs at the step **Build the fixtures
those rows resolve**. Both stop on the same line:

- Job 126382246956 (rows whose board IS this runner, runner `GitHub Actions
  1000128960`): `cyclonedds-sys`'s nested CMake build, under
  `examples/workspaces/bridge-cyclonedds/target-fixtures/…`. The build script
  warns first:

      cyclonedds-sys: a FIFO jobserver is inherited (`--jobserver-auth=fifo:`) and the pinned make is not provisioned, so CMake uses the make on PATH
      gmake: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-224273-0/fifo'.  Stop.

- Job 126382800895 (rows whose board is NOT this runner): the Zephyr
  native_sim link of `build-ws-rs-qos-entry-zenoh`. Zephyr runs its
  native_simulator Makefile through `/usr/bin/make`:

      make[1]: *** internal error: invalid --jobserver-auth string 'fifo:/tmp/nros-jobserver-255351-0/fifo'.  Stop.

The pool is `nros`'s own jobserver
(`packages/cli/nros-cli-core/src/builder/jobserver.rs`, which names the FIFO
`nros-jobserver-<pid>-<seq>`). GNU make 4.3, the Ubuntu 22.04 system make,
cannot join a FIFO jobserver; 4.4 can.

## What it is not

- **Not issue 2058's build-script half.** That fix is working:
  `cyclonedds-sys` names the pinned make when one exists, and here it says
  plainly that none is provisioned. The second failure is not a build script
  at all. It is Zephyr's own `make` call, which nothing in our tree chooses.
- **Not issue 2119's PATH half.** No pinned make exists on this runner to put
  first on `PATH`.
- **Not new code in the failing rows.** The lane was green on 2026-11-01. On
  2026-11-02 and 2026-11-03 it stopped earlier on the board-facts panic
  (issues 2224/2229), so this step was not reached. Whether it was also broken
  on those days is unknown.

## What would close it

Either half is defensible, and that choice is why this is filed rather than
patched:

1. **Provision the tool:** `nros setup --tool make` in `live-peer.yml`
   before the fixture build, for both jobs. This matches what the fixture
   drivers assume elsewhere.
2. **Make the pool safe without it:** `builder/jobserver.rs` hands out a FIFO
   jobserver only when every make the build can reach is new enough. With no
   pinned make, it falls back to the pipe style, which make 4.3 understands, or
   to no pool. That would close the class for any lane missing the pin, not
   just this one.

Acceptance: a `live-peer regression` run gets past "Build the fixtures those
rows resolve" in both jobs.
