---
id: 2445
title: "A build writes into the installed release tree, and fails when that tree is read-only"
status: resolved
type: bug
area: cli, sdk-store, install
purpose: [industrial, education]
related: [phase-551]
found: 2026-11-11
resolved_in: "fix-2445 branch: `nros_build_paths::shared_build_root` never names an installed root, the lock seed writes a fresh file, and installing `nros` writes its central patch"
---

**Home: [phase-551](../../roadmap/phase-551-starter-image.md)** — found sizing the W2 starter image (release `0.6.0-nros8`).

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

## Resolution

Four writers, not three. The fourth showed up once the first three were fixed.

- **Probe caches:** `nros_build_paths::shared_build_root` is the one rule. It returns `NROS_BUILD_ROOT` when set, `<root>/build` for a checkout (a `.git` exists, file or directory), and nothing for an installed root. In that last case the caller keeps its cache with the project. The metadata probe, the Corrosion probe and the sizes probe all ask it, so `NROS_BUILD_ROOT` now moves all three.
- **Read-only copy:** the cause was confirmed. `registry_pins::write_seed` used `fs::copy`, which keeps the source's mode. A read-only pin set therefore became a read-only `Cargo.lock` in the project, on both the cargo road and the cmake road. It now reads the pin set and writes a fresh file.
- **Central patch file:** the first `nros configure` (sync) wrote `<root>/nros-patch.toml`. A release cannot ship that file, because its paths are absolute. Installing `nros` now writes it (`sdk_store::finish_installed_sdk_root`) from both install funnels: `nros setup --tool nros`, and `sdk-front nros`, which `install.sh` runs.

**Acceptance:** a release asset was built from the fix commit (`just probe platform-asset`) and installed in a container. The release tree was then made read-only with `chmod -R a-w`, and both first projects were built. The Linux Rust and the FreeRTOS C projects both build, the tree's file list, sizes, modes and mtimes are unchanged, and the tree has no `build/`.

**Negative control:** the same run against `0.6.0-nros8` fails on both roads with `failed to write .../Cargo.lock: Permission denied`.
