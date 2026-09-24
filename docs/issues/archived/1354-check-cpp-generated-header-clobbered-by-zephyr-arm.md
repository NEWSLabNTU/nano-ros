---
id: 1354
title: "`check cpp`'s runtime probes link a posix archive against a Zephyr
  `nros_cpp_config_generated.h` — one shared `target/nros-cpp-generated/` for
  two feature sets, and the loser is an undefined `config_variant` symbol"
status: resolved
type: bug
area: [ci, api, build]
related: [0088, 0114, 0122, 0834, 0196, phase-456]
---

## Symptom

`just check cpp` dies at its third runtime probe:

```
  - RCLCPP_* reach nros_log at the right severity; spin_once(-1) is refused (runtime)
/usr/bin/ld: /tmp/ccXXXX.o:(.data.rel.ro+0x8): undefined reference to
  `nros_cpp_config_variant_alloc_panic_platform_platform_zephyr_rmw_cffi_ros_humble'
collect2: error: ld returned 1 exit status
```

The two probes before it link the same archive and pass, so the lane reports
failure only after it has already told you twice that linking works.

## Cause, measured

The variant symbol is the config-header/archive interlock: the generated header
declares `NROS_CPP_CONFIG_VARIANT` and the archive defines the matching symbol,
so a TU compiled against one header and linked against another feature set's
archive fails at link rather than running with mismatched constants. It is doing
exactly its job here.

What it is catching is that **one directory serves two feature sets**:

```
$ nm target/debug/libnros_cpp.a | grep -o 'nros_cpp_config_variant_[a-z_]*' | sort -u
nros_cpp_config_variant_alloc_env_platform_posix_rmw_cffi_ros_humble_std

$ grep CONFIG_VARIANT target/nros-cpp-generated/nros/nros_cpp_config_generated.h
#define NROS_CPP_CONFIG_VARIANT "alloc_panic_platform_platform_zephyr_rmw_cffi_ros_humble"
```

The archive is the lane's own
`cargo build -p nros-cpp --no-default-features --features "std,rmw-cffi,platform-posix,ros-humble"`.
The header is `platform-zephyr`. Both carry the same mtime.

`target/nros-cpp-generated/` is a build-script BYPRODUCT with no feature in its
path, so whichever `nros-cpp` build ran last owns it — and the lane builds
nros-cpp more than once with different features.

## Why re-running does not fix it

This is issue 0834's shape rather than a race that settles. Once the Zephyr
build is up to date, cargo does not re-run its build script, so the byproduct is
never re-emitted with the posix variant no matter how many times the lane runs.
Measured: three consecutive `just check cpp` runs failed identically.

Forcing the emitter repairs it —

```sh
touch packages/api/nros-cpp/build.rs
cargo build -p nros-cpp --no-default-features \
    --features "std,rmw-cffi,platform-posix,ros-humble" --quiet
```

— and the next `check cpp` run puts the Zephyr variant back, which is what
identifies the clobber as being inside the lane rather than in the developer's
environment.

## Why it was not noticed

`check cpp` is `build-serial`, which no merge-gating event runs (issues 1226,
1331). And it only surfaces once BOTH feature sets have been built in one
`target/`, which is the state a developer reaches after touching Zephyr and
posix in the same session, and which a fresh CI checkout does not.

## What would fix it

The byproduct path must carry the feature set, the way the fixture artifact dirs
already carry their coordinate — `target/nros-cpp-generated/<variant>/`, with
each consumer passing the `-I` for the variant it is linking. That is the same
remedy the sizes-header mirror family (0088 / 0114 / 0122) converged on: one
directory per coordinate, never one shared directory plus ordering.

A cheaper stop-gap, if the path cannot move: have the lane re-run the posix
build immediately before the runtime probes rather than once at the top, and
accept that it is ordering-dependent — which is what the 0088 family kept
failing to make stick.

## Wider than the title, measured 2026-09-24

The title says "Zephyr arm" and the first report was about `just check cpp`.
Both are narrower than the defect. Two further collisions, on one tree, neither
involving Zephyr:

**1. `census-hooks-complete` vs `check cpp`, on `target/nros-c-generated/`.**

```
nros-cpp: target/nros-c-generated/nros/nros_config_generated.h was written by
another crate with DIFFERENT probed sizes.
  on disk: build/sizes-probe/.../fcfdf80b2360878f/.../libnros.rlib
  current: build/sizes-probe/.../3d6a28c75a088e0a/.../libnros.rlib
Disagreeing defines:
  EXECUTOR_OPAQUE_U64S:       on-disk=11301 vs would-write=11306
  NROS_EXECUTOR_SIZE:         on-disk=90408 vs would-write=90448
  NROS_EXECUTOR_VALUE_SIZE:   on-disk=1856  vs would-write=1896
