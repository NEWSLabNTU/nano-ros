---
id: 1777
title: "The Zephyr west lane configures its two Cyclone compile-check rows without
  asking for a host `idlc`, so a runner without one fails them at configure"
status: open
type: bug
area: ci, zephyr, cyclonedds
severity: medium
found: 2026-10-10
related: [issue-1627, issue-1536, issue-1758]
---

## Measured

`live-peer regression` run **36963917981** (2026-10-02), job **110703711587**:
two west compile-check rows died on the same configure error —

```
CMake Error at …/zephyr/cmake/nros_rmw_cyclonedds.cmake:375 (message):
  host Cyclone idlc not found.
    searched: SDK store hints [], host PATH, then …/build/{cyclonedds,install}/bin
    Remedies: install ROS 2 (idlc on PATH), run `nros setup <board> --rmw cyclonedds`,
    or set IDLC_EXECUTABLE.
```

- `west_bringup_zephyr_cyclone_user_config` (`west-build`) — counted FAILED.
- `west_board_import` (`west-configure`, board.cmake defaults `NANO_ROS_RMW` to
  `cyclonedds`) — counted ok, because its declared output was `CMakeCache.txt`.

Issue 1627 fixed the miscount: a `west-configure` row now declares
`build.ninja`, so `west_board_import` reports this failure too instead of hiding
it. That turns this gap from silent into a SECOND visible red, which is what
the phase-477 D1 decision said it would do.

## The gap

`SDK store hints []` is the index saying it was never consulted: the lane that
built these rows never ran `nros setup <board> --rmw cyclonedds`, and nothing in
the Zephyr lane asks for an `idlc` before configuring a Cyclone row.
`just/freertos.just` and `just/threadx-*.just` each have that question
(`nros_lane_wants_rmw <platform> cyclonedds` → require `idlc` or set
`IDLC_EXECUTABLE`); `just/zephyr-ci.just` → `scripts/build/west-fixtures.sh` has
none.

Locally (a host with `/opt/ros/humble/bin/idlc` and the
`third-party/dds/cyclonedds` submodule initialised) `west_board_import`
configures and generates and reports `ok (build.ninja)`. In a fresh worktree
WITHOUT the submodule it fails one step earlier — `Cyclone DDS submodule not
initialised` — and is now counted FAILED for that too. Both are provisioning,
not code.

## Which lane reaches these rows now

Since issue 1536, live-peer's board job narrows `NROS_ZEPHYR_FIXTURE_FILTER` to
`build-ws-rs-qos-entry-zenoh` and no longer builds any west compile-check row.
Tier 2 (`run-matrix.yml` `just build tier2`) and the tier-2 nightly build all
five unfiltered (issue 1627's D2 finding). So the lane to check is tier 2's: if
its runner image carries no `idlc` (or its setup provisions no
`--rmw cyclonedds`), these two rows go red there.

## What would close this

One of the two options phase-477 D1 named:

1. Provision it: the Zephyr lane's setup runs the Cyclone provisioning when its
   rows include a Cyclone one (`nros setup <board> --rmw cyclonedds`, or the
   `--tool cyclonedds` dist that ships `bin/idlc`), and the image gains it if
   the runner is a container (CLAUDE.md: an IMAGE fix, never a host apt).
2. Gate the rows: `west-fixtures.sh` asks for `idlc` before a Cyclone row the
   way the FreeRTOS/ThreadX recipes do, and FAILS (issue 1758: a selected row
   whose tool is missing is a failure, not a skip) with the remedy.

Acceptance: a tier-2 run builds `west_board_import` and
`west_bringup_zephyr_cyclone_user_config` to `ok`, or fails before configuring
them with a message that names `idlc` and the provisioning command.
