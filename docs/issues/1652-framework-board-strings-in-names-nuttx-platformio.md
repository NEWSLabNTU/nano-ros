---
id: 1652
title: "Two board descriptors still list a framework's own board string in `names` — NuttX's `qemu-armv7a-nsh` and PlatformIO's `esp32dev`"
status: open
type: tech-debt
area: boards, cli
severity: low
found: 2026-10-03
related: [1519, 1517, 0606]
---

## What this is

Issue 1519 retired the Zephyr framework string from the `zephyr` descriptor's
`names` (`ImageBlock::board` is a nano-ros board id, never a framework's own
board string) and moved it to `[board.zephyr] west_board`. Its resolution
recorded two descriptors with the same shape it did not fix:

| descriptor | `names` | the framework string | authored by |
| --- | --- | --- | --- |
| `packages/boards/nros-board-nuttx-qemu/nros-board.toml` | `["nuttx", "NuttX", "qemu-armv7a-nuttx", "qemu-armv7a-nsh"]` | `qemu-armv7a-nsh` — NuttX's own board config name | one row: `packages/testing/nros-tests/fixtures/multi_pkg_workspace_nuttx/src/demo_bringup/system.toml` |
| `packages/boards/nros-board-esp32-qemu/nros-board.toml` | `[..., "esp32dev", ...]` | `esp32dev` — a PlatformIO board id | no row |

Zephyr was fixable mechanically because the descriptor already had a typed
field for the framework id (`[board.zephyr] west_board`). NuttX and PlatformIO
have **no typed per-ecosystem field**, so nothing can tell "a nano-ros alias" from
"a framework id" in `names` — which is why 1519 left these.

## What to do

1. Give each ecosystem a typed field for its own id (the `[board.zephyr]`
   precedent), move the string there, and have whatever consumes it (the NuttX
   board-config selection; anything reading `esp32dev`) read the typed field.
2. Migrate the one NuttX row to the nano-ros id.
3. Extend `check-deploy-board-resolves`' framework-id rule (added by 1519) to the
   new fields, so a framework string on an `[image.*]` board fails for these
   ecosystems too.

Low severity: both strings resolve to the right descriptor today. The cost is
the rule `ImageBlock::board` states being true for Zephyr only.
