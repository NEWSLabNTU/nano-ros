---
id: 2222
title: "A shared Corrosion cargo store grows one 1 GB image-crate folder per build dir, and gc-shared-cargo cannot see inside a store"
status: open
type: bug
area: [build, cmake, disk]
found: 2026-11-03
related: [issue-0805, issue-1954, issue-1990, phase-506]
---

## Symptom

On 2026-11-02 the build host's data disk filled (`/mnt/wd`, 3.7 TB, 100 %,
about 900 MB free), and every box2 build failed with `No space left on
device`. The box2 checkout held 2.3 TB, of which `build/corrosion-cargo` was
488 GB. One store in it, `build/corrosion-cargo/metadata-probe/5eab8b54fb38`,
was **410 GB**: about 400 folders named `nros-cpp-image_<hash>`, each about
1007 MB.

`just gc-shared-cargo` reported all 31 stores in `build/corrosion-cargo` as
unreachable (488.6 GB) and `--prune` reclaimed them (disk back to 456 GB
free). But the LIVE stores left behind already hold 32 `nros-cpp-image_*`
folders and many `nros-c-image_*` folders (`native/1a805ae64402` alone lists
more than ten), so the growth continues on a live store, where the GC never
looks.

## Mechanism (partly measured)

- Measured: phase-506 W4a's image crate (`cmake/NanoRosImageCrate.cmake`,
  called from `packages/api/nros-cpp/CMakeLists.txt:65` and nros-c's twin)
  generates its `Cargo.toml` under `${CMAKE_CURRENT_BINARY_DIR}`, i.e. inside
  each cmake BUILD dir.
- Measured: Corrosion names a cargo target folder `<root>_<hash>`, the hash
  taken over the manifest path (issues 1954, 1985), and all leaves of one
  configuration share one store through their `cargo` symlink (issue 0805).
  So every distinct build dir that reaches a store adds its own folder there,
  about 1 GB for a C++ image.
- Not yet measured: why the metadata-probe store reached about 400. The
  suspicion is that the metadata probe configures in a build dir whose path
  changes per run (a temp or per-invocation dir), so each run mints a new
  hash and orphans the last folder. Confirm by listing the probe's build-dir
  paths across two runs.
- Measured: `scripts/build/gc-shared-cargo.py` decides reachability per STORE
  ("a dir is live iff some leaf's `cargo` symlink resolves to it"). A folder
  inside a live store that no live build dir's `build.ninja` names is never
  collected.

## Fix direction

The class, not the probe: a folder's liveness is a property of the build dir
that names it, so either

1. key the image crate's manifest path on something stable per CONFIGURATION
   rather than per build dir (so siblings share one folder, which issue 1954
   says is safe only for the staleness probe's reading of `.d` files, so check
   that first), or
2. teach `gc-shared-cargo` to collect folders inside a live store that no live
   build dir passes as `--target-dir` (issue 1954 already reads that from
   `build.ninja`), with the same exact-reachability rule it uses for stores.

And whichever producer mints a fresh build dir per run (the suspected metadata
probe) should reuse a stable one, or delete its folder when it exits.

## Acceptance

- Two consecutive metadata-probe runs leave the store's folder count unchanged.
- `just gc-shared-cargo` reports orphan folders inside live stores, with a
  negative control (a folder no build dir names is reported; one a live
  `build.ninja` names is not).
