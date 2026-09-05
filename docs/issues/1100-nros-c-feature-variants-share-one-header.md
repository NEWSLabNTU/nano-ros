---
id: 1100
title: "Two feature variants of `nros-c` in one build dir write the same
  generated header, and the writer guarantees a second build script run"
status: open
type: bug
area: build
related: [issue-0528, issue-0475, rfc-0044]
---

## Symptom

A Zephyr image builds and links. Any SECOND build in the same directory —
including the one `west build --target run` performs before starting a model —
stops with:

```
nros-cpp: .../nros-rust/nros-c-generated/nros/nros_config_generated.h was
written by another crate with DIFFERENT probed sizes. The C and C++ halves of
this build resolved different runtime layouts, so one of them would size its
`_opaque` storage wrong (silent overflow at runtime).
  on disk: .../sizes-probe/<rustc>/<key>/aarch64-unknown-none/nros-relwithdebinfo/libnros.rlib 1788600981985110285 3356020
  current: .../sizes-probe/<rustc>/<key>/aarch64-unknown-none/nros-relwithdebinfo/libnros.rlib 1788600981985110285 3356020
Disagreeing defines:
  EXECUTOR_OPAQUE_U64S:         on-disk=59636 vs would-write=59667
  NROS_EXECUTOR_MAIN_STACK_MIN: on-disk=3104  vs would-write=3216
  NROS_EXECUTOR_SIZE:           on-disk=477088 vs would-write=477336
  NROS_EXECUTOR_STORAGE_SIZE:   on-disk=477088 vs would-write=477336
  NROS_EXECUTOR_VALUE_SIZE:     on-disk=1552  vs would-write=1608
  SUBSCRIPTION_OPAQUE_U64S:     on-disk=2121  vs would-write=2128
```

Note the two stamps: **same rlib, same mtime, same size**. Which is exactly the
case `write_header_if_absent_or_verify` documents as *"both halves read the same
artifact and still resolved different layouts. That is the real
divergent-features case, and it must stop the build"*. The guard is correct.
This issue is about what it is catching.

Reproduced on Autoware Safety Island's `zephyr-fvp` lane, on a clean build dir:
`./build.sh --platform zephyr-fvp -d <dir>` succeeds, then
`./build.sh --platform zephyr-fvp -d <dir> --run` fails as above. Same on a
GitHub runner, same numbers.

## Cause

Two `nros-c` compilations coexist in one build directory with different feature
sets. Their own generated headers say so:

```
$ diff <(grep '^#define' build/<d>/nros-rust/.../build/nros-c-8c9aa73f430f95e6/out/nros-c-generated/nros/nros_config_generated.h) \
       <(grep '^#define' build/<d>/nros-rust/.../build/nros-c-0408b612d50fd1f9/out/nros-c-generated/nros/nros_config_generated.h)

< #define NROS_CONFIG_VARIANT "alloc_critical_section_global_allocator_panic_platform_param_services_platform_zephyr_rmw_cffi_ros_humble"
< #define NROS_EXECUTOR_SIZE 477336
---
> #define NROS_CONFIG_VARIANT "critical_section_global_allocator_panic_platform_platform_zephyr_rmw_cffi_ros_humble"
> #define NROS_EXECUTOR_SIZE 477088
```

The variants differ by `alloc` and `param_services`. Both write to the SAME
shared path, `<build>/nros-rust/nros-c-generated/nros/nros_config_generated.h`,
so whichever build script runs second finds the other's numbers and panics.

`NROS_CONFIG_VARIANT` is already in the header. The path it is written to does
not use it.

## Why a second build always reaches it

`write_header_if_absent_or_verify` declares the header it writes as one of its
own inputs:

```rust
println!("cargo:rerun-if-changed={}", dest.display());
println!("cargo:rerun-if-changed={}", dest.with_extension("h.stamp").display());
```

The comment above it explains why (a byproduct nothing declares is a byproduct
cargo cannot notice, and the self-heal was unreachable without this). The side
effect is that the script writes a file it has declared as an input, so the
crate is never up to date afterwards: every subsequent cargo invocation re-runs
both build scripts, and with two variants present, one of them loses.

So a workspace with these two variants builds exactly once. The second build —
a rebuild, an IDE, `--target run`, anything — fails, and keeps failing.

## What this cost downstream

ASI's FVP lane starts its model with `west build --target run`, which re-enters
the build graph. Eight CI rounds went into environment differences between the
build and that reconfigure (the sizing knobs, `AMENT_PREFIX_PATH`, the loader
path for `idlc`) before the stamps showed both halves reading one identical
rlib, which ruled the environment out. The lane now runs the model straight
from the FVP command line cmake records in `build.ninja`, never re-entering
cargo — a workaround for this, not a fix.

## Suggested fix

Either would do; the first looks closer to the existing design:

1. **Key the header path by variant.** `NROS_CONFIG_VARIANT` is already
   computed and already in the file. Writing to
   `nros-c-generated/<variant>/nros/nros_config_generated.h` (with the consumer
   including the variant it built against) makes the collision structurally
   impossible, and two variants in one build dir stop being an error at all.

2. **Make the two variants one.** If `alloc` + `param-services` on one nros-c
   consumer and not the other is unintended, unify the features and the shared
   path stays sound. This needs someone who knows which consumer is which —
   from outside it is not obvious that the split is deliberate.

Worth deciding separately whether a build that has just succeeded should be
immediately dirty again. Even with one variant, the `rerun-if-changed` on a
written byproduct means every second build re-runs these scripts; that is
cheap when it agrees, but it is what turns this defect from latent into fatal.
