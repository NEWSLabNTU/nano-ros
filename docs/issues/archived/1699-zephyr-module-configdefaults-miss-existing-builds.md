---
id: 1699
title: "The Zephyr module's `configdefault` values reach only FRESH configures — an existing build dir keeps the old value"
status: resolved
type: bug
area: [zephyr, build]
severity: medium
found: 2026-10-05
related: [1674]
---

## Summary

Split from issue 1674 ([archived](1674-zephyr-native-sim-cyclonedds-delivers-nothing-and-floods-select-failed.md)), which measured the mechanism. A
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

## Resolution (2026-10-06)

The first option: a module Kconfig edit is now a configure input for existing
build dirs.

- Zephyr's regenerate decision is a checksum over the conf fragments, and that
  list includes every `*.conf` in the application build dir.
- `zephyr/CMakeLists.txt` writes `<build>/nros-module-kconfig.conf` there. Its
  only content is a comment with the SHA-256 of `zephyr/Kconfig`, so it assigns
  nothing.
- The module runs after Kconfig, so the pass that sees a new digest has already
  used the stale `.config`.
- `nros_reconfigure_on_change` (issue 0991's lever) future-dates the file, so
  ninja re-runs cmake inside the same build. That pass's checksum differs, and
  `.config` is regenerated from the fragments.
- A clean build dir writes the file without arming the re-run.

Measured on a scratch `native_sim/native/64` `c/talker` Cyclone build, with
`NET_TCP_WORKQ_STACK_SIZE` (a `configdefault` that no fragment sets):

- **No-op incremental `ninja`:** 0 configures.
- **Default edited 4096 → 4160:** one `ninja` ran 2 configures and
  `.config` read 4160.
- **Control,** the same edit (4160 → 4224) with the module change
  reverted: 1 configure, and `.config` kept 4160.

The rule is recorded in the Kconfig comment above the `configdefault` block.
A menuconfig edit in such a build dir is reset after a module Kconfig edit,
exactly as any conf-fragment edit resets it.
