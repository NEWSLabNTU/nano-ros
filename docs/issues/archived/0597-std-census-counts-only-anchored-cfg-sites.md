---
id: 597
title: The `std` census counts only anchored `cfg` sites, so 69 of 252 are invisible to its own ratchet
status: resolved
type: bug
area: testing
related: [phase-359, phase-361, issue-0196, issue-0587]
resolved_in: "a9d54004e (phase-359 W2)"
---

> **RESOLVED before it was filed — by `a9d54004e` (phase-359 W2), 2026-08-15
> 03:10, hours ahead of this file.** Upstream hit the same defect from the other
> end and fixed it: W2 deleted four `cfg` lines from `spin.rs` and the gate did
> not move, "which is the one thing a ratchet must never do: it read as 'no
> progress' when the truth was 'cannot see it'." `CFG_RE` now matches any
> `feature = "std"` inside a `cfg` attribute at any nesting, and the baseline was
> re-measured in the same commit — 181 → **242** cfg mentions, 425 → 421 paths,
> with `nros-node` alone at 131/342.
>
> Kept, archived, for the independent derivation rather than the conclusion: this
> file measured the blind spot from the OTHER direction (69 of 252 sites, 27 %,
> and which combinator hid each) and carries a mutation test upstream's commit
> does not — two planted `std`-conditional items using no `std::` path leave the
> gate green, while a third that also named `std::string::String` is caught by
> the PATH metric instead. That distinction is worth keeping: it shows the two
> metrics are not redundant, which is the reason the `cfg` half had to be fixed
> rather than leaned on. Upstream's count is 242 where this file measured 252;
> the ~10 gap is comment-stripping detail and upstream's number is the one the
> gate now enforces.
>
> Two items in "Direction" below were NOT taken and remain open questions for
> phase-359: splitting `not(std)` from positive `std` sites (they are opposite
> sides of the migration and are still summed), and verifying the widened matcher
> in both directions.


## The defect

`scripts/check-std-census.py` (phase-359 W0) freezes a per-crate count of
`cfg(feature = "std")` sites and fails when one goes up. Its matcher is:

```python
CFG_RE = re.compile(r'cfg(?:_attr)?\s*\(\s*(?:not\s*\(\s*)?feature\s*=\s*"std"')
```

`feature = "std"` has to appear **immediately** after `cfg(` or `cfg(not(`. Any
`std`-conditional site that reaches the feature through a combinator is not
counted:

| spelling | counted? |
| --- | --- |
| `#[cfg(feature = "std")]` | yes |
| `#[cfg(not(feature = "std"))]` | yes |
| `#[cfg_attr(feature = "std", derive(Debug))]` | yes |
| `#[cfg(all(feature = "std", feature = "rmw-cffi"))]` | **no** |
| `#[cfg(all(feature = "rmw-cffi", feature = "std"))]` | **no** |
| `#[cfg(any(feature = "alloc", feature = "std"))]` | **no** |
| `#[cfg(any(feature = "std", feature = "alloc"))]` | **no** |
| `#[cfg(not(any(feature = "alloc", feature = "std")))]` | **no** |

## Evidence

**It under-counts the tree it was measured on.** Counting every `cfg` line that
mentions `feature = "std"` anywhere in the predicate, over the census's own
scope (`packages/core` + `packages/api`, same exclusions, same comment
stripping):

| | sites |
| --- | --- |
| counted by `CFG_RE` (the frozen baseline) | 183 |
| mention `feature = "std"` at all | 252 |
| **invisible** | **69 (27 %)** |

Per crate, and by which combinator hides them:

```
nros-node    55        all(…)   66   e.g. executor/node_record.rs:324
nros          6                      #[cfg(all(feature = "alloc", not(feature = "std"), …))]
nros-cpp      4        any(…)    3   e.g. executor/types.rs:1144
nros-params   2                      #[cfg(any(all(feature = "std", feature = "rmw-cffi"), test))]
nros-c        2
```

`nros-node` is phase-359's largest work item — 85 counted sites — and 55 more
sit next to them uncounted.

**Mutation test.** Two `std`-conditional items appended to `nros-rmw`
(baseline `cfg 1, path 0`), neither using a `std::` path:

```rust
#[cfg(all(feature = "std", feature = "log"))]
pub fn planted_probe() -> u32 { 0 }
#[cfg(any(feature = "alloc", feature = "std"))]
pub fn planted_probe2() -> u32 { 1 }
```

```
nros-rmw         1     0   cfg 1, path 0
std census: OK (no crate moved)
```

Green. The `path` metric is what usually saves the gate — a third plant that
also named `std::string::String` was caught as `path 0 -> 2` — but that is the
*other* metric doing the work, and the census's own docstring is explicit that
the two are tracked separately because "they move independently. W2 (collapsing
duplicated fields) deletes `cfg` branches without touching `path` counts."
**W2 is precisely the work item this blind spot cannot measure.**

**It missed a real 88-site regression.** phase-361's W2.a briefly deleted the
`std ⇒ alloc` edge from six manifests, which re-created the implication at the
use sites as `cfg(any(feature = "alloc", feature = "std"))` — 123 of them, +88
net std-mentioning branches against `main`, 66 in `nros-node`. The census
reported **183 sites for both trees**. It caught one symptom via the other
metric (`nros-node: path 346 -> 347`, from a `std::vec::Vec` that could no
longer be spelled `alloc::vec::Vec`) and nothing else. The regression was found
by writing a second, wider counter by hand.

## Why it matters beyond the count

This is the issue-0196 shape — *a gate whose coverage is narrower than the rule
it enforces* — and phase-359 W1's own write-up already records hitting it once
(`check-no-std` compiled the crate shell because `has_rmw` was off, so a planted
`std::string::String` passed in 0.06 s). Same phase, same class, second
instance.

The ratchet is also the campaign's progress report: work items are accepted by
"the count went down". A metric blind to 27 % of the population will report W2
as complete while a quarter of the branches remain, and — worse for a ratchet —
cannot see a *regression* introduced in an uncounted spelling, which is exactly
what a migration in flight will produce as authors reach for `all(...)` to keep
two features apart.

## Direction

1. **Widen `CFG_RE` to any `cfg` predicate mentioning `feature = "std"`**, and
   re-baseline to 252. A `cfg` attribute is a single line in this tree in every
   case observed, so a line-oriented `cfg(...)` + `feature\s*=\s*"std"` match is
   sufficient; a nesting-aware parse is not needed and would be harder to trust.
2. **Re-baseline per crate in the same commit**, so the jump from 183 to 252 is
   visible as a measurement correction and not as 69 new sites appearing.
3. Consider counting `not(...)` separately from positive sites. `cfg(not(std))`
   is the branch that *survives* dropping `std`; `cfg(std)` is the branch that
   gets deleted. Today they are summed, so a conversion that turns one into the
   other reads as no progress.
4. **Verify the widened matcher in both directions** — a planted
   `all(feature = "std", …)` must fail, and the existing 183 must still be
   counted exactly once — before the baseline is trusted. The mutation above is
   the ready-made first case.

Not urgent for correctness of the shipped code: nothing here is a runtime
defect. It is urgent for phase-359, because the campaign's acceptance criteria
are stated in this gate's numbers.
