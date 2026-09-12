---
id: 1354
title: "`check cpp`'s runtime probes link a posix archive against a Zephyr
  `nros_cpp_config_generated.h` — one shared `target/nros-cpp-generated/` for
  two feature sets, and the loser is an undefined `config_variant` symbol"
status: open
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

## Repro

```sh
cargo build -p nros-cpp --no-default-features \
    --features "alloc,rmw-cffi,platform-zephyr,ros-humble" --quiet   # or any Zephyr-featured build
just check cpp        # dies on the undefined config_variant symbol
grep CONFIG_VARIANT target/nros-cpp-generated/nros/nros_cpp_config_generated.h
nm target/debug/libnros_cpp.a | grep -o 'nros_cpp_config_variant_[a-z_]*' | sort -u
```
