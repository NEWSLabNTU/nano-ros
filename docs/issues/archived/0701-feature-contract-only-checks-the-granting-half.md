---
id: 701
title: "`check-feature-contract` clause (a) enforces only the GRANTING half —
  a capability may require `std`/`alloc` without naming it, and one did"
status: resolved
type: bug
area: build
related: [phase-359, issue-0594, issue-0196, issue-0687]
---

## The rule, and the half that is checked

ARCHITECTURE §2 clause (a):

> A capability/backend/platform feature REQUIRES the heap, it does not grant
> it — emit `compile_error!` naming the feature.

`check-feature-contract`'s clause (a) enforces the first half. It scans every
manifest for a capability whose body enables `std`/`alloc` (`= ["std"]`, or a
`dep/std` forward) and fails. That is the "does not grant it" clause.

Nothing checks the second half — **"emit `compile_error!` naming the
feature."** A capability whose gated code calls `std::` while its crate carries
no guard passes every gate, and the user's build fails with

```
error[E0433]: cannot find module or crate `std`
  --> packages/api/nros-cpp/src/metadata_hooks.rs:135:8
```

four frames from anything they can act on, naming no feature.

## Measured, on a live instance

`nros-cpp`'s `metadata-mode` writes the metadata sidecar
(`std::fs::write` in `nros_cpp_metadata_dump`) and declared no guard. It had
never needed one **by accident**: `nros`'s `metadata-mode` guard required `std`,
and `nros-cpp/metadata-mode = ["nros/metadata-mode", …]`, so the named error
came from the crate one layer down.

Issue 0669's follow-up then corrected `nros`'s guard — its half of the feature
records into a heap-allocated global and hands back a `String`, so it requires
`alloc`, not `std`. Correct in itself, and it uncovered the missing guard here.
**Relaxing a guard in one crate can expose a capability in another that never
named its own requirement**, and no gate says so.

`nros-cpp/env` was in the same position (its `$NROS_ENTRY_SPIN_MS` reader is
`std::env::var`, covered only by `nros`'s guard firing first). Both guards were
added; the gap that let them sit unguarded is what this issue is about.

## The sweep is cheap, and that is the argument for a gate

Bounded by `check-std-census`, not by the feature list — a capability can only
REQUIRE a flavour if its gated code names it, and the census already enumerates
every non-test `std::` site in the tree:

```
python3 scripts/check-std-census.py     # the sites
# for each site: which cfg gates it?
#   gated by `std`/`alloc` itself      -> fine, that IS the flavour
#   gated by a capability feature F    -> crate must carry
#                                         #[cfg(all(feature = F, not(feature = "std")))]
#                                         compile_error!(... F ...)
```

Run over the 25 sites on 2026-08-19, that yields exactly two candidates, both
in `nros-cpp`, both now guarded. So the enforcement gap is currently at zero —
which is the moment to gate it, not after it drifts.

## Why it was not gated in the same commit

The cheap spellings are unsound. A file-level grep ("this file mentions
`feature = "F"` and also `std::`") has false positives wherever a file carries
several cfgs. Doing it properly needs cfg-scope tracking — which
`check-std-census.py` half has (it tracks `#[cfg(test)]` bodies by brace depth)
and would have to grow to a general form. That is a real change to the one
script the whole campaign is ratcheted against, and it wants its own review
rather than riding a fix for the site it found.

## Options

1. **Extend `check-std-census.py`** to track the enclosing cfg per site and
   emit the capability-without-guard list as a second failure mode. Most
   accurate; touches the ratchet.
2. **A separate gate that BUILDS** each capability feature with
   `--no-default-features --features F,alloc` and requires the first error to
   be a `compile_error!`. Decisive and sound, but 110 features across the nine
   crates is minutes, so it belongs on the build tier or nightly, not the fast
   lane.
3. **Leave it to review**, with the sweep recipe above recorded. Cheapest;
   relies on whoever relaxes a guard remembering to re-run it, which is exactly
   what did not happen here.

## Resolved 2026-08-20 — option 1, and option 2 measured out

`just check capability-flavour-guards`
(`scripts/check-std-census.py --check-guards`), in the fast tier.

**Option 2 was tried first and abandoned on measurement.** Building every
capability is sound but the candidate set has to be narrowed or it is hours:
the coarse selection ("this file mentions `feature = "F"` and also `std::`")
yields **1514** candidates across the tree, because one `std::` anywhere in a
file flags every feature named in it. Narrowing it correctly needs per-site cfg
attribution — which is option 1. So option 2 collapses into option 1 plus a
build.

**What the attribution has to get right**, both learned by being wrong first:

* the gate is the CONJUNCTION of the module declaration and the enclosing item.
  `metadata_hooks.rs` is declared `#[cfg(feature = "rmw-cffi")] mod
  metadata_hooks;` in `lib.rs`, while the `std::fs::write` sits in a function
  gated `metadata-mode`. Neither alone is the answer, and the file cannot see
  the first.
* an item's `#[cfg]` can sit several attribute lines above a MULTI-LINE
  signature. The first version cleared the pending cfg at the first
  non-attribute line, which is `pub extern "C" fn dump(` — so it attributed the
  site to `rmw-cffi` alone and reported a false violation on the very case it
  was written for.

Both shapes are in `--self-test`, which builds a synthetic crate and checks
three cases: unguarded capability fires, guard present passes, and a site gated
on the FLAVOUR itself is not a violation.

**Scope is the whole tree, not the census's nine.** An unnamed `std`
requirement is just as opaque in a board or a backend. Measured before
committing to it: 132 further `no_std` crates under `packages/`, **zero**
violations, 0.4 s total. The census stays scoped to the nine, because that is
what phase-359 ratchets.

**Deliberately conservative in one direction.** Only `feature = "x"` and
`all(...)` conjunctions are attributed; `any(...)` alternatives contribute
nothing, because a site reachable through either of two features does not let
the gate say which one needs the guard. It under-reports rather than crying
wolf.

### Not covered: the `alloc` half, and why that is a measurement not an omission

Clause (a) says "REQUIRES the heap", so the same gate over `alloc::` looks
free. It is not: **20** candidates appear, and the first one checked is a false
positive — `nros/src/node_runtime.rs` carries its own `extern crate alloc;` at
file scope, so `alloc::` resolves there whatever the feature says. `std::` has
no such escape in a `#![no_std]` crate, which is why the `std` half is sound
with this much machinery and the `alloc` half is not. Covering it means
tracking file-local `extern crate alloc` declarations; filed here rather than
guessed at.

## Reproduce (the state before the fix)

```
git stash   # or check out the parent of the fix commit
cargo check -p nros-cpp --no-default-features --features metadata-mode,alloc,rmw-cffi
# error[E0433]: cannot find module or crate `std`   <- names no feature
```
