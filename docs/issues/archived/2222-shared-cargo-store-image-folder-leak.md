---
id: 2222
title: "A shared Corrosion cargo store grows one 1 GB image-crate folder per build dir, and gc-shared-cargo cannot see inside a store"
status: resolved
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

## Resolution

Fixed in the GC, the half that holds for every producer. The suspected
producer was measured and is not one.

**The metadata probe's build dir is stable (measured, 2026-11-03).** The C/C++
probe configures in `<ws>/build/nros-metadata/metadata-probe-cmake/build`
(`probe_dir_for_workspace`, `run_probes`), a function of the workspace root
only. Two consecutive probe runs of `examples/workspaces/levels-cpp` (each
forced by deleting one generated sidecar), once with
`NROS_METADATA_PROBE_CACHE=0` (build dir discarded after each run) and once
with the default (kept): the store
`build/corrosion-cargo/metadata-probe/b8f6085d1055` held 2 folders before and
after every run, the same two (`nros-c-image_48511`, `nros-cpp-image_393ad`,
dated 2026-10-24), and their dep-info names the stable probe build dir. A
discarded build dir is recreated at the same path, so it hashes to the same
folder and reuses it warm. So the probe mints no fresh build dir per run, and
nothing in this checkout's live stores today was made by one: the 400 folders
of the first sighting belonged to build dirs that were already gone, so their
producer can no longer be attributed. What the code says is that a folder in
that store comes from a probe of some workspace root with that key, so about
400 distinct workspace roots (copies or scratch workspaces) were probed against
this checkout. To make the next sighting attributable, the report now prints,
per orphan folder, the build dir it was compiled for, read back from the image
crate's dep-info (`built for: <build dir>`).

**The GC sees inside a live store.** `scripts/build/gc-shared-cargo.py` keeps
the store rule and adds the folder rule, with the same exact reachability one
level down: a Corrosion folder (`<root>_<5 hex>`) in a live store is live iff
the generated build files of a build dir whose `cargo` link resolves to that
store pass `<build>/cargo/<folder>`. `build.ninja` for Ninja, every
`CMakeFiles/*.dir/build.make` for Unix Makefiles: the metadata probe configures
with the Makefiles generator, so a ninja-only reader could not decide exactly
the store that leaked (the first real run reported it undetermined, which is
how this was found). A store is folder-collected only when every linking build
dir has a readable generated build file, and never when a build file names the
store itself (the NuttX road, whose contents are cargo's own layout). Anything
undecidable is reported as a NOTE and left alone. Deleting a folder leaves the
store directory and every link into it in place, so issue 1990's `File exists`
under a live link cannot arise from it. The walk also covers `tests/` and
`tmp/` now, where scratch build dirs live. Reporting stays the default.

**Self-test, mutation-checked.** `python3 scripts/build/gc-shared-cargo.py
--self-test`, gated as `just check gc-shared-cargo-selftest` (fast line). On a
synthetic tree under `tmp/`: an orphan folder in a live store is reported and
pruned (Ninja and Makefiles cases); a folder a live build file names, a store
reached only through a link whose build dir has no generated build file, a
store a build file names directly, and a non-Corrosion entry are left alone; an
unreachable store is still pruned; report mode deletes nothing. Six mutations,
each red: ignore the named set, drop the undetermined guard, drop the baked
guard, never report folders, delete in report mode, read `build.ninja` only.

**On box2 (2026-11-03).** `just gc-shared-cargo`: 0 orphan folders in live
stores (0 GB), so nothing was pruned. The 30 `nros-*-image_*` folders of
`native/1a805ae64402` (and the rest of the 74 folders across the live stores)
are each named by a live build dir: one per `(leaf, build-<rmw>)` dir under
`examples/linux/c/*` and `packages/testing/nros-tests/bins/*`. The earlier
`--prune` of whole unreachable stores (488.6 GB) is what reclaimed the 410 GB
store. The report does list one empty unreachable store,
`metadata-probe/09c33450a15c` (0 bytes), whose key is the probe's key without
`late_features=panic-platform`; it was left in place.

**Not keyed per configuration.** Option 1 (one image crate manifest per
configuration rather than per build dir, so siblings share a folder) was not
taken, because nothing establishes that it is safe. The image crate's manifest
and `src/lib.rs` are a function of the flavor, the RMW and its backend
descriptor, the API features, the edition and whether the crate is a member of
a generated workspace root (`WORKSPACE_DIR`) or its own root with its own
seeded lock (`cmake/NanoRosImageCrate.cmake`), and the store key is derived for
the API crate's cargo inputs, not for that list. Equal keys therefore do not
imply identical image crates. Where two differ, sharing one folder means cargo
uplifts both to the same unhashed `libnros_c.a` / `libnros_cpp.a` and a leaf
links the other's archive: the 0500/0616 failure mode that issue 0805's key
exists to rule out. Even where they agree, one shared manifest location is
written by every sibling's configure, which is the input set issue 1954's
staleness probe had to be scoped away from. Making it safe would mean deriving
the folder key from the image crate's own inputs and proving the key covers
them; the per-build-dir folder costs disk, which the GC now bounds.
