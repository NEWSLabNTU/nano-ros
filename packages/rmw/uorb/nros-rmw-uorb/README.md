# nros-rmw-uorb

Phase 115.K.4 — C++ port of `nros-rmw-uorb` (the previous Rust
implementation, slated for retirement once this backend reaches
parity).

## Layout

```
packages/rmw/uorb/nros-rmw-uorb/
├── CMakeLists.txt          standalone CMake project
├── cmake/
│   └── NrosRmwUorbConfig.cmake.in
├── include/
│   └── nros_rmw_uorb.h     public C entry point: nros_rmw_uorb_register()
├── src/
│   ├── internal.hpp        shared decls across .cpp files
│   ├── vtable.cpp          assembles nros_rmw_vtable_t + register entry
│   ├── session.cpp         open / close / drive_io  (stubs in K.4.0)
│   ├── publisher.cpp       create / publish_raw / destroy (stubs in K.4.0)
│   ├── subscriber.cpp      create / take_serialized / has_data / destroy (stubs in K.4.0)
│   └── service.cpp         service slots — default UNSUPPORTED (K.4.4 may change)
└── tests/
    └── register_smoke.cpp  K.4.0 smoke test (stubs cffi_register locally)
```

## Build

Standalone (smoke test only):

```bash
cmake -S packages/rmw/uorb/nros-rmw-uorb -B build/nros-rmw-uorb \
    -DNROS_RMW_CFFI_DIR=$PWD/packages/core/nros-rmw-abi/include
cmake --build build/nros-rmw-uorb
./build/nros-rmw-uorb/nros_rmw_uorb_register_smoke
```

Inside a PX4 module (K.4.1 onward):

```cmake
find_package(NrosRmwUorb CONFIG REQUIRED)
target_link_libraries(my_px4_module PRIVATE NrosRmwUorb::NrosRmwUorb)
```

Pass `-DNROS_RMW_UORB_LINK_PX4=ON -DPX4_FIRMWARE_DIR=<path>` to
pull in the uORB / workqueue / log headers from a PX4-Autopilot
checkout.

## Status

The planned K.4.0 to K.4.5 steps are done. What the backend implements today:

- **Sessions:** `session.cpp`.
- **Pub/sub over uORB:** `publisher.cpp` and `subscriber.cpp`.
  - Subscribers are woken by the PX4 WorkItem `ScheduleNow()`
    (`px4_callback_glue.cpp`).
  - Off-PX4 there is a polling fallback (`callback_default.cpp`,
    `register_fallback.c`).
- **Topic registry:** a fixed-capacity `(name, type_name) -> orb_metadata *`
  table, sized per image (`topic_registry.cpp`).
- **Services:** every service slot returns `NROS_RMW_RET_UNSUPPORTED`
  (`service.cpp`). uORB has no request/reply primitive.
- **The legacy Rust stack** (the Rust `nros-rmw-uorb` and `nros-px4`) is
  removed. RFC-0011 still describes it; its rewrite is pending in the
  phase-555 design-doc review.

The original plan is kept in [`docs/roadmap/phase-115-runtime-transport-vtable.md` §D.7](../../../../docs/roadmap/archived/phase-115-runtime-transport-vtable.md).
