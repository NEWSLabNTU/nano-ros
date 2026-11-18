---
id: 2503
title: "After a leaf becomes a workspace, its leftover leaf-era configure stamp and census leave every hooked configure unseeded"
status: resolved
resolved_in: "the change that archives this issue"
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

## Fix

Both readers now ask the same question issue 2480 asked of the
`.leaf-selection` marker: is this project still a leaf
(`configure::is_leaf`)? Only a leaf writes a stamp under a bare image id
or a `leaf.census.json`.

- `configure_stamp::stamp_key_for`: when the project is not a leaf, a
  `<bringup>:<id>` stamp beats a bare-id stamp, and the bare one is named on
  stderr as ignored. With no workspace stamp yet, the bare one still answers,
  and it reads stale (its inputs are the leaf's), so a hooked configure does
  not seed from it. That is the behaviour a project with no workspace stamp
  had before. The predicate is `is_leaf_era_key`.
- `configure_stamp::recorded_censuses`: in a project that is not a leaf,
  `build/nros/census/leaf.census.json` is not a census of this project, so
  it is neither a digest input nor a recorded census digest
  (`is_leaf_era_census`).

Tests, each red with its guard disabled:

- `a_migrated_leafs_bare_stamp_is_not_the_images_stamp`: the measured
  layout, a leaf stamp under the bare id and then a workspace stamp.
  `nros locate configured` answers from the workspace stamp and reports it
  current.
- `a_migrated_leafs_census_is_not_read`: the leaf census is no input, and
  editing it leaves the workspace stamp current.

The existing `the_raw_recorder_document_is_not_a_census` fixture now builds
a leaf, since `leaf.census.json` is only a census there.

## Not changed

Nothing deletes the leaf-era files. They are gitignored build output that
no reader now trusts, and deleting a person's build state is not the
reader's job. `ws`'s census freshness report still lists the leaf census.
That report is for a person, and a stale census they may believe is
exactly what it exists to show.

Sweep: `git grep -n 'stamp_key_for\|recorded_censuses\|leaf_census_path\|leaf_selection_file' -- packages/cli/nros-cli-core/src`
