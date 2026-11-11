---
id: 2445
title: "A build writes into the installed release tree, and fails when that tree is read-only"
status: open
type: bug
area: cli, sdk-store, install
purpose: [industrial, education]
related: [phase-551]
found: 2026-11-11
---

**Home: [phase-551](../roadmap/phase-551-starter-image.md)** — found sizing the W2 starter image (release `0.6.0-nros8`).

## Measured

1. **Builds leave about 4.3 GB inside the installed release tree.** After one Linux Rust and one FreeRTOS C first-project build, `<store>/nros/<ver>/share/nano-ros/build/` held `corrosion-cargo` (1.7 GB), `metadata-probe` (1.5 GB) and `sizes-probe` (1.1 GB). That is rung (2) of `metadata_build::probe_target_dir`, used because the install happened to be writable.
2. **`NROS_BUILD_ROOT` moves two of the three, not all of them.** With `NROS_BUILD_ROOT=/tmp/nb`, the metadata and Corrosion dirs went to `/tmp/nb` (3.1 GB), but `share/nano-ros/build/sizes-probe` was still written. `nros-sizes-build/src/lib.rs` hand-joins `try_repo_root()/build/sizes-probe` and never asks `nros_build_paths::build_root`.
3. **A read-only install fails the C road.** With the release tree made read-only (`chmod -R a-w`, the case rung (3) exists for), the Linux Rust project still builds. The FreeRTOS C project fails with `failed to write /tmp/w/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-c/nros-c-image/Cargo.lock: Permission denied`. The file is inside the project, so a copy taken from the install seems to keep the install's read-only mode. That cause is not yet confirmed.

## Why it matters

A system-wide or container install is read-only, or shared by several users. A starter image either carries 4.3 GB of one user's caches or has to delete them by hand, which is what phase-551 W2 does now.

## Next

- One rule for "where does build output go": `build_root`, then a per-project dir, never the install.
- Make the sizes probe ask that rule.
- Have whatever copies a tree out of the install restore write permission on the copy.
- Acceptance: both first projects build against a read-only install, and the install is byte-identical afterwards.
