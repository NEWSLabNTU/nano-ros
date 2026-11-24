---
id: 2944
title: "bringup_scoped_root hard-codes the packages pubsub_bringup launches, so a launch added in phase-582 W1d failed it on a correct root"
status: resolved
type: bug
area: [testing, cmake]
severity: medium
found: 2026-11-24
resolved: 2026-11-24
related: [2754, 2945, phase-582, phase-590]
---

## What happened

Release run-matrix 45351389552 (main `0d9fa3e470`), tier-2 nightly:
`bringup_scoped_root::a_real_build_scopes_its_root_to_the_bringup` failed with

    left: {"clock_board_pkg", "listener_pkg", "talker_pkg"}
    right: {"listener_pkg", "talker_pkg"}

It is tracker row B27 in phase-590.

## Cause

It is a test defect; the root is right. `4f05d59e6d` (phase-582 W1d,
2026-11-22) added `talker_clock_board.launch.xml`, which runs
`clock_board_pkg`, to `pubsub_bringup`. So the bringup now launches three
packages, and `nros build` scoped the root to exactly those three. The test
(`a49a3dcf82`, issue 2754, 2026-11-20) had listed the two that existed then.

The fixture is not stale either. Today's nightly fixture build rebuilt
`workspace-cpp-native-talker` on the runner's tree, and the test's own
resolver passed it as current.

## Fix

The test reads the expected set from the bringup's launch files (every
`<node pkg="…">`), and first asserts that set is a strict, non-empty subset
of the workspace's node packages, so the comparison cannot pass vacuously.
