---
id: 2390
title: "The fixture census pre-pass lists no Zephyr `west` leaf and then turns configure's own take off, so a Zephyr C/C++ leaf counted by its census refuses at configure"
status: resolved
type: bug
severity: high
area: [build, zephyr, ci, census]
related: [phase-530, 2378, 2386, 1556]
found: 2026-11-10
resolved: 2026-11-11
resolved_in: "tier-2 run 42971493635: the census pre-pass took the Zephyr west leaves and the Zephyr cells ran"
---

> **Resolved (2026-11-11).** tier-2 run 42971493635 (schedule, 2026-11-11, `main` `7d60c42daf`, runner `nano-ros-runner-newslab-118-server`): the census pre-pass listed 8 leaf candidates, among them the four Zephyr west template leaves (`examples/templates/{cpp-port-minimal-publisher,rclcpp-compat-smoke,topic-state-monitor-port,ament-workspace-port}/zephyr`), and printed no failed leaf. The Zephyr fixture stage then built past configure and Zephyr cells ran and passed (`boot_smoke` cases among them). The runner's `build/census-prepass/` directory is not uploaded, so the per-leaf log files themselves were not inspected; the job log's pre-pass listing is the evidence used.

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**. W2 made the census the default entity source for a C/C++ cross leaf that states no `entities` (RFC-0120 D4), and this is the fixture road that takes it.

## What

Tier-2 run 42677993077 (`workflow_dispatch` on main, 2026-11-09 18:52Z) stopped at
configure on every Zephyr C++ west leaf for mps2/an385:
`build-cortex-m-cpp-talker-zenoh` and the four `build-cortex-m-cpp-port-*`
rows (minimal-publisher, rclcpp-compat-smoke, topic-state-monitor,
ament-workspace). Each log (artifact `post-submit-junit`,
`build/zephyr-fixtures/`) reads:

```
CMake Error at .../zephyr/cmake/nros_cargo_build.cmake:489 (message):
  nros: `nros dev image west-leaf-sizing` failed for .../examples/zephyr/cpp/talker
  ... no `entities` (a C/C++ leaf is counted by its census by default), and its
  census is missing (no census at
  `.../examples/zephyr/cpp/talker/build/nros/census/leaf.census.json`).
```

The module hook's `nros configure` reported `configured (0 s)`, so it took
no census either.

## Why

Two things together:

1. `scripts/build/census-prepass.sh` takes the leaf censuses from
   `fixtures-manifest.py census-leaves`, which filtered rows through
   `matches_filters`. That function keeps `builder = "west"` rows out of
   every listing unless the caller passes `--builder west` (phase-350 W1:
   the cargo/cmake consumers must never be handed one). `census-leaves` did
   not, so it listed no Zephyr leaf at all. The run's
   `build/census-prepass/` holds logs for three NuttX and two ThreadX leaves
   and none for Zephyr.
2. `build-test-fixtures` then exports `NROS_CENSUS_PREPASS=done`, which
   makes `census_take_is_ours` false (`cmd/build.rs`), so
   `configure_zephyr_leaf` does not take the census at the module hook
   either. Nothing on the road took it.

## The C talker rows passed on a leftover census

`examples/zephyr/c/talker` is counted by its census the same way and was not
on the pre-pass list either, yet its rows (`build-c-talker-xrce`,
`build-nucleo-f767zi-c-talker-serial`) configured: their logs say
"entity inventory from its census". The fixture tree on the self-hosted
runner persists between runs, so the census those rows read was one an
earlier run or a hand-run `take --leaf` had left in
`<leaf>/build/nros/census/`. So a tier-2 Zephyr green for a census-counted
leaf since phase-530 W2 does not show that the fixture road takes a census.
It shows only that a census was on disk.

What that leftover census could not do is size an image from stale code. A
missing census refuses (this issue's symptom), and a present one is read only
when `census_freshness` (`cmd/entity_census.rs`) finds it fresh: the recorder
schema matches and every input it recorded in its provenance hashes the same
as when it was taken. A stale one refuses with "its census is stale". So the
reuse was of a census whose recorded inputs had not moved, which is the
contract; what was missing is the producer on a fresh tree, and that is what
the fix restores. A census counts only what its recorded inputs cover, so a
change outside them would not re-take it, and that holds on every road, not
only this one.

## Fix

`census-leaves` asks for the rows under both shapes (the caller's filters, and
the same filters with `--builder west`), still one line per leaf directory. On
the current manifest the tier-2 coordinates now list the Zephyr C talker, the
C++ talker and the four C++ port templates beside the NuttX/ThreadX leaves.
The other listings keep the west opt-in.

Test: `tests/census-leaves-west-rows-tests.sh` (gate
`census-leaves-west-rows`) against the real manifest: an unfiltered list and
one narrowed to `zephyr-cortex-m,cpp,zenoh` both name
`examples/zephyr/cpp/talker`, each leaf once, and `list` still omits west
rows. Checks 1 and 2 fail on the old code.

Acceptance: a tier-2 run on main reaches the Zephyr cells past configure, and
the run's `build/census-prepass/` holds a log for each Zephyr leaf.
