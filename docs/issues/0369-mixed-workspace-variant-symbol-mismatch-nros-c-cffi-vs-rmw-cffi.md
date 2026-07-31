---
id: 369
title: "Mixed C+C++ workspace fixture link-fails: the C msg archive references a `rmw_cffi_rmw_zenoh` config-variant symbol that nros-c (built with `cffi-zenoh-cffi`) never emits"
status: open
type: bug
severity: medium
area: build
related: [issue-0360, issue-0268, phase-321]
---

## Finding (2026-08-01, building native workspace fixtures for ci-matrix)

`just native build-workspace-fixtures` link-fails on the **mixed** workspace:

```
/usr/bin/ld: src/c_talker_pkg/libstd_msgs__nano_ros_c.a(std_msgs_msg_int32.c.o):
  (.data.rel.ro+0x0): undefined reference to
  `nros_config_variant_alloc_cffi_zenoh_cffi_platform_posix_rmw_cffi_rmw_zenoh_ros_humble_std'
collect2: error: ld returned 1 exit status
```

## Root cause

The generated C message archive (`libstd_msgs__nano_ros_c.a`) references a
config-variant alloc symbol whose feature suffix includes **`rmw_cffi_rmw_zenoh`**,
but the `nros-c` staticlib linked into the same mixed workspace is built with a
DIFFERENT feature set and so emits a variant symbol WITHOUT that fragment.

From the build log, the mixed workspace's two halves compile with divergent
features:

- `nros-c`   → `--features=ros-humble,cffi-zenoh-cffi,std,platform-posix`
- `nros-cpp` → `--features=ros-humble,rmw-zenoh-cffi,std,platform-posix`

and the feature graphs differ in the RMW spelling:

- `nros-c`:  `cffi-zenoh-cffi = ["rmw-zenoh"]`  (no `rmw-cffi`)
- `nros-cpp`: `rmw-zenoh-cffi = ["rmw-cffi", "nros-c/rmw-zenoh", "dep:nros-rmw-zenoh"]`

So the C-side msg codegen stamps its variant reference from the WORKSPACE feature
set (which carries `rmw-cffi` via the C++ half), producing
`…_cffi_zenoh_cffi_…_rmw_cffi_rmw_zenoh_…`, while the `nros-c` staticlib actually
built for that workspace carries only `cffi-zenoh-cffi` (→ `rmw-zenoh`, NOT
`rmw-cffi`) and emits `…_cffi_zenoh_cffi_…` without the `rmw_cffi_rmw_zenoh`
fragment. The reference is unresolved at link.

This is the same variant-consistency class that issue 0360 ("stamp the feature
variant into the generated headers and archives") addresses, but 0360 does not
reconcile the MIXED workspace, where the C and C++ halves legitimately select
different rmw-feature spellings (`cffi-zenoh-cffi` vs `rmw-zenoh-cffi`) yet must
agree on ONE variant symbol for the shared C msg archive to link.

## Fix direction

The C msg codegen's variant string and the `nros-c` staticlib's emitted variant
must be computed from the SAME feature set for a given workspace build. Either:
1. Build `nros-c` in the mixed workspace with the same effective RMW features the
   C++ half pulls in (so it emits the `rmw_cffi_rmw_zenoh` variant the C archive
   references), or
2. Compute the C-side variant reference from `nros-c`'s OWN features (what it was
   actually built with here), not the union that includes the C++ half's
   `rmw-cffi`.

(2) is the safer contract — the reference should match what the linked `nros-c`
provides, not what the workspace as a whole enables. The right home is the same
codegen/stamp seam issue 0360 touched.

## Scope

Surfaced on the `mixed` workspace (fixtures `workspace-mixed-*`). Pure-C and
pure-C++ workspaces do not trip it because their two halves share one RMW feature
spelling. It blocks `just native build-workspace-fixtures` (the mixed rows), hence
the `native,mixed,*` tier-2 coordinate.

## Repro

```
source ./activate.sh && source /opt/ros/humble/setup.bash
just native build-workspace-fixtures
# … undefined reference to nros_config_variant_alloc_..._rmw_cffi_rmw_zenoh_ros_humble_std
```
