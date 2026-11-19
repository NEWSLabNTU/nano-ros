---
id: 2738
title: "The Zephyr log sink formats every record into a 1280-byte buffer on the CALLER's stack"
status: open
type: bug
severity: medium
area: [zephyr, log]
found: 2026-11-19
related: [2165, 2000, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
(an island finding: it was the first suspect for issue 2165). Owner: nros 139.

## What the code does

`nros_platform_log_write`, the Zephyr port's `nros_log` sink
(`packages/platform/nros-platform-zephyr/src/platform.c:1610`, main
93368ca929), declares `char buf[NROS_PLATFORM_LOG_BUFSZ]` (1280 bytes, line
1597) on the stack of whichever thread raised the record, `snprintf`s
`"<name>: <msg>"` into it, then calls `LOG_ERR/WRN/INF/DBG("%s", buf)`. In
`CONFIG_LOG_MODE_IMMEDIATE` the log core and its backend also run on that
stack.

So every `nros_log` call costs a thread at least 1280 bytes plus `snprintf`
plus the immediate-mode log path. A Zephyr thread sized for its own work
(1-2 KiB is common for a sensor or watchdog thread) overflows the first time
it logs. The record that overflows is raised inside `nros_log`'s dispatch, so
the thread dies holding `nros_log::DISPATCH_GUARD` (issue 2000). After that,
every record from every thread goes to the early ring, fills it, and is
counted lost, and the loss report never runs because it needs the same guard.

## Why this is filed although 2165 closed

Issue 2165 (four records, then silence, on ASI's SMP image) was this
mechanism's first suspect. It does not reproduce at main: on ASI's own image
with its workaround compiled out, `DISPATCH_GUARD`, `early::PENDING` and
`early::LOST` read 0 at 10, 31, 61 and 121 s and every record reached the
console. That run had no small-stack thread logging. The hazard in the code
is unchanged, and nothing reports it when it fires.

## Fix direction

1. **No caller-stack buffer.** Pass name and message to the log macro
   directly (`"%.*s: %.*s"`), or format into a buffer the sink owns. Under the
   deferred log mode Zephyr copies `%s` arguments into the message package by
   reading up to a NUL, and `nros_log`'s message is a length-delimited slice,
   not NUL-terminated. Check what Zephyr 3.7 and 4.4 copy for `%.*s` before
   choosing the first option. A sink-owned buffer must be safe across CPUs on
   SMP and across preemption.
2. **A dead guard holder is loud.** If a thread dies while it holds the
   dispatch guard, say so once on the console (the fatal-error hook can check
   the guard's owner), instead of losing every later record silently.

## Done when

A Zephyr SMP image (`qemu_cortex_a53/smp`) with a thread on a small stack
that logs runs as a gate cell. It shows the records from that thread reaching
the console, and it fails on main's sink (the negative control).
