---
id: 1223
title: "`check-cpp-freestanding-includes` scores every `#elif` and `#else` arm as
  guarded, so the 14 arms that actually break the FreeRTOS build are invisible
  to the gate written to catch them"
status: resolved
type: bug
area: [cpp, ci]
related: [1187, 1023, 0112, 0332, 1204]
---

## Problem

`check-cpp-freestanding-includes` reports

```
check-cpp-freestanding-includes: OK (64 file(s) … no ungated hosted STL includes)
```

on a tree where 19 of 45 `nros-cpp` headers fail to compile on the pinned
embedded toolchain for exactly the reason the gate exists to prevent — a hosted
STL header reached with `NROS_CPP_STD` undefined (issue 1187).

The gate's awk tracks guard depth with three rules: push `"std"` on a
`#if`/`#ifdef` naming `NROS_CPP_STD`, push `"other"` on any other
`#if`/`#ifdef`/`#ifndef`, and pop on `#endif`. **`#elif` and `#else` match none
of them.** They are ordinary text, so the frame from the opening `#if` survives
into the alternative arm, and every include there is scored against a guard that
is false in that arm by construction.

## Reproduction

Both arms below are scored `guarded=1`; only the first one is.

```c++
#if defined(NROS_CPP_STD)
#include <string>          // guarded=1  — correct
#elif defined(__has_include)
#if __has_include(<vector>)
#include <vector>          // guarded=1  — WRONG: NROS_CPP_STD is off here
#endif
#endif
```

`#else` is the strictly worse case, because the include there is the *fallback* —
the code that runs precisely when `NROS_CPP_STD` is undefined:

```c++
#if defined(NROS_CPP_STD)
#include <string>          // guarded=1  — correct
#else
#include <vector>          // guarded=1  — WRONG, and this arm is the common one
#endif
```

Instrumented run of the gate's own awk over the first file:

```
1 sp=1 STDPUSH #if defined(NROS_CPP_STD)
2 sp=1 guarded=1  #include <string>
3 sp=1 PASSTHRU #elif defined(__has_include)     <- neither push nor pop
4 sp=2 OTHER  #if __has_include(<vector>)
5 sp=2 guarded=1  #include <vector>
```

## Reach

Every one of the **14 two-arm `#if NROS_CPP_STD` / `#elif defined(__has_include)`
blocks** across 11 headers (`client`, `fixed_string`, `heap_string`, `log` ×2,
`nros`, `options` ×2, `polling_subscription`, `publisher`, `service`,
`subscription` ×2, `timer` ×2) is invisible to this gate. `log.hpp:115`, the
`<string>` include that is the entry point for 17 of issue 1187's 19 header
failures, is one of them.

There are also 10 `#else` lines in the same headers, each of which needs
checking against this rule rather than against the gate's green verdict.

## Why it survived

Issue 1023 already found this gate's stack discipline wrong — a backend TU with
a real `#if`/`#elif`/`#else` where a nested `#endif` popped the `NROS_CPP_STD`
region early — and the fix was the neutral `"other"` push. That fix addressed
the `#endif` half of the same defect and left `#elif`/`#else` unhandled, which is
CLAUDE.md's "fix the CLASS, not the reported site" one layer down: the class is
"a preprocessor conditional the tracker does not model", and only one of its
three spellings got modelled.

The gate then reported green over the very construct issue 1187 is about, which
is why 1187's class landed under a green check.

## Fix direction

Model the alternative arms. On `#elif` and `#else`, the current frame's
condition no longer holds, so the frame must be *replaced* rather than kept: pop
the top and push `"other"` (an `#elif` arm is guarded by something, just not by
what the `#if` said). An `#elif` that itself names `NROS_CPP_STD` pushes `"std"`.

Acceptance is a negative control the gate does not have today: both snippets
above must FAIL. A gate whose selftest cannot distinguish the arms is the same
silence with extra steps — and this one currently reports OK on both.

Ordering: this is a prerequisite for phase-438, whose W2 deletes the `#elif`
arms. Fixed after them, the arms are gone and the blindness ships uncaught for
whatever writes the 15th.

Fixed first, the gate sees 14 real violations — and it must NOT simply go red
on them. It is on the fast line and therefore on the `pre-push` hook, so a red
one on `main` blocks every push in the repository by every contributor. The fix
lands with `.config/cpp-freestanding-includes-baseline.txt`, a shrink-only
ratchet holding the 14 known sites; phase-438 W2 empties it. A baseline line
that no longer offends FAILS, so the file is still W2's acceptance rather than
its paperwork.

## Resolution

Landed. `walk_file()` replaces the top frame on `#elif`/`#else` rather than
leaving it — replaces rather than pops, because at `strict=0` the Cyclone
backend legitimately takes `<chrono>`/`<thread>` in the `#else` of an
`NROS_PLATFORM_*` chain, and a pop would make depth 0 there.

Measured: **14 violations across 10 headers**, with `nros.hpp` correctly absent
— its `STD_CHRONO` block has no `#elif` arm.

The gate had no selftest at all, which is how the first version of this defect
(issue 1023's `\b`) and this one both survived. It has six cases now, and three
of them flip when the `#elif` rule is reverted:

```
SELFTEST FAIL: 'elif __has_include arm' should have been flagged and was not
SELFTEST FAIL: 'else fallback arm' should have been flagged and was not
SELFTEST FAIL: 'elif naming NROS_CPP_STD' should be clean, got: 4: #include <string>
```

Case 5 is the `strict=0` platform-`#else` shape the fix must not break; case 6
is depth 0. Both ratchet directions are mutation-tested — removing a baseline
line surfaces the violation it was hiding, adding a paid-off one fails as stale.