```

The two probe directories differ because the two lanes build `nros` with
different features, and `nros-sizes-build` keys its probe directory by
`(rustc, target, features)` precisely so they can. **The 40-byte difference is
CORRECT** — a different feature set genuinely has a different executor layout.
The defect is that both then write one path.

**2. It is a RACE under `ci gate`, not only an ordering problem.**

Measured in one sitting: `just check cpp` alone exits 0; `just ci gate`
immediately afterwards fails that same lane with

```
undefined reference to `nros_cpp_config_variant_alloc_default_env_panic_platform_rmw_cffi_rmw_zenoh_cffi_ros_humble_std'
```

`ci gate` runs the build tier at `-P48`, so a concurrent gate rebuilds
`nros-cpp` with its own features and replaces the header between the compile and
the link. The stop-gap this issue proposed — "have the lane re-run the posix
build immediately before the runtime probes" — **cannot fix that**: there is no
ordering between parallel gates to fix. Only the per-variant path does.

## Consequence worth stating

On a machine where more than one feature set has been built, `ci gate`'s
`check::build` cannot give a verdict on the C++ lane: a red there may be this,
and a green may be luck about which gate wrote last. That is not a hypothetical
— it is why phase-456 W5 could be verified by `check fast` (352 gates),
`api-parity`, `cpp-fmt`, the compile-probe sweep and `check cpp` standalone, but
not by `ci gate`.

## Repro

```sh
cargo build -p nros-cpp --no-default-features \
    --features "alloc,rmw-cffi,platform-zephyr,ros-humble" --quiet   # or any Zephyr-featured build
just check cpp        # dies on the undefined config_variant symbol
grep CONFIG_VARIANT target/nros-cpp-generated/nros/nros_cpp_config_generated.h
nm target/debug/libnros_cpp.a | grep -o 'nros_cpp_config_variant_[a-z_]*' | sort -u
```

## Fix — 2026-09-25

The remedy this issue proposed was a per-VARIANT byproduct path, and it named
the cost: every consumer's `-I` would have to carry the variant, across ~165
sites in cmake, west, px4 and the check lanes. That was never done, which is why
this sat open.

What shipped instead reaches the same invariant from the other side: **a build
that probes its own feature set gets its own `CARGO_TARGET_DIR`**, so no two
feature sets share a byproduct path and the flat layout stays. The colliding
builds were few and are all in the check lanes:

| build | feature set | dir |
| --- | --- | --- |
| `check c` | `C_API_SHIPPED_FEATURES` | `target-check-c` |
| `check cpp` | `C_API_SHIPPED_FEATURES` | `target-check-cpp` |
| `check cpp`'s zenoh clippy | `std,rmw-zenoh-cffi,platform-posix,ros-humble` | `target-check-cpp-clippy-zenoh` |
| `check cpp`'s embedded cyclone | `<cyclone>,panic-platform` | `target-check-cpp-cyclone-embedded` |
| `census-hooks-complete` | `std,rmw-cffi,metadata-mode,param-services` | `target-check-census-hooks` |

`nros_scoped_target_dir` already existed for exactly this and its doc warns it is
only for dirs with no consumer reading a fixed relative path. That is honoured:
all 118 `-I` paths, both `libnros_*.a` link arguments and the lane's scratch
directory are derived from the same variable, so no fixed path escapes the
recipe.

## What the fix taught that the diagnosis had wrong

The first repair scoped the lanes and `check cpp` still failed — on an archive,
not a header: the lane linked `target/debug/libnros_cpp.a` from the shared tree
while its own build had moved. The `-I` paths were only half the coupling.

Then it failed a third time, and this is the part worth keeping: **the lane
overwrites its own input across runs.** `check cpp` builds `nros-cpp` twice with
different feature sets — the shipped set at the top, `rmw-zenoh-cffi` for the
clippy at the end — so the clippy left a zenoh-variant header in the lane's own
directory, and the NEXT run compiled a runtime probe against it while linking
the shipped-features archive:

```
header   NROS_CPP_CONFIG_VARIANT "…_rmw_cffi_rmw_zenoh_cffi_ros_humble_std"
archive  nros_cpp_config_variant_…_rmw_cffi_ros_humble_std
```

Green on a fresh directory, red on the second run. That is why it read as a race
between gates: the symptom was timing-shaped, and one of its causes was not.
"`just check cpp` alone exits 0" was true and misleading — it was true of the
FIRST run.

## Verification

* `just check cpp` green, and green again on an immediate second run — the case
  that failed before.
* `just check c` green.
* `just ci gate` green, with zero occurrences of either symptom (the
  config-variant undefined reference and the "DIFFERENT probed sizes" panic).

## What is NOT fixed

Builds outside the check lanes still share the flat path: cmake, west, px4 and
the fixture builders. They do not currently run concurrently with each other or
with the lanes, so they do not collide today — but the invariant is a property
of who happens to run when, not of the layout. The per-variant path remains the
structural answer, and this issue stays the record of why.
