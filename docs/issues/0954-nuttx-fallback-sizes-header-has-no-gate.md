---
id: 954
title: "The committed NuttX fallback sizes header is a hand-maintained twin with no gate, and went stale again"
status: open
type: bug
area: api-c, boards
related: [issue-0196, issue-0245, issue-0268, issue-0899, issue-0924]
---

## What went wrong this time

`packages/api/nros-c/include/nros/nros_config_generated_nuttx.h` is the Phase 159
"Path C" fallback: NuttX cannot run the host size probe, so the sizes are baked
once, by hand, and the file must be an UPPER BOUND over every per-build value.

`_z_session_t` grew eight bytes when it gained `_mutex_transport` and
`_reconnecting` ([[issue-0899]], [[issue-0924]]). Freshly built per-build headers
now read:

    #define NROS_SESSION_SIZE 536

The committed fallback still read:

    #define NROS_SESSION_SIZE 528
    #define SESSION_OPAQUE_U64S 66      /* 66 * 8 = 528 */

So `uint64_t _opaque[SESSION_OPAQUE_U64S]` was **eight bytes shorter than the
struct it stores** on every NuttX image. The file's own static assert
(`SESSION_OPAQUE_U64S * 8 >= NROS_SESSION_SIZE`) still passed, because both
halves were stale together — it checks the two numbers against each other, never
against reality.

Raised to 536 / 67 in the same commit as this issue.

## This is the third time, and the file says so

Its own history is the pattern:

* **#167** — `NROS_EXECUTOR_STORAGE_SIZE` was 79296, "stale"; raised to 98304.
* **#464** — the same #167 edit "MISSED" `EXECUTOR_OPAQUE_U64S`, the value that
  actually sizes the array, leaving it contradicting its own macro by 19008
  bytes. Also `ACTION_SERVER_OPAQUE_U64S` was 786 "below a real per-build value
  (a host probe of the same type measures 799)".
* **now** — `NROS_SESSION_SIZE` / `SESSION_OPAQUE_U64S`.

Every one was found after it had already failed, or by reading the file. This is
the sizes-header mirror class CLAUDE.md tracks through
0088 -> 0114 -> 0122 -> 0123 -> 0245 -> 0268: a value with two spellings and no
gate between them.

## Why the existing guards do not cover it

* `nros-build-helpers/src/shared.rs` compares two PER-BUILD writers (the C and
  C++ halves of one build) and panics when they disagree. It caught a real
  mismatch during this same work — but both writers are generated, so a stale
  COMMITTED file is invisible to it.
* The fallback's own `NROS__NUTTX_FALLBACK_ASSERT`s are internally consistent
  only: `OPAQUE_U64S * 8 >= SIZE` holds fine when both are equally wrong.
* Nothing in `scripts/` or `just/` reads this file. `grep -rl
  nros_config_generated_nuttx scripts/ just/` returns one hit, a comment in
  `just/nuttx.just` explaining the fallback exists.

## Direction

The check wants to be: for every macro in the fallback, `fallback >= the largest
value any per-build header emits`. The awkward part is where the per-build
numbers come from — they are the output of `nros-sizes-build`'s rlib probe,
which needs a build.

Two shapes worth weighing:

1. **Probe-backed gate.** Run the same host probe the build runs (it already
   caches under `build/sizes-probe/`), and compare. Accurate, and it works on a
   clean checkout. Costs a probe build in the fast lane, which may be too slow —
   measure before committing to it.
2. **Opportunistic comparison.** Scan any per-build `nros_config_generated.h`
   present in the tree and assert the fallback dominates. Free, but VACUOUS on a
   clean checkout — the shape `check-no-vacuous-tests` exists to reject, so it
   would need to fail loudly when it finds nothing to compare rather than pass.

Whichever lands, the rule to encode is the one the file's header already states
in prose and nothing enforces: **this file is an upper bound over all per-build
values.**

## Acceptance

* Growing a struct behind one of these macros fails a gate, naming the macro and
  both numbers, rather than being found by reading the header later.
* The gate cannot pass by finding nothing to compare.
