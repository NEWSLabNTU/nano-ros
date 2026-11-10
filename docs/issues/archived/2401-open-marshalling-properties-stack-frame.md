---
id: 2401
title: "`CffiSession::open_marshalling_properties` puts an 8.4 KB frame on the stack of every session open that carries properties"
status: resolved
type: enhancement
severity: medium
area: [memory, rmw, stack]
related: [phase-548, issue-2398, issue-2360, issue-2400]
found: 2026-11-10
resolved: 2026-11-10
resolved_in: "phase-548: on an alloc build the property strings go to one heap block sized by the data; the frame is 8,472 B -> 240 B on Cortex-M7 (O3)"
---

**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md)**,
beside issue 2360 (registration stack frames).

## What

`CffiSession::open_marshalling_properties` (`packages/rmw/cffi/src/lib.rs`)
built the session's properties as NUL-terminated C strings in two stack
arrays, `[[0u8; 256]; 16]` each: 16 x 2 x 256 = 8,192 B of buffers whatever
the data. On Cortex-M7 the frame was **8,472 B**, the largest single frame on
the session-open path (about 15.3 KB before any backend frame, issue 2398).
It is paid on every open that carries properties, so it set the floor of
open-bound images such as a PX4 `BRINGUP` module.

## Where the strings go, and why

The C ABI fixes one thing: `create_session` takes every property at once, as
an array of `rmw_session_property_t` whose strings are NUL-terminated and
borrowed for the duration of the call (`nros/rmw_entity.h`). So all `2 x n`
strings must exist together, and the question is only where.

- **Incremental (one property at a time into the C call): rejected, the ABI
  does not allow it.** There is no per-property slot, and the strings must
  all be live during one `create_session`. Streaming them would need a new
  vtable slot, which is an ABI change, for a cost that is only stack.
- **A shared static buffer: rejected.** Session opens can run concurrently
  (a multi-session executor, a bridge, a second C/C++ support context), so a
  static needs a lock that every open contends on, and a spinning lock on an
  RTOS is a priority-inversion hazard on the boot path. It also moves the
  8 KiB into `.bss` of EVERY image that links the seam, including the
  registration-bound images (Nucleo action images) that never paid it at
  their peak: a permanent cost to remove a transient one.
- **The heap, where the build has one: CHOSEN.** The function already used
  `alloc` on this path (an over-long value, a whole configuration document,
  phase-514 W3b). On an `alloc` build the strings now go to ONE block,
  packed (`key\0value\0` per property) and sized by the data actually stated,
  reserved with `try_reserve_exact` (issue 1551: exhaustion is
  `TransportError::BadAlloc`, never the aborting path) and freed when the
  open returns. Two short properties cost a few dozen heap bytes for the
  duration of the open instead of 8 KiB of stack. `alloc` is the user's
  explicit statement that a heap exists (phase-361 W2), so this never reaches
  for a heap that is not there.
- **Without `alloc` the stack buffers stay**, behind
  `#[cfg(not(feature = "alloc"))]`. There is no heap to size by the data, and
  the static was rejected above. The arm is one `[[[u8; 256]; 2]; 16]` now,
  same size as before.

Behaviour is unchanged, and the refusals have one spelling for both arms
(`check_session_properties`): more than 16 properties, an empty key or
value, a key of 256 bytes or more, and (without `alloc`) a value that long
are `InvalidArgument`, checked before anything is copied. The only new
outcome is `BadAlloc` when the heap is exhausted on an `alloc` build.

## Measured

Method: `nros-rmw-cffi` cross-built alone for `thumbv7em-none-eabihf` with
the pinned nightly (`nightly-2026-04-11`), `RUSTFLAGS=-Z emit-stack-sizes`,
`lto = off` (so the rlib holds object code, not bitcode), and each function's
frame read from the rlib's `.stack_sizes` with the toolchain's
`llvm-readobj --stack-sizes`. The before numbers match issue 2398's
`thumbv7em-nuttx-eabihf` measurement of the same function exactly, so the
target difference does not move them.

| function | before O3 | after O3 | before Oz | after Oz |
|---|---|---|---|---|
| `open_marshalling_properties` (alloc) | 8,472 | **240** | 8,464 | **248** |
| `open_marshalling_properties` (no alloc) | 8,432 | 8,424 | | |
| `open_with_vtable` | 1,168 | 1,168 | 1,192 | 1,192 |
| `CffiRmw::open_with_rmw` | 648 | 648 | 600 | 600 |
| `<CffiRmw as Rmw>::open` | 624 | 624 | 592 | 592 |
| `open_named_with_properties` | 96 | 96 | 96 | 96 |
| `open_with_properties` | 80 | 80 | 80 | 80 |

The packing helper is inlined at both levels; what is left of the frame is
the 16-entry pointer array (128 B on a 32-bit target) and spills. The heap
calls it makes return before `open_with_vtable` runs, so they do not stack
on top of it. The open path from issue 2398's table drops from about 15.3 KB
to about 7.1 KB before any backend frame.

## Siblings

Sweep: `git grep -nE "\[\[0u8; *[A-Z_0-9]+\]; *[A-Z_0-9]+\]" -- packages/rmw/cffi/src packages/rmw/zenoh/nros-rmw-zenoh/src packages/rmw/bridge/src`,
plus the single `[0u8; NAME_BUF_LEN]`-class buffers in `packages/rmw/cffi/src`.

- **`ZenohSession::new` (`nros-rmw-zenoh/src/shim/session.rs`)** marshals the
  same properties again for `zp_config_insert`, in
  `[[0u8; CONFIG_PROPERTY_SIZE]; MAX_SESSION_PROPERTIES]` x 2. Not changed:
  on a `no_std` build that is 8 x 2 x 64 = 1 KiB, already cut for issue #64,
  and the 16 x 2 x 256 form is `std`-only (hosted stacks). It is on the open
  path of a zenoh image, below this frame, and would take the same fix if a
  zenoh RTOS image turns out to be open-bound.
- **`rust_adapter.rs` graph visitors** (node names, names-and-types, endpoint
  info): 768 B to 2.3 KiB of name buffers per callback. Not changed: these
  run on graph queries, not on an open or a create.
- **`open_with_vtable`** (1,168 B: a 256 B locator buffer plus the session's
  two 256 B name buffers, which the session keeps) and the entity creates
  (a 128 B hash buffer plus two 256 B name buffers each). Not changed: the
  name buffers are the entity's own storage, and the creates are the
  registration path issue 2360 owns.

## Not done here

The acceptance item "the `px4-thread-stacks` floor and the shared stack floor
read the new number from one source" belongs to issue 2400, which derives the
floor from the build's own `-Z emit-stack-sizes` frames; this frame reaches it
through that measurement with nothing to hand-copy. No run-time high-water
mark was taken.

## Tests

`packages/rmw/cffi/tests/session_properties.rs`, run with and without
`--features alloc`: 16 properties with 255-byte keys and values arrive whole
and in order; the key bound is exact (255 carried, 256 refused) and a bad last
row refuses the whole open before the backend sees anything; 16 properties
mixing 1 KB values with short ones arrive whole with a heap and are refused
as a whole without one.
