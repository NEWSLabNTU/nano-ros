---
id: 2227
title: "`just nuttx build-fixtures` re-stales its own arm fixtures: the riscv half rewrites the shared tree's `include/nuttx/config.h`"
status: open
type: bug
severity: medium
area: [build, nuttx, testing]
related: [0433, 0525, 1007]
found: 2026-11-03
---

## What

`just nuttx build-fixtures` builds arm, then riscv. Both reconfigure the one
checkout at `third-party/nuttx/nuttx`, and the riscv configure rewrites
`include/nuttx/config.h`. Every arm Rust fixture built before it then resolves
as STALE:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: build/cargo-fixtures/nuttx/armv7a-nuttx-eabihf/nros-minsizerel/talker
  newer:  third-party/nuttx/nuttx/include/nuttx/config.h
```

Timestamps from one family build: talker 16:29:22, `config.h` 16:46:21. The
staleness probe exempts one regenerated-in-place header; `config.h` is not it.

## Measured

After `just nuttx build-fixtures` (rc=0), six NuttX arm tests in the issue 2221
run failed STALE (`test_rtos_{pubsub,service,action}_e2e` Rust, `quiet_link`,
`nuttx_arm_carves_the_parameter_store_and_delivers`). After
`just nuttx build-fixtures-arm` alone, the same NuttX lanes ran 17 of 17 green.

So the family build cannot produce a fresh arm set. This is the
"last configured wins" class of issues 0433/0525 (both archived), reached by a
newer path: the riscv half of the same recipe.

## Fix direction

Either the freshness probe compares `config.h` by the configuration that
produced the fixture (the arm defconfig), not by mtime, or the two arches stop
sharing one configured tree (per-arch out-of-tree config, as issue 1007 did for
the arch export).
