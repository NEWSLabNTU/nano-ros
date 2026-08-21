---
id: 753
title: "SKIP_INSTALL parity between the canonical and Zephyr-module
  `nros_generate_interfaces()` — resolved on arrival (Phase 210.E.3.c)"
status: resolved
type: enhancement
area: cmake, zephyr
related: [phase-210]
resolved_in: "Phase 210.E.3.c (pre-existing; verified 2026-08-22)"
---

## What was filed

ASI (autoware-safety-island, the canonical FreeRTOS/Zephyr consumer) carries
a lane gate in its message aggregation:

```cmake
if(TARGET zephyr_interface)
    set(_nros_skip_install "")          # zephyr module variant
else()
    set(_nros_skip_install SKIP_INSTALL) # canonical workspace variant
endif()
nros_generate_interfaces(std_msgs ... ${_nros_skip_install})
```

on the belief that the Zephyr module's variant of
`nros_generate_interfaces()` would misparse `SKIP_INSTALL` as a message
path, while the canonical variant needs it (an in-app aggregation never
populates the `INSTALL(EXPORT)` layout).

## Resolution: the gap does not exist at any current pin

`zephyr/cmake/nros_generate_interfaces.cmake` parses `SKIP_INSTALL` as an
option (`cmake_parse_arguments(_ARG "SKIP_INSTALL;NO_FFI_CRATE" ...)`) and
accepts-and-ignores it, explicitly for parity with the canonical variant —
since Phase 210.E.3.c. Zephyr emits directly to the `app` target, so there
is no install layout to skip; silently accepting the flag is the correct
behaviour.

One vocabulary, both variants, already true. The consumer gate is dead
weight: passing `SKIP_INSTALL` unconditionally is correct on both lanes.
ASI drops its `_nros_skip_install` gate on its side (phase-5 W6 item).

Filed as resolved so the reserved id stays contiguous and the verification
is on record — the consumer comment claiming the misparse was stale.
