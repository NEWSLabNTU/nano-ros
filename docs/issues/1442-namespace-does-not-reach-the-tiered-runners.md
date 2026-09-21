---
id: 1442
title: "The launch-declared identity reaches `run_components` and not `run_tiers`
  — four C-ABI tier runners pass a NULL namespace, and the hosted Rust one
  passes no identity at all"
status: open
type: bug
area: boot, boards
related: [rfc-0045, 0794, 1410, 1434]
---

## Problem

Issue 1434 threaded the `.nros_boot_config` namespace from the generated entry
down to `nros_cpp_init`, which has always had a parameter for it. It did that
on the SINGLE-EXECUTOR path only. The tiered path is untouched, and it is
untouched in two different ways.

**The four C-ABI tier runners still pass NULL.** Measured, all four:

```
packages/api/nros-cpp/src/lib.rs           nros_board_native_run_tiers
packages/boards/nros-board-freertos/c/freertos_run_tiers.c:561
packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c:598
packages/boards/nros-board-nuttx-qemu/c/nuttx_run_tiers.c:685
```

each reaching `nros_cpp_init(locator, domain_id, sn, NULL, boot_storage)`. The
generated entry's `run_tiers` call carries `nros_boot_config_node_name(...)` and
nothing else; both boot wrappers
(`packages/cli/nros-cli-core/src/codegen/entry/packs/entry/{c,cpp}/boot_wrapper.jinja`)
name this issue at the call site.

**The hosted Rust `run_tiers` carries no identity at all.**
`nros-board-linux::run_tiers` ignores its `DeployOverlay` by a decision that
predates this (issue #48, "kept for signature parity"), so before 1434 it opened
its session with `ExecutorConfig::from_env()` — the env rung over nothing. 1434
gave it the baked NAMESPACE and deliberately left `node_name` alone, because
threading that changes what `ros2 node list` prints for every tiered native
image and is a decision on its own.

## Why it was carved out of 1434

Cost and reach, both concrete:

* Covering the C-ABI half means `_ns` twins of four more symbols, on top of the
  two 1434 added — and each of the three RTOS tier runners is its own file, so
  that is four more places for the copies to drift, which is exactly what
  `nros_rtos_run_components.c` exists to avoid on the components path.
* The blob only ever carries IDENTITY for a one-node plan
  (`boot_config_view`: `if plan.nodes.len() != 1 { return }`), and a tiered plan
  rarely is one. So the reachable population is a single-node image that also
  declares tiers.
* A tiered image's PER-NODE namespace already travels, by a different road: the
  `(node name, node namespace, group)` triples issue 1172 added to
  `nros_native_tier_spec_t`. What is missing is the SESSION rung — the identity
  the executor opens with, and the field a post-link patcher rewrites.

## Acceptance

A single-node tiered image — one C or C++, one Rust — whose launch declares a
namespace opens its primary session under it, asserted on the ON-WIRE name. Plus
the negative direction 1434 already pins one layer over: a blob with
`BOOT_SET_NAMESPACE` clear must leave the next rung speaking, never deliver `""`.

Decide `node_name` on `nros-board-linux::run_tiers` in the same change, or say
why it stays: leaving half the identity threaded is the shape that made this
issue necessary.
