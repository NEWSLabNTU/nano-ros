---
id: 52
title: Zephyr FFI interface generator emits duplicate include!() for diamond deps → Rust E0428
status: resolved
type: bug
area: codegen
related: [phase-218, rfc-0023]
resolved_in: ASI FVP workspace-mode bring-up (zephyr/cmake/nros_generate_interfaces.cmake — REMOVE_DUPLICATES)
---

## Symptom

A downstream Zephyr consumer generating several message packages with
overlapping dependency closures failed at the FFI staticlib compile:

```
error[E0428]: the name `nros_cpp_publish_builtin_interfaces_msg_time`
  is defined multiple times
 --> …/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_ffi.rs
```

The per-package FFI crate `lib.rs` had `include!("…builtin_interfaces_msg_time_ffi.rs")`
**twice** (and Duration twice), e.g. for `tier4_debug_msgs DEPENDENCIES
builtin_interfaces std_msgs`.

## Root cause

The **Zephyr-module** interface generator
(`zephyr/cmake/nros_generate_interfaces.cmake`) builds the `lib.rs`
`include!()` set by iterating each dependency's `${dep}_GENERATED_RS_FILES`
(which already holds that dep's **transitive closure**) and appending includes
**with no cross-dependency de-dup**. A diamond dep double-includes the shared
leaf: `std_msgs`'s closure re-contains `builtin_interfaces`, so listing both
`builtin_interfaces` and `std_msgs` emits Time/Duration twice → duplicate
`#[no_mangle]` FFI fns → E0428.

The **canonical** generator (`cmake/NanoRosGenerateInterfaces.cmake`) already
collects all dep + own `.rs` files into one list and calls
`list(REMOVE_DUPLICATES …)` before emitting. The Zephyr fork (which also emits
absolute include paths and the `nros-fast-release` cargo profile) was missing
that step — the two generators had drifted.

## Fix

Mirror the canonical generator: collect dep closures + own files into one
`_ffi_rs_all`, `list(REMOVE_DUPLICATES _ffi_rs_all)`, then emit one `include!()`
per unique `.rs`. Verified: a 2-dep diamond (`tier4_debug_msgs`) now emits 11
unique includes (was 13 with 2 dups); E0428 gone.

## Follow-up

The two generators (`cmake/NanoRosGenerateInterfaces.cmake` vs
`zephyr/cmake/nros_generate_interfaces.cmake`) duplicate the FFI-crate emission
logic and have already drifted once (de-dup, relative-vs-absolute include
paths). Consider funnelling the `lib.rs` assembly through one shared helper so a
fix in one can't miss the other.
