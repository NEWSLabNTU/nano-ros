---
id: 114
title: "Native C/C++ cmake fixtures race the per-build config-header mirror → `*_OPAQUE_U64S` undefined (build-fixture-extras)"
status: resolved
type: bug
area: build
related: [phase-267, 0088, 0090]
resolved_in: phase-267
---

## Summary (confirmed)

The native (posix) C/C++ cmake fixtures (`just native build-fixture-extras` →
`fixture-make-driver native-cmake-rmw`) failed compiling with the generated size
constants undefined:

```
nros_generated.h: ‘SERVICE_SERVER_OPAQUE_U64S’ / ‘NROS_LIFECYCLE_CTX_OPAQUE_U64S’ … undeclared
subscription.hpp:473: ‘Subscription<std_msgs::msg::Int32>’ has no member named ‘storage_’  (cascade)
```

## Root cause

The same header-mirror race as issues 0088/0090, but on the **native/posix** path
that those fixes excluded (`if(... AND NOT NANO_ROS_PLATFORM STREQUAL "posix")`).
`nros-c`'s `build.rs` writes the per-build `nros_config_generated.h` (the
`*_OPAQUE_U64S` sizes), and a custom command (`nros_c_config_header`) mirrors it
into `${BINARY_DIR}/include/nros/`. That mirror dir IS on the include path AHEAD
of the in-tree `#error` stub — but with no hard per-TU dependency edge, a parallel
build compiles the consumer TU before the mirror command runs, so the file is not
there yet and the compile falls through to the stub. Confirmed: the mirror dir was
on the `-I` list first, but the mirrored file was absent at compile time.

Two consumer kinds were unguarded on posix:
1. the entry example target (`nano_ros_entry`, e.g. cpp `safety-listener`'s
   `main.cpp`), and
2. the generated message-library STATIC targets (`<pkg>__nano_ros_c`, whose `.c`
   TUs include `<nros/nros_generated.h>`).

## Fix (phase-267)

Add the hard edge for the posix path:
- `cmake/NanoRosEntry.cmake` — for `NANO_ROS_PLATFORM == posix`, `add_dependencies`
  on the `nros_{c,cpp}_config_header` mirror targets AND a file-level
  `OBJECT_DEPENDS` on EVERY source of the entry (incl. the user `main.cpp`).
- `cmake/NanoRosGenerateInterfaces.cmake` — same edge on the `<pkg>__nano_ros_c`
  STATIC message lib's generated sources.

Verified: `fixture-make-driver native-cmake-rmw` builds all four native cells
(`c_listener`, `c_talker`, `cpp_safety_listener`, + xrce) clean — `rc=0`, zero
`*_OPAQUE_U64S` / `storage_` errors. Embedded paths keep their existing 0088/0090
wiring (the posix gate scopes this fix to native).
