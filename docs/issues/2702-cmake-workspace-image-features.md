---
id: 2702
title: "A C/C++ workspace image builds and sizes with the `[system]` features, not its own `[image.<id>]` ones"
status: open
type: limitation
area: [cli, cmake, sizing]
severity: low
found: 2026-11-18
related: [2700]
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

## Acceptance

Once a choice is made: an image-level `node_interfaces = "none"` on a C/C++
workspace image changes that image's build and descriptor as the choice
says, with a test on the cmake workspace road, and the allowlist entry for
`coordinate_member_model` in `check-node-interface-readers` goes away.
