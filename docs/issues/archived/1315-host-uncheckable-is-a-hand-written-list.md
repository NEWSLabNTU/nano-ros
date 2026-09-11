---
id: 1315
title: "`HOST_UNCHECKABLE` is the hand-written exclude list issue 0287 retired on
  the embedded side and nobody retired on the host side — 5 of its 8 entries are
  already stale, and it is why four cross-only crates cannot be workspace members"
status: resolved
type: tech-debt
area: [ci, build, tooling]
severity: medium
found: 2026-09-11
related: [0287, 1309, 1155, phase-450, phase-451]
---

## What

Two lanes exclude crates from a workspace-wide clippy, and they answer the same
question by opposite methods:

| lane | question | mechanism | derived? |
| --- | --- | --- | --- |
| `check::workspace-embedded` | may this crate build for a thumb target? | `[package.metadata.nros] host-only = true` + `host-only-reason` | **yes** — `scripts/build/host-only-members.sh`, 25 crates declare it |
| `check::test-targets` | may this crate build for the HOST? | `HOST_UNCHECKABLE` in `just/check.just:36` | **no** — one string, 8 crates, no reasons |

`scripts/build/host-only-members.sh`'s own docstring is the argument against the
surviving half, written when the other half was fixed:

> That was handled by a hand-written `--exclude` list in the justfile: 20 lines,
> no reasons, and nothing tying an entry to the crate it excludes. **A list like
> that only stays correct while someone remembers it exists.**

Nobody remembered. `HOST_UNCHECKABLE` is that list, still.

## Measured, 2026-09-11

`check::test-targets` has two arms and both consult the list. Its per-crate arm
runs, for every member not in `HOST_UNCHECKABLE`:

```sh
cargo clippy --quiet -p "$p" --all-targets -- -D warnings
```

Running exactly that for the eight excluded crates:

| crate | result |
| --- | --- |
| `nros-c` | **clean** |
| `nros-rmw-xrce-cffi` | **clean** |
| `nros-build-helpers` | **clean** |
| `nros-zpico-build` | **clean** |
| `nros-build-paths` | **clean** |
| `nros-cpp` | fails — `unwinding panics are not supported without std` |
| `nros-rmw-zenoh-staticlib` | fails — `` `#[panic_handler]` function required `` |
| `nros-rmw-xrce-cffi-staticlib` | fails — `` `#[panic_handler]` function required `` |

**Five of eight are excluded from the per-crate arm without needing to be**, so
five crates are lint-exempt for no reason anyone can point at. The three real
ones are all one cause (a `staticlib`/`cdylib` with no host panic runtime), and
that cause is a MANIFEST FACT — `crate-type` — which is exactly what a derived
rule could read.

Scope note, stated rather than glossed: this measures the PER-CRATE arm, whose
command it reproduces exactly. The other arm is
`--workspace --all-targets --no-default-features {{HOST_UNCHECKABLE}}`, where
feature unification across members can make a crate fail for a reason its own
build does not have. A crate clean here is not automatically safe to delete from
the list; it is a crate whose exclusion has no *measured* justification, which is
the defect.

The list's one comment covers `nros-c` and `nros-cpp` together —
"staticlib/cdylib requires a platform-specific panic/runtime setup" — and only
`nros-cpp` needs it. A shared reason for two crates where it is true of one is
how a hand list rots.

## Why it matters beyond the five

This is the blocker [issue 1309](1309-excluded-crate-is-built-by-no-lane.md)
ran into. Four crates —
`nros-platform-{mps2-an385,stm32f4,esp32-qemu}` and `nros-board-esp32-qemu` —
depend on `cortex-m` / `esp-hal` and so cannot build for the host. They are kept
out of the workspace ENTIRELY, because there is no derived way to say "member,
but not in the host lane". Being out of the workspace means no lane compiles,
lints, tests or formats them, and
`packages/platform/nros-platform-stm32f4/src/phy.rs:182` carries three `#[test]`
functions over `detect_phy_type` that have consequently never run.

`host-only` has no mirror. That missing mirror is the whole of 1309's remaining
work, and it is this issue.

## Suggested shape

1. **A declared `embedded-only` twin** of `host-only`, same shape:

   ```toml
   [package.metadata.nros]
   embedded-only = true
   embedded-only-reason = "staticlib with no host panic runtime"
   ```

   derived by a sibling of `host-only-members.sh` and consumed by both arms of
   `check::test-targets`, replacing the string.
2. **Delete the five stale entries** as part of it, each on the measurement
   above rather than by inspection.
3. Consider deriving the three real ones instead of declaring them: their cause
   is `crate-type = ["staticlib"]` / `["cdylib"]` in the manifest, and a derived
   rule cannot go stale the way the string did. Declaring is the fallback where
   a reason is not a manifest fact — the same split
   `.config/workspace-exclude-reasons.txt` already uses.
4. With the mirror in place, 1309's four cross-only crates become members that
   declare which lane they cannot enter, and `nros-platform-stm32f4`'s three
   tests become reachable.

## Not to be confused with

`.config/workspace-exclude-reasons.txt` (phase-451 W3) answers "is this
exclusion from the WORKSPACE justified". This one answers "is this exclusion
from a LANE justified". Both are shrink-only ratchets in spirit; only the first
one exists.

## Resolved (phase-451 W4, 2026-09-11)

`HOST_UNCHECKABLE` is DERIVED. `scripts/build/embedded-only-members.sh` is the
mirror of `host-only-members.sh`, reading
`[package.metadata.nros] embedded-only = true` from each crate's own manifest,
and `just/check.just` calls it instead of carrying the string.

**The five stale entries are gone**, on the measurement this issue recorded:
`nros-c`, `nros-rmw-xrce-cffi`, `nros-build-helpers`, `nros-zpico-build` and
`nros-build-paths` are clippy-clean under the per-crate command the lane runs.

**The three real ones declare it beside the `crate-type` that causes it.** They
were not derived from `crate-type` in the end, and the reason is worth keeping:
all three were ALREADY `host-only`, so each was excluded from the embedded lane
by its manifest and from the host lane by a string in the justfile — a workspace
member that NO lane compiled. Both sides are stated in one table now, which is
the thing a derivation would have hidden rather than fixed.

**What this did NOT unblock, and that is the finding.** Issue 1309 expected the
mirror to make its four `cortex-m` / `esp-hal` crates members. It does not:
`cortex-m`, `esp-hal` and `nros-platform-critical-section` each select a
different `critical-section` restore-state width, and critical-section refuses
more than one —

    error: You must set at most one of these Cargo features: restore-state-none, ...

Upstream exclusivity, so no workspace build can hold them. Established by trying
three arrangements (all four, the cortex-m pair, the esp32 pair); the embedded
lane fails inside `critical-section` every time, a crate none of them names.
`Cargo.toml` carries that measured reason now instead of the guess.

Two things landed on the way: `nros-platform/src/resolve.rs`'s nine identical
`#[cfg(feature = "platform-<x>")] ConcretePlatform` arms collapsed to one
`cfg(any(...))` — mutually exclusive by convention only, so two platform
features at once was E0428, which nothing hit until crates became members — and
both member-list scripts moved off `grep -q` onto `nros_grep_q` (issue 0726),
shrinking that ratchet by one.
