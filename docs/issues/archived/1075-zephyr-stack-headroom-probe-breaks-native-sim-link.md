---
id: 1075
title: "The Zephyr stack-headroom probe calls a syscall that is not COMPILED unless two Kconfigs are set, so every native_sim image fails to link"
status: resolved
type: bug
area: platform, build
severity: high
found: 2026-09-05
related: [0589, 0876]
---

# A fallback that only works at run time, for a failure that happens at link time

## FIXED 2026-09-05 — option A, guarded on BOTH Kconfigs

`nros_platform_task_stack_unused_bytes` is now compiled two ways: the real probe
under `#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)`, and
a `return 0` otherwise — the ABI's documented "this port does not instrument it",
the same answer `nros_platform_heap_used_bytes` gives.

Both symbols, not one: guarding on `CONFIG_INIT_STACKS` alone reproduces the bug
wherever only that one is set, which is exactly the mistake the original comment
made. The old comment is kept in the replacement, quoted, with why it was wrong —
someone will read this function again and the trap is worth leaving visible.

**Verified by rebuilding the fixture that failed**, not by a compile-tier check
(which passed before the fix and would pass after it):

    just zephyr build-one cpp/service-client xrce

    before:  undefined reference to `z_impl_k_thread_stack_space_get'
             ninja: build stopped: subcommand failed.
    after:   [148/148] Running utility command for native_runner_executable
             Built: …/zephyr.elf   →   zephyr.exe, 11.8 MB, exit 0
             undefined references: 0

The two `ld: error in …(.eh_frame); no .eh_frame_hdr table will be created`
lines survive and are unrelated — they were present before the fatal error too.

**Not done:** turning the two Kconfigs ON. That would make the probe return a
real number instead of `0`, at the cost of stack painting at every thread start.
It is a decision about the shipped image, and it belongs with whoever needs the
safety argument the probe was written for — see option B below.

## Symptom

Every Zephyr `native_sim` fixture fails at the final link, e.g.
`build-cpp-service-client-xrce`:

```
/usr/bin/ld: …/zephyr.elf.loc_cpusw.o:
  …/include/generated/zephyr/syscalls/kernel.h:101:
  undefined reference to `z_impl_k_thread_stack_space_get'
collect2: error: ld returned 1 exit status
ninja: build stopped: subcommand failed.
```

Found by a tier-2 fixture build on 2026-09-05. The Zephyr lane is the one that
breaks; the compile tiers are green, which is why it reached main.

## Cause

`411addfd2` (*"feat(platform): a portable stack-headroom probe"*) added
`packages/platform/nros-platform-zephyr/src/platform.c:1225`:

```c
size_t nros_platform_task_stack_unused_bytes(void) {
    size_t unused = 0;
    if (k_thread_stack_space_get(k_current_get(), &unused) != 0) {
        return 0;
    }
    return unused;
}
```

Its own comment states the fallback:

> Needs CONFIG_INIT_STACKS to have painted the stack; without it the kernel has
> no watermark to read and **returns an error, which becomes the documented 0**.

**That reasoning is about run time, and the failure is at link time.** In the
Zephyr we build against, `zephyr/kernel/thread.c:806`:

```c
#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)
```

encloses `z_impl_k_thread_stack_space_get` (`:862`). Without both symbols the
implementation **is not compiled at all**, so there is no function to return an
error — the reference is simply undefined and `ld` fails.

Two things are wrong with the comment, and the second is the one that bites:

1. it names only `CONFIG_INIT_STACKS`; `CONFIG_THREAD_STACK_INFO` is equally
   required;
2. it assumes the function exists and answers. It does not exist.

Neither Kconfig is set anywhere in this repo — `git grep CONFIG_THREAD_STACK_INFO`
over `zephyr/`, `cmake/` and `examples/` returns nothing.

## The class

This is the shape issue 0589 records for `std::println!` on native_sim: a call
that is *armed in every image* and only bites when something finally reaches it.
Here it is stricter — the link always reaches it, so every Zephyr image is
affected, not just the ones that call the probe.

It is also the "a diagnostic makes the failure worse" pattern: the probe exists
to support a safety argument about stack headroom, and its effect today is that
no Zephyr image builds at all.

## Options

**A. Guard the call.** `#if defined(CONFIG_INIT_STACKS) && defined(CONFIG_THREAD_STACK_INFO)`
around the body, returning the documented `0` otherwise. Smallest, matches what
the comment already believes, and keeps the ABI's "0 means not instrumented"
contract. The guard must name BOTH symbols — guarding on `CONFIG_INIT_STACKS`
alone reproduces the bug wherever only that one is set.

**B. Turn both Kconfigs on** for the nano-ros Zephyr configs. Gives a real
answer instead of `0`, at the cost of stack painting on every thread — which is
exactly the runtime cost the probe's consumers may or may not want, and a
decision about the shipped image rather than about this bug.

**C. Both** — A as the correctness fix, B as a separate, measured decision for
the lanes that want the number.

**Recommendation: A now**, because main is red for every Zephyr build; B if the
safety argument the probe was written for actually needs a non-zero answer.

## Not covered

* Whether the FreeRTOS side (`uxTaskGetStackHighWaterMark`) has the same shape.
  That one is a plain function rather than a syscall, so it probably links
  unconditionally — unverified.
* Whether any other symbol introduced by `411addfd2` has the same
  compiled-out-under-Kconfig property.
* The tier-1 lanes stayed green throughout, so nothing in the compile tiers
  would have caught this — only a Zephyr fixture build does.
