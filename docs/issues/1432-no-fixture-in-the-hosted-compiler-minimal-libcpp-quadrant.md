---
id: 1432
title: "No C++ fixture sits in the (hosted compiler, minimal libcpp) quadrant,
  which is the only one where the `__STDC_HOSTED__` half of the both-probes
  rule can bite — so issue 1431 was found by an outside team, not by a lane"
status: open
type: tech-debt
area: [testing, zephyr, api]
severity: medium
found: 2026-09-21
related: [1431, 0112, 1240, 0332, 0196, 1016]
---

## What this is

Issue 1431 was a hosted STL include (`<map>`) admitted on an
`__STDC_HOSTED__`-only arm. Every lane was green; the defect was reported by the
NXP team building a real Zephyr board with `-nostdinc++`. This issue is the
reason no lane could have found it.

## The two axes, and the empty cell

Whether that arm fires is decided by two INDEPENDENT facts:

* **`__STDC_HOSTED__`** — a property of the COMPILER INVOCATION. Measured on
  this host's `arm-none-eabi-g++`:

  ```
  $ echo __STDC_HOSTED__ | arm-none-eabi-g++ -E -P -x c++ -                  -> 1
  $ echo __STDC_HOSTED__ | arm-none-eabi-g++ -ffreestanding -E -P -x c++ -   -> 0
  ```

* **Whether the include path HAS the header** — a property of the libcpp.
  Zephyr's minimal set is `zephyr/cxx-compat/`: atomic chrono cstdarg cstddef
  cstdint cstdio cstdlib cstring initializer_list new random thread type_traits
  utility. No `map`, no `string`, no `vector`.

Four combinations; the tree's C++ fixtures cover two, and a third is harmless:

| | full libcpp | minimal libcpp |
| --- | --- | --- |
| **`__STDC_HOSTED__`=1** | 18 `native_sim/native/64` rows | **EMPTY — 1431 lives here** |
| **`__STDC_HOSTED__`=0** | (not a shape we build) | 1 `mps2_an385` row |

The 19 C++ Zephyr fixture rows are 18 × `native_sim/native/64` and 1 ×
`mps2_an385`, counted from `examples/fixtures.toml`.

* `native_sim` builds against the HOST's full libstdc++, so `<map>` resolves and
  the arm firing costs nothing.
* `mps2_an385` builds `-ffreestanding` against picolibc — stated by this tree's
  own `zephyr/cxx-compat/type_traits:3`, "Zephyr builds C++ `-ffreestanding`
  against picolibc, which ships no C++ headers" — so `__STDC_HOSTED__` is 0, the
  `||` arm never fires, and the include is never reached.

**The bottom-right cell is the only one where an `__STDC_HOSTED__`-only guard
does damage, and nothing in the tree occupies it.** NXP's board does: a hosted
compiler invocation with a minimal libcpp on the include path.

## Why this is worth an issue and not a footnote

`check-cpp-freestanding-includes` is a TEXT gate. Issue 1431 tightened it and it
now catches the shape it missed — but a text gate can only refuse spellings
someone anticipated, which is how it carried a one-sided rule for two issues
running. The compile in the empty cell is the instrument that does not depend on
anticipating the spelling.

This is also the second half of 1431's own "what is NOT established": that fix
was verified by a syntax-only compile I constructed by hand, not by a fixture.
A hand probe proves the fix; it does not stay in the tree and does not run
again.

## What is NOT established

* **That `mps2_an385`'s Zephyr C++ build really passes `-ffreestanding`.**
  Inferred from the tree's own comment and consistent with the row staying green
  through 1431, but NOT read out of a build log — no build dir survives on this
  host. If that board is in fact hosted=1, the analysis above is wrong and the
  `mps2_an385` row should have failed, which would make this a coverage question
  about WHICH LANE BUILDS IT rather than an empty-quadrant question.
* **Which board would fill the cell cheaply.** NXP's is an out-of-tree NXP part.
  Whether an in-tree board can be configured hosted-with-minimal-libcpp without
  inventing a fixture nobody ships has not been looked at.
* **Whether any other header is exposed the same way.** 1431's mirror check makes
  the scanned trees green today; it says nothing about a header reached through a
  route the gate does not scan.

## Direction

A C++ fixture whose compile is hosted and whose libcpp is minimal. Two shapes
worth weighing:

1. **A real leaf** on a board configured that way — highest fidelity, costs a
   board and a lane slot, and a new west build name must join
   `check-west-leaf-vocabulary`'s model or its verdict is a STALE message
   indistinguishable from a failure (issue 1016).
2. **A compile-only probe** in the existing C++ check lane: `-nostdinc++
   -I zephyr/cxx-compat` against the public headers, asserting they compile. That
   is what I ran by hand for 1431 and it took seconds. It does not prove the
   image links or runs, but it occupies the empty cell and runs every time, which
   the hand probe does not.

(2) is the cheap one and would have caught 1431 on the commit that introduced it.
Its known limit: the bare probe stops at the per-build `nros_config_generated.h`
stubs, so it needs a generated config or a stub set to get past the includes —
which is exactly what made 1431's reproduction stop where it did.
