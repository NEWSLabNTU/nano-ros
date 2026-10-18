---
id: 1971
title: "Every executor pays a 16-rule remap table and the 8 KiB arena floor, whatever it hosts"
status: open
type: enhancement
area: [memory, zephyr]
severity: low
found: 2026-10-19
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
