---
id: 59
title: Zephyr rust service/action examples fail — generated `example_interfaces` crate not resolvable at cargo build
status: resolved
type: bug
area: codegen
related: [phase-244]
---

> **RESOLVED 2026-06-14** — added `ros-humble-example-interfaces` to
> `ci/docker/zephyr-ros/Dockerfile` (c71df9c55) + bumped the image tag to
> `humble-sdk0.17.4-r2` (51b504d19) so the dual-line lane pulls the rebuilt
> image. The image auto-republished (build-zephyr-ci-image run 27479056473) and
> the dual-line lane (run 27479915027) went **green on both 3.7 + 4.4** for all
> four `rust/service-*` + `rust/action-*` cells. Follow-up (still worth doing,
> tracked here): make `nros ws sync` fail loudly when a declared interface pkg's
> source dir is missing instead of silently skipping codegen + emitting a
> dangling `[patch]` entry — the silent skip is what masked the missing pkg.


`zephyr-dual-line.yml` cells `rust/service-server`, `rust/service-client`,
`rust/action-server`, `rust/action-client` fail on **both** 3.7 and 4.4 lines
(8 jobs). Root cause:

```
Zephyr: generating Rust interface crates for rust/service-server
ws sync: refreshed [patch.crates-io] block in .../service-server/Cargo.toml
ws sync: done.
...
error: no matching package named `example_interfaces` found
FAILED: .../librustapp.a  (recipe `build-one` failed with exit code 1)
```

## What's known

- The companion pubsub cells (`rust/talker`, `rust/listener`) — which depend on
  the builtin `std_msgs` — **pass**. So the per-example codegen + `ws sync`
  `[patch.crates-io]` rewrite mechanism works in general.
- service/action cells differ only in depending on the **external**
  `example_interfaces` package (srv `AddTwoInts`, action types +
  `action_msgs`/`unique_identifier_msgs`).
- The build log shows the Zephyr build step ran the interface-crate generation
  and `ws sync` *did* refresh the patch block:
  `example_interfaces = { path = "generated/example_interfaces" }`.
- Yet cargo resolves dep `example_interfaces = "*"` to nothing →
  `no matching package named example_interfaces found`. In a no-index no_std
  build this message means the `[patch]` did **not** bind: either
  `generated/example_interfaces/` was never materialized, or its `Cargo.toml`
  `package.name` ≠ `example_interfaces` so the patch fails to match the
  crates-io name.

## Root cause — CONFIRMED

The Zephyr CI image `ci/docker/zephyr-ros/Dockerfile` is `FROM
ros:humble-ros-base` and its apt block installs **no ROS interface packages**.
`ros:humble-ros-base` ships `std_msgs` / `builtin_interfaces` / `action_msgs` /
`unique_identifier_msgs` but **not `example_interfaces`** (that's a separate
demos/tutorials dep). `nros ws sync` codegen reads each declared interface
pkg's `.msg`/`.srv`/`.action` from `/opt/ros/humble/share/<pkg>`
(`NROS_EXAMPLE_INTERFACES_DIR`); when the dir is absent it **silently skips**
that crate's codegen, yet still refreshes the `[patch.crates-io]` block to point
`example_interfaces = { path = "generated/example_interfaces" }` at a dir that
was never created. cargo (offline, no registry index in the no_std build) then
can't satisfy `example_interfaces = "*"` from the dangling patch →
`no matching package named example_interfaces found`.

Confirmed two ways:
- talker/listener depend only on `std_msgs` (present in ros-base) → codegen
  succeeds → green; service/action depend on `example_interfaces` → skipped →
  red. Exactly the observed split.
- Local `nros ws sync examples/zephyr/rust/service-server` (full ROS install)
  emits all four crates incl. correctly-named `example_interfaces` → the
  mechanism is sound; only the CI pkg is missing.
- CI log shows `ws sync: refreshed [patch.crates-io] block` + `done` but **none**
  of the per-crate `ws sync: codegen <pkg>` lines a full sync prints.
- `ci/docker/ci-base/Dockerfile` (also FROM ros-base) already installs
  `ros-humble-example-interfaces`; `zephyr-ros` simply forgot it.

Note: the **native** rust action/service examples were fixed in phase-244 E3
(example_interfaces feature forwarding + `rmw-cyclonedds` wiring). The Zephyr
variants regress here for an unrelated reason (the missing CI pkg).

## Fix

1. **(done, in this commit)** Add `ros-humble-example-interfaces` to
   `ci/docker/zephyr-ros/Dockerfile`'s apt block. **Requires republishing the
   image** (`.github/workflows/build-zephyr-ci-image.yml` → a new
   `humble-sdk<ver>` tag) + bumping the tag the lane pulls — a maintainer step
   (registry push). The lane stays red until the new image lands.
2. **(follow-up)** Make `nros ws sync` **fail loudly** when a declared
   `<depend>`/`build_depend` interface pkg has no resolvable source dir, instead
   of silently skipping codegen and emitting a dangling `[patch]` entry. The
   silent skip is what let a missing CI pkg masquerade as a codegen bug.

Found 2026-06-13 while triaging the chronically-red zephyr-dual-line lane;
root-caused 2026-06-14 (no Zephyr SDK needed — repro'd the codegen step
standalone with `nros ws sync`).
