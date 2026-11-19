---
id: 2726
title: "Whether an image crate bundles a backend is decided twice: in Rust (`image_crate_bundles`) and again in `cmake/NanoRosImageCrate.cmake`"
status: open
type: tech-debt
area: [cmake, rmw, build]
severity: low
found: 2026-11-19
related: [2705, rfc-0071, rfc-0094]
---

## What is duplicated

Issue 2705 made the NuttX make road refuse every RMW the generated image crate
does not bundle. The Rust side states the rule as
`RmwDescriptor::image_crate_bundles()` (`packages/cli/cargo-nano-ros/src/rmw_descriptor.rs`):
`[rmw.link]` strategy `umbrella` plus a Rust register crate. The CLI's refusal
(`nros locate nuttx-cargo-config`) asks it.

`nros_image_backend()` in `cmake/NanoRosImageCrate.cmake` makes the same
decision on its own, from the dispatch rows `NROS_RMW_LINK_STRATEGY` and
`NROS_RMW_REGISTER_CRATE`:

```cmake
if(NOT NROS_RMW_LINK_STRATEGY STREQUAL "umbrella" OR NOT NROS_RMW_REGISTER_CRATE)
```

Two spellings of one rule. If one changes without the other, the refusal and
the image disagree: the road either refuses a backend the crate would bundle,
or lets through one it leaves out, which is issue 2705's silent image with no
backend.

## Held for now

`check-image-crate-bundling-agreement` (`just check image-crate-bundling-agreement`)
runs `nros_image_backend()` in cmake script mode for every RMW the scan
announces and compares it with the CLI's answer
(`nros dev rmw dispatch --known --image-bundles`), in both directions. Its
negative controls mutate the dispatch rows and require the gate to go red.

## Direction (option ii)

`nros dev rmw dispatch --lines` emits a Rust-computed row (for example
`NROS_RMW_IMAGE_BUNDLED ON|OFF`), and `nros_image_backend()` reads that row
instead of deciding. The gate then has nothing to compare and is deleted.

The CLI and `NanoRosRmwDispatch.cmake` must move together: the module
FATAL_ERRORs on a row it does not know, and on a row it expects that the CLI
does not emit. So a cmake tree that expects the new row REFUSES an older CLI
that lacks it. That is a loud refusal, not a silent drop of the backend, and
the CLI freshness stamp already refuses a stale in-tree CLI on most roads
before the dispatch query runs. Check the roads that run a released/prebuilt
CLI against a newer tree (the SDK store, a consumer's pinned CLI) and say in
the fix what they print.
