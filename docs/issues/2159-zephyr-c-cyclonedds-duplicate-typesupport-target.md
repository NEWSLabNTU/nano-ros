---
id: 2159
title: "`build-c-talker-cyclonedds` fails at configure: `builtin_interfaces__cyclonedds_ts` is created twice"
status: open
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

## Reading (not yet confirmed)

The interfaces are generated twice in one configure, once per LANGUAGE (C++
then C), and the Cyclone typesupport library is named `${target}__cyclonedds_ts`
with no language in it (`zephyr/cmake/nros_generate_interfaces.cmake:620`). The
typesupport is language-independent, so the second pass should reuse the first
pass's target rather than create its own. Why a C leaf asks for C++ interfaces
at all is the other half of the question.
