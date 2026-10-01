---
id: 1232
title: "A tier's declared `stack_bytes` does nothing on Zephyr — every tier thread gets the fixed pool slot"
status: resolved
resolved: 2026-10-02
type: bug
area: zephyr
related: [phase-436]
---

## Problem

`stack_bytes` is a contract field. It rides the whole way from `system.toml`
through the resolver, `NativeTierSpecC` and the generated entry into
`nros_zephyr_tier_task_create` — and is then **not used to size the stack**.

```c
k_tid_t tid = k_thread_create(&nros_tier_threads[idx], nros_tier_stacks[idx],
                              NROS_ZEPHYR_TIER_STACK_SIZE, nros_zephyr_tier_trampoline,
                              (void*)entry, arg, NULL, (int)priority, 0, start_delay);
```
(`zephyr/nros_platform_zephyr_shims.c:696`)

The stacks are a compile-time pool:

```c
#ifndef NROS_ZEPHYR_TIER_STACK_SIZE
#define NROS_ZEPHYR_TIER_STACK_SIZE 16384
K_THREAD_STACK_ARRAY_DEFINE(nros_tier_stacks, NROS_ZEPHYR_MAX_TIERS, NROS_ZEPHYR_TIER_STACK_SIZE);
```
(`shims.c:639`)

So **every** tier thread gets `NROS_ZEPHYR_TIER_STACK_SIZE`, whatever the tier
declared. `stack_bytes` is read once, only to decide whether to print a
warning:

```c
if (stack_bytes > (size_t) NROS_ZEPHYR_TIER_STACK_SIZE) {
    printk("nros: tier stack request %u > fixed slot %u tier=`%s` — running with the "
           "slot; raise NROS_ZEPHYR_TIER_STACK_SIZE\n", ...);
}
```

## Why this is worse than a missing feature

A tier that declares **less** than the slot gets silently more, which is
merely wasteful. A tier that declares **more** gets a printk and runs
undersized — and that warning is the only signal, on a lane where a stack
overflow is the classic spatial-freedom-from-interference failure ISO 26262
asks about.

Worse, the number is *believed* elsewhere. Anything deriving from
`stack_bytes` on Zephyr computes against a stack that does not exist. Phase-436
hit this directly: the first version of the stack-headroom derive used
`stack_bytes` and had to be changed to ask `nros_zephyr_tier_stack_size()` for
what the thread actually got. That workaround is in place, but it works
*around* the contract rather than fixing it.

Two neighbouring defects found in the same pass, both fixed there:

* `ctx->stack_bytes` in `zephyr_run_tiers.c` was **never assigned** — the
  field existed and nothing set it.
* The boot tier's `stack_bytes` describes no thread at all: it runs on the
  Zephyr `main()` thread, sized by `CONFIG_MAIN_STACK_SIZE`.

## Contrast: FreeRTOS honours it

```c
uint32_t stack_words = (t->stack_bytes > 0u) ? (uint32_t)(t->stack_bytes / 4u) : (262144u / 4u);
```
(`freertos_run_tiers.c`) — declared size honoured, with a documented default.
So the same contract field means two different things depending on the board,
and only one of them is what it says.

## Fix direction

Three options, roughly increasing cost:

1. **Say so.** Document `stack_bytes` as advisory-on-Zephyr in the contract and
   in `NativeTierSpecC`, and make the oversize case louder than a printk. Cheap
   and honest, but leaves the field lying by default.
2. **Refuse.** Fail the build (or the resolve) when a Zephyr tier declares a
   `stack_bytes` the pool cannot provide, rather than warning at runtime and
   continuing undersized. Matches the repo's "refuse rather than degrade"
   discipline (issue 0709).
3. **Honour it.** Size the pool slots from the resolved tiers at build time —
   the toolchain holds every tier's `stack_bytes` before the image is built, so
   `NROS_ZEPHYR_TIER_STACK_SIZE` could be generated as the max, or per-tier
   stacks emitted. This is the only option under which the contract is true.

