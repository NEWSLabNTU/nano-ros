---
id: 1431
title: "`node.hpp` admits `<map>` on the `__STDC_HOSTED__` arm alone, so a
  hosted-but-`-nostdinc++` Zephyr C++ image fails to compile — and the gate
  written for exactly this class scored the shape SAFE"
status: resolved
resolved: 2026-09-21
type: bug
area: [api, build, zephyr]
severity: high
found: 2026-09-21
related: [0112, 1240, 0332, 0196, phase-417]
---

## What happens

`packages/api/nros-cpp/include/nros/node.hpp` guarded its `<map>` include with

```c
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
```

`__STDC_HOSTED__` is a claim about the COMPILER; the question is about the
INCLUDE PATH. On Zephyr the compiler is hosted (`__STDC_HOSTED__` is 1 —
measured) and the include path is Zephyr's minimal libcpp, which ships

```
atomic chrono cstdarg cstddef cstdint cstdio cstdlib cstring
initializer_list new random thread type_traits utility
```

and no `map`. So the arm fires and the header cannot be opened. MEASURED:

```
$ g++ -std=c++17 -nostdinc++ -fsyntax-only -I zephyr/cxx-compat \
      -I packages/api/nros-cpp/include -I packages/api/nros-c/include probe.cpp
packages/api/nros-cpp/include/nros/node.hpp:18:10: fatal error: map: No such file or directory
```

That is issue 0112's half of the both-probes rule, which CLAUDE.md records and
which `std_detect.hpp` documents at length.

## Why nothing caught it

Two independent reasons, and it needed both.

**No fixture can reach the condition.** The tree's only C++ Zephyr fixture board
is `native_sim`, which builds against the host's full libstdc++. `<map>` resolves
there, so every lane was green. Reported by the NXP team building a real board
with `-nostdinc++`, who needed `CONFIG_REQUIRES_FULL_LIBCPP=y` to get past it.

**`check-cpp-freestanding-includes` scored the shape SAFE.** Its whole subject is
this class, its `HOSTED` list contains `map`, and its baseline is EMPTY (all
fourteen 1240 entries paid). It still passed, because of `std_frame`:

```awk
function std_frame(line) {
    if (line !~ /NROS_CPP_STD/) { return 0 }
    if (line ~ /__has_include/ && line !~ /__STDC_HOSTED__/) { return 0 }
    return 1
}
```

The rule is that BOTH probes are required. This enforced ONE ORDERING of it —
`__has_include` without `__STDC_HOSTED__` (issue 1240's direction) — and had no
mirror, so a region widened by `__STDC_HOSTED__` with no `__has_include` named
the token, carried no `__has_include`, and fell through to `return 1`.

The function's own comment says it is "the 0196 rule applied to this gate: its
reach must be the rule it enforces, not the spelling the rule happened to have
when it was written." It was the 0196 shape.

## Fix

**The include, not the probe.** `<map>` is now guarded by
`#if defined(NROS_CPP_STD)` alone — no hosted arm — because that is the
condition its only consumers already have. `Node::declare_parameters` and
`lifecycle.hpp`'s twin (which includes no `<map>` of its own and relies on this
one) both sit inside `#ifdef NROS_CPP_NODE_HOSTED`, and that implies
`NROS_CPP_STD` since each of its four capability probes is
`#if defined(NROS_CPP_STD)`. So the include is now guarded exactly as tightly as
the code needing it, which restores the property `node.hpp` states as the rule a
few lines below: a hosted STL include lives inside an `NROS_CPP_STD` region,
never one an `||` arm can reach from a freestanding board (issue 0332).

`<cstdlib>` and `<cstdio>` keep the widened guard and are correct there: Zephyr's
minimal libcpp ships both, and neither is in the gate's `HOSTED` list.

**The gate** gained the mirror check, so the shape cannot return:

```awk
if (line ~ /__STDC_HOSTED__/ && line !~ /__has_include/) { return 0 }
```

## Verification

* Zephyr shape (`-nostdinc++ -I zephyr/cxx-compat`): the `map` error is gone —
  zero occurrences of `map` in the compiler output, where before it was fatal.
* Hosted shape (`-DNROS_CPP_STD`): likewise zero. Both shapes now stop only at
  the per-build `nros_config_generated.h` stubs, which is the bare probe's own
  limitation and is identical in kind before and after.
* Gate mutation: restoring the pre-fix guard makes the tightened gate fail and
  name the line (`node.hpp:49: #include <map>`); restoring the fix returns it to
  OK. Two selftest rows added — the broken shape must be flagged, and the same
  widened shape WITH both probes must stay clean, so the check refuses the
  widening and not the token. 11 cases, all green.

## What is NOT established

**No Zephyr C++ board image was built.** The reproduction is a syntax-only
compile in the shape that board uses, not that board's build; the tree has no
fixture that can reach the condition, which is the second half of why this
survived. A C++ Zephyr fixture on a board with a minimal libcpp would have
caught it and would catch the next one — worth its own issue, not filed here.

Whether other headers in the `HOSTED` list are reachable through a widened arm
elsewhere is answered for the scanned trees by the gate now being green with the
mirror in place; it is not answered for code the gate does not scan.
