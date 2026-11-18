---
id: 2700
title: "`features` and `node_interfaces` stated in `[image.<id>]` do not reach a generated workspace entry, in its build or its sizing"
status: open
type: bug
area: [cli, sizing]
severity: medium
found: 2026-11-18
related: [2423, 2483]
---

## Measured

Found while measuring phase-556 W11 (the `sim_time` capability) on the
`examples/workspaces/rust` FreeRTOS image for MPS2-AN385. Stating
`features = ["sim_time"]` under `[image.freertos]` produced a byte-identical
ELF and a byte-identical sizing descriptor. The `nros.toml` overlay beside the
model did carry the feature. Stating the same feature in `[system]` changed
both.

## Cause

`SystemToml::with_image_features(image)` composes the system-level features
with an image's `[image.<id>]` overrides. Only some roads call it:

- The owned-entry road does (`cmd/build.rs`, the issue 2423 fix), and so does
  one facade path (`orchestration/facade.rs`, `sys.with_image_features(image_id)`).
- The generated workspace-entry road does not: `facade::entry_selection_in`
  and `facade::write_leaf_selection_in` derive from the system-level
  `SystemToml`.
- The sizing side does not either. Several descriptor and inventory producers
  call `InfraServices::from_model(model, None)`, i.e. with no image:
  `cmd/sizing_descriptor.rs` (two sites), `sizing_descriptor.rs`,
  `entity_inventory.rs`, `leaf_entity_env.rs` and `orchestration/model_ingest.rs`.

So on that road a per-image statement is silently ignored, in the build AND
in the pools.

## Why it matters

Phase-556 W13 turns the stock node interfaces on for every image, and a board
too small for them opts out per image with `node_interfaces = "none"` (Pico W,
PX4). On a generated workspace entry that opt-out does not take effect: the
image still builds the parameter services, `/rosout` and now `sim_time`, sized
as if it had opted out or not, depending on which half reads the image.

Fixing only one half is worse than neither: building a capability in without
sizing it (or the reverse) is a boot-time pool failure or a dead reservation.

## What would close it

- ONE function composes an image's effective features: the system level plus
  that image's `[image.<id>]` overrides, then `node_interfaces`
  (`nros_orchestration_ir::node_interfaces::effective_features`).
- Every road calls it, building and sizing alike: `entry_selection_in`,
  `write_leaf_selection_in`, and every `InfraServices::from_model` caller, which
  gets the image id rather than `None`.
- A test per road that an image-level opt-out (and an image-level opt-in)
  reaches both the build's feature set and the sizing descriptor.

## Acceptance

`[image.<id>] node_interfaces = "none"` on a generated workspace entry builds
an image without the stock interfaces and a descriptor whose `infra_families`
is `none`; `[image.<id>] features = [...]` adds an axis to both.
