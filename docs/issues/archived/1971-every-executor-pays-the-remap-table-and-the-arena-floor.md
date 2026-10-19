---
id: 1971
title: "Every executor pays a 16-rule remap table and the 8 KiB arena floor, whatever it hosts"
status: resolved
type: enhancement
area: [memory, zephyr]
severity: low
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "PR #2411 (branch fix/1971-remap-arena-sizing)"
related: [1970, 0563]
---

## What happens

With issue 1970 a tier's executor is sized from its own members, and what is
left of its cost is per EXECUTOR rather than per member:

- the remap table, `MAX_REMAPS` (16) x `size_of::<Option<RemapRule>>()`
  (404 B on thumbv7em) = 6,464 B in every backing (`storage.rs`, "Fixed count
  (`MAX_REMAPS`) rather than a new `ExecutorSizing` knob"), on an image whose
  launch file states no remap;
- the arena floor, `ARENA_FLOOR` 8,192 B, applied to a tier's derived demand
  as to the image's.

Measured on the Autoware safety island split into two tiers (issue 1970): the
second tier costs +14,928 B of DTCM and misses the image's 8 KiB guard by
4,504 B. The remap table alone is 12,928 B across the two executors; sized to
the launch's remaps (none) the split fits with room to spare, and the
one-tier image gains 6,464 B.

## Direction

- Make the remap table a sizing count (`ExecutorSizing`), derived per
  executor from the remaps its nodes are launched with, plus whatever the
  hosted `--ros-args -r` road needs (it reads the process arguments; an RTOS
  image has none).
- Apply the arena floor to an image whose demand is unknown, not to a tier
  whose demand was derived.

## Resolution

- **The remap table is a sizing count.** `ExecutorSizing` gains `remaps`;
  `carve` and the layout use it. `config::MAX_REMAPS` (the default, also
  `nros_node::executor::spin::MAX_REMAPS`) is derived by `nros-node`'s build
  script from the sizing descriptor's new `[image] remap_rules`, the launch's
  `<remap>` count over the image's nodes, plus 16 for the `--ros-args -r`
  road on a target with process arguments (`target_os != "none"`; NuttX
  keeps it). An image whose descriptor states no count keeps the old 16.
- **Each tier states its own.** `nros ws sizing-descriptor --from-model`
  writes `[image] remap_rules` and each `[[tier]]`'s `[tier.image]
  remap_rules`, the rules of the nodes it hosts (what the generated entry
  declares on that tier's executor); `TierSizing` carries `remaps`. A tier
  hosting a node the model does not name states none and keeps the image's
  table.
- **The arena floor applies only to an unknown demand.** `ARENA_FLOOR` now
  raises only the undeclared budget over `max_cbs` slots; an image or tier
  whose entities are declared is sized to its model.

## Measured (the safety island, `just board-build`, S32K344, not flashed)

| image | DTCM used | free vs the 8 KiB guard |
| --- | ---: | --- |
| one tier `mrm` (as shipped) | 106,000 (was 112,464; -6,464, the table) | 25,072 free |
| two tiers, `estop` = the 30 Hz emergency operator, `mrm` = the rest | 113,784 (was 127,384 with #1970 alone) | 17,288 free: fits |

The descriptor states `remap_rules = 0` for the image and both tiers (the
launch states no remap), so every backing carves no remap table; the
`estop` tier's arena is its model's 7,500 B instead of the 8,192 B floor.
The second tier now costs +7,784 B over the one-tier image.

Tests: `nros-node` (`the_remap_table_is_carved_at_the_sizing_count`,
`a_tier_is_carved_at_its_own_counts` extended), `nros-cli-core`
(`each_tier_states_its_own_nodes_remaps`).
