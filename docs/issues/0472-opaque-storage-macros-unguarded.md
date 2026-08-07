---
id: 472
title: "Thirteen of fifteen opaque-storage macros have no compile-time size check, so a wrong size is a short buffer rather than a build error"
status: open
type: bug
area: build
related: [issue-0464, issue-0360, phase-340]
---

## The gap

C and C++ callers allocate opaque byte arrays for Rust types, sized by macros
the build derives from `size_of::<T>()`. If the derived number is smaller than
the real type, the caller writes past its buffer.

Exactly **two** of the fifteen carry a compile-time assertion:

```
guarded:    EXECUTOR_OPAQUE_U64S, CPP_EXECUTOR_OPAQUE_U64S
unguarded:  ACTION_CLIENT, ACTION_SERVER, CPP_ACTION_CLIENT, CPP_ACTION_SERVER,
            CPP_GUARD_HANDLE, GUARD_HANDLE, NROS_CPP_RAW_ACTION_SERVER,
            NROS_LIFECYCLE_CTX, PUBLISHER, SERVICE_CLIENT, SERVICE_SERVER,
            SESSION, SUBSCRIPTION
```

The guarded form already exists and is the model — `packages/api/nros-c/src/executor.rs`:

```rust
const _: () = assert!(
    core::mem::size_of::<nros_node::ExecutorInlineStorage>()
        <= EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "EXECUTOR_OPAQUE_U64S too small for Executor + backing — increase \
     NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_MAX_CBS, or adjust the overhead in build.rs"
);
```

## Why it matters independently of issue 0464

0464 removed the two fallbacks that were *a* source of wrong sizes — a poll that
could return another consumer's rlib, and a table of committed constants that had
rotted ~11 % low. That work is done and verified.

**This issue is the other half, and it outlives that fix.** The guards are what
make a wrong size *fail* rather than *corrupt*, whatever produces it: a future
fallback, a feature-set mismatch between header and archive, a probe that reads
the wrong variant, a hand-edited constant. Without them the failure mode is a
short buffer at runtime, in C, at a distance from the cause.

The one guard that exists has already earned its keep — 0464 records it as the
mechanism that caught the rotted NuttX constant.

## A related unenforced path

`nros-cpp` emits, when the probe returns zero:

> `EXECUTOR_SIZE probe returned 0 — likely a cargo check --no-default-features
> run. The emitted CPP_EXECUTOR_OPAQUE_U64S will be 1; do not link the resulting
> rlib.`

The accommodation is legitimate: a `cargo check` run genuinely has no rlib to
probe, so hard-failing would break `just check`. What is missing is enforcement.
"Do not link" lives only in a build-script warning, and `1` is the most
under-sized value the macro can take. Nothing stops that rlib being linked.

Issue 0360 already established the mechanism this wants: a symbol whose name
encodes the variant, so a header/archive mismatch is an undefined reference
*naming what it wanted* instead of a silent `_opaque` overflow. The same shape
would turn "probed zero" into a link error.

## Fix shape

1. Give every opaque macro the assertion the executor has. Mechanical, and it
   should be generated rather than hand-written fifteen times — the list is
   already known to the build script that emits the macros.
2. Make "probed zero" poison the artifact at link time rather than warn, reusing
   0360's variant-symbol mechanism.
3. Gate it: a check that fails when an opaque macro exists without a
   corresponding assertion, so the next macro added does not silently join the
   unguarded thirteen.

Item 3 is what keeps this fixed. The class here is CLAUDE.md's "fix the CLASS,
not the reported site" — the thirteen are unguarded because each was added
without the guard, one at a time.

## Provenance

Found 2026-08-06 while removing the size probe's fallbacks (issue 0464), by
checking whether the "the const assertion catches it" reassurance in the probe's
own comments was true. It is true for the executor and for nothing else.
