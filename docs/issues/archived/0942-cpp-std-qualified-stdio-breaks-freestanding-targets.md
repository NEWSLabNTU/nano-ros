---
id: 942
title: "`std::fprintf` in a cross-compiled library TU built everywhere except the one freestanding target"
status: resolved
type: bug
area: rmw-cyclonedds, tooling
related: [issue-0589, issue-0196]
---

## What happened

`just build-test-fixtures lane=all` failed the `threadx_riscv64` stage:

    nros-rmw-cyclonedds/src/graph.cpp:234:14:
      error: 'fprintf' is not a member of 'std'; did you mean 'printf'?

Two call sites, both behind an `NROS_GRAPH_DUMP` env gate added with the
phase-381 graph work.

## Why it built everywhere else

`<cstdio>` is required to declare the C names in namespace `std`. Whether it
ALSO declares them in the global namespace is explicitly unspecified. The
freestanding libstdc++ in the riscv64 cross toolchain does the reverse of what
hosted implementations do: the C library's `<stdio.h>` provides `::fprintf`, and
nothing hoists it into `std`.

So `std::fprintf` is not a portability-neutral style choice — it is the one
spelling that depends on a guarantee a freestanding C++ library does not have to
make. Every hosted build, and every embedded build whose toolchain happens to be
generous, compiled it fine.

The sibling TU in the same crate already had it right: `descriptors.cpp:106`
includes `<stdio.h>` and calls `fprintf` unqualified. `graph.cpp` was the only
library TU in ANY RMW backend using the `std::` spelling — every other
`std::fprintf` in the crate is under `tests/`, which is host-only.

## Fix

`graph.cpp` now matches `descriptors.cpp`: `#include <stdio.h>`, unqualified
`fprintf`. Two call sites.

## The class

`scripts/check-cpp-no-std-stdio.py`, on the fast line as
`just check cpp-no-std-stdio` — the C++ twin of `check-no-std-stdio.py`
(issue 0589), which forbids the same shape in Rust `#![no_std]` crates.

**Width, chosen the same way 0589 chose its own.** 68 tracked C/C++ files use
`std::printf`/`std::fprintf`. Almost all are host-only — `examples/native/cpp/**`,
`tests/**` — where the guarantee holds and the spelling is harmless. Banning it
there would be ~60 files of churn to prevent nothing, and would teach people to
write exemptions. What matters is the library code linked into every embedded
image: the RMW backends, the C/C++ API, the core, the board C shims, the
drivers. Under that rule the tree is at ZERO after this fix, so the gate is a
tripwire and not a cleanup.

Verified non-vacuous: run against the pre-fix `graph.cpp` it reports both call
sites and exits 1; against the fixed file it passes. A gate that has never been
shown to fail is not known to work.

## Why it took a full-lane sweep to find

`threadx_riscv64` + cyclonedds is only built by `lane=all`. Nothing narrower
compiles this TU with a freestanding C++ library, so the red sat on `main`
unnoticed. That is a coverage fact worth remembering rather than a defect in
this file: the gate now catches the shape at push time, where the lane cannot.

## Acceptance

* ~~`graph.cpp` compiles for threadx-riscv64.~~ Met.
* ~~The shape cannot return to a cross-compiled library TU.~~ Met, and the gate
  is proven against the original defect.
