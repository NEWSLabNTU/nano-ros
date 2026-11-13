---
id: 2511
title: "The zephyr-ci image bakes Zephyr SDK 0.17.4 while the 3.7 workspace pins 0.16.9, and `merge-runtime.yml` sets up Zephyr with `--skip-sdk`, so the `west_board_import` fixture refuses and the zephyr shard's fixture build fails"
status: open
type: bug
area: [ci, zephyr]
severity: medium
found: 2026-11-14
related: [2475, 1811, phase-535, phase-559]
---

**Home: [phase-559](../roadmap/phase-559-ci-workflow-organization.md) W5b.**
Found by issue 2475's after-run. A missing runner dependency is fixed in the
IMAGE.

## Measured

`merge-runtime.yml` run 43438754804 (`lane=tier1`, branch
`p535-lane-scoped-codegen` at 7bff8a9356), shard `zephyr`, image
`nano-ros-zephyr-ci:humble-sdk0.17.4-r7`:

- Step "Register the baked Zephyr SDK": `Zephyr SDK 0.17.4 Setup`
  (`/opt/zephyr-sdk`).
- Step `just setup zephyr --skip-sdk` reports the pinned directory
  `/github/home/.nros/sdk/zephyr-sdk/0.16.9/zephyr-sdk-0.16.9` and installs
  nothing there. `nros-sdk-index.toml` pins `version = "0.16.9"`.
- West fixtures, 6 of 7 ok. The seventh refused:

```
== west-fixture: west_board_import (west-configure, board=board.cmake) ==
error: board `` builds with the Zephyr SDK, and this tree pins SDK 0.16.9
       (/github/home/.nros/workspaces/zephyr/3.7/zephyr/SDK_VERSION), which is not installed. ...
   SKIPPED west_board_import — no pinned Zephyr SDK for  (see above).
west-fixtures: 1 of 7 fixture(s) FAILED to build.
```

The six native_sim fixtures build with the host gcc. `west_board_import`
(read by `board_import.rs`) is the one row on this shard that needs the SDK
itself.

The refusal is correct: issue 1811 measured a newer SDK failing in Zephyr
3.7's own `libc-hooks.c`. The image and the pin disagree.

## Fix

Bake the pinned 0.16.9 SDK into the zephyr-ci image, at the path the
`zephyr-sdk` index row resolves to, or bake both if the 4.4 line needs 0.17.4.
Then `--skip-sdk` stays right. Do not drop `--skip-sdk` in the workflow; that
moves a ~1 GB download into every run.

Two message defects in the same refusal:

- The board name is empty: "board ``" and "no pinned Zephyr SDK for ".
- It names an issue id in user-facing text, against the 2026-11-12 ruling.

## Done when

The zephyr shard of a `merge-runtime.yml` tier1 dispatch builds
`west_board_import` with the image as shipped, and the refusal names its board
and carries no issue id.
