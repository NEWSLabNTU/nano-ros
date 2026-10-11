---
id: 1777
title: "The Zephyr west lane configures its two Cyclone compile-check rows without
  asking for a host `idlc`, so a runner without one fails them at configure"
status: resolved
type: bug
area: ci, zephyr, cyclonedds
severity: medium
found: 2026-10-10
resolved: 2026-10-11
related: [issue-1627, issue-1536, issue-1758, issue-1546, phase-477]
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

## Resolved (2026-10-11) — option 2, plus the class

Option 2: the Zephyr lane now ASKS for a host `idlc` before configuring a
Cyclone row, and a selected row without one is a FAILURE that names the tool
and the provisioning command.

- **One probe** — `scripts/lib/host-idlc.sh`: `nros_host_idlc` resolves in the
  Cyclone configure's own order (`IDLC_EXECUTABLE` → the store's PINNED
  `<store>/sdk/cyclonedds/<pin>/bin/idlc` → `PATH` →
  `build/{cyclonedds,install}/bin`), with no CLI needed;
  `nros_host_idlc_missing_reason` is the one remedy sentence
  (`nros setup native --rmw cyclonedds`, ROS 2, or `IDLC_EXECUTABLE`).
- **Which rows are Cyclone is DERIVED, not listed** —
  `west_compile_check_rmw()` in `scripts/build/fixtures-manifest.py` reads what
  the configure reads (`-DNANO_ROS_RMW`, the board crate's `default_rmw` behind
  `nano_ros_use_board()`, the CONF_FILE list's `CONFIG_NROS_RMW_*`, the Kconfig
  default). `list-compile-checks --rmw cyclonedds` exposes it; today it selects
  exactly `west_bringup_zephyr_cyclone_user_config` and `west_board_import`.
- **`west-fixtures.sh`** — a selected Cyclone row with no idlc is counted
  FAILED before `rm -rf` / sync / configure; the other rows still build.
- **The class, swept** (`git grep -n 'west build\|idlc' -- just scripts`):
  - `zephyr-fixture-leaves.sh` had its own probe, which ENUMERATED
    `<store>/cyclonedds/*/bin` (the issue-1546 class) and ignored
    `IDLC_EXECUTABLE`; it now calls `nros_host_idlc`. It also DROPPED a
    selected Cyclone leaf silently; it now reports each one on stderr
    (`NROS_ZEPHYR_IDLC_MISSING:`), and `just/zephyr-ci.just` turns that into
    `nros_lane_skip_note zephyr …` — a failure when the lane is named or
    coordinate-scoped (issue 1758), a reported skip in the unscoped sweep. The
    emitter stays exit 0, because gates run it read-only on idlc-less hosts.
  - `just zephyr build-fvp-ws-entry` / `build-fvp-board-import` configure the
    FVP board (`default_rmw = "cyclonedds"`) and now ask first, through
    `nros_lane_skip` (named → failure).

Measured: with every rung hidden (`IDLC_EXECUTABLE=`, `/opt/ros` and every
`idlc` dir off `PATH`, `NROS_HOME` at an empty dir),
`NROS_ZEPHYR_FIXTURE_FILTER='west_board_import|west_bringup_zephyr_cyclone_user_config'
bash scripts/build/west-fixtures.sh` printed `FAILED <id> — not configured`
for both rows with the remedy, `2 of 2 fixture(s) FAILED`, rc=1, and ran no
west. With idlc present, `west_board_import` printed `host idlc: …`,
configured and generated: `ok (build.ninja)`, rc=0.

Not covered: the FreeRTOS / ThreadX recipes keep their own `command -v idlc`
spellings (they are not west lanes and were out of this issue's scope); moving
them onto `nros_host_idlc` is the obvious follow-up. Option 1 (the tier-2
image carrying `idlc`) is unchanged — `just zephyr setup` already runs
`nros setup native --rmw cyclonedds` via `nros_setup_board_rmws`; whether the
tier-2 runner image has one is now answered by the lane itself instead of by
a configure log.

