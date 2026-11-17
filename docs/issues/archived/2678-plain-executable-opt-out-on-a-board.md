---
id: 2678
title: "A board build has no public way to declare an executable that is not an image; consumers set the private `_NROS_IN_IMAGE_ADD_EXECUTABLE`"
status: resolved
type: enhancement
area: [cmake, build]
severity: low
found: 2026-11-17
resolved: 2026-11-17
resolved_in: "feat(cmake): nano_ros_add_plain_executable, a board build's executable that is not an image (issue 2678)"
related: [phase-478, phase-518]
---

**Home:** phase-478 I30 (found by the safety island). GitHub #4016.

## Symptom

On a board platform the ament surface wraps `add_executable`, so every
executable a CMakeLists declares becomes an image: the board's entry calls the
program's renamed `main` (phase-518, `cmake/NanoRosAmentSurface.cmake`). That
is right for a stock package's program. It is wrong for a helper that keeps its
own `main` and links none of the runtime. The island builds one,
`can_vcan_sender`, a SocketCAN test driver compiled in the FreeRTOS POSIX board
build (`actuation_module/src/controller_pkg/CMakeLists.txt`). To keep the
builtin it sets `_NROS_IN_IMAGE_ADD_EXECUTABLE` around the call: the private
switch nano-ros's own image sites use. A rename of that variable would turn the
helper into an image with no error.

## Fix

`nano_ros_add_plain_executable(<name> <add_executable arguments>...)`, in
`cmake/NanoRosVerbs.cmake` beside `nano_ros_add_executable`. It is the builtin
`add_executable` on every platform and marks the target
`NROS_PLAIN_EXECUTABLE TRUE`. The switch it sets is function-scoped, so the
caller's next `add_executable` is an image again. No sources, or `IMPORTED` /
`ALIAS`, is refused by name. The island's three lines become:

    nano_ros_add_plain_executable(can_vcan_sender
      $<TARGET_OBJECTS:can_vcan_sender_obj> "${_variant_c}")

## Test

`tests/cmake-ament-surface-tests.sh` case 14 (`just check cmake-ament-surface`):
on `freertos` both plain `add_executable` calls reach the image verb and the
helper does not, and is an `EXECUTABLE` marked plain; on `posix` the same; a
call with no sources is refused naming the target.
