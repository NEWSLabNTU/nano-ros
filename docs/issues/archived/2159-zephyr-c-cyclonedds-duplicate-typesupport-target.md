---
id: 2159
title: "`build-c-talker-cyclonedds` fails at configure: `builtin_interfaces__cyclonedds_ts` is created twice"
status: resolved
resolved_in: "2026-10-31"
type: bug
area: [zephyr, cmake, rmw-cyclonedds]
severity: medium
found: 2026-10-30
related: [2157]
---

## What was measured

The Zephyr fixture row `build-c-talker-cyclonedds` (`examples/zephyr/c/talker`,
image `zephyr_cyclonedds`, native_sim, Zephyr 3.7) stops at configure:

```text
-- Generating nros C++ interfaces for builtin_interfaces
-- Generating nros C++ interfaces for std_msgs
-- Generating nros C interfaces for builtin_interfaces
CMake Error at cmake/NanoRosAmentSurface.cmake:132 (_add_library):
  _add_library cannot create target "builtin_interfaces__cyclonedds_ts"
  because another target with the same name already exists.  The existing
  target is a static library created in source directory
  ".../examples/zephyr/c/talker".
Call Stack (most recent call first):
  zephyr/cmake/nros_generate_interfaces.cmake:620 (add_library)
  cmake/NanoRosCodegenCore.cmake:1442 (nros_generate_interfaces)
```

and the same for `std_msgs__cyclonedds_ts`.

**It is on `main`, not introduced by issue 2157's change**: built from a clean
`origin/main` worktree (375fd91f41) with
`NROS_ZEPHYR_FIXTURE_FILTER='^build-c-talker-cyclonedds$' just build zephyr`,
the identical error, four times. The sibling rows built in the same run
(`build-c-talker-zenoh`, `build-ws-c-entry-zenoh`, `build-ws-cpp-entry-cyclonedds`).

## Cause

`nros_generate_interfaces` runs once per (package, LANGUAGE), and the image
legitimately needs both: the leaf's component is C, and it registers TYPED
(`nros_components_register_node(... TYPED)`), whose carrier is C++. Issue 1675's
idempotency guard is keyed on (package, language), which is right for the
bindings -- C and C++ are different outputs -- and wrong for the Cyclone stage
below it, whose output is the same C descriptors for both, in ONE
`<pkg>__cyclonedds_ts` that `app` whole-archives. So the C++ pass created it and
the C pass tried to create it again.

## Resolution

The Cyclone stage now runs once per PACKAGE, on its own global property
(`_NROS_CYC_TS_GENERATED_<pkg>`, `zephyr/cmake/nros_generate_interfaces.cmake`).
The first pass already linked the library into `app`, so a later pass has
nothing to add.

Measured (Zephyr 3.7, native_sim): `build-c-talker-cyclonedds` and
`build-c-listener-cyclonedds` both build, and
`nros-tests::zephyr example_e2e::case_11_cyclonedds_c_pubsub_e2e` passes against
the two fresh images (it asserts the listener counted at least one sample).

## Left

The HOSTED road has the same shape: `cmake/NanoRosGenerateInterfaces.cmake`'s
guard is keyed on the language library (`<pkg>__nano_ros_c` /
`<pkg>__nano_ros_cpp`), and the backend's `per_message` hook
(`nros_rmw_cyclonedds_typesupport_for_target`) creates `<pkg>__cyclonedds_ts`
unconditionally. Not changed: no hosted image was measured hitting it, and the
fix there is not this one -- the second language LIBRARY still needs the
descriptors attached, and on CMake < 3.24 (this host has 3.22) attaching the
same archive twice through `--whole-archive` duplicates every symbol.
