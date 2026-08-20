---
id: 742
title: "`build-test-fixtures` fails on main — 36 parallel compile-check units
  share one `bridge-cpp` dir and `rm -rf` each other's output"
status: resolved
resolved: 2026-08-21
type: bug
area: build, testing
related: [issue-0520, issue-0738]
---

## Symptom

`just build-test-fixtures lane=native` exits 2. Reproduced twice in a row on
upstream `main`, before any of this branch's changes:

```
Error: write header for DebugKeyValue
Caused by: No such file or directory (os error 2)
   at rosidl-bindgen/src/generator.rs:905

Error: read message file .../bridge-cpp/.px4_msg_stage/msg/DebugKeyValue.msg
Caused by: No such file or directory (os error 2)

rm: cannot remove '.../bridge-cpp/.px4_msg_stage/msg': Directory not empty
cc1plus: fatal error: .../bridge-cpp/px4_msgs/msg/debug_key_value.hpp: No such file
make: *** [.../compile-check-3755759.mk:95: u29] Error 1
```

Four different errors, all on files that plainly exist, and the `rm` one is the
tell: two processes removing the same tree.

## Cause

`scripts/build/compile-check-fixtures.sh` runs ONCE PER COMPILE-CHECK UNIT in
parallel — 36 of them on `lane=native` — and every invocation drives the
`px4_bridge_ffi` block against the SAME
`build/px4-msgs-codegen/bridge-cpp` path.

Issue 0520 already found this shape for the Rust px4 leaves and answered it
with a repo-level advisory lock. The C++ bridge block that #0738 added took the
lock too — but only around `nros generate-px4-msgs`. Three things that touch
the same shared tree stayed OUTSIDE it:

* `rm -rf "$bridge_gen"` immediately before it, which deletes a sibling's
  finished output and its in-flight `.px4_msg_stage`;
* the `-fsyntax-only` header check, which reads what another unit may delete
  between generation and compile;
* the `cargo check`, which reaches the same tree through
  `NROS_PX4_BRIDGE_GEN`.

So the lock excluded the one operation that was already safe against itself and
left the destructive one unguarded.

## Fix

The critical section is the whole block. Held on FD 9 across it, released after
the `cargo check`.

This is nearly free here, unlike the Rust path above it — whose comment about
not serializing 87 `cargo check`s on the lock is correct FOR THAT PATH and was
the reason this block copied only the narrow spelling. The bridge generates ONE
message, syntax-checks ONE header, and its `cargo check` is already serialized
by cargo's own build-directory lock (the failing run's log is full of "Blocking
waiting for file lock on build directory").

Verified: `just build-test-fixtures lane=native` exits 0 with 36 concurrent
invocations of the block and none of the four errors, against 2/2 failures
before.

## The general shape, for the next one

A lock added to fix a concurrency bug tends to get placed around the operation
that FAILED, not around the shared state. Here the generator failed, so the
generator got the lock — while the `rm -rf` one line above it, which was
causing the failure, kept running unguarded.
