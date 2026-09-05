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

## Resolution (2026-09-05) — option A

Guarded on BOTH Kconfigs, returning the ABI's documented `0` otherwise.

This also restores consistency the commit had broken rather than inventing a
policy: of the five ports `411addfd2` touched, three already gate
(`INCLUDE_uxTaskGetStackHighWaterMark` on FreeRTOS, `TX_ENABLE_STACK_CHECKING`
on ThreadX, POSIX returns 0 outright). Zephyr was the one left ungated, and it
is the only one of the five whose probe is a SYSCALL — the only kind that can
vanish from the link rather than answer an error.

### The two "not covered" items, answered

* **FreeRTOS has the same shape and already handles it.** Verified in the
  commit's own diff: `#if (INCLUDE_uxTaskGetStackHighWaterMark == 1)` with a `0`
  fallback. The guess in this issue ("probably links unconditionally") was
  right about the mechanism and unnecessary about the risk.
* **No other symbol from `411addfd2` has the property.** The commit adds exactly
  one function per port and nothing else executable; the rest is the header, the
  Rust binding and the cffi mirror.

Left for whoever wants the number, and it is not free: on this tree the probe
now returns `0` on Zephyr, so `stack-headroom-runtime` is INERT there. That is
honest — `0` is the ABI's "not instrumented" and matches POSIX — but it means
the safety argument the probe was written for is not yet supported on Zephyr.
Option B (turning both Kconfigs on) buys a real answer at the cost of stack
painting at boot and per-thread bytes, and it is a decision about the shipped
image.

### What this fix was NOT verified against

No Zephyr SDK is provisioned on the host that made it, so the link was not
reproduced or re-run here. The Kconfig names come from the issue's own reading
of `zephyr/kernel/thread.c:806`, and `git grep` confirms neither is set anywhere
in this repo, so every image in this tree takes the `#else`. A Zephyr fixture
build is the acceptance, and it has not run.

### And the reason it reached main

The compile tiers stayed green throughout — nothing below a Zephyr fixture build
can see a Zephyr link failure. That is the same gap issue 1029 records for the
Zephyr dual-line nightly being skipped on every scheduled run, and it is not
closed by this fix.
