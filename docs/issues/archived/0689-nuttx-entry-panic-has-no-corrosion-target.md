---
id: 689
title: "`nano_ros_entry`'s PANIC application was exempted for Zephyr alone, so the NuttX lane hit the same fatal the moment it was made fatal"
status: resolved
type: bug
area: build
related: [issue-0618, issue-0617, phase-366, rfc-0077]
---

## Symptom

`just build-test-fixtures lane=tier2` fails in the nuttx lane, at configure:

```
CMake Error at cmake/NanoRosEntry.cmake:263 (message):
  nano_ros_entry(c_talker): PANIC platform cannot be applied — no
  nros-c/nros-cpp Rust target exists at this point in the configure.  The
  staticlib must be imported before the entry declares its ending.
Call Stack:
  cmake/NanoRosVerbs.cmake:157 (nano_ros_entry)
  CMakeLists.txt:10 (nano_ros_add_executable)
```

The consumer is the standard shape — `find_package(nano_ros)` then
`nano_ros_add_executable()` — so nothing about the example is unusual.

## Cause

phase-366 made a silent skip here fatal, correctly: an entry that states its
ending while the archive is built without it is the failure the phase exists to
remove. The application is `corrosion_set_features()` on `nros_c` / `nros_cpp`,
which requires those to be **Corrosion targets**.

Three hours earlier, W7.d had found the first lane where they are not
(`9f858e5c6` — "the Zephyr lane lowers PANIC itself — the entry cannot reach a
target it never creates") and exempted Zephyr with a `NROS_ZEPHYR_PANIC_APPLIED`
global property.

**NuttX has the same shape and was not covered.** Its Rust side is an
`add_custom_target` cargo build of `nros-nuttx-ffi`
(`packages/api/nros-c/cmake/nros-nuttx.cmake`), not a Corrosion import — so the
scan finds nothing, and the entry fails.

The ending is nevertheless applied on this lane, just earlier and in a different
place: `nros-nuttx-ffi`'s COMMITTED manifest names it.

```toml
nros-c = { path = "…", default-features = false, features = [
    "alloc", "global-allocator", "panic-platform", … ] }
```

So the image had a correct, single ending the whole time. What was missing was
any way for the entry to KNOW that.

## Fix — generalise the exemption rather than add a second one

A `NROS_NUTTX_PANIC_APPLIED` beside `NROS_ZEPHYR_PANIC_APPLIED` would have been
the "second spelling" CLAUDE.md names, and the third lane would repeat it. The
property is now lane-neutral, and carries what a good message needs:

```cmake
NROS_ENTRY_PANIC_APPLIED       the policy this lane applied
NROS_ENTRY_PANIC_APPLIED_BY    the lane's name, for the message
NROS_ENTRY_PANIC_APPLIED_HOW   how to change it, which differs per lane
```

`nano_ros_entry()` verifies agreement against whichever lane declared one;
Zephyr migrated to it, NuttX now declares `platform`.

NuttX supports exactly one policy, because its manifest is committed rather than
computed at configure time. Declaring that is the point: an entry asking for
`halt` or `own` now fails saying *this lane bakes `platform` into
`nros-nuttx-ffi`'s manifest*, instead of *no Rust target exists* — the mechanism
rather than the cause.

## Verified

`examples/qemu-arm-nuttx/c/talker/build-zenoh` configures (34 "Build files have
been written" across the lane, zero `CMake Error`, zero occurrences of the PANIC
message). The lane build was cut short by a harness timeout, not a failure.

## Provenance

Found 2026-08-19 by `build-test-fixtures lane=tier2`, which is 1-wise over
platform — so nuttx is in the cover and its absence blocked the whole tier.
That is the lane doing its job: a per-link-line or per-image gate cannot see a
CONFIGURE-time exemption that was written for one platform.
