---
id: 294
title: "The C/C++ metadata probe rebuilds the entire Rust runtime once per component — 2.6 GB and a cold build each"
status: open
type: bug
severity: high
area: cli, build
related: [phase-308]
---

## Measurement (2026-07-28)

Probing `examples/workspaces/cpp` (six C++ components) after a partial run:

```
2.7G  cpp_add_client_pkg__add_client
2.7G  cpp_add_server_pkg__add_server
2.7G  cpp_fib_client_pkg__fib_client
732M  cpp_fib_server_pkg__fib_server   (killed mid-build)
----
8.8G  total, and only 3.5 of 6 components reached
```

Inside one probe:

```
2.6G  build/cargo          <- the whole Rust runtime, from scratch
 54M  build/nano_ros
 33M  build/component_pkg  <- the component itself
 22M  build/corrosion
```

**2.6 GB of the 2.7 GB is a private cargo target dir.** The component — the
only thing that differs between probes — is 33 MB.

## Cause

`metadata_probe_cmake::run_probe` generates one CMake project per component,
each with its own `CMAKE_BINARY_DIR`. Corrosion derives its cargo target dir
from that, so every probe gets a private target dir and shares nothing: N
components means N cold builds of nros-cpp, nros-c, and their whole dependency
graph.

The runtime configuration is IDENTICAL across components in a workspace — same
backend, platform and ROS edition, all derived from the workspace, not the
component. There is nothing to justify rebuilding it per component.

## Why it went unnoticed

Unit tests assert the generated CMake text; they cannot see build cost. The
first real run was expected to take minutes and instead ran for hours, which is
how it surfaced. A wrong hypothesis about it costs an hour to disprove, which
is itself the reason two probe defects took so long to find (see phase-308).

## Fix

**One CMake project per workspace, N probe executables.** A single configure, a
single runtime build, and one small TU per component:

```cmake
find_package(nano_ros REQUIRED)
add_subdirectory(<pkg_a> component_a)
add_subdirectory(<pkg_b> component_b)
add_executable(probe_a probe_a.cpp)   # links component_a lib
add_executable(probe_b probe_b.cpp)   # links component_b lib
```

Each probe still writes its own sidecar, so the driver's per-component
`Ok/Err` reporting is unchanged; only the build is shared. Expected cost drops
from N cold runtime builds to one.

Interim mitigation if the restructure is deferred: point every probe at a
shared `CARGO_TARGET_DIR`. Whether Corrosion honours it needs checking — it may
override with its own `--target-dir`.

## Also worth fixing alongside

The probe build dirs are large and gitignored but never cleaned; a full
workspace probe leaves ~16 GB behind for six components.
