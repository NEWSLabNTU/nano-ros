---
id: 639
title: The fixture lane invoked ROS's `idlc` without `LD_LIBRARY_PATH`, and it is
  not established what drops it between `activate.sh` and the build
status: open
type: bug
area: build
related: [0601, 0633]
---

## What is actually known

Issue 0633 fixed why a broken `idlc` was cached and reused. It left one thing
unexplained, and this issue exists so that gap is recorded rather than implied
away.

`/opt/ros/humble/bin/idlc` on this host links `libiceoryx_binding_c.so`, which
lives in `/opt/ros/humble/lib/x86_64-linux-gnu`. Whether it runs depends
entirely on the loader path:

```
no LD_LIBRARY_PATH                                   -> 127
LD_LIBRARY_PATH=/opt/ros/humble/lib                  -> 127   (wrong dir)
under `source ./activate.sh`                         ->   0
```

`activate.sh` sets

```
LD_LIBRARY_PATH=/opt/ros/humble/opt/zmqpp_vendor/lib:/opt/ros/humble/opt/rviz_ogre_vendor/lib:/opt/ros/humble/lib/x86_64-linux-gnu:/opt/ros/humble/lib
```

so the tool is not intrinsically broken here.

## The unexplained part

`just build-test-fixtures lane=native` was launched from a shell that HAD
sourced `activate.sh`, and the cyclone leaves still failed with

```
/opt/ros/humble/bin/idlc: error while loading shared libraries:
libiceoryx_binding_c.so: cannot open shared object file: No such file or directory
FAILED: [code=127] …
```

Something between that shell and the `idlc` invocation does not carry
`LD_LIBRARY_PATH`. Candidates not yet distinguished:

* the generated make driver, or the `make -j` leaves it spawns;
* `cmake -E env` launchers, which replace rather than extend the environment
  when handed a full assignment;
* ninja's own invocation of a raw baked command;
* a `just` recipe boundary.

None of these has been confirmed and this issue does not guess between them.
Whoever picks it up should find the boundary first — printing
`LD_LIBRARY_PATH` from inside the failing `idlc` rule is the cheapest probe —
rather than adding propagation somewhere plausible.

## Why it is not urgent, and why it should not simply be closed

It is currently masked. After 0633, `idlc` resolves to the SDK-provisioned
`~/.nros/sdk/cyclonedds/<ver>/bin/idlc`, which has no such loader dependency,
so no build on this host reaches ROS's copy. The lane is green.

That masking is the reason to file rather than drop it:

* the environment gap is real and is not specific to `idlc` — any host tool a
  build reaches through the environment is exposed to the same boundary;
* `_nros_idlc_runs` derives `<prefix>/lib/${CMAKE_LIBRARY_ARCHITECTURE}` and
  bakes an `LD_LIBRARY_PATH` launcher when the bare invocation fails, so a host
  WITHOUT an SDK idlc will depend on that path working — and it was never
  exercised, because 0633's cache meant the probe never ran;
* the probe validates in the CONFIGURE environment and, when the tool runs
  there, bakes no launcher at all. A build later executed in a thinner
  environment then fails at `code=127` with nothing recording that the
  configure had made an assumption. That shape is worth checking on its own.

## Provenance

Split out of issue 0633 on 2026-08-16. 0633's first draft asserted that
propagating ROS's library path "would not have worked here either", on the
strength of the `LD_LIBRARY_PATH=/opt/ros/humble/lib -> 127` line above. That
was the wrong directory, and the conclusion drawn from it was wrong; the
correction is recorded in 0633 and the real question is this one.
