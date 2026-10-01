---
id: 1425
title: "Heap exhaustion is a printk that returns NULL, and a BufferTooSmall on
  the C++ take path returns RET_FULL with out_len 0 - neither reaches
  nros_platform_panic or the boot report, so on a board with no console
  neither is seen"
status: resolved
type: bug
area: [zephyr, cpp, memory]
severity: high
found: 2026-09-21
related: [issue-1424, issue-1612, issue-1036, issue-0900, issue-0757, phase-366, phase-460, rfc-0077]
resolved_in: "branch fix/zephyr-heap-1424-1425-1498-1324"
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

## Resolution

Both faults reach a hook a console-less board can read: phase-460 W7
(`f74ebdade`) wired them, and the half its own commit left open -- a REAL
native_sim run of the exhaustion path -- is measured here. Branch
`fix/zephyr-heap-1424-1425-1498-1324`.

**Exhaustion, on a real image.** `examples/zephyr/c/talker`, zenoh,
`native_sim/native/64`, built with `-DCONFIG_NROS_ZEPHYR_HEAP_SIZE=8192`, run
under gdb with a breakpoint on `k_sys_fatal_error_handler` that dumps the boot
record (`read-boot-report.py --addr-only` gives the address):

    nros: HEAP EXHAUSTED: request 2048 bytes, arena 8704 bytes, caller 0x431936
    nros: PANIC platform heap exhausted (see the HEAP EXHAUSTED line above, and the boot report's failed_alloc_size)
    <err> os: >>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
    FATAL-HANDLER-REACHED

and the dump taken INSIDE the fatal handler decodes to

    stage      2  BootConfigResolved -- arguments accepted; executor not yet open
    platform heap PEAK            6288 bytes   (72.2% of the heap)
    platform heap capacity        8704 bytes   (NROS_ZEPHYR_HEAP_SIZE)
    HEAP EXHAUSTED: the PLATFORM HEAP refused an allocation of 2048 bytes.
      raise CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 10752

So the fatal handler ran, and the record that survives it names the request
size and the knob. That is the native_sim half of the acceptance.
`CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL` now defaults ON for every native_sim
image (issue 1424 turned `CONFIG_NROS_BOOT_REPORT` on there), so an exhausted
heap stops a test cell instead of degrading it. The knob-OFF arm
(NULL-and-log) is held by the W7 host test (`just check zephyr-heap-exhaustion`,
whose knob-off build failing the fatal case is its negative control); it was
not re-run on a native_sim image.

**BufferTooSmall** is counted, pushed into the record as
`samples_dropped_too_small`, and logged (first, then every 64th) by W7. The
acceptance's "a log line naming BOTH sizes" cannot be met on the current ABI:
the RMW take returns bytes-or-error with no required-length out-param, so no
layer below the C++ wrapper knows the sample's size. That remainder is filed as
issue 1612 rather than kept open here.

**Not measured:** the knob-off native_sim arm; any real console-less board.
