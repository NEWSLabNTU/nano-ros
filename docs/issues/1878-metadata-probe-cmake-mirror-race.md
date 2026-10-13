---
id: 1878
title: "The cmake metadata probe builds nros-cpp twice with different features into one cargo dir, so its sizes-header mirror is stale after every fresh build"
status: open
type: bug
area: [build, cmake, tooling]
severity: medium
found: 2026-10-13
related: [0268, 1783, 1832, 0616, phase-466]
---

## Measured

`just build tier1` in a fresh worktree stopped at `check fast`, on
`sizes-header-mirrors`. It happened twice, in two fresh worktrees, with and
without phase-498's change. Both times the same two build dirs failed:

```
STALE MIRROR: examples/workspaces/features/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h
         vs: examples/workspaces/features/build/nros-metadata/metadata-probe-cmake/build/cargo/<ws>_<hash>/nros-cpp-generated/nros/nros_cpp_config_generated.h
STALE MIRROR: examples/templates/multi-node-workspace-cpp/build/nros-metadata/metadata-probe-cmake/build/...   (same pair)
```

The two headers differ in ONE thing, the config variant, by the
`panic_platform` feature:

```
< #define NROS_CPP_CONFIG_VARIANT "alloc_env_lifecycle_services_metadata_mode_panic_platform_param_services_..."
> #define NROS_CPP_CONFIG_VARIANT "alloc_env_lifecycle_services_metadata_mode_param_services_..."
```

The mirror was written at 23:42:51 and the cargo-side header at 23:43:50. So
inside one metadata-probe configure, `nros-cpp` is built twice: once with
`panic-platform` and once without, into the same cargo target dir. The
mirror's copy step ran after the first build, and the second build
overwrote the header it copies from. Every C++ TU compiled after that point
reads a variant the linked archive is not.

## Why it surfaces now

The probe dir is a Makefile build. `check-sizes-header-mirrors` read only
ninja build dirs until #2032 (issue 1832) taught it the Makefile generator,
so this pair was never compared before. That gate is on the `check fast`
line, which `just build tier1` runs first. So **a fresh tier-1 build, which
is what the self-hosted runner does, now stops here**, before any fixture is
built. That is phase-466's subject: the nightly reaching its cells.

## Not yet known

- Which two targets in the probe build request the two feature sets, and
  whether they should agree (one `nros-cpp` per configure) or need separate
  cargo dirs (the issue 0616 rule: one target dir per feature set).
- Whether the variant mismatch matters at run time for the probe, which
  only prints metadata. The gate is right either way: the class has broken
  real images before (0268).

## Workaround

`python3 scripts/check-sizes-header-mirrors.py --fix` re-mirrors the pair,
and the next `check fast` passes. That is a local repair, not a fix.
