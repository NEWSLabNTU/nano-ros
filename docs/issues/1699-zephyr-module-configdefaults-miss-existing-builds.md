---
id: 1699
title: "The Zephyr module's `configdefault` values reach only FRESH configures — an existing build dir keeps the old value"
status: open
type: bug
area: [zephyr, build]
severity: medium
found: 2026-10-05
related: [1674]
---

## Summary

Split from issue 1674 ([archived](archived/1674-zephyr-native-sim-cyclonedds-delivers-nothing-and-floods-select-failed.md)), which measured the mechanism. A
`configdefault` added to `zephyr/Kconfig` changed `NET_SOCKETS_POLL_MAX` to 8
in a fresh configure, while every existing build dir kept 3. Zephyr's Kconfig
treats the existing `.config` value as a user choice and keeps it. A
conf-fragment change triggers a regenerate; a module Kconfig default does not.
1674 therefore also set the value in all 22 `prj-cyclonedds.conf` files and
added an `#error` guard in `session.cpp`.

`zephyr/Kconfig` has 26 other `configdefault` lines. They cover 14 symbols:
`COMMON_LIBC_MALLOC_ARENA_SIZE`, `DYNAMIC_THREAD_STACK_SIZE`,
`HEAP_MEM_POOL_SIZE`, `MAIN_STACK_SIZE`, `MAX_PTHREAD_{COND,MUTEX}_COUNT`,
`NET_BUF_{RX,TX}_COUNT`, `NET_PKT_{RX,TX}_COUNT`, `NET_SOCKETS_POLL_MAX`,
`NET_TCP_WORKQ_STACK_SIZE`, `POSIX_THREAD_THREADS_MAX` and
`SYSTEM_WORKQUEUE_STACK_SIZE`. Every one visible in a Cyclone `.config` is
also set in the per-example conf, so nothing shows any of them reaching an
incremental build. By 1674's mechanism they would not, so raising one of them
changes fresh builds only, and an existing dir shows no change.

## Fix direction

Either make a module Kconfig edit a configure input for existing build dirs, or
rule that a value an image depends on lives in a conf fragment and the
`configdefault` is only a default for new users, then gate that rule. Decide
which, and record it in the Kconfig comment beside the block.

## Acceptance

* Changing one of these `configdefault` values and running an incremental build
  of an existing dir either picks up the new value, or the rule above is
  documented and gated.
