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
