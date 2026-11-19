---
id: 2725
title: "The census tests gave `nros_cpp_publisher_create` storage one pointer short, and the overrun corrupted the caller's frame"
status: resolved
resolved_in: "the change that files this issue"
type: bug
severity: high
area: [cpp-api, testing, soundness]
related: [2714]
found: 2026-11-20
---

## What happened

`nros_cpp_publisher_create` writes a `CppPublisher`: the `RmwPublisher`
handle plus a `*const PubMonitorCell`. A const assert in
`packages/api/nros-cpp/src/publisher.rs` fixes that size at the handle plus
one pointer, and `rclcpp/publisher.hpp`, `rclcpp/generic.hpp` and
`rclcpp/rosout.hpp` all reserve `NROS_PUBLISHER_SIZE + sizeof(void*)`.

Three test setups in `nros-cpp` gave it `MaybeUninit<RmwPublisher>`
instead, 8 bytes short. Two are in `census_funnel_tests` (lib.rs) and one in
`census_fixture_tests` (metadata_hooks.rs). Each create wrote the monitor
pointer past the end of its buffer.

On the stack, that write landed in the frame of the runner that called the
setup. Issue 2714 enlarged that frame, and the write then landed on the
runner's `census` option. A census run read it as `None`, took the spin arm,
and hung (`census-hooks-complete`, 2400 s). Adding a log line moved the frame
and hid it.

## Wrong first fix

2714 diagnosed this as the executor keeping the handles by pointer past
`setup`, and moved all six test handles (3 nodes, 2 publishers, 1
subscriber) into `static mut`. The census tests passed. But no create keeps
a caller pointer: node, subscription and guard creates write a value of
their own type, and so does the publisher create. The static only moved the
8-byte overrun from the stack into whatever `.bss` held next to it. nros 133
found it in a soundness pass (phase-573).

## Fix

- The three storages are `MaybeUninit<crate::publisher::CppPublisher>`.
- The six handles are back on the stack. The census tests pass 5 runs of 5.
  Mutation-checked: shrinking the stack publisher back to `RmwPublisher`
  brings the hang back.
- The comments claiming "the executor keeps it by pointer" are removed.
- `nros_cpp_rosout_publisher_create`'s doc asked for `NROS_PUBLISHER_SIZE`
  bytes. It forwards to the same create, so it now asks for
  `NROS_PUBLISHER_SIZE + sizeof(void*)`, as `rosout.hpp` already reserves.
  `nros_cpp_ffi.h` is regenerated.

The class, swept: every storage handed to an entity create, Rust callers
and C++ reservations both. Only the three publisher sites were short.

```sh
git grep -n -E 'MaybeUninit(::)?<\s*[A-Za-z_:]+>' -- packages/api/nros-cpp/src packages/api/nros-c/src
git grep -n -E 'ptr::write\(\w+ as \*mut [A-Za-z_:]+' -- packages/api/nros-cpp/src
git grep -n -E 'storage_\[NROS_PUBLISHER_SIZE' -- packages/api/nros-cpp/include
```

The wider question, whether the C ABI should let a caller hand a create a
buffer smaller than what it writes, is phase-573's.
