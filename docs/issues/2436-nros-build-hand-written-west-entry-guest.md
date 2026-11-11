---
id: 2436
title: "A hand-written C west entry needs its image and its language spelled in its own CMakeLists.txt, which `nros build` and the module do not supply"
status: open
type: bug
severity: medium
area: [cli, zephyr, cmake, build]
found: 2026-11-11
related: [phase-530, issue-2405, issue-2435, issue-2157]
---

**Home: [phase-530](../roadmap/phase-530-nros-configure-single-pass.md)**. Found while building issue 2405's acceptance road (`packages/testing/nros-tests/fixtures/west_hand_written_c_entry`).

## Measured

A workspace whose bringup sets `[image.zephyr] entry = "zephyr_entry"` and
whose `zephyr_entry/CMakeLists.txt` calls
`nano_ros_add_executable(... LANG c TYPED ... BRINGUP <bringup>)`:

1. **The image is not named.** `nros build` of that image runs the west
   build with no `-DNROS_IMAGE_SYSTEM_DIR` and no `-DNROS_IMAGE`. The module
   hook then treats the application as a guest (issue 2157's rule for an
   application no image claims), so it runs no `nros configure` and no seed
   is read. The fixture spells both in the entry's `CMakeLists.txt` to get
   past this:
   `get_filename_component(NROS_IMAGE_SYSTEM_DIR ".../../demo_bringup" ABSOLUTE)`
   and `set(NROS_IMAGE zephyr)`.
2. **A TYPED C entry needs the C++ runtime headers.** Without
   `set(NROS_IMAGE_LANGUAGE cpp)` before `find_package(Zephyr)`, the build
   fails including `rclcpp/detail/nros_cpp_ffi.h`: the typed registration
   path needs the C++ runtime headers, and nothing derives that from
   `TYPED`.

## Expected

The bringup already says which image boots `zephyr_entry`, so the road that
builds it (`nros build`, or the hook reading the image's `entry`) should
name the image, and the language the entry needs should follow from its own
`nano_ros_add_executable` arguments, not from a variable the author must
know to set.