(2) is the minimum that stops the field misleading. (3) is what the field
already promises.

## Resolution

Option 3 — **honoured**, by the "entry declares, runner uses" method of issues
1551/1568/1598, not a bigger pool. Branch
`fix/1597-1598-1232-tier-storage-and-stacks`, commits `c4ca24088` (C/C++),
`2fdf1f616` (Rust), `6a7233b6b` (pool no longer linked).

- C/C++ entry: includes `<nros/tier_task_memory_zephyr.h>` and declares each
  spawned tier's `static K_THREAD_STACK_DEFINE(__nros_tier_stack_<i>, bytes)` +
  `struct k_thread __nros_tier_thread_<i>`; `bytes` is the tier's declared
  `stack_bytes`, or CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE when it declares none
  (so that knob keeps its meaning). The row carries `K_THREAD_STACK_SIZEOF`.
- `nros_board_zephyr_run_tiers_tasks_in` spawns through the new
  `nros_zephyr_tier_task_create_static` (entry's thread + stack); the pool and
  it share ONE create/pin/start helper in the shim. Boot tier (`tiers[0]`)
  must have no memory, every spawned tier must — refused before the session
  opens. `_in` keeps the pool for an older entry, and is now the ONLY referrer
  of it, so a generated tiered image drops the pool at link.
- The stack-headroom derive uses the stack the thread was CREATED with (the
  1232 workaround asked for the slot), and every spawned tier prints
  `nros: tier stack tier=… bytes=… kernel=…` (kernel = `stack_info.size`).
- Rust `nros::main!` Zephyr arm: `run_tiers_with_task_memory` with a
  `TierTaskMemory<words, 0>` static per tier that DECLARES `stack_bytes`
  (spawned through `nros_zephyr_tier_task_create_stack`: entry stack, pool
  thread object — Rust cannot size `struct k_thread`); an undeclared tier keeps
  the pool slot, whose size is a Kconfig knob the cargo lane cannot read (issue
  0460). The shim refuses an entry-stack spawn where the image enforces kernel
  stack objects (userspace / MPU guard / HW stack protection).
- The `NROS_ZEPHYR_MAX_TIERS >= 1` guard (issue 1131) is untouched: the pool
  still exists for the `_in`/`_ns` road and the Rust road's undeclared tiers.

### Measured — native_sim/native/64, realtime-c `demo_bringup:zephyr`

With `[tiers.high.zephyr] stack_bytes = 24576` declared (temporarily, for the
measurement; the committed bringup is unchanged) and
`CONFIG_THREAD_STACK_INFO=y`:

| | before | after |
| --- | ---: | ---: |
| spawned tier `high`'s stack | 16,384 (pool slot, declared size ignored) | `nros: tier stack tier=\`high\` bytes=24576 kernel=24576` |
| `nm`: `__nros_tier_stack_1` / `__nros_tier_thread_1` | absent | 0x6000 / 0xb8 |
| `nm`: `nros_tier_stacks` / `nros_tier_threads` | 0x10000 / 0x2e0 | **absent** (gc'd) |

Both tiers ran against `rmw_zenohd` (2,179 `[ctrl]`, 218 `[telem]` lines in
25 s). The headroom bound now derives from 24,576 (`a 3072 B minimum is set`).

### Not measured

- `realtime_tiers_e2e` zephyr cells and the derived-tier lane were not run
  through the harness (fixture stamps not built here; issue 1597's note). The
  image above is the `demo_bringup:zephyr` row built with `nros build` into a
  private build dir and run by hand.
- The Rust Zephyr road (`nros_zephyr_tier_task_create_stack`) was compiled
  (macro + board crate) but no Rust Zephyr image was built or booted.
- C++ Zephyr and the SMP (`qemu_cortex_a53`) row were not built.
