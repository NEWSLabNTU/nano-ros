---
id: 1665
title: "Nightly tier 2: Zephyr workspace Rust entries fail with `can't find crate for core` (byteorder), 10 of 32 images"
status: open
type: bug
area: ci, zephyr, build
severity: medium
found: 2026-10-03
related: [1288, 1353, 1603]
---

## What happens

The tier-2 nightly's Zephyr fixture build failed on main at `649d97f8d`.

- Run **37099034948** (`nightly`, schedule 05:11Z), job **111134753717**
  (`tier 2 nightly (pairwise cover)`), step `build-test-fixtures-leaves`.
- 22 of 32 Zephyr images built. The other 10 did not.

The first-error excerpt the lane prints from its `zephyr.log` names one leaf,
four times:

```
error[E0463]: can't find crate for `core`
error: could not compile `byteorder` (lib) due to 1 previous error
FATAL ERROR: command exited with status 101: /usr/bin/cmake --build .../build/zephyr-workspace-builds/3.7/build-ws-rs-lifecycle-entry-zenoh
```

The log tail also shows `zephyr-fixture-27-build-ws-rs-safety-entry-zenoh` failing
with `Error 101`. Its error text was not in the excerpt, and the run uploads no
`zephyr.log` artifact (only `zephyr-gate-inputs`).

## What it is NOT

- **Not the previous night's failure.** On 2026-10-02 (run 36967923081, job
  110715597919) the same lane failed on `.../build/zephyr-zenoh/zephyr_entry does
  not exist` (issue 1288) and on `the #[global_allocator] in zephyr conflicts
  with global allocator in: nros_platform`. This night is a different error in a
  different leaf.
- **Not disk** (issue 1353). The step failed on a compile error and reached its
  tier-priority judgment afterwards.

## Not yet known

- Which commit introduced it. `E0463 ... core` means `byteorder` was compiled for
  a target whose `core` is unavailable: an embedded triple without `build-std`,
  or an uninstalled rustup target. The candidate is a merge between the two
  nights that changed how Zephyr Rust leaves resolve dependencies. That
  includes `63d649686` (#1621, the Zephyr Rust leaves split into a
  host-buildable node package). That commit touched `examples/zephyr/rust/*`,
  not `examples/workspaces/*`, so the link is unproven.
- The error text for the other 9 failed images.

## What would close it

- A local `west` build of `build-ws-rs-lifecycle-entry-zenoh` at `649d97f8d` that
  reproduces the error, then a bisect over the 10-02..10-03 merges.
- A tier-2 nightly whose Zephyr fixture build reports 32 of 32.
- Uploading `tmp/build-test-fixtures-*/zephyr.log` as an artifact on failure
  would have named all 10 failures here. That is a cheap separate improvement.
