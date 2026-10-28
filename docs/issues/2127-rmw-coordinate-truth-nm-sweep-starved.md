---
id: 2127
title: "`a_rows_rmw_is_the_backend_its_artifact_linked` ran its 8-thread `nm` sweep counted as one nextest thread, and a loaded runner left 16 rows unread"
status: open
type: bug
area: [testing, ci]
severity: low
found: 2026-10-29
related: [phase-466, 1984, 1690]
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-29.

## Measured

`run-matrix` tier 2, run 40643270547 (2026-10-28, runner 118):

```
16 row(s) got no `nm` verdict within budget (located 93 binaries in 2.6s,
`nm` over them in 26.4s; slowest: workspace-cpp-native-service-server 9.0s,
workspace-cpp-native-service-client 8.9s, workspace-c-nuttx-riscv-realtime 8.8s, ...)
```

On this host, idle, `nm` over the same `linux_service_server_entry` (14 MB,
8 933 symbols) takes 0.04 s. So the 9 s in CI is contention, not the binary.
This is the failure issue 1984 designed for: the test now names its slow rows
instead of timing out anonymously. Issue 1984 also ruled against raising the
budget, and this issue keeps that ruling.

## Cause

The test runs `nm` on its own pool of `POOL_CAP` = 8 threads. nextest counted
it as one thread and scheduled a full set of other tests beside it, so the
pool competed with them for the CPU.

## Fix

`.config/nextest.toml` gives this test `threads-required = 8`, which is what it
uses. nextest then leaves room for the pool instead of oversubscribing the
host. The budgets are unchanged.

## What would close it

A `run-matrix` tier-2 run on runner 118 where this test passes. The line it
prints says how long the sweep took; it was 5.6-6.9 s on the passing runs
before 2026-10-28.
