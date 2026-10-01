---
id: 1597
title: "The hosted C/C++ tier runner (`nros_board_native_run_tiers{,_ns}`) still takes each tier's executor storage from the host heap"
status: open
type: tech-debt
area: [memory, native, cpp]
severity: low
found: 2026-10-01
related: [issue-1568, issue-1571, issue-1551]
---

## What is left

Issues 1568 and 1571 moved every RTOS executor (tiered and single, C, C++
and Rust) onto storage the generated entry owns: a `.bss` static sized from
the build, checked by `nros_cpp_executor_storage_check` (C/C++) or
`check_tier_executor_backing` (Rust).

The one runner left out is the HOSTED C/C++ tier runner,
`nros_board_native_run_tiers{,_ns}` in `packages/api/nros-cpp/src/lib.rs`.
It still allocates each tier's executor storage on the host heap. Issue
1571's resolution names it as untouched.

On a host this costs no correctness: the heap is large and nothing prices
it. It does make native the one family where "every executor is allocated one
way" is false. And native is where most C/C++ cells run, so a storage-sizing
regression in the shared path is first seen on a runner that does not use the
shared method.

## Direction

Give the native runner the RTOS shape:

- an `_in(…, storage, stride)` runner;
- `CAbiRunners::takes_executor_storage = true` for native in
  `nros-entry-lower`;
- the generated C/C++ entry emits `__nros_tier_executor_storage` for native
  too;
- the same library refusal.

Keep `_ns` for previously generated entries, as every RTOS runner does.

## Acceptance

- Native C and C++ tiered images name `__nros_tier_executor_storage` in
  `nm` / `mem-report`, and no tier executor is heap-allocated (an allocation
  trace or a counting allocator shows it).
- `realtime_tiers` native/c and native/cpp still pass, and a short stride is
  refused.
- The `takes_executor_storage` test covers every family with no exemptions.
