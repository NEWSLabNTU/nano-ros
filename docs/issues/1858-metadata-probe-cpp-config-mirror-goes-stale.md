---
id: 1858
title: "After `nros sync`, the metadata probe's mirrored `nros_cpp_config_generated.h` is stale against the header cargo last wrote, and `check-sizes-header-mirrors` fails on any checkout that synced a C++ workspace"
status: open
type: bug
area: [build, cmake]
severity: low
found: 2026-10-13
related: [issue-1783, issue-0268, issue-0834, issue-0616]
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
</content>
</invoke>

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
