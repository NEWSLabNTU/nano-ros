---
id: 1652
title: "Two board descriptors still list a framework's own board string in `names` — NuttX's `qemu-armv7a-nsh` and PlatformIO's `esp32dev`"
status: resolved
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

## Resolved (2026-10-10, phase-477 D5)

Both strings left `names` for a typed field, on issue 1519's precedent:

- `nros-board-nuttx-qemu`: `[board.nuttx] board_config = "qemu-armv7a-nsh"`
  (bound to the arm `[[board]]` entry, before the riscv one).
- `nros-board-esp32-qemu`: `[board.platformio] board = "esp32dev"`. It is NOT
  deleted: the PlatformIO integration's deploys name it
  (`integrations/platformio`, `[package.metadata.nros.deploy] board`), so it
  must keep resolving.
- `BoardDescriptor` gains `nuttx` / `platformio` (`deny_unknown_fields`);
  `answers_to` and `framework_board` read them. `west build -b` now reads a new
  Zephyr-only `zephyr_board()`, so a non-Zephyr id can never reach it.
  `refuse_framework_board`'s message no longer says `west build -b`.
- Each package's `<nano_ros_provides kind="board">` drops the string
  (`check-provider-announcements` compares it to `names`).
- The one image authoring `qemu-armv7a-nsh`
  (`multi_pkg_workspace_nuttx`) moved to `qemu-armv7a-nuttx`, and its
  `[board_config]` key with it; `check-site-config`'s duplicate row went.
- `check-deploy-board-resolves`'s `framework_id` / answer set mirror the two
  fields.

Guards: `nuttx_and_platformio_framework_ids_follow_the_image_deploy_rule`
(tier_resolver) — refused on an image, resolved on a deploy, for both — FAILS
with the two `framework_board` arms removed. Planting `board =
"qemu-armv7a-nsh"` back on the fixture's image makes
`check-deploy-board-resolves` fail naming `board = "nuttx"`. RFC-0064 records
the rule for all three ecosystems.
