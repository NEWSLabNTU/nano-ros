---
id: 282
title: "Zephyr C + XRCE fixtures compile against the nros_cpp_config_generated.h stub (0088/0090 class, XRCE-only)"
status: open
type: bug
severity: medium
area: zephyr
related: [0088, 0090]
---

## Finding (2026-07-26, phase-305 lane sweep)

Four Zephyr fixtures fail to build:

- `build-c-talker-xrce`
- `build-c-service-server-xrce`
- `build-c-service-client-xrce`
- `build-c-action-server-xrce`

Each dies compiling the component's C TU:

```
packages/core/nros-cpp/include/nros/nros_cpp_config_generated.h:32:2:
  error: #error "nros_cpp_config_generated.h must be supplied per-build by
  the build system; see the comment in this stub for guidance."
```

i.e. the in-tree `#error` stub was reached instead of the per-build header
in `<build>/nros-rust/nros-cpp-generated/` — the 0088 / 0090 race class,
here on the Zephyr **C + XRCE** path only.

Scope is narrow and reproducible: every C+zenoh, C+cyclonedds and ALL C++
Zephyr fixtures build clean in the same sweep; only the four XRCE C
fixtures fail. The include ORDER is correct (the generated dir precedes the
stub dir on the command line), so this is an ordering/race problem — the
header does not exist yet when the TU compiles — not a search-path problem.

## Not caused by the RFC-0057 migration

Verified by two independent A/B runs on a clean disk (the earlier sweep's
noise was host ENOSPC, since resolved):

1. Reverting `examples/zephyr/c/talker/CMakeLists.txt` to the pre-phase-305
   fused `nano_ros_add_node(...)` spelling → same failure.
2. Additionally checking out `cmake/` from the commit before phase-305 W1
   → same failure.

So it predates phase-305; the migration only made the sweep run far enough
to surface it.

## Direction

Mirror the 0088/0090 remedy on the XRCE C path: ensure the component's TUs
carry a hard file-level `OBJECT_DEPENDS` on
`<build>/nros-rust/nros-cpp-generated/nros/nros_cpp_config_generated.h`
(phase-305 made `nano_ros_auto_add_library` APPEND rather than overwrite
that property, which is necessary but evidently not sufficient here), or
find why the XRCE Kconfig path pulls the nros-cpp header into a C TU at all
— the zenoh/cyclonedds C lanes do not.
