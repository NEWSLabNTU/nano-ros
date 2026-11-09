---
id: 2401
title: "`CffiSession::open_marshalling_properties` puts an 8.4 KB frame on the stack of every session open that carries properties"
status: open
type: enhancement
severity: medium
area: [memory, rmw, stack]
related: [phase-548, issue-2398, issue-2360]
found: 2026-11-10
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**,
beside issue 2360 (registration stack frames). The same owner works on the stack
floor.

## What

`CffiSession::open_marshalling_properties`
(`packages/rmw/cffi/src/lib.rs`, around line 2384) builds the session's
properties as NUL-terminated C strings in two stack arrays:

```rust
let mut key_bufs = [[0u8; SESSION_PROPERTY_BUF_LEN]; MAX_SESSION_PROPERTIES];
let mut val_bufs = [[0u8; SESSION_PROPERTY_BUF_LEN]; MAX_SESSION_PROPERTIES];
```

That is `MAX_SESSION_PROPERTIES` x 2 x `SESSION_PROPERTY_BUF_LEN` = 16 x 2 x 256
= 8,192 B of buffers. Measured on Cortex-M7 (`thumbv7em-nuttx-eabihf`, PX4
1.17's NuttX, `-Z emit-stack-sizes`), the frame is **8,472 B**. It is the
largest single frame on the session-open path, which totals about 15.3 KB
before any backend frame (issue 2398).

The buffers exist because the RMW vtable takes NUL-terminated C strings, and a
Rust `&str` has no terminator.

## Who pays

- The function is `#[inline(never)]`, so only an open that carries properties
  pays the frame.
- **Open-dominated images** pay it at their peak. A PX4 `BRINGUP` module is one:
  its main task runs the open, and issue 2398 now refuses a `STACK_MAIN` below
  the floor.
- **Registration-dominated images** do not. On the Nucleo action images the
  peak is registration, about 29-30 KB after PR #3381 (issue 2360), against an
  open of about 10 KB. Shrinking this frame does not lower their floor.

## Fix options (not chosen)

1. Callers pass `&CStr`, so no copy is needed.
2. Bake `SESSION_PROPERTIES` NUL-terminated at build time, so the strings are
   already C strings.
3. Marshal with `alloc` where an allocator exists, or one property at a time
   through the vtable, so the stack holds one key/value pair rather than 16.

## Acceptance

- The open path's stack need on Cortex-M7 drops by most of the 8.4 KB,
  measured the same way as issue 2398.
- No `TransportError` or behaviour change for 16 properties of up to 255 bytes.
- The `px4-thread-stacks` floor and the shared stack floor (issue 2360's
  owner) read the new number from one source.
