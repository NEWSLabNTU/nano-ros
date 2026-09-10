---
id: 1212
title: "`check-core-crates-are-no-std` lists 5 of the 10 Rust core crates and the
  `no-std` build lane omits `nros-serdes-packed`, so half the core's `no_std`
  claim is unwatched"
status: resolved
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

## Resolution

**Re-measured 2026-09-10, and the tree had already moved.** `packages/core/`
now holds **12** directories, not 11: `nros-executor-layout` landed after this
issue was written. It is `#![no_std]`, it is target-side, and it appeared in
**no** core-crate list anywhere in the repo — not this issue's own accounting,
not either gate, not the build lane, not the census baseline, not ARCHITECTURE,
not RFC-0001. That is the argument for deriving rather than correcting: this
issue's own fix sketch, applied literally, would have shipped with the defect
one crate wider.

Current measurement: 12 directories, 11 with a `Cargo.toml`, of which **9 are
target-side core** (all nine already unconditionally `#![no_std]`) and 2 are
host (`nros-macros`, `nros-orchestration-ir`); `nros-rmw-abi` holds no Rust.

**The definition, stated once.** *A core crate is one that must compile for a
target with no operating system and no standard library.* Its home is
`packages/core/`, so membership is DERIVED from that directory with a per-crate
opt-out, in `scripts/lib/core_crates.py`. The two ways out are both properties
of the crate readable from its own manifest: `proc-macro = true` (structural,
unfakeable) and `[package.metadata.nros] host-only = true` with its
`host-only-reason`.

`host-only` is **the marker issue 0287 already established** — reused, not
respelled. That reuse is load-bearing rather than tidy: a `packages/core` crate
that has not declared it is already compiled for `thumbv7em-none-eabihf` by
`check workspace-embedded`, so the obligation this derivation hands out is one
the crate is already living under. A crate cannot be inside the embedded
workspace build and exempt from the property that build depends on.

The old gate's stated reason for a list — "a new crate landing there should have
to be added here on purpose" — was right about the DECISION and wrong about
where to keep it. The polarity is now fixed: core is the default, and opting
**out** is what costs a person a written reason. Three target-side crates read
as considered-and-excluded when they had simply never been enumerated.

**Consumers, now one definition instead of four:**

| site | before | after |
| --- | --- | --- |
| `check-core-crates-are-no-std` | 6 hardcoded paths | derived + 3 named seams = **12** |
| `just check no-std` | 12 hardcoded `-p` flags | derived core + 5 named extras |
| `check-std-census` `EXCLUDE` | `{nros-macros, nros-orchestration-ir}` | derived (resolves to the same 2) |
| ARCHITECTURE §2 agnosticism contract | 6 names, one nonexistent | "every crate under `packages/core/`" |

The three vtable seams (`nros-platform-api`, `nros-platform-cffi`,
`nros-rmw-cffi`) stay a literal list, deliberately: they are core by the no-OS
property while resident beside their layer, and the contract defining them is
closed — a fourth seam is an architecture change with an RFC, not a crate
landing in a directory.

**The three placement rulings** (recorded in `scripts/lib/core_crates.py` and
normatively in ARCHITECTURE §2 "What core means"), each decided rather than
papered over by widening a list:

- **`nros-rmw-abi` — correctly placed.** `packages/core` is the FOUNDATIONAL
  layer, not "the Rust crates directory"; the RMW C ABI SSoT (RFC-0054) that
  both the Rust seam and every C backend implement is as core as anything here,
  and moving it would put the ABI definition further from the layer it defines
  than the code consuming it. It is not a Rust crate, so it carries no
  `#![no_std]` obligation and the derivation skips it STRUCTURALLY (no
  `Cargo.toml`), never by name.
- **`nros-macros` — correctly placed, and host.** ARCHITECTURE §2 already ruled
  that entry macros emitting per-target boot code "legitimately live in
  `nros`/`nros-macros`". `proc-macro = true` classifies where its code runs
  without anyone maintaining a list.
- **`nros-orchestration-ir` — correctly placed, and host.** It exists to be
  shared by the host halves of the core (the CLI's codegen and `nros::main!`);
  `packages/tooling` is build-support for this repo and would misfile it.

**Newly covered, all pure ratchet — measured, not assumed.**
`nros-serdes`, `nros-serdes-packed`, `nros-diagnostics` and
`nros-executor-layout` join the declaration gate; `nros-serdes-packed` and
`nros-executor-layout` join the build lane. All nine core crates check clean on
both `thumbv7m-none-eabi` and `riscv32imc-unknown-none-elf` today.

**A latent defect the mutation test exposed.** `CONDITIONAL` in the declaration
gate was `#!\[cfg_attr\([^)]*no_std` — a character class excluding `)` cannot
cross the one that closes `not(...)`, so the exact spelling the gate's docstring
exists to name, `#![cfg_attr(not(feature = "std"), no_std)]`, never reached the
conditional branch. The verdict was always red; the *reason* said "no
`#![no_std]` at all", advice that sends the reader to add a line already there.
Fixed, and the self-test now asserts the REASON rather than only the count —
asserting the count is why it read green for two phases.

**Mutation tests.** (1) `nros-serdes-packed` switched to the conditional form →
red, `packages/core/nros-serdes-packed: conditional cfg_attr(..., no_std)`.
(2) `host-only = true` added to `nros-executor-layout` with no reason → red,
`opts out of core without saying why`. (3) `std::string::String` planted in
`nros-executor-layout` → `just check no-std` red, `E0433: cannot find module or
crate std`. All three restored, all three green after.

Not addressed: the census BASELINE still enumerates crate names, and
`.github/workflows/docs.yml`'s path filter names 2 of the core crates — both are
separate questions with their own reasoning. Issues 1209 and 1210 are untouched.
