---
id: 1212
title: "`check-core-crates-are-no-std` lists 5 of the 10 Rust core crates and the
  `no-std` build lane omits `nros-serdes-packed`, so half the core's `no_std`
  claim is unwatched"
status: open
type: tech-debt
area: [ci, core, testing]
related: [0196, 1209, phase-359, 1177]
---

## What

Two gates back the claim in ARCHITECTURE §2 that "the terminal state of the core
crates is `core` and `core+alloc`". Both are narrower than the class.

**1. The `#![no_std]` declaration gate.**
`scripts/check-core-crates-are-no-std.py:38-45` hardcodes:

```python
CORE_CRATES = [
    "packages/core/nros-node",
    "packages/core/nros-core",
    "packages/core/nros-rmw",
    "packages/core/nros-log",
    "packages/core/nros-params",
    "packages/platform/nros-platform-api",
]
```

`packages/core/` holds **10** Rust crates. Not listed:
`nros-serdes`, `nros-serdes-packed`, `nros-diagnostics`, `nros-macros`,
`nros-orchestration-ir`.

Two of those five are legitimately out of scope and are excluded *by name* in the
sibling gate — `scripts/check-std-census.py:89` excludes `nros-macros`
(`proc-macro = true`, host) and `nros-orchestration-ir` (self-documented host
schema code). The other three are not host crates: `nros-serdes` (5,023 LOC),
`nros-serdes-packed` and `nros-diagnostics` all declare `#![no_std]` today
(`packages/core/nros-serdes/src/lib.rs:28`,
`packages/core/nros-serdes-packed/src/lib.rs:57`,
`packages/core/nros-diagnostics/src/lib.rs:13`) and none of the three is gated on
keeping it.

The list is a deliberate list, and the file says why
(`check-core-crates-are-no-std.py:35-37`): "Deliberately a LIST rather than a
glob over `packages/core` -- a new crate landing there should have to be added
here on purpose". That reasoning is sound; the defect is that the deliberate
decision was never made for these three, so they read as "considered and
excluded" when they were simply not enumerated.

**2. The cross-target build lane.**
`just/check/lanes.just:1082-1084` builds 12 crates on `thumbv7m-none-eabi` and
`riscv32imc-unknown-none-elf`. `nros-serdes-packed` is absent from
`$crates`, `$rmw_crates` and `$lending_feats`.

Measured: it builds clean —
`cargo check -p nros-serdes-packed --no-default-features --target thumbv7m-none-eabi`
succeeds. So this is a ratchet gap, not a break.

## Why it matters

The lane's own comment (`just/check/lanes.just:1085-1101`) explains the class it
was written to close:

> This lane covered 9 of the 32 crates that DECLARE `no_std`, and `nros-node` —
> the one with 85 of the ~190 `cfg(feature = "std")` sites […] — was not among
> them. […] A lane written to prevent the issue-0196 shape nearly shipped with
> that exact shape.

It shipped *with* a smaller version of that shape. `nros-serdes` is in the build
lane but not the declaration gate; `nros-serdes-packed` is in neither. So a
`cfg_attr(not(feature = "std"), no_std)` regression in `nros-serdes`, or any
`std` reach in `nros-serdes-packed`, passes every merge-gating lane. The census
(`scripts/check-std-census.py`) does walk all of `packages/core`, but it counts
`std::` paths and cfg sites — it cannot see a crate that drops `#![no_std]` and
starts using `String` from the prelude, which is precisely the failure mode the
declaration gate exists for.

The three crates are also the *cleanest* part of the core: `nros-serdes`,
`nros-serdes-packed` and `nros-diagnostics` contain **zero** `unsafe` and zero
`std::` paths between them. They are the easiest possible things to hold at zero,
and nothing holds them.

## Fix sketch (not applied)

1. Add `nros-serdes`, `nros-serdes-packed`, `nros-diagnostics` to `CORE_CRATES`
   in `scripts/check-core-crates-are-no-std.py`. All three pass today, so this is
   a pure ratchet with no work behind it.
2. Add `-p nros-serdes-packed` to `$crates` in `just/check/lanes.just:1082`.
3. Record the *reason* the two host crates are out, in the gate itself rather
   than only in the census — a reader of `CORE_CRATES` currently cannot tell
   "host code" from "nobody got to it", which is what let three crates sit in the
   second category looking like the first. The census already has the wording
   (`check-std-census.py:78-88`); reuse it.
4. Consider deriving the list from one place. Two gates and one lane each keep
   their own idea of "the core crates" (5, 12, and a census scope of
   `packages/core` + `packages/api` minus 2) and no two agree — that divergence
   is what this issue is.
