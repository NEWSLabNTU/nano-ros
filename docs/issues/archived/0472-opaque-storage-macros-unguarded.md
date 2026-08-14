---
id: 472
title: "Thirteen of fifteen opaque-storage macros have no compile-time size check, so a wrong size is a short buffer rather than a build error"
status: resolved
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

## FIXED 2026-08-15 — items 1 and 3; item 2 deliberately left

### What the guards actually compare (the naive version is a no-op)

The Rust constants in `opaque_sizes.rs` are already `u64s_for::<T>()` — derived
from `size_of::<T>()` directly. Asserting *those* against `size_of::<T>()` is
tautologically true and buys nothing, which is the trap in "give every macro the
assertion the executor has".

The executor's guard is meaningful for a different reason: its value comes from
`crate::config`, i.e. from PROBING a compiled rlib in `nros-build-helpers::c`.
The header hands C a probe-derived width; the runtime writes a `size_of`-sized
value into it. **Two derivations of one fact** — and the guard compares them.

So the fix plumbs the probe-derived widths into the Rust config (nine more
alongside the executor's) and asserts each type against *the number the header
states*:

```rust
guard_opaque!(crate::config::PROBE_SESSION_U64S, nros::internals::RmwSession,
              "SESSION_OPAQUE_U64S");
```

* `<=`, not `==`: over-sizing wastes bytes and is safe; under-sizing is the bug,
  and the probe's `max(8)` floor makes equality wrong for small types anyway.
* Skipped when the stated value is `0` — "no rlib to probe", the
  `cargo check --no-default-features` case this issue records as a legitimate
  accommodation. That is item 2's territory, below.
* The Rust config is now emitted AFTER the probes are read. It used to be
  written before any of them existed, which is the mechanical reason only the
  executor could be guarded.

Verified by tripwire, not by inspection: forcing `SESSION` to be stated as 1 u64
fails the build with

```
error[E0080]: evaluation panicked: SESSION_OPAQUE_U64S: the generated header
states a SMALLER opaque size than the Rust type needs, so a C caller's `_opaque`
buffer would be written past. … see issue 0472.
```

### Item 3 — the gate, and the vacuous first version of it

`scripts/check-opaque-storage-guards.py`, in `check-fast`: every
`*_OPAQUE_U64S` emitted into a generated header must be named by a guard.
15 macros emitted, all guarded.

**Its first version was false coverage and its own tripwire caught it.** It
asked "does this macro name appear in a guard-site file?" — but every macro is
also DEFINED in `opaque_sizes.rs` as a `pub const`, so the answer was always yes
and the gate passed with a guard deliberately removed. It now matches guard
CONSTRUCTS (`guard_opaque!` invocations and `assert!` bodies), and removing a
guard fails while naming the macro. A gate that only ever passes is the same
defect as the missing assertions it polices.

### Corrections to this issue's own contents

* **The list of fifteen was stale.** `CPP_ACTION_CLIENT`, `CPP_ACTION_SERVER`
  and `CPP_GUARD_HANDLE` no longer exist — `nros-cpp/src/lib.rs` records them
  removed in phase 87.11 and 87.6. The live set is ten C macros plus five
  `NROS_CPP_RAW_*`, which is still fifteen, but not the same fifteen.
* **The five `NROS_CPP_RAW_*` are the same fact as their C counterparts** — same
  probe of the same `nros::sizes::RAW_*_SIZE`, same Rust types — so the gate
  treats a guarded C-side counterpart as covering them, with the mapping written
  out. A DIVERGENCE between the two crates' probes is a real hazard and a
  different one: that is issue 0360's variant-symbol territory.

### Item 2 NOT done

"Probed zero" still warns rather than poisoning the artifact at link time. The
guards skip that case rather than failing it, for the reason this issue already
gives: hard-failing would break `just check`, which legitimately has no rlib to
probe. Turning it into a link error needs 0360's variant-symbol mechanism and is
a separate change; the accommodation remains unenforced. Split out as issue
**0578** rather than left inside a resolved issue, because the guards make the
WRONG-size path safe and leave the ABSENT-probe path exactly as it was.

## Provenance

Found 2026-08-06 while removing the size probe's fallbacks (issue 0464), by
checking whether the "the const assertion catches it" reassurance in the probe's
own comments was true. It is true for the executor and for nothing else.
