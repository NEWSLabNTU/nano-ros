---
id: 1739
title: "Seven recently-changed gates run on no workflow, and `default-gates-run-somewhere` cannot see a lane-exempt `check::` step of `ci gate`"
status: open
type: bug
area: [ci, tooling]
severity: medium
found: 2026-10-07
related: [issue-1040, issue-1071, issue-1226, issue-1487, phase-472]
---

## The meta-gate's blind spot (confirmed by mutation)

`check-default-gates-run-somewhere` R1 has two scopes: the gate lanes
(fast + `build-serial` + `default`) and the NON-`check::` steps of `just ci
gate`. A `check::` step of `ci gate` is assumed covered "through the lane" —
but a lane-EXEMPT gate is in no lane, so it falls through both.

- Mutation: add `check::book-identifiers` (exempt, run by no workflow) to
  `just/ci.just`'s `steps=(…)` → rc 0.
- Control: add the non-`check::` step `doctor` (run by no workflow) → rc 1.
- **Live:** `check::launch-resolve-fresh` is exactly that shape today. Its
  exemption reason says "`ci gate` is where it belongs (issue 1487)", and no
  workflow runs `just ci gate` or `just check launch-resolve-fresh`.

## Gates changed since 2026-09-28 that no workflow reaches

| gate | where it runs | note |
| --- | --- | --- |
| `book-identifiers` | nowhere | exempt "in no lane and no caller found" |
| `executor-stack-floor` | nowhere | same |
| `nextest-test-filters` | nowhere | same |
| `workspace-rmw-agreement` | nowhere | same, and DEAD: workspace roots are generated (RFC-0098 D9) from `spec.rmw`, so its population is 0 by construction; it prints `OK — 0 call sites` rather than W4's `NOTHING TO CHECK … not a pass` |
| `weak-symbols-image` | nowhere | exempt "needs built fixtures"; nothing that builds fixtures calls it |
| `dist-runtime-deps` | `just workspace doctor` (no workflow) | **red on this host's clean tree**: `zephyr-sdk` 0.16.8 and 1.0.1 dists report unlisted sonames AND path-shaped pseudo-sonames (`…/lib/librt-2.27.so:`, a parse artifact). Nobody sees it |
| `launch-resolve-fresh` | `ci gate` step only | see above |

Issue 1071 (resolved) is the class these exemptions cite; nothing open tracks
running them. `.config/gate-lane-exempt.txt` also says
`archive-lang-items` has "no caller found" — it is called by
`build-test-fixtures` (`justfile`), so that reason is stale.

## Also observed (W4 shape, not mutation-confirmed)

`check-sizes-header-mirrors.sh` with no build trees prints `OK — 0 mirror/source
pair(s)` and exits 0 with no `nros_check_skip` ledger record.

## Fix direction

Extend R1's scope to every `check::` step of `ci gate`; give each gate above a
workflow step (or a schedule) or retire it; route `sizes-header-mirrors` and
`workspace-rmw-agreement`'s empty populations through `require_population` /
`nros_check_unverified`.

## A helper no member exercises

`scripts/build/check-store-corrosion.sh` (issue 1553): making its MISSING
branch `return 0` (run the gate, which then clones Corrosion) left `check fast`
green — it has no self-test, and on a host whose store holds the pin the branch
is never reached. It needs a normal-path control driving a fake
`nros setup --check` that answers `[MISSING]`.
