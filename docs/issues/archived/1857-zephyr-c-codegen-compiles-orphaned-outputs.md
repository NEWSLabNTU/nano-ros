---
id: 1857
title: "The Zephyr generator globs its output directory, so after phase-492's C rename an existing build compiles yesterday's files beside today's and fails the codegen-version `#error`"
status: resolved
type: bug
severity: high
area: [build, zephyr, codegen]
related: [1360, phase-492, phase-306, phase-486]
found: 2026-10-13
---

## Measured

`just build-test-fixtures lane=tier2` (phase-486 W8c's run) failed in the Zephyr
family. Two C leaves broke, `build-cortex-m-c-talker-zenoh` and
`build-cortex-m-c-boot-report-zenoh`:

```
nano_ros_c/std_msgs/msg/std_msgs_msg_bool.h:24:2: error: #error "nros: this generated tree
was emitted at a codegen version the runtime does not accept ..."
```

Both build dirs had been configured the day before, at codegen version 12.
Phase-492 (W7/W8: C messages take rosidl's file names) then landed, and the
runtime now requires 14. The regeneration ran correctly and wrote 61 new files
(`std_msgs/msg/bool.c`, ...). Yesterday's 61 old-named files
(`std_msgs_msg_bool.c`, ...) stayed beside them, and `build.ninja` compiled
both sets into `libapp.a`. The old files state version 12, so their `#error`
fires.

## Cause

`zephyr/cmake/nros_generate_interfaces.cmake` collected what to compile with
`file(GLOB_RECURSE … "${_output_dir}/*.c")`, and `*.h`, `*.hpp` and `*.rs` the
same way. Codegen never deletes an output it no longer writes. The directory
lives in a build tree that outlives renames: on CI that is issue 1360's
persistent self-hosted Zephyr workspace. So any rename of a generated file
leaves an orphan the glob picks up.

This is the second time in this generator:

- **phase-306:** splitting `*_ffi.rs` into `*_types.rs` + `*_exports.rs`
  left the old file `include!()`d beside the new pair (E0428). It was fixed
  by deleting that one suffix before globbing, which covered that instance
  and not the class.
- **phase-492:** the rename above.

The canonical generator (`cmake/NanoRosGenerateInterfaces.cmake`) never had the
bug. It builds from `_nros_predict_generated_outputs()`, the list of what
codegen emits, which the Zephyr generator already computed for its freshness
check and then ignored.

## Fix

- **Compile the prediction:** both arms of the Zephyr generator now use
  `_exp_hdr` / `_exp_src` / `_exp_rs`, the same lists the canonical generator
  builds from, so they are proven complete for C and C++ by every cmake
  fixture.
- **phase-306's one-off deletion is gone:** the prediction never lists a
  legacy file.
- **Gate:** `check-generator-output-glob` (fast line, self-testing) refuses a
  `file(GLOB …)` of a codegen output directory in any tracked cmake file. On
  `main`'s version of this file it reports the five globs this fix removed.

## Verified

See the PR: the two leaves that failed are rebuilt INCREMENTALLY in the same
build dirs, orphans still on disk, and link.
