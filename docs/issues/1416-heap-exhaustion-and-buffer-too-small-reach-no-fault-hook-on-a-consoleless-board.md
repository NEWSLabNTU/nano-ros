---
id: 1416
title: "Heap exhaustion is a printk that returns NULL, and a BufferTooSmall on
  the C++ take path returns RET_FULL with out_len 0 - neither reaches
  nros_platform_panic or the boot report, so on a board with no console
  neither is seen"
status: open
type: bug
area: [zephyr, cpp, memory]
severity: high
found: 2026-09-21
related: [issue-1036, issue-0900, issue-0757, phase-366, phase-460, rfc-0077]
---

## The two faults (verified at 783cdfa14)

**Heap exhaustion.** `packages/platform/nros-platform-zephyr/src/platform.c:212`
prints `nros: HEAP EXHAUSTED: request N bytes, arena N bytes, caller 0x..`
via `printk` and returns NULL to the caller. The hook that a console-less
board CAN honour exists in the same file: `nros_platform_panic`
(`platform.c:1265`, phase-366 / RFC-0077) prints synchronously and calls
`k_panic()`, so an image's `k_sys_fatal_error_handler` runs and the boot
report (`CONFIG_NROS_BOOT_REPORT`) survives the halt. Exhaustion does not
call it and does not write the report's failed-allocation fields, which
issue 0900 added for the ARENA case only. On the MR-CANHUBK344 (no console
UART; the second UART carries the transport) the line goes nowhere, and the
NULL is handled only by code written to handle it. The island's C++
components were not.

**BufferTooSmall on the C++ take.** `packages/api/nros-cpp/src/subscription.rs:606`
(and `:682`, `:741`):

```rust
Err(TransportError::BufferTooSmall | TransportError::MessageTooLarge) => {
    // The backend drops the oversized message; `out_len` stays 0
    unsafe { *out_len = 0; }
    NROS_CPP_RET_FULL
}
```

The sample was received, acknowledged and discarded. The Rust arena dispatch
counts and logs the same event (`packages/core/nros-node/src/executor/arena.rs:1121`,
"subscription take DROPPED ... Dropped N so far (issue 0757)"); the C++ path
counts nothing and the only reader of `NROS_CPP_RET_FULL` in the headers is
the parameter-service probe (`nros/node_parameters.hpp:339`). The island's
board `.conf` documents this at line 151 as "SILENT on the C++ arena dispatch
path" and sizes the buffer generously to avoid it, which is a workaround
written into a config file.

## Which hook, decided

* Exhaustion: write the boot report's failed-allocation fields, then
  `nros_platform_panic` when `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL=y` (new;
  default y when `CONFIG_NROS_BOOT_REPORT=y`, else n, so a development image
  keeps NULL-and-log). An allocation that fails after init on a static-pool
  image is not a condition the image was designed to continue from.
* Too-small take: not fatal - a drop is a QoS fact. The C++ take increments
  the per-entity drop counter the Rust path uses, the first drop per entity
  logs topic and both sizes through `nros_log`, and the boot report carries
  the total as `samples_dropped_too_small`.

Issue 1036 owns the sink question (which `nros_log` sites reach nothing on a
console-less target); this issue names the two sites whose consequence is a
wrong run rather than a missing line.

## Acceptance

phase-460 W7: a host test registers a C++ subscription with a 16-byte buffer,
publishes 64 bytes, asserts counter 1 and a log line naming both sizes; a
native_sim test exhausts the heap with the fatal knob on and asserts the fatal
handler ran and the report names the request size; the knob off keeps today's
NULL-and-log, asserted.
