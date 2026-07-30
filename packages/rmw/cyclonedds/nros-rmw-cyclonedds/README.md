# nros-rmw-cyclonedds

Cyclone DDS RMW backend for nano-ros (Phase 117).

Standalone CMake project — **not a Cargo crate**. Builds a C++ static
library that implements `nros_rmw_vtable_t` (see
`<nros/rmw_vtable.h>` from `packages/core/nros-rmw-cffi`) on top of
Eclipse Cyclone DDS. Wired into nros-cpp consumers via the CMake
option `-DNROS_CPP_RMW=cyclonedds` (Phase 117.8).

## Status

Phase 117 pub/sub, services, and raw-CDR data paths are implemented.
Phase 171 made Cyclone DDS the canonical DDS backend after dust-DDS
retirement.

## Freestanding / Allocation Audit

The wrapper is built as freestanding-friendly C++14:

- no exceptions: `-fno-exceptions`
- no RTTI: `-fno-rtti`
- no thread-safe local static guards: `-fno-threadsafe-statics`
- no `std::vector`, `std::string`, `std::shared_ptr`, or `std::unique_ptr`
  in the production wrapper

The wrapper still uses bounded per-entity dynamic state because the C ABI
stores backend handles as `void *`:

| Site | Allocation | Lifetime | Notes |
|------|------------|----------|-------|
| `session.cpp` | `SessionState` via `new (std::nothrow)` | `session_open` to `session_close` | Holds the Cyclone participant entity. Removing this requires expanding the C ABI storage. |
| `publisher.cpp` / `subscriber.cpp` | `{Pub,Sub}State` via `new (std::nothrow)` | entity create to destroy | Holds topic, reader/writer, descriptor, and `SertypeMin`. |
| `service.cpp` | `{Server,Client}State` via `new (std::nothrow)` | service/client create to destroy | Holds request/reply topics, endpoints, pending request metadata, and two `SertypeMin` helpers. |
| `sertype_min.cpp` | `ops_copy_` via `std::malloc` | `SertypeMin` lifetime | Owns a compact copy of the Cyclone descriptor ops array for stream read/write. |
| publish/service write paths | typed sample via `std::calloc` | one call | Required by `dds_stream_read_sample` before `dds_write`; released after `dds_stream_free_sample`. |
| receive paths | `dds_ostream_t` internal buffer | one call | Cyclone stream writer grows the CDR buffer internally; released with `dds_ostream_fini`. |

Cyclone DDS owns additional allocation internally. Notable calls from this
wrapper:

- `dds_create_participant`, `dds_create_topic`, `dds_create_reader`,
  `dds_create_writer` allocate Cyclone entities and release through
  `dds_delete`.
- `dds_create_qos` allocates QoS objects and releases through
  `dds_delete_qos`.
- `dds_take` may loan samples; loans return through `dds_return_loan`.
- `dds_write` / discovery / writer history allocate inside Cyclone DDS
  according to Cyclone's own configuration and QoS limits.

For no-heap targets, this backend is not allocation-free; use it only where
the platform budget includes Cyclone DDS's hosted runtime and heap.

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
