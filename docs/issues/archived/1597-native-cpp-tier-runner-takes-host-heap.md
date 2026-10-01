---
id: 1597
title: "The hosted C/C++ tier runner (`nros_board_native_run_tiers{,_ns}`) still takes each tier's executor storage from the host heap"
status: resolved
resolved: 2026-10-02
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

## Resolution

Resolved on `fix/1597-1598-1232-tier-storage-and-stacks` (commit `347e052c8`).

**A correction to the premise first.** The runner did not take the executor
storage from the HEAP: the boot tier's `CppContext` was a `MaybeUninit` on the
calling thread's stack, and each spawned tier's one on its own task stack
(`size_of`-exact, as issue 1568's table says). It was still the one family not
on the shared road, so the fix is the same.

- `nros_board_native_run_tiers_in(…, storage, stride)` and
  `nros_board_native_run_components_named_in(…, setup, storage, bytes)`
  (nros-cpp), both refusing through `nros_cpp_executor_storage_check` before a
  session opens. `_ns` kept for older entry TUs; it hands the runner ONE heap
  block at the library's size, like every RTOS `_ns`.
- `<nros/main.h>` declarations, `LinuxBoard::run_tiers(…, storage, stride)`,
  cbindgen exclusion for the tier twin (it names `NativeTierSpecC`).
- `nros-entry-lower`: native `takes_executor_storage = true`; the test is now
  `every_runner_takes_its_executor_storage_from_the_entry`, with no exemptions.
- C pack: both host calls pass the storage. Goldens: 10 native goldens changed,
  every line read.

### Measured

| | before | after |
| --- | --- | --- |
| `nm native_entry` (realtime-c, native) | no storage symbol | `__nros_tier_executor_storage` 0x2c2f0 = 181,232 B (2 × 90,616) |
| same, realtime-cpp | — | `__nros_tier_executor_storage` 181,232 B |
| allocations ≥ 80 KB during a 6 s run (`ltrace -f -e malloc+calloc+realloc`) | — | **0** (C and C++) |

Both images run both tiers against a local `rmw_zenohd` (C: 298 `[ctrl]` / 54
`[telem]` lines; C++: 242 / 46). Unit tests: a stride, and a component block,
one word short are refused with `INVALID_ARGUMENT` and `setup` never runs
(`nros-cpp` `executor_storage_check_tests::native_runners`).

### Not measured

- `realtime_tiers_e2e` itself was not run green: the lane needs the fixture
  STAMP that `just native build-workspace-fixtures` writes, and that recipe's
  `_codegen` step (regenerating every example's bindings) did not finish in
  this session's budget on the loaded host. The images were built with
  `nros build` on the fixture rows' own images and run by hand instead.
- No "before" ltrace: the old path allocated no executor on the heap either
  (see the correction above), so there was nothing for it to show.
