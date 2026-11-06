---
id: 2298
title: "A workspace image's take buffer and payload classes are never derived, and the obvious closure is the whole workspace's `generated/`"
status: open
type: tech-debt
area: [cli, memory]
severity: medium
found: 2026-11-06
related: [phase-531, 1233, 1125, 1393]
---

**Home: [phase-531](../roadmap/phase-531-workspace-first-project-layout.md)**: W6 moves
single-node examples onto the workspace road. Until this is fixed, every image
it moves loses the sizing it had as a cargo leaf.

## What happens

A cargo leaf derives two sizing facts from its message-bound inventory:

- the take buffer (`RX_BUF`, `NROS_SUBSCRIPTION_BUFFER_SIZE`; issue 1233), from
  `leaf_take_buffer::take_buffer_for_leaf`;
- the payload classes (`LARGE_PAYLOADS`; issue 1125), from
  `leaf_payload_classes`.

The workspace road states neither. `cmd/build.rs` `derived_pool_env` passes
`TakeBuffer::Refused` and `PayloadClasses::Refused` with the reason "not
derived for a workspace image (no leaf message-bound inventory)". So the crate
defaults stand. Issue 1125 measured 131 KB of extra `.bss` from the
payload-class default alone.

## Why the obvious fix is wrong

The inventory reader (`leaf_payload_classes::leaf_bound_inventory`) reads a
leaf's `generated/` tree, which is that leaf's closure. A workspace's
`generated/` holds every message package any member names. Phase-531 W5 tried
pointing the reader at it: the closure then held 38 unbounded types, which
refuses the knob for every image (one unbounded type in the closure refuses the
whole take buffer, per the CMake lane's rule). It is also an over-derivation in
the other direction. An image that never links a large type would still be
sized for the largest type in the workspace.

## Direction

Derive over the image's own closure: the message packages its nodes reach.
That is the transitive `<depend>` set of the packages the image's launch
selects, intersected with `generated/`. The closure is already known on this
road, because the entry generator walks it to write path deps
(`cmd/build.rs` `registry_patches_from`). Feed that package set to the same
`derive` functions the leaf road uses, so the rule stays in one place. Then
render the result through `leaf_entity_env::render_env_sidecar`, as
`derived_pool_env` does for the pool knobs.

Acceptance: a workspace image and the cargo-leaf build of the same node state
the same `NROS_SUBSCRIPTION_BUFFER_SIZE` and the same payload classes. Compare
`linux_service_server` and one image that receives `std_msgs/String`.
