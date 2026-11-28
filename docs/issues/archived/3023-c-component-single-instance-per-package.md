---
id: 3023
title: "Two nodes of one C component share one instance: `RCLC_C_COMPONENT` keeps a single static state and `rcl_node_t` per package"
status: resolved
type: bug
severity: medium
area: [c-api, components, codegen]
related: [issue-3021, issue-2780, rfc-0128]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "the change that adds this line"
---

## What happens

features' `c_lifecycle_pair.launch.xml` launches the same C exec twice, as
`talker` and `talker2`, each a `<lifecycle_node>`. On
`zephyr_mps2_c_lifecycle_pair_xrce` (mps2/an385, XRCE, QEMU) one counter
publishes:

```text
Published: 0
Published: 1
Published: 2
Published: 3
```

Two instances at 1 Hz would print each value twice (`0, 0, 1, 1, ...`). The
same output was seen on main `a73e626748` and on the issue 3021 fix branch,
with no error line.

## Why

`RCLC_C_COMPONENT(StructT, configure_fn)` (`packages/api/nros-c/include/rclc/component.h`)
emits ONE static `StructT` and ONE static `rcl_node_t` per package
(`__nros_c_inst_<pkg>`, `__nros_c_node_<pkg>`). Its `_create` returns the
address of that one instance every time. When a launch file instantiates a
component twice, the generated entry calls `_create` and `_configure` once per
node, and both nodes get the same storage. The second configure
re-initialises the first's publisher, timer and counter, and binds the shared
`rcl_node_t` to the second node. One timer is left running.

The header says why it is static: "no heap, no sizeof leak to the entry".
That holds for one instance per package and nothing else. The C++ register
macro is keyed the same way (`NROS_PKG_NAME`), so it should be checked for the
same limit.

## Why no test saw it

The cell counted `Published:` lines (four), which one talker alone satisfies.
The issue 3021 fix made the cell assert that every service server of both
nodes exists. That assertion passes, and the publishes still come from one
instance.

## The class, swept

Each language's road for "one class, many nodes":

- **Rust** (`nros::node!`, `nros-macros`): already per instance.
  `__NROS_COMPONENT_<pkg>_SLOT_STORE` is a `ComponentSlotStorage` of
  `NROS_RUNTIME_MAX_CLASS_INSTANCES` slots (default 2), and installing past
  the cap is a registration error.
- **C++ classes** (both entry arms): already per instance. The generated entry
  gives every node its own `__nros_comp_buf_<idx>` / `__nros_comp_<idx>`,
  indexed by the node's position, never by the package.
- **C components**, from either the C entry or the C++ entry's C arm: the
  defect. One static per package.

## Fix

- `RCLC_C_COMPONENT` keeps a static table of `RCLC_C_COMPONENT_MAX_INSTANCES`
  slots (default 2, the Rust per-class default; `-D` on the component's
  compile raises it). Each slot holds its own state struct and `rcl_node_t`.
  `create` hands out the next free slot. `configure` binds the node into the
  slot `self` belongs to. No heap, and no sizeof leak to the entry, as before.
- Past the cap, `create` returns NULL and logs one error naming the package
  and the define. Both entry templates (C, and the C++ entry's C arm) return
  `NROS_CPP_RET_FULL` instead of configuring. Sharing is gone, and a
  refusal is what replaces it.
- `packages/api/nros-c/tests/run/component_instances.c` (run by
  `check c-link-smoke`) expands the macro and checks three things: two
  distinct instances, each configured once on its own node, and a third that
  is refused with one error line.
- The board cell asserts that each talker published its first sample.

Acceptance, on host 139 (2026-11-28):

- **Negative control.** `component_instances.c` built against main's
  `component.h` fails at `a != b`, which is the defect itself.
- **Fix.** `component_instances: OK`. The `zephyr_mps2_c_lifecycle_pair_xrce`
  cell prints `Published: 0, 0, 1, 1` (both instances) and passes. Heap
  peak 215,208 of 320,000 B, against 214,672 on main: the second instance's
  slot and timer.
