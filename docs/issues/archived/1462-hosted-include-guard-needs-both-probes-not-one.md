---
id: 1462
title: "`node.hpp` guarded `<map>` on `__STDC_HOSTED__` alone, so every Zephyr
  C++ image failed to compile — and the gate that exists to catch this enforced
  only the other half of the same rule"
status: resolved
type: bug
area: api, ci, build
severity: high
related: [0112, 1240, 0196, 0332, phase-417, phase-456]
---

## Symptom

`just build-test-fixtures lane=all` dies in its first module:

```
/home/aeon/repos/nano-ros/packages/api/nros-cpp/include/nros/node.hpp:18:10:
  fatal error: map: No such file or directory
FATAL ERROR: command exited with status 1:
  cmake --build .../zephyr-workspace/build-c-service-server-xrce
```

`lane=all` builds the zephyr module FIRST, so this starved every other module:
the native and C fixtures behind it were never rebuilt, which is a second reason
the C pubsub coordinates had no runtime result.

## Cause

`node.hpp` guarded the include like this:

```cpp
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdlib>
#include <map>
```

CLAUDE.md states the rule and why each probe ALONE is wrong:

> `__STDC_HOSTED__` alone: a hosted compiler can run `-nostdinc++` against
> Zephyr's minimal libcpp and have no `<string>`. `__has_include` alone: under
> `-ffreestanding` a FULL libstdc++ HAS `<string>` and opens it with
> `#error "This header is not available in freestanding mode."`

This is the first half, measured: Zephyr's arm-none-eabi C++ build reports
`__STDC_HOSTED__` and ships a minimal libcpp in which `<map>` is simply absent.

The guard dates from phase-417 W4.a and predates phase-456. What is new is that
`lane=all` had not run in weeks, so nothing compiled these leaves.

## Why the gate said OK

`check-cpp-freestanding-includes` reported **OK on 76 files** on the same tree.
Its frame predicate read:

```awk
function std_frame(line) {
    if (line !~ /NROS_CPP_STD/) { return 0 }
    if (line ~ /__has_include/ && line !~ /__STDC_HOSTED__/) { return 0 }
    return 1
}
```

It rejected `__has_include` without `__STDC_HOSTED__` — the half issue 1240
added — and accepted `__STDC_HOSTED__` without `__has_include` for exactly as
long. The rule is a conjunction, and the gate enforced one conjunct.

This is issue 0196's shape inside the gate whose own comment invokes issue 0196:

> This is the 0196 rule applied to this gate: its reach must be the rule it
> enforces, not the spelling the rule happened to have when it was written.

The comment was right and the code was half of it.

## Fix

* `node.hpp` gives `<map>` the conjunction, and `<cstdio>`/`<cstdlib>` stay
  under the plain hosted guard — they are in the freestanding-guaranteed set
  that the gate deliberately does not police.
* `std_frame` rejects a frame naming EITHER probe without the other.
* Two selftest cases, the mirror of the existing case 8: the `(__STDC_HOSTED__
  + 0)` idiom and the long `defined(__STDC_HOSTED__) && __STDC_HOSTED__`
  spelling, so the case is about the missing probe and not about an idiom. Both
  were confirmed to FAIL against the pre-fix predicate — the gate reported
  "should have been flagged and was not" for each.

Measured after: 15 selftest cases, 76 files clean, the compile-probe sweep
unchanged at 43 PASS / 14 FAIL, and a `-nostdinc++ -ffreestanding` compile
against the ThreadX shim succeeds.

## Sweep

Every hosted STL include in `packages/api/nros-cpp/include` was checked against
the frame guarding it, by walking the `#if`/`#endif` stack rather than grepping:

```
packages/api/nros-cpp/include/nros/node.hpp:18  <map>
--- 1 hosted include(s) behind a __STDC_HOSTED__-only guard
```

One site. The other `__STDC_HOSTED__`-only frames in these headers wrap
`<cstdlib>` / `<cstdio>`, which are freestanding-guaranteed and legal ungated.
The gate now covers the class, so a second one cannot land silently.

## What this does not answer

Whether the three C pubsub coordinates pass once they can be rebuilt. They have
been reporting a stale verdict for 18 days (issue 1461), and this failure kept
`lane=all` from rebuilding them. Those are two separate reasons for one silence.
