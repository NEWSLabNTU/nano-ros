# nros-rmw-cyclonedds

Cyclone DDS RMW backend for nano-ros (Phase 117).

Standalone CMake project — **not a Cargo crate**. Builds a C++ static
library that implements `nros_rmw_vtable_t` (see
`<nros/rmw_vtable.h>` from `packages/core/nros-rmw-cffi`) on top of
Eclipse Cyclone DDS. Wired into nros-cpp consumers via the CMake
option `-DNROS_CPP_RMW=cyclonedds` (Phase 117.8).

## Status

Phase 117.3 — vtable scaffolding only. Every entry point returns
`NROS_RMW_RET_UNSUPPORTED`. Real implementation lands in 117.4
(session) → 117.6 (pub/sub) → 117.7 (services).

## Build

```bash
# Cyclone DDS itself must already be built + installed:
just cyclonedds setup

# Then this backend:
cmake -S . -B build \
      -DCMAKE_PREFIX_PATH="$PWD/../../../build/install" \
      -DCMAKE_INSTALL_PREFIX="$PWD/../../../build/install"
cmake --build build
cmake --install build
ctest --test-dir build
```

`just cyclonedds build-rmw` (Phase 117.16, stubbed) will run the same
cmake invocation from the repo root once 117.3 lands the project.

## Pin

Cyclone DDS tag `0.10.5` — matches `ros-humble-cyclonedds` 0.10.5 and
`ros-humble-rmw-cyclonedds-cpp` 1.3.4 for ROS 2 Humble wire compat.
See `docs/roadmap/phase-117-cyclonedds-rmw.md`.
