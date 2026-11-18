---
id: 2700
title: "`features` and `node_interfaces` stated in `[image.<id>]` do not reach a generated workspace entry, in its build or its sizing"
status: resolved
type: bug
area: [cli, sizing]
severity: medium
found: 2026-11-18
related: [2423, 2483]
resolved: 2026-11-18
resolved_in: "fix(issue 2700): an image's own features and node_interfaces reach its build and its sizing on every road"
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

## Resolution

ONE function composes an image's capability axes:
`nros_orchestration_ir::node_interfaces::image_features` (the system list, then
what the image adds over `[image_defaults]`, through the switch the most
specific table states). Its three callers are the three readers every road
goes through: `Overlay::features_for_image` (beside a model),
`LeafSystem::features` (the leaf reader, which before this did not add the
image's own `features` at all) and `SystemToml::with_image_features` (the
strict CLI schema).

The image is now a fact of the LOAD, not an argument a reader may leave out.
`LoadedModel` carries the image it was read for (`LoadedModel::image`), every
constructor and loader takes it (`with_overlay_beside`, `load`,
`model_ingest::load_model`, `plan_from_model`, `leaf_model`, `leaf_env`,
`with_leaf_frame`), and `LoadedModel::features()` and
`InfraServices::from_model(model)` answer for it. So every reader downstream
of a load (the inventory's frame, the descriptor's `infra_families`, the
Cyclone type count, `codegen entry`'s plan, `nros::main!`'s capability
asserts) reads the same image the build does.

Per road, building and sizing together:

- Generated workspace entry (cargo): `facade::entry_selection_in` derives from
  the claimed image's `with_image_features`; `nros build`'s stage 3.5 loads the
  model for the image, so the composed inventory and the descriptor carry its
  families; `nros::main!` reads the model for `NROS_IMAGE` (now an `env-dep`
  of the expansion).
- Single-package leaf: `facade::write_leaf_selection_in` and the leaf's
  `[env]`, west fragment and descriptor (`leaf_model(leaf, image)`).
- cmake / Zephyr bake: `codegen entry`, `config show --format cmake`,
  `dev image sizing-descriptor --from-model` and `dev image entity-inventory`
  take `--image`, which the cmake modules pass from `NROS_IMAGE`;
  `codegen-system` composes for its resolved target.

`check-node-interface-readers` now refuses an image-less load (a literal
`None` image passed to any of the loaders above, outside a short allowlist of
topology-only reads) and a facade `derive(` with no `with_image_features`
before it. It also no longer stops reading a file at a `#[cfg(test)] mod x;`
declaration, which had hidden the whole of `codegen/entry/mod.rs` from it.

## Measured after the fix

`examples/workspaces/rust`, image `linux`, `nros build linux`:

| `system.toml` | `nros` features | `infra_families` | ELF |
|---|---|---|---|
| as committed | `alloc rmw-cffi macros ros-humble` | `none` | 12,395,832 B |
| `[image.linux] features = ["param_services", "rosout"]` | adds `param-services rosout` | `param+rosout` | 13,009,344 B |
| `[system] features = ["param_services", "rosout"]`, `[image.linux] node_interfaces = "none"` | `alloc rmw-cffi macros ros-humble` | `none` | byte-identical to the first row |

In the third row the sibling image `linux_xrce` (no switch of its own) keeps
`param-services rosout` and `param+rosout`.

## Not covered

A C/C++ WORKSPACE image is configured in one shared cmake configure per
coordinate, which names no single image (`NROS_IMAGE` is unset there), so its
build and sizing both still read `[system]`. They agree with each other, and
`nros build`'s precomposition for that configure deliberately reads no image
either, so that it matches what the configure's own producers read. Giving
that road per-image features needs a decision about what a runtime shared by
several images builds with.

