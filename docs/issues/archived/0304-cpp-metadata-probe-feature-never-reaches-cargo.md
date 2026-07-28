---
id: 304
title: "C++ metadata probe fails to link — `NROS_EXTRA_CPP_FEATURES=metadata-mode` never reaches the cargo feature set"
status: resolved
type: bug
severity: high
area: cmake, build
related: [0286, 0305]
resolved_in: "issue-0304 (hook added to the second nros-cpp build path)"
---

## Symptom

`nros sync` (and therefore `nros ws sync`) fails for any C++ workspace whose
components are metadata-probed:

```
probe_main.cpp:(.text+0x15b): undefined reference to `nros_cpp_metadata_dump'
collect2: error: ld returned 1 exit status
gmake[2]: *** [CMakeFiles/nros_metadata_probe.dir/build.make:120: nros_metadata_probe] Error 1
```

Reproduced 2026-07-28 on `examples/workspaces/cpp` (three components:
`cpp_add_client_pkg`, `cpp_fib_server_pkg`, …), on a **clean** probe build dir
(`rm -rf build/nros-metadata` first), so it is not stale build state.

`nros sync` still exits 0 and the launch models resolve — the probe failure is
reported but does not fail the command, so it is easy to miss.

## Diagnosis

`nros_cpp_metadata_dump` is `#[cfg(feature = "metadata-mode")]`
(`nros-cpp/src/metadata_hooks.rs`), inside a module gated on `rmw-cffi`
(`nros-cpp/src/lib.rs:152`). **Both** features must be on for the symbol to
exist.

The generated probe CMakeLists does the right thing — the variable is set
before nano-ros is pulled in:

```cmake
14: set(NROS_EXTRA_CPP_FEATURES "metadata-mode")
16: find_package(nano_ros REQUIRED)
20: add_subdirectory(<component pkg> component_pkg)
```

But the feature never arrives. The cargo fingerprint for the nros-cpp actually
built by the probe records:

```json
["alloc","lifecycle-services","param-services","platform-posix",
 "rmw-cffi","rmw-zenoh-cffi","ros-humble","std"]
```

`rmw-cffi` IS present, so the module compiles; `metadata-mode` is NOT, so the
function inside is `cfg`'d away. `nm` on the resulting archive confirms it:
**137** `nros_cpp_*` symbols are exported and **zero** metadata symbols.

So this is not a missing dependency or a stale artifact — it is the
`NROS_EXTRA_CPP_FEATURES` hook failing to propagate into the umbrella crate's
feature list that `NanoRosRuntimeCrate.cmake` assembles.

## Why the obvious "already fixed" answer is wrong

`df81e852e` ("fix(308-W1): the probe's feature var was inert; link the
component lib") addressed this exact hook, and the comment it left in
`cmake/NanoRosRuntimeCrate.cmake:238` describes precisely this failure:

> *"a consumer setting a cache variable had NO effect: the metadata probe's
> `metadata-mode` reached CMakeCache.txt and never a cargo invocation, and the
> recording backend was silently absent from the link."*

The extension point exists (`if(NROS_EXTRA_CPP_FEATURES) list(APPEND
_cpp_features ...)`) and the probe sets the variable — yet the feature is still
absent from the built crate. Something between the probe's directory scope and
`_nros_runtime_platform_features` is still dropping it. The
`include()`-inside-a-function scope trap that CLAUDE.md documents for
`_NROS_ENTRY_DIR` is the shape to check first.

## Impact

Every C++ component silently loses its source-metadata sidecar, so the bake
falls back to the SystemModel executor bound instead of exact sizing — the
under-counting failure mode issue 0257 exists to prevent. Because `nros sync`
exits 0, nothing surfaces this except reading the log.

## Repro

```
cd examples/workspaces/cpp
rm -rf build/nros-metadata
nros sync 2>&1 | grep "undefined reference"
```

Inspect what cargo actually got:

```
cd build/nros-metadata/metadata-probe-cmake/<pkg>__<component>
python3 -c "import json,glob;print(json.load(open(glob.glob(
  'build/cargo/*/x86_64-unknown-linux-gnu/debug/.fingerprint/nros-cpp-*/lib-nros_cpp.json'
)[0]))['features'])"
nm -g --defined-only build/nano_ros/packages/core/nros-cpp/libnros_cpp.a | grep metadata
```

## Notes

Found while verifying phase-312 W2 (nano-ros re-pointed at the new
`ros-launch-resolve` layer). Not caused by that work — the failing path is the
C++ metadata probe, which shares no code with launch resolution, and it
reproduces independently of the submodule change.

Owned by whoever is landing phase-308's C++ producer.

## Resolution (2026-07-28)

There are **two** ways nros-cpp gets built, and the hook was only on one.

- `nros_workspace()` synthesises a runtime umbrella and appends
  `NROS_EXTRA_CPP_FEATURES` there (`cmake/NanoRosRuntimeCrate.cmake:241`).
- A plain `find_package(nano_ros)` + `add_subdirectory` — which is exactly what
  the generated probe project does — builds nros-cpp through
  `packages/core/nros-cpp/CMakeLists.txt`, which built `_cpp_features` from
  scratch at line 84 and consulted no hook.

So `df81e852e` added the extension point to the path the probe does not take.
The probe's `set(NROS_EXTRA_CPP_FEATURES "metadata-mode")` was read by nothing,
`nros_cpp_metadata_dump` stayed `cfg`'d out, and the link failed.

Fix: the same `if(NROS_EXTRA_CPP_FEATURES) list(APPEND ...)` block now exists on
the second path too.

Receipt — the probe's nros-cpp feature set before and after:

```
- ["alloc","lifecycle-services","param-services","platform-posix",
   "rmw-cffi","rmw-zenoh-cffi","ros-humble","std"]
+ ["alloc","lifecycle-services","metadata-mode","param-services",
   "platform-posix","rmw-cffi","rmw-zenoh-cffi","ros-humble","std"]
```

`nm` on the resulting archive now finds the metadata symbol, and the probe
links and runs.

Also fixed in passing: `packages/testing/nros-tests` still path-dep'd
`ros-launch-manifest` at its pre-phase-312 location, which broke the probe's
cargo resolution outright. A W2 miss.

## Correction (2026-07-28) — the "records nothing" claim was wrong

This document originally ended with a section reporting that the probe now
linked and ran but recorded nothing (`nros_cpp_metadata_dump` returning -2),
and called that a separate recording-path defect belonging to phase-308.

**That was an artifact of a stale `nros` binary**, built before
`4f6482685` (issue 0305's one-project-per-workspace restructure). Re-measured
with a current CLI, all six C++ components in `examples/workspaces/cpp` record
real entities: `talker.json` carries its node, its `/chatter` publisher, and
the `std_msgs/msg/Int32` interface. `nros sync` reports "6 rebuilt, 0 already
current".

So this issue's fix DOES restore exact executor sizing for C++ components;
there is no additional recording defect to chase.

The lesson is one this repo already documents for fixtures: a stale in-tree CLI
produces failure signatures indistinguishable from code bugs. Run
`just setup-cli` before trusting any probe measurement — I did not, and filed a
non-existent defect as a result.
