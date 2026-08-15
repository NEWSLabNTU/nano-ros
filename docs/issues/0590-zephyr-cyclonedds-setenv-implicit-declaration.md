---
id: 590
title: "Zephyr + Cyclone: `ddsrt` posix `environ.c` calls setenv/unsetenv, which
  Zephyr's libc does not declare — the cell cannot compile"
status: open
type: bug
area: rmw/zephyr
related: [issue-0557, issue-0566, issue-0371, rfc-0005]
---

## Symptom

On a freshly provisioned Zephyr workspace (3.7 line, SDK 0.16.8), every
cyclonedds cell of `just zephyr build-fixtures` fails to compile:

```
FAILED: modules/nros/CMakeFiles/nros.dir/…/cyclonedds/src/ddsrt/src/environ/posix/environ.c.obj
…/ddsrt/src/environ/posix/environ.c:53:7: error: implicit declaration of
    function 'setenv'; did you mean 'getenv'? [-Wimplicit-function-declaration]
…/ddsrt/src/environ/posix/environ.c:75:7: error: implicit declaration of
    function 'unsetenv'; did you mean 'getenv'?
```

The zenoh and xrce cells on the same workspace build: 12 `zephyr.elf` images
produced, 4 failures, all of them cyclonedds.

## Reading

Cyclone's ddsrt picks its **posix** environ backend for Zephyr, and that file
calls `setenv`/`unsetenv`. Zephyr's libc declares them only behind its POSIX
options (`CONFIG_POSIX_API` / the `_POSIX_C_SOURCE` surface); without that they
are implicitly declared, which gcc ≥ 14 — the compiler the Zephyr SDK 0.16.8
ships — treats as an error rather than a warning.

Same *class* as the two zenoh-pico fixes already on that fork
(`int-conversion`, `implicit decl is fatal on gcc >= 14`) and as issue 0566,
which found the Zephyr port reaching for `CONFIG_POSIX_API` and giving up when
it is absent. The pattern to check for is a port assuming the POSIX surface on
a kernel that gates it.

## What is NOT established

* Whether this is new. The cyclone-on-Zephyr cells are known to RUN on the
  maintainer's host (issue 0557 reports a RUNTIME failure of the ACTION images,
  which presupposes they built), so this may be specific to the Zephyr version /
  SDK on this host rather than a regression.
* Which fix is right, and it is deliberately not chosen here: enable the POSIX
  env option in the Zephyr conf; give ddsrt a Zephyr environ backend (the shape
  issues 0371/0557 took for the sync backend — `DDSRT_WITH_ZEPHYR` already picks
  Zephyr-native types); or stub the two calls. **The Zephyr ddsrt seam is under
  active change by another session** (0557's native `k_mutex`/`k_condvar`
  backend), so whoever owns that work should pick, not a drive-by.

## It now fails TIER 1, not just the Zephyr lane (2026-08-15)

Provisioning the Zephyr workspace turned previously-SKIPPED cells into required
ones: `sched_dims_applied` reports

```
sched_dims: 4 of 12 cell(s) FAILED:
  CorePin/zephyr/rust: Test fixture binary MISSING for an in-lane coordinate:
    zephyr-workspace/build-ws-rs-realtime-entry-zenoh/zephyr/zephyr.exe
```

That entry is `zephyr-fixture-65` in the lane's make driver; the cyclonedds
cells are 13–18, and the driver has no `-k`, so the build stops long before 65
and the workspace entries are never produced. On a tier-1 run at HEAD this is
the ONE real failure — the other 39 are capability skips (no ROS 2 on this
host).

So the cost of this issue is no longer "the cyclone cells do not build": while
it stands, no Zephyr fixture after the cyclone group can be built at all, and
tier 1 is red on any host with a provisioned Zephyr workspace.

## Reproduce

```sh
just zephyr setup           # workspace + SDK (PEP 668 hosts: see e3225404c)
just zephyr build-fixtures  # zenoh + xrce build; the 4 cyclonedds cells fail
```
