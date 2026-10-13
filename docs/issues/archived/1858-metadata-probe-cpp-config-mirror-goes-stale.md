---
id: 1858
title: "After `nros sync`, the metadata probe's mirrored `nros_cpp_config_generated.h` is stale against the header cargo last wrote, and `check-sizes-header-mirrors` fails on any checkout that synced a C++ workspace"
status: resolved
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-13
resolved: 2026-10-13
related: [issue-1783, issue-0268, issue-0834, issue-0616, issue-0805, issue-1700]
---

## Measured

Found while measuring phase-486 W8c. Every example workspace was synced
(`nros sync`) in two checkouts: `origin/main` (`87991980f6`) and the W8c pin
bump. Then `just ci gate`'s fast line failed in the W8c checkout on
`check-sizes-header-mirrors`, and the SAME three failures reproduce on the
`origin/main` checkout. The bump plays no part:

```
STALE MIRROR: examples/workspaces/{cpp,features,derived-tiers-cpp}/build/nros-metadata/
  metadata-probe-cmake/build/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h
       vs: .../metadata-probe-cmake/build/cargo/<checkout>/nros-cpp-generated/nros/nros_cpp_config_generated.h
```

The drift is the VARIANT stamp, not a storage size:

```
< #define NROS_CPP_CONFIG_VARIANT "alloc_env_metadata_mode_platform_posix_profile_mode_..."
> #define NROS_CPP_CONFIG_VARIANT "alloc_env_metadata_mode_panic_platform_platform_posix_profile_mode_..."
```

and the matching `nros_cpp_config_variant_*` extern. The mirror was written at
06:13:15 and cargo rewrote its header at 06:16:16, three minutes later, with
`panic-platform` in the feature set. No feature list in the probe's
`build.make` names `panic-platform`, so it arrives transitively in a second
cargo run of the same probe build.

The probe build uses the Makefile generator. Its mirror rule
(`nros_cpp_config_header.dir/build.make`) depends on
`CMakeFiles/nros_cpp_config_header.rerun`, a stamp, and not on the
cargo-generated header. Nothing re-runs it after the second cargo write.

## Why it matters, and why it is low

- **Gate noise.** Any checkout that has synced a C++ workspace fails the
  fast line LOCALLY until `python3 scripts/check-sizes-header-mirrors.py
  --fix` re-runs the mirror. CI's fast lane runs on a fresh tree with no probe
  build dirs, so it never sees this. That makes it local-only noise, which is
  how a contributor learns to ignore this gate.
- **Runtime.** None measured. The sizes in the two headers agree; only the
  variant symbol differs. The probe only reads metadata, so a TU compiled
  against the stale mirror would show up as an undefined variant symbol at
  link, not as a silent `_opaque` overflow.

## Open

- Which second cargo invocation in the probe build enables `panic-platform`,
  and whether the two runs should agree on features at all (two feature sets
  on one shared `cargo/<checkout>` dir is issue 0616's shape).
- The edge: the mirror should depend on the header it copies, as issue 1783
  made it for the normal C/C++ build. Check whether 1783's fix reaches the
  Makefile generator or only ninja.

### 2026-10-13: it now stops the scheduled tier-1 lane

The host-tests schedule run
[38341537002](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38341537002)
(main at `e79e8cb88`), job "tier-1 gates (just check)", on a hosted runner in
a FRESH checkout, failed its fast line on exactly this gate:

```
===== FAIL (sizes-header-mirrors, rc=1, 3172ms) =====
STALE MIRROR: examples/workspaces/features/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h
STALE MIRROR: examples/templates/multi-node-workspace-cpp/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h
2 stale sizes-header mirror(s)
```

The four scheduled host-tests runs before it (2026-10-09 to 10-12) passed. So
the stale mirror no longer needs a long-lived checkout; one `nros sync` inside
the lane is enough, and severity `low` understates it now. The workspace set
differs from the measurement above: `features` and the
`multi-node-workspace-cpp` template, not `cpp` or `derived-tiers-cpp`.

### 2026-10-13 05:xx: it stops tier 2 too, before any cell

Both tier-2 nightly runs on main at `d134dc83f` stopped at the head of the lane,
in `check fast`, on this gate. Neither reached the build or a cell:

- nightly dispatch 38346448295, job 115128892923 "tier 2 nightly (pairwise cover)"
- nightly schedule 38350414982, job 115141298601, same job

```
check-fast (parallel): 1 of 441 gate(s) FAILED
error: recipe `sizes-header-mirrors` failed on line 754 with exit code 1
STALE MIRROR: examples/workspaces/features/build/nros-metadata/…
STALE MIRROR: examples/templates/multi-node-workspace-cpp/build/nros-metadata/…
```

The workspaces are the same two as tier 1's. While this stands, tier 2 has no
signal at all, which is the issue-1158 shape (a lane that never reaches its
cells).

## Resolution (2026-10-13)

**Root cause: the shared cargo dir's key missed a feature.** The probe's
`cargo/` is a symlink into `build/corrosion-cargo/metadata-probe/<key>`,
shared by every probe build with an equal key (issue 0805). `nros-c` computes
the key from its features at import time. `nros_apply_panic_policy` adds
`panic-platform` later, with `corrosion_set_features()`, so it was not in the
key. Two workspaces equal on the key but differing in their ending (`features`
with `panic-platform`, `managed` without; likewise `mixed` / `cpp`) therefore
took turns rewriting one `nros-cpp-generated/nros_cpp_config_generated.h`.
Each probe's mirror was right when it was copied and went stale on the next
sync of the other workspace. That explains both observations: the "second
cargo run" was another workspace's probe, and a CI fresh checkout reaches the
failure because `just generate-bindings` syncs every workspace in one pass.
The header was not the only shared output: the archive in the same dir was
shared too, so a later incremental probe build could link the other
workspace's archive.

Reproduced on `origin/main`: syncing `features`, then `managed`, made
`features`' mirror STALE. Syncing `mixed`, then `cpp`, did the same to
`mixed`'s.

**Fix.** `nros_apply_panic_policy` records the feature it applies in the
GLOBAL property `NROS_SHARED_CARGO_LATE_FEATURES`. The end-of-configure re-key
(`_nros_entity_facts_rekey_shared_cargo_dir`, issue 1700) appends a
`late_features=` field, and it now re-keys when that field is non-empty even
if there are no entity facts. An image with no late feature keeps its key.
Measured after the fix, rebuilding the four probes in the failing order:

- `features` and `managed` key to `64bbb5df69c4` and `38265e322fb9`;
- `mixed` and `cpp` key to `ddb2119461cd` and `d1088a68552b`;
- `check-sizes-header-mirrors` reports OK over 15 pairs across 5 build dirs.

The sweep for other late cargo inputs on the runtime targets was
`git grep -nE 'corrosion_(set_env_vars|add_target_rustflags|set_cargo_flags|set_features)\(' -- cmake packages/api zephyr`.
Only the features call was outside the key; profile, board facts, image env,
knobs and entity facts are already key fields. The mirror rule's stamp-only
edge (the "Open" item above) was not the cause, and was left as it is.
