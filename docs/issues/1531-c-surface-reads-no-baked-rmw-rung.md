---
id: 1531
title: "The C surface reads no baked RMW rung: `NROS_ENTRY_RMW` is on every C
  compile line and has only C++ consumers, so a C image cannot name its backend
  even though `nros_support_init_rmw` takes one"
status: open
type: bug
area: [api, rmw]
severity: medium
found: 2026-09-28
related: [1050, 1530, 0196, phase-424]
---

## What is missing

`cmake/NanoRosEntry.cmake:855` bakes `NROS_ENTRY_RMW` onto every entry target,
whatever its language:

```
DEFINES = -DNROS_ENTRY_RMW=\"cyclonedds\" -DNROS_HOST_POSIX -DNROS_PLATFORM_POSIX ...
```

It has exactly two consumers, both C++:
`packages/api/nros-cpp/include/nros/executor.hpp:152` and `node.hpp:2675`.

The C road reads it nowhere. Every in-tree C main calls the three-argument
`nros_support_init`, whose `rmw` is `NULL`, so an image that holds its own
answer in its own preprocessor never passes it. `nros_support_init_rmw` exists
and takes the selector — what is absent is a consumer of the bake.

This is the half of issue 1050 that landed for one language surface only.
`b64e3655a` added the baked rung to cmake and to the C++ headers, and in the same
commit made a selector-less open with more than one registered backend a hard
refusal. The C road got the new failure mode and not the new capability.

## What it cost, and what it still costs

It has already cost one high-severity outage: issue 1530, where a recorder
self-registering into every native C and C++ image made the registry ambiguous
and every native C example failed `nros_support_init` with
`NROS_RET_INVALID_ARGUMENT` for eighteen days. 1530 is fixed by removing the
second registrant, so these images hold exactly one backend again and a nameless
open resolves.

What remains is the capability gap: **a C image that legitimately links two
backends still cannot name one.** No in-tree native C image does today, which is
why this is filed separately rather than folded into 1530 — the symptom is gone
and the asymmetry is not.

## What a fix has to look like

The consumer has to be a HEADER-side spelling, not a change to the mains. There
are twenty-odd in-tree C mains plus every out-of-tree C app, and fixing the
sites rather than the class is what CLAUDE.md's "fix the CLASS" rule is about.
A function-like macro is the C analogue of what the C++ headers do at their own
call site — it only expands when followed by `(`, so `&nros_support_init` is
unaffected — and it has to sit AFTER the generated declaration in
`nros_generated.h`, which rules out `entry_config.h` (included first, and
documented as ladder-only).

`nros/init.h` is the hand-written header that already documents
`nros_support_init`, and is the likely home. Both `nros_support_init` and
`nros_support_init_named` need it; `$NROS_RMW` must keep outranking the bake,
which `nros_support_init_rmw` already implements.

## Acceptance

* A native C image with two registered backends opens the one its entry baked.
* `$NROS_RMW` still wins over the bake, measured, not assumed.
* Taking the address of `nros_support_init` still compiles.
* A gate holds the two language surfaces to the same rung set, so the next rung
  added to one cannot skip the other — which is the defect class here, not the
  individual macro.
