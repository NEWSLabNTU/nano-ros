---
id: 2702
title: "A C/C++ workspace image builds and sizes with the `[system]` features, not its own `[image.<id>]` ones"
status: resolved
type: limitation
area: [cli, cmake, sizing]
severity: low
found: 2026-11-18
resolved: 2026-11-22
resolved_in: "feat(cli, issue 2702): an image with its own capability axes gets its own cmake configure"
related: [2700, 2833]
---

## What

Issue 2700 made an image's own `[image.<id>] features` and `node_interfaces`
reach its build and its sizing on the generated (cargo) workspace entry, the
leaf roads and every cmake verb that is told its image (`--image`, passed from
`NROS_IMAGE`). One road is left: a C or C++ WORKSPACE image.

Such images are configured by ONE cmake configure per coordinate, and that
configure builds one Rust runtime shared by every entry in it (RFC-0100 D12,
`nros/sizing/runtime/shared.toml`). It sets no `NROS_IMAGE`, so the runtime's
build and its sizing both read the `[system]` list. They agree with each
other, so nothing is mis-sized, but an image-level opt-out (for example
`[image.mcu] node_interfaces = "none"`) or opt-in has no effect on a C/C++
workspace image. `nros build`'s precomposition loads the coordinate's member
model with no image on purpose (`coordinate_member_model` in
`packages/cli/nros-cli-core/src/cmd/build.rs`), so that it matches what the
configure's own producers read; the issue 2700 gate allowlists that call with
this reason.

## The design question

What does a runtime shared by several images build with, when the images
state different features?

1. **The UNION of the images' features.** The shared runtime compiles every
   capability any of its images wants; each image's sizing stays per image
   (each entry's descriptor prices only its own families). Cost: code for a
   capability an image opted out of is still linked into the shared runtime
   (flash, not pools), and an image cannot opt OUT of code another image in
   the same configure opts into. The opt-out still saves that image's RAM.
2. **One configure per image.** Each image gets its own runtime, built with
   exactly its own features, which is what the cargo road does. Cost: one more
   configure and one more runtime build per image in the coordinate (build
   time and disk), and the shared-runtime work of RFC-0100 D12 stops sharing.
3. **Refuse a disagreement.** An image-level feature that differs from its
   coordinate's other images is refused by name, until one of the above is
   chosen. Cost: none at runtime; it turns the silent ignore into an error.

## Interim: option 3 (2026-11-22, approved by the nros 241 manager)

Until 1 or 2 is chosen, a C/C++ workspace image whose own `features` /
`node_interfaces` differ from the `[system]` list is REFUSED at `nros build`
(`refuse_image_features_a_shared_configure_drops` in `cmd/build.rs`), naming
the image, the features it adds or drops, and every image on its coordinate.
An image whose list equals `[system]`'s builds as before. Measured over every
in-tree C/C++ workspace row (274, by `nros build --dry-run`): none is refused.
23 of them are owned-entry images with no launch file, so no model, and the
check skips them. The verdict row `image_features_shared_configure_refused`
records the refusal; `tests/image_features_shared_configure.rs` asserts it.

**1 vs 2 is still open, for the user.** The refusal forecloses neither: lifting
it is part of either choice.

## Acceptance

Once a choice is made: an image-level `node_interfaces = "none"` on a C/C++
workspace image changes that image's build and descriptor as the choice
says, with a test on the cmake workspace road, and the allowlist entry for
`coordinate_member_model` in `check-node-interface-readers` goes away.

## Resolved, 2026-11-22: one configure per image

Ruled by the user on 2026-11-22. Every C/C++ workspace image gets a cmake
configure and runtime of its own, keyed on its image id
(`<key>-<bringup>-<image>`), built and sized for exactly that image. The
design and the measurements are in
[RFC-0100, "D12 amended, 2026-11-22"](../../design/0100-rmw-agnostic-sizing-model.md#d12-amended-2026-11-22--every-workspace-image-has-its-own-configure-and-runtime).

Why not share even among images with equal axes: measured on this issue's
fixture, two images with identical axes and models had `.bss` 732,360 B
sharing one configure, against 331,448 B each alone.

- The configure is passed `-DNROS_FEATURE_IMAGE=<id>`, which the readers of
  the capability axes take (`_nros_feature_image_arg`).
- The interim refusal is gone, and so is the `coordinate_member_model` entry in
  `check-node-interface-readers`.
- A build retires both older root layouts, `<key>` and `<key>-<bringup>`.
- 235 fixture rows' `build_subdir` follow, checked by
  `every_generated_workspace_fixture_row_names_its_cmake_build_dir`.

Measured on this issue's fixture (`size` text/data/bss):

- `slim` (`node_interfaces = "none"`): from 2,202,476 / 80,152 / 732,360 to
  2,078,748 / 68,184 / 127,144 bytes. Its descriptor states
  `infra_families = "none"`.
- `plain`: its whole ELF and its descriptor are byte-identical to `plain`
  built as the only image.
- `examples/workspaces/features` (17 images): 541 s and 22,643 MB, against
  483 s and 1,828 MB shared.

`linux_cpp_params` now states its string and array parameter bounds. Alone,
its declarations are stated, and phase-446 requires them.

The verdict row `image_features_shared_configure_refused` records both
images' configures, feature lists and descriptors.
`tests/image_features_shared_configure.rs` asserts them, and was
mutation-checked.

The N:1 runtime road (`runtime/shared.toml`, the shared parameter union)
remains for a hand-written root that registers several entries.

Found on the way: issue 2833. A `fixture-script` row is reused after a CLI
change.
