---
id: 1010
title: "The derived executor arena is one allocation ~6x larger than the heap it
  comes from, so every zephyr XRCE example dies at boot"
status: open
type: bug
area: zephyr, platform, examples
severity: high
found: 2026-09-03
related: [issue-0968, issue-1003]
---

## Measured

All nine `example_e2e` zephyr XRCE cells, on freshly built images, all three
languages:

```
Summary [290.668s] 9 tests run: 0 passed, 9 failed, 45 skipped
```

Zero skips — every one of the nine actually ran. 72 copies of one message:

```
nros: HEAP EXHAUSTED: request 329648 bytes, arena 66048 bytes
      (raise CONFIG_NROS_ZEPHYR_HEAP_SIZE / NROS_ZEPHYR_HEAP_SIZE)
```

By workload: pubsub 329648, service/action 423808 and 427952. Arena 66048
throughout (65536 + 512 of allocator overhead).

## Why it can never work

From the built images' own `autoconf.h` (`build-rust-talker-xrce`):

```
#define CONFIG_NROS_ZEPHYR_HEAP_SIZE 65536
#define CONFIG_NROS_EXECUTOR_ARENA_SIZE 0
```

`0` means derive, and `zephyr/Kconfig` says what the derive does:

> budgets EVERY callback slot at action-client size (~18 KiB) -- correct, but
> an order of magnitude over for a pub/sub-only image

So the executor arena derives to ~322-418 KiB and is requested as a **single**
allocation (`nros_platform_alloc` -> `nros_zephyr_heap_alloc`, one call, the
size printed above) from an arena of 64 KiB. This is not fragmentation, not a
leak, and not load-dependent: one block strictly larger than the whole arena
fails 100% of the time, on every host, forever.

Both knobs are individually defensible and jointly impossible:

* `NROS_ZEPHYR_HEAP_SIZE` default 65536 — a sane default for a small image.
* `NROS_EXECUTOR_ARENA_SIZE` default 0 — a deliberately conservative derive.

**Nothing compares them**, and no zephyr XRCE example sets either. The only
mention across the whole example tree is a COMMENT in
`examples/zephyr/c/talker/prj-xrce.conf` explaining the knob exists.

## What this does and does not close

This is the boot failure for the whole zephyr XRCE cluster in issue 0968 — the
`action` sub-signature there (`Transport(BadAlloc)`, `rc=-6`, kernel panic) is
this allocation failing and the caller reacting to NULL.

**It is not yet proven that fixing it makes the tests pass.** Raising the heap
and re-running is the next step; other failures may sit behind this one. The
established fact is narrower and stronger: these images cannot boot as
configured, so no runtime result from them has ever been evidence about
anything else.

That also means 0968's earlier `pubsub`/`service` "boots, then no delivery"
reading needs re-examination against this, rather than being carried forward.

## Suggested shape of a fix

Sizing the examples is the immediate unblock, but the durable fix is the
comparison nothing does today: the derived executor arena is known at BUILD
time and the heap arena is a Kconfig integer, so an image whose arena cannot
hold its executor should fail to LINK, not at boot. `NROS_EXECUTOR_ARENA_SIZE`'s
own help already warns "too small fails at runtime, not at link" about the
other direction; this is the same complaint one level up.

## Acceptance

* [ ] A zephyr XRCE image's derived executor arena fits its heap arena, or the
      build fails naming both numbers.
* [ ] The nine `example_e2e` XRCE cells boot.
* [ ] 0968's zephyr section is re-measured against images that boot.
