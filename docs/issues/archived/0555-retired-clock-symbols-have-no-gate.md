---
id: 555
title: "The retired `nros_platform_clock_{ms,us}` had no gate, so one rename broke four consumers in a row"
status: resolved
resolved_in: issue-0555
type: tech-debt
area: platform
related: [issue-0548, issue-0547, issue-0541, rfc-0073, phase-350]
---

## Why

RFC-0073 / phase-350 (`bde6638ed`) replaced `nros_platform_clock_{ms,us}` with
`nros_platform_clock_ns()` plus `static inline` wrappers in
`packages/platform/nros-platform-api/include/nros/platform.h`. No port defines
those symbols any more — **the wrappers are the definition**, and a caller gets
them by including the header.

That makes the header a load-bearing dependency of anyone who says the name, and
nothing checked it. The rename then broke four consumers in a row, each visible
only after the previous cleared, because the zephyr lane aborts at the first
failure:

| | consumer | shape |
| --- | --- | --- |
| #541 | a committed `nros_generated.h` | stale copy |
| — | upstream `5dc2fa869` | stale copy, second one |
| #547 | Cyclone `internal.hpp` | hand-declared the ABI in three `extern "C"` blocks — compiled, then `undefined reference` at LINK |
| #548 | the XRCE C shim | same family, five undefined refs, took the whole tier-2 fixture build down |

#548 says outright that this "should probably become a gate given this is the
second consumer the rename missed."

## Fix

`scripts/check-retired-platform-clock-symbols.py`, in `check-fast`. Three arms,
each matching a shape that actually shipped:

1. a code reference to a retired name in a TU that does not include
   `nros/platform.h` — the wrapper is not in scope, so it is an undefined
   reference at link (#548's stated symptom);
2. a hand-written declaration of a retired name outside the defining header — an
   `extern` copy cannot out-rank a `static inline`; it only lets the file compile
   and then fail at link (#547);
3. more than one TRACKED file defining the wrappers (#541, `5dc2fa869`).

Comments are stripped before matching, which is not cosmetic: two files mention
the retired names ONLY in prose saying they are retired
(`nros-board-threadx-qemu-riscv64/startup.c`,
`zephyr/nros_platform_zephyr_shims.c`), and a naive grep reports both. A gate
that cries wolf on its own documentation gets bypassed.

Adding another retired symbol is one entry in `RETIRED`.

## What it does NOT cover — measured, not assumed

Replaying the actual pre-fix sources through the finished gate:

```
#547  internal.hpp        -> 3 hits (lines 27, 33).   CAUGHT
#548  platform_aliases.c  -> CLEAN.                   NOT caught
```

So the gate does not catch the issue that asked for it. #548's file included
`nros/platform.h` and declared nothing; what failed was the include RESOLVING to
a stale copy on Zephyr's own include path — a property of the build's `-I`
order, not of any source text. No source-scanning gate can see that. Arm 3
covers it only when the second copy is TRACKED, which #548's was not.

Recorded rather than papered over, because "we added a gate" reads as "the class
is closed" and here it is not. The remaining shape is build-side, and its rule
already exists: issue 0196 — a stale probe must watch the same inputs the
consumer actually reads.

## Acceptance

* `python3 scripts/check-retired-platform-clock-symbols.py` → OK over 576
  tracked C/C++ sources; in `check-fast`.
* Self-tests both directions for all three arms, including that a call is not a
  declaration and a caller is not a definer. The declaration arm's first draft
  reported `return nros_platform_clock_ms();` as a declaration — its own
  self-test caught that before the gate ran on the tree.
* Verified against real history: #547's `internal.hpp` is reported at the right
  lines; the current tree is clean.
