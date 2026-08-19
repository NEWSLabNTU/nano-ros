---
id: 701
title: "`check-feature-contract` clause (a) enforces only the GRANTING half —
  a capability may require `std`/`alloc` without naming it, and one did"
status: open
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

## Reproduce (the state before the fix)

```
git stash   # or check out the parent of the fix commit
cargo check -p nros-cpp --no-default-features --features metadata-mode,alloc,rmw-cffi
# error[E0433]: cannot find module or crate `std`   <- names no feature
```
