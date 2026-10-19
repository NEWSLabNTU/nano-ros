---
id: 1971
title: "Every executor pays a 16-rule remap table and the 8 KiB arena floor, whatever it hosts"
status: resolved
type: enhancement
area: [memory, zephyr]
severity: low
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "PR #2421 (arena floor) + branch fix/1971-launch-remaps-in-flash (launch remaps in flash)"
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

RFC-0107 steps 1 and 2 (the design is RFC-0107, "One executor core per
image"; this issue's first fix, draft PR #2411, sized the remap table per
executor and was superseded).

- **Arena floor** (PR #2421): `ARENA_FLOOR` applies only where the demand is
  unknown. A declared image's arena and every tier's arena are their model,
  unfloored (`packages/core/nros-node/build/arena_floor.rs`,
  `tests/arena_floor.rs`). The island's `estop` tier: 8,192 -> 7,500 B.
- **Launch remaps in flash**: the generated C and C++ entries emit the
  launch's `<remap>` rules once, as `static const nros_cpp_launch_remap_t
  __nros_launch_remaps[]`, and install it with `nros_cpp_set_launch_remaps`
  as the first statement of the entry point, before the runner sets up any
  tier (`launch_remap_table.jinja`, `launch_remap_install.jinja`). Every
  executor reads that table through a borrow (`executor::launch_remap`); no
  `nros_cpp_declare_remap` call is emitted. Launch rules stay authoritative
  and argv rules the fallback (`RemapView`, the same two parameters of
  `resolve_name_layered`).
- **The RAM table is the runtime table**: `ExecutorSizing::remaps`, default
  `NROS_EXECUTOR_RUNTIME_REMAPS` = 16 on a target with process arguments,
  0 on bare metal. At 0, `declare_remap` is refused by name
  (`RemapRefused::NoRuntimeTable`; `NROS_CPP_RET_FULL` with a logged reason)
  and the knob restores slots. Bare metal saves 6,464 B per executor on
  thumbv7em; a hosted image keeps its 16.

Tests: the existing layering tests (`ros_args`, `node.rs`) pass unchanged;
new ones cover the flash table above argv on both roads, the named refusal,
and a full table. Goldens for both entry packs (`c_native_rich`,
`cpp_native_rich`); every remap-free golden is byte-identical; and a test
per pack asserts the table is installed in the entry point before the runner
calls any tier setup.

