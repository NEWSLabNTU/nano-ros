---
id: 542
title: "The C/C++ metadata probe asks `nros-c` for `metadata-mode`, a feature only `nros-cpp` has — so C/C++ components cannot regenerate their sidecars"
status: open
type: bug
area: build
related: [issue-0522, issue-0304, phase-313]
---

## Symptom

`nros sync` on a workspace with C or C++ components reports missing producers,
and the probe build under it fails:

```console
$ nros sync examples/workspaces/safety
sync: source metadata — no producer for c_safety_listener_pkg::c_safe_listener
  (metadata probe build failed (exit 2):
   error: the package 'nros-c' does not contain this feature: metadata-mode
   gmake[3]: *** [.../packages/api/nros-c/CMakeFiles/_cargo-build_nros_c] Error 101)
```

Three of the four C/C++ components in that workspace lose their sidecar
(`c_safety_talker_pkg`, `c_safety_listener_pkg`, `cpp_safety_listener_pkg`).
The Rust components are unaffected — they take the cargo harness, not this path.

## Cause

`metadata_probe_cmake.rs` writes the probe project's CMakeLists with

```cmake
# The recording RMW backend rides in on nros-cpp's `metadata-mode` cargo feature
set(NROS_EXTRA_CPP_FEATURES "metadata-mode")
```

and `nros_feature_set()` in `cmake/NanoRosFeatureSet.cmake` appends that list to
the feature set of **whatever crate it is assembling**:

```cmake
if(NROS_EXTRA_CPP_FEATURES)
    list(APPEND _feats ${NROS_EXTRA_CPP_FEATURES})
endif()
```

`metadata-mode` exists on `nros-cpp` (`= ["nros/metadata-mode",
"dep:nros-rmw-metadata"]`) and has **never** existed on `nros-c` — `git log -S`
over that manifest returns nothing. A probe project that pulls both crates
therefore fails at the `nros-c` cargo build.

The variable is named `..._CPP_FEATURES` and its comment says "nros-cpp's
`metadata-mode`", so the intent is clear; the hook is just not scoped to the C++
crate. That hook is the same one whose per-path duplication caused issue 0304,
and the fix then was to apply it in exactly one place — this is the other half:
applying it in one place is not enough if the place serves two crates.

## Why nobody noticed

The C/C++ sidecars are gitignored (`examples/**/metadata/*.json`) and already
present on any tree that probed successfully before the feature diverged, so a
warm checkout keeps working. It only surfaces when a sidecar is deleted or a new
component is added — and no lane does either, which is the same
"path nothing executes" shape as issue 0488 residue 4.

## What it costs beyond the sidecars

Issue 0522 measured 14 `metadata-probe-cmake` trees at 50.26 GiB, of which 4.7
of every 4.8 GiB is Corrosion's cargo tree. Those trees are the residue of
probe builds that get as far as compiling the runtime and then fail at
`nros-c` — so the disk is being spent on a build that cannot finish, and
0522's "is the cache worth keeping?" trade cannot be measured until this is
fixed. Warm `nros sync` on `examples/workspaces/safety` takes 30.9 s and ends in
this failure.

## Direction

Scope the hook to the crate it names. Either apply `NROS_EXTRA_CPP_FEATURES`
only when assembling `nros-cpp`, or give the probe a crate-keyed pair
(`NROS_EXTRA_C_FEATURES` / `NROS_EXTRA_CPP_FEATURES`) so a C-only workspace never
sees a C++ feature. The second is closer to what the call site means and makes
the mistake unspellable rather than merely absent.

## Acceptance

* `nros sync examples/workspaces/safety` regenerates all four C/C++ sidecars
  with no "no producer" line.
* A C-only workspace probes without `nros-cpp` in its graph at all.
* Then 0522's measurement becomes possible: time a cold probe (tree deleted)
  against a warm one, and decide whether the 50 GiB is worth keeping.
