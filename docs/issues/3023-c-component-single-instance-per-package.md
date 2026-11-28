---
id: 3023
title: "Two nodes of one C component share one instance: `RCLC_C_COMPONENT` keeps a single static state and `rcl_node_t` per package"
status: open
type: bug
severity: medium
area: [c-api, components, codegen]
related: [issue-3021, issue-2780, rfc-0128]
found: 2026-11-28
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

## Fix (open)

Either size the component's storage per instance, with the instance count from
the model, which knows how many times a launch file names the exec, or refuse
a second instance of a single-instance C component at build time, naming the
node. Silent sharing is the one wrong answer. Acceptance: the cell asserts
that each talker published its first sample (`Published: 0` twice).
