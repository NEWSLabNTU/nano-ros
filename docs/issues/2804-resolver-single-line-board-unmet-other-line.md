---
id: 2804
title: "A fixture resolver for a board that only one Zephyr line supports reports UNMET on the other line"
status: open
type: test-gap
severity: low
area: [zephyr, testing]
related: [issue-1016, issue-1758, issue-2793]
found: 2026-11-21
---

## What

A Zephyr board can be supported on one west line only. The Nucleo-F767ZI is
one: its HAL module `hal_stm32` has `lines = ["3.7"]` in
`[zephyr_module.hal_stm32]` of `nros-sdk-index.toml`, so no 4.4 workspace
fetches it. Its images cannot be built on 4.4:

    st/f7/stm32f767zitx-pinctrl.dtsi: No such file or directory

The fixture resolvers for such a board's west leaves (for example
`build_zephyr_nucleo_c_serial_talker`, `build-nucleo-f767zi-c-talker-serial`)
do not ask which line the run is on. On the 4.4 line they find no artifact
and report `[UNMET PRECONDITION]`, a red. The same red is what an in-line
fixture that was never built produces, so a 4.4 run cannot tell "this board
is not on this line" from "this fixture is missing".

Seen on 2026-11-21 (host 241, PR #4521): `zephyr_link_layout`'s
`an_image_without_sched_tasks_carries_no_pool_in_dtcm` passed on 3.7 and was
UNMET on 4.4, for this reason alone.

## Class

Every resolver for a west leaf whose board (or a module it needs) is absent
from the current line has the same shape. The line is a property of the RUN
(`nros_tests::zephyr::zephyr_line()`), and the board's line support is a
property of the index (`[zephyr_module.*] lines`, and the board's own
packages), so the decision is the lane's, not the host's. CLAUDE.md's rule
applies: scope decides what runs, and a probe that skips is not the answer.

## Acceptance

- A Zephyr west leaf's lines are DERIVED from the index (the board's modules'
  `lines`), never authored per test.
- On a line the leaf is not supported on, its tests are deselected by the
  lane's scope (`lane_skip!`, `[SKIPPED:lane]`) before any fixture probe, and
  that line's fixture build does not attempt the leaf.
- On a line it IS supported on, a missing artifact still fails as UNMET.
- A negative control: a 3.7-only leaf run on 4.4 reports `[SKIPPED:lane]`, and
  the same leaf with its artifact deleted on 3.7 reports UNMET.

## Fix (2026-11-22, user ruling A)

The acceptance's first line, "derived from the index", did not hold up.
`[zephyr_module.*] needed_by` mixes a board's HAL (needed on every line the
board is on: `hal_stm32` for the Nucleo) with per-line components
(`tinycrypt` on 3.7 only and `cmsis_6` on 4.4 only, both naming
`mps2_an385`). An intersection puts `mps2_an385` on no line, and a union is
wrong as well. The board descriptors' `[board.provisioning] zephyr_line` is
only `nros setup`'s default: mps2 states 3.7 there and runs on 4.4. So a board
STATES its lines, once, and gates keep the statement present and consistent:

- `[board.zephyr] lines = [...]` on every Zephyr board descriptor (7). Two lines
  where no index row excludes one; 3.7 for the Nucleo and Pico (HAL on 3.7
  only) and for the FVP and S32Z270 (whose only lanes are 3.7).
  `BoardZephyr.lines` in the CLI, and `nros board new` writes the field.
- `scripts/lib/zephyr_board_lines.py` is the one reader. It also holds the
  line -> manifest table, which `check-zephyr-module-allowlist.py` now imports.
- Build: `fixtures-manifest.py west-leaves --line <l>` omits a leaf whose board
  does not state the line. `zephyr-fixture-leaves.sh` passes the run's line, so
  4.4 builds 102 of the 103 leaves, all but the Nucleo serial talker. Without
  `--line` the output is byte-identical to before.
- Run: `fixtures-manifest.py west-leaf-lines` exports each leaf's lines.
  `require_west_leaf_in_lane` checks the line FIRST, whatever the coordinate
  scope, and raises `[SKIPPED:lane]`. A leaf the table does not model still
  runs (fail-closed). A leaf for a board no descriptor names makes the manifest
  refuse, never "every line".
- Gate `just check zephyr-board-lines`: every Zephyr board states non-empty,
  known lines. Every module naming it is on one of its lines, and in that
  line's allowlist on each line both share. Negative control: the Nucleo stated
  on 4.4 alone fails (`needs module hal_stm32, which is on ['3.7']`).
- Unit tests in `fixtures::lane`: a 3.7-only leaf is deselected on 4.4 and not
  on 3.7, and the live table puts the Nucleo leaf on 3.7 only.

Still to run (the batch's Zephyr cell): the acceptance pair on a real run. On
4.4 the Nucleo cell must read `[SKIPPED:lane]`; on 3.7, with the artifact
removed, it must read UNMET.
