---
id: 311
title: "`_cpp_features` is assembled in two places for one crate — a consumer hook must be added twice or it silently does nothing"
status: open
type: tech-debt
area: build
related: [0304, phase-308, phase-313]
---

## Finding (phase-308 W1, 2026-07-28)

The cargo feature list for `nros-cpp` is built independently in two files:

| File | Path it serves |
| --- | --- |
| `cmake/NanoRosRuntimeCrate.cmake` (`set(_cpp_features …)`) | the synthesized `nros_ws_runtime` umbrella |
| `packages/core/nros-cpp/CMakeLists.txt` (`set(_cpp_features …)`) | direct `add_subdirectory` / import |

Same variable name, same crate, no shared source.

## How it failed

The phase-308 metadata probe needs `nros-cpp`'s `metadata-mode` feature. A
`NROS_EXTRA_CPP_FEATURES` hook was added to the umbrella path only. The probe
uses the direct path, so:

* the feature never reached any cargo invocation;
* `nros_cpp_metadata_dump` — `#[cfg(feature = "metadata-mode")]` — was absent
  from `libnros_cpp.a`;
* the probe failed at LINK with `undefined reference`, ~40 min into a build.

Nothing reported "that feature went nowhere". The hook looked correct in
isolation and was verified by a unit test that asserted the generated CMake
text — which contained the `set()` faithfully. Only `nm` on the built archive
showed the truth.

Fixed by adding the hook to both paths (`0304`). That leaves the duplication.

## Why it should be collapsed

Two independent assemblies of one crate's feature set means:

* every future consumer hook must be added twice, and the failure mode for
  forgetting is silent — a feature that simply does not apply;
* the two can drift on the *base* features too, which is worse than a missing
  hook: `nros-c` and `nros-cpp` resolving different `nros` features in one build
  is exactly the layout divergence the `defines_of` guard in
  `nros-build-helpers` now catches after the fact.

This is the same shape as two other defects found the same day — two writers of
`nros_config_generated.h`, and a recording backend that existed in three places
without being reachable from any of them. One logical thing, more than one
source, nothing checking they agree.

## Options

1. **One function, two callers.** Factor the assembly into a single cmake
   function (`nros_cpp_feature_list(out …)`) that both sites call. Smallest
   change; keeps both entry points.
2. **One path.** Make the direct-import path go through the umbrella, so there
   is only one assembly. Cleaner, but the umbrella exists for workspace builds
   and forcing it on standalone consumers may not be free.
3. **Assert agreement.** Leave both, add a check that the two produce the same
   list for the same inputs. Cheapest, but keeps the duplication and only
   catches drift, not a missing hook.

(1) is the recommended fix: it removes the "add it twice" trap, which is the
part that actually bit.
