---
id: 1232
title: "A tier's declared `stack_bytes` does nothing on Zephyr — every tier thread gets the fixed pool slot"
status: open
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
