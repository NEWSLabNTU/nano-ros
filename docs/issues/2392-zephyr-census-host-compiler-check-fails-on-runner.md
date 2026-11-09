---
id: 2392
title: "A Zephyr leaf's census host build fails CMake's C compiler check on the self-hosted tier-2 runner"
status: open
type: bug
severity: high
area: [zephyr, testing, ci]
related: [phase-466, 2386]
found: 2026-11-10
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**.
Tier 2 on main needs this and the census pre-pass fix (PR #3397) before it
reaches the Zephyr cells.

## Symptom

Tier-2 run 42605683876 (workflow_dispatch on main, 2026-11-09 11:01Z, the
rebuilt 241 runner) stopped in `just build tier2`'s Zephyr fixture stage:

```
zephyr-fixture-run-one: failed: zephyr/native_sim/native/64/c/talker/xrce
  collect2: error: ld returned 1 exit status
```

with the same `collect2` line in two more leaves, one of them
`nucleo-f767zi-c-talker-serial`. The job log carries no symbol, and the inner
logs were not uploaded.

## What the log does show

This is not the image's link. Each `collect2` sits under:

```
CMake Error at /usr/share/cmake-3.22/Modules/CMakeTestCCompiler.cmake:69
CMake Error at .../cmake/NanoRosLeafCensus.cmake:79 (message):
  ... entity_census.rs:1028
Call Stack: zephyr/cmake/nros_image_kconfig.cmake:297 (nros_leaf_census_take)
```

So the Zephyr configure took the leaf's census. The census verb configures the
leaf's sources for the HOST in `<leaf>/build/census-host`, and that host
configure failed CMake's check that the C compiler can link a test program.

## Not reproduced elsewhere (2026-11-10, ros2 box)

On current main, the same in-configure take succeeds. With the leaf's census
and configure stamp moved aside, `NROS_CENSUS_PREPASS` unset and the Zephyr
build dir reconfigured, `nros configure ... configured (39 s)` takes the census
from inside the Zephyr configure and the image configures. The verb alone
(`nros dev image entity-census take --leaf examples/zephyr/c/talker`) passes
too. So the failure is in the runner's environment: its host toolchain, or a
variable the Zephyr configure exports into the child configure.

Not the cause:

- Issue 2386 (census host image on a clean tree) records configure stamps so a
  cross image's census can configure. It does not touch the compiler check.
- The census pre-pass (PR #3397) is the other half of the block: there, no
  census is taken at all. Once the pre-pass takes the Zephyr leaves again, this
  host configure runs in the pre-pass instead, so it can still fail there.
- [Issue 2025](archived/2025-zephyr-xrce-c-leaves-link-posix-platform-twice.md)
  (the xrce C leaves linking the posix platform twice) was an IMAGE link. Its
  shape cannot be confirmed or excluded without the symbol.

## Not known yet

The linker's message. It is in the runner's persistent fixture tree at
`examples/zephyr/c/talker/build/census-host/CMakeFiles/CMakeError.log` (cmake
3.22 names it that way), unless a later run has overwritten it.

## Acceptance

- The failing link is named, from the runner's `CMakeError.log` or a fresh
  failure.
- A Zephyr leaf's census host build configures on the self-hosted tier-2
  runner, and tier 2 on main reaches the Zephyr cells past configure.
- A census host configure that fails leaves its `CMakeFiles` logs in the run's
  uploaded reports, so the next one is diagnosable from the run.
