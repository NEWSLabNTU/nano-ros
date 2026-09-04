---
id: 1061
title: Entity budgets are derived for CMake consumers only, so pure-cargo leaves hand-set them
status: open
area: build
severity: medium
related: [1052, 0827, 0832, 0190]
---

## What

`nros-cli-core/src/entity_inventory.rs` computes how many entities of each kind
a component declares, and publishes the result as CMake variables:

```rust
"set(NROS_DERIVED_MAX_SUBSCRIBERS {})\n",
```

That reaches every leaf built through `cmake`. It reaches **no pure-cargo leaf**
— `examples/qemu-esp32-baremetal/rust/{talker,listener}` and
`packages/testing/nros-tests/bins/logging-smoke-esp32-qemu` are built by plain
`cargo build`, so they take `zpico-sys`'s compiled-in defaults (8 subscribers,
8 queryables) whatever they declare.

## Why it matters here specifically

On `qemu-esp32-baremetal` an over-budget static is not a footprint nicety. The
stack is the **linker leftover** after `.bss` (`link.x` fills DRAM up to
`_stack_start`, `.bss` grows up from below), so every byte of unused pool comes
straight out of the stack, and there is no runtime overflow guard: the image
writes frames into `.bss` and dies later as a wild jump somewhere unrelated.

That is issue 1052. The talker shipped with an **18,572 B** stack against
node.rs's ~67 KB budget and faulted with `sp` outside the stack, inside
`nros_smoltcp::TCP_RX_BUFFER_0`. Two of its pools were sized for entities it does
not declare.

## The workaround now in the tree, and why it is one

`examples/fixtures.toml` hand-sets the budgets per row:

| row | declares | hand-set |
| --- | --- | --- |
| talker | 1 pub, 1 timer | `ZPICO_MAX_QUERYABLES=2`, `ZPICO_MAX_SUBSCRIBERS=1` |
| listener | 1 sub | `ZPICO_MAX_QUERYABLES=2`, `ZPICO_MAX_SUBSCRIBERS=1` |
| logging-smoke | — | `ZPICO_MAX_QUERYABLES=2` |

Stacks recovered: talker 18,572 → 49,148 B, listener 22,060 → 52,636 B.

It works, and it is the third time the same edit has been made by hand. The
numbers restate what `register` already says, in a different file, with nothing
tying them together — so they go stale exactly when a leaf gains an entity, which
is the moment being wrong costs the most. The queryable half of this table was
already applied "a row too narrowly the first time" (its own comment says so),
which is this failure mode having happened once.

## Fix

Deliver the inventory to the cargo path as well as the CMake path, so a leaf's
budgets follow its declarations. The inventory is already computed; what is
missing is a channel `zpico-sys`'s `build.rs` can read — the same shape as
`nros_zephyr_build::knob_usize` reading `$DOTCONFIG`, and subject to the same
rule as issue 0460: a knob that reaches one lane and not the other is an ABI
split, not just a missing feature.

Until then `scripts/check-stack-floor.py` is the backstop: it fails the build if
an ESP32 image drops below a 32 KB stack, so a leaf that gains an entity without
gaining budget is caught at build time rather than as a wild jump at runtime.
The gate bounds the damage; it does not remove the hand-maintenance.

## Not to do

Do not raise the ESP32 stack by shrinking the heap. node.rs documents that as a
two-sided constraint (issue 0190): 16 KB is too small for the executor backing
and 96 KB starves the stack. The heap is not where the slack is — the unused
pools are.
