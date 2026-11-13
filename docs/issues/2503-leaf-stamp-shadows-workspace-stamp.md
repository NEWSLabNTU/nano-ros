---
id: 2503
title: "After a leaf becomes a workspace, its leftover leaf-era configure stamp and census leave every hooked configure unseeded"
status: open
type: bug
severity: medium
area: [build, cli, cmake, zephyr]
found: 2026-11-13
related: [2504]
---

## What

Measured on 2026-11-13 in a persistent agent worktree, during a tier-2
fixture build (`just build-test-fixtures lane=tier2`), on both main
`8b07c45633` and a PR head on top of it. Two Zephyr rows refused at
configure every time: `build-c-talker-cyclonedds` and
`build-nucleo-f767zi-c-talker-serial`. The build log says:

```
-- nano-ros: nros configure .../examples/zephyr/c/talker --image zephyr_cyclonedds: configured (0 s)
-- nano-ros: image `zephyr_cyclonedds`'s configure is stale (inputs changed (recorded sha256:dff6..., now sha256:56b1...)): this configure reads no seed from it ...
```

and then the configure's promise check (`cmake/NanoRosPromise.cmake`) fails
with "the two halves of this image disagree". A configure that has just
been run is reported stale a moment later.

## Why

`examples/zephyr/c/talker` became a workspace on 2026-11-10 (commit
`3eefb44f93`). The checkout's gitignored `build/nros/` still held what the
leaf had written:

- `configured/zephyr_cyclonedds/stamp.toml` and
  `configured/nucleo_serial/stamp.toml`, keyed by the BARE image id and
  naming the old leaf-era resolved dir;
- `census/leaf.census.json`, recording a leaf-root `CMakeLists.txt` that no
  longer exists.

Two readers in `packages/cli/nros-cli-core/src/configure_stamp.rs` take
those files as current:

- `stamp_key_for` returns the bare key whenever `stamp_file(project, image)`
  exists, so `nros locate configured --image zephyr_cyclonedds` (what the
  cmake hook asks) reads the leaf-era stamp instead of the workspace's
  `<bringup>:<id>` one that `nros configure` just wrote, and reports it
  stale.
- `recorded_censuses` walks every `*.json` under `build/nros/census`, so the
  stale leaf census enters the input digest and the workspace stamp itself
  can never be current.

Result: every hooked configure of that image runs unseeded, and an unseeded
configure on a fresh build dir needs three passes to converge (issue 2504).
Only rows whose image had a leaf-era bare stamp were affected.

Control (measured): with the two bare stamp dirs and the leaf census moved
out, `nros locate configured` reports current for both images and a fresh
build dir configures in one pass with "entity inventory seeded" and
"message-bound sizes seeded".

A fresh clone never has these files, so CI does not see it; any checkout
that built the leaf before the migration does, and the error points at
neither file.

## Fix direction

- `stamp_key_for`: prefer the workspace `<bringup>:<id>` stamp when the
  project is a workspace, or treat a bare stamp whose recorded resolved
  dir no longer exists as absent (and say so).
- `recorded_censuses`: ignore a census whose recorded root file no longer
  exists, or only read the censuses the current project shape produces.
- Either way, report what was ignored rather than silently using it.

## Acceptance

A checkout holding a leaf-era bare stamp and leaf census for an image that
is now a workspace image configures seeded on the first pass; a test builds
that layout in a temp dir.
