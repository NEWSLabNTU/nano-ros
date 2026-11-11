---
id: 2449
title: "dep-chain cannot pick a workspace witness's image when its bringup names the board by an alias"
status: resolved
type: bug
area: ci, examples
related: [phase-531]
---

## Symptom

Post-submit `dep-chain (7 board×rmw cells)` has been red on every run of main
since 7d60c42daf (last green b9a55a57df; latest seen run 43046358459, job
130611226415). Three cells fail:

```
[FAIL] cannot pick the mps2-an385-freertos/zenoh image of the workspace witness examples/mps2-an385-freertos/rust/talker
[FAIL] cannot pick the qemu-armv7a-nuttx/zenoh image of the workspace witness examples/qemu-armv7a-nuttx/rust/talker
[FAIL] nros configure wrote no single threadx-linux_entry with nros-cargo.toml under examples/threadx-linux/rust/talker/build
```

## Cause

The window holds the phase-531 W6 batch that made those three Rust talkers
one-package workspaces (PR #3522). `scripts/ci/dep-chain-check.sh` picks the
witness's image for a cell by comparing spellings, and both comparisons were
wrong for the new witnesses:

- The image row's `board` was compared to the cell's board as text. The
  bringups name their boards the way the catalog lets them, `freertos` for
  mps2-an385-freertos and `nuttx` for qemu-armv7a-nuttx, so no image matched.
  The linux witness passed only because its image says `board = "linux"`.
- The entry directory was globbed as `<image>_entry`. The generated entry is
  a cargo package, so an image id's `-` is `_` in its name: the threadx-linux
  image boots `threadx_linux_entry`, not `threadx-linux_entry`.

## Fix

The picker resolves both boards through `nros board info` and compares the
board package directories, so any name the catalog accepts matches. The entry
glob uses the image id with `-` replaced by `_`.

Measured with `just check dep-chain` on the fixed tree: 7 passed, 0 failed
(of 7 cells). The three workspace witnesses now resolve their entries:
`build/freertos-zenoh/mps2_an385_freertos_entry`,
`build/nuttx-zenoh/qemu_armv7a_nuttx_entry` and
`build/threadx-linux-zenoh/threadx_linux_entry`.
