---
id: 1677
title: "The `features` Zephyr Rust entries (params, lifecycle, qos) halt at boot: the parameter store is one 285,696-byte allocation on a 256 KiB heap"
status: resolved
type: bug
area: [zephyr, sizing]
severity: high
found: 2026-10-05
related: [1324, 1424, 0756]
resolved_in: (this change)
---

## Symptom

Three `entry_e2e::entry_matrix` cells failed on the Zephyr native_sim Rust
lane, each with a message that blamed the wire:

- `zephyr/rust/params`: "subscriber never saw the live-read baked param value (120)";
- `zephyr/rust/lifecycle`: "`ros2 lifecycle nodes` listed no managed node";
- `zephyr/rust/qos`: "observer never saw 3 `/qos_ok` republishes".

None of them was about delivery. Booted by hand against a router, every image
halts two seconds in:

```
nros: HEAP EXHAUSTED (TOO SMALL): request 285696 bytes, arena 262656 bytes, free 248976 bytes, largest free block 248976 bytes
nros: PANIC platform heap exhausted
```

## Cause

gdb on `nros_platform_alloc` with `size > 200000`:

```
alloc::boxed::Box::<nros_params::server::ParameterStorage<32>>::new_uninit
nros_node::executor::spin::Executor::leak_parameter_storage
nros_node::executor::spin::Executor::new_param_state
nros_node::executor::spin::Executor::ensure_parameter_store
nros_node::executor::spin::Executor::register_parameter_services
nros::node_runtime::...::apply_param_services
```

`ParameterStorage<32>` is a single allocation of ~8.5 KiB a slot (phase-382
W2'; `ParameterValue` is sized by its `StringArray` variant). Issue 1324
(2026-10-02) correctly made `nros_platform_alloc` the Rust global allocator, so
that allocation moved from picolibc's 1 MiB arena onto
`CONFIG_NROS_ZEPHYR_HEAP_SIZE`. The six confs that had sized picolibc for "a
~75 KiB param-service allocation" were each given 262144. The 75 KiB figure
predates phase-382, so the store could never fit.

The e2e heap gate (issue 1424) could not catch this. It reads the boot record
after a cell has done its work, and these images die before that point.

## Fix

The only conf whose images build a parameter store is
`examples/workspaces/features/src/demo_bringup/boards/native_sim_native_64/prj-zenoh.conf`.
It now sets `CONFIG_NROS_ZEPHYR_HEAP_SIZE=524288` and states what that number
was measured from.

Peaks were read with a 1 MiB heap from the live boot record, using the gate's
own decoder (`read-boot-report.py --heap-headroom`):

| image | peak |
| --- | --- |
| lifecycle | 399,600 |
| params | 399,696 |
| qos | 406,592 |

512 KiB leaves about 115 KiB above the worst of them.

The other five workspace Rust images that state 262144 were measured the same
way and peak at 16–23 KB: `entry`, `safety`, `realtime`, `realtime-derived` and
`mh-robot1`. They are unaffected and unchanged.

## Verification

1. The three images were rebuilt incrementally, and their `.config` reads
   `524288`.
2. `int32-sink-zenoh` was rebuilt, because it was stale against the tree.
3. `cargo nextest run -p nros-tests --test entry_e2e -E 'test(entry_matrix)'`
   reported `entry_matrix: 3 ran, 14 skipped, 0 failed`. The 14 skipped cells are
   other platforms' stale or unbuilt fixtures.

