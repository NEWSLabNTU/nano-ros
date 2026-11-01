---
id: 2199
title: "`nros ws board-facts` ignores a leaf's chosen image, so its refusal names an image nobody selected"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [cli, zephyr, build]
severity: low
found: 2026-11-01
related: [0755, 0941, 1827]
---

## What was measured

An out-of-tree Zephyr 4.4 board (`xiao-rp2350`, west board
`xiao_rp2350/rp2350a/m33`) built from a C leaf whose `system.toml` declares two
images on that board, `xiao_serial` and `xiao_cdc`, with
`[system] default_images = ["xiao_serial"]`. Built without
`NROS_EXTRA_BOARD_PATH`, the configure rendered `xiao_serial` (its log says
`image \`xiao_serial\` conf -> …`) and then failed in
`NanoRosBoardFacts.cmake` with:

```
Error: no board descriptor claims `xiao-rp2350` (deploy `xiao_cdc`).
```

The refusal was right (the descriptor was not on any search path) but named
the wrong image.

## Cause

Two halves, each enough on its own:

1. **`pick_deploys` discarded the leaf's own answer.** It called
   `leaf_system::read(ws)` (to refuse retired keys) and dropped the result,
   which is the leaf's ONE image by the rule `nros ws leaf-system` applies
   (exactly one `[image.*]`, or `[system] default_images`). Every image the
   file declared became a candidate; `resolve` then resolved them in key order
   and the first refusal won — `xiao_cdc` sorts before `xiao_serial`. On a leaf
   whose images name *different* boards the same discard produced
   `deploys … resolve DIFFERENTLY` instead of the default image's facts.
2. **The Zephyr lane never forwarded the image.** The module hook selects the
   image from `NROS_IMAGE` (a leaf's `-DNROS_IMAGE=<id>`, a generated
   application's `set(NROS_IMAGE …)`), and `NanoRosRmwUserConfig` and
   `nros_cargo_build` read it, but `nros_resolve_board_facts` did not: it asked
   with no `--deploy`, so a leaf built with `-DNROS_IMAGE=xiao_cdc` would have
   received its default image's board facts.

## Fix

- `pick_deploys` uses the leaf's image as the candidate when the configure
  names neither a deploy nor a board (a named one still chooses).
- The board refusal names every candidate on the unclaimed board
  (`deploys \`a\`, \`b\``) when nothing chose one, and points at
  `$NROS_EXTRA_BOARD_PATH` for an out-of-tree board.
- `nros_resolve_board_facts` forwards `NROS_IMAGE` as `--deploy`, after
  `NROS_LEAF_IMAGE_ID` and before `NROS_DEPLOY`.

Tests (`cmd::board_facts::tests`):
`a_leaf_resolves_its_default_image_when_none_is_named` and
`an_unknown_board_refusal_names_the_right_images`. Both fail with the
`pick_deploys` change reverted.

Measured on the spike leaf with the rebuilt CLI: `nros ws board-facts
apps/talker-c` without the board path now says `(deploy \`xiao_serial\`)`, with
`--deploy xiao_cdc` it says `xiao_cdc`, and with `NROS_EXTRA_BOARD_PATH` set it
resolves `NROS_BOARD=xiao-rp2350`.
