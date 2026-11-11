---
id: 2436
title: "A hand-written C west entry needs its image and its language spelled in its own CMakeLists.txt, which `nros build` and the module do not supply"
status: resolved
type: bug
severity: medium
area: [cli, zephyr, cmake, build]
found: 2026-11-11
related: [phase-530, issue-2405, issue-2435, issue-2157, issue-2439]
---

**Home: [phase-530](../../roadmap/phase-530-nros-configure-single-pass.md)**. Found while building issue 2405's acceptance road (`packages/testing/nros-tests/fixtures/west_hand_written_c_entry`).

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

## Resolution

Resolved 2026-11-11. Not a design call: the language already has one rule,
`leaf_kconfig::package_language` (a `Cargo.toml` entry is Rust; a
`CMakeLists.txt` entry is the TYPED carrier, C++, whatever its nodes are
written in), the one the API row and every generated west application use.

`nros build`'s west arm now states a hand-written entry as its image
(`hand_written_image_args`): `-DNROS_IMAGE_SYSTEM_DIR=<bringup>
-DNROS_IMAGE=<id> -DNROS_IMAGE_LANGUAGE=<package_language>`, the three facts
`west_app::image_statement_cmake` writes into a generated application. An
entry that sets them itself still wins (a normal variable shadows the cache).
Unit test: `a_hand_written_west_entry_is_stated_as_its_image`.

Coverage: the fixture's `zephyr_entry/CMakeLists.txt` no longer names its
image. The raw-west row passes the statement in `west_extra`, as
`west_bringup_zephyr` already does, and a new row,
`west_hand_written_c_entry_nros_build`, builds the same entry through `nros
build demo_bringup:zephyr` (new optional row field `west_nros_image`, read by
`scripts/build/west-fixtures.sh`). `tests/west_hand_written_entry.rs` asserts
both configures hold to their seeds. The new row's cache shows
`NROS_IMAGE_LANGUAGE=cpp` and `.config` `CONFIG_NROS_CPP_API=y`.

Negative control: with the statement removed from the CLI, the new row's
configure is unseeded and refused by `NanoRosPromise.cmake`, and its test
fails. Taking it found issue 2439: the row is REUSED across that CLI change
unless its build dir is deleted.
