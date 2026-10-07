---
id: 1739
title: "Seven recently-changed gates run on no workflow, and `default-gates-run-somewhere` cannot see a lane-exempt `check::` step of `ci gate`"
status: resolved
type: bug
area: [ci, tooling]
severity: medium
found: 2026-10-07
related: [issue-1040, issue-1071, issue-1226, issue-1487, phase-472]
resolved_in: "gate-reach follow-up, item 5 (+ the issue-index content freeze, item 0)"
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

## Resolution

### The seven gates

| gate | ruling |
| --- | --- |
| `book-identifiers` | FAST lane (exemption removed; 5 s, buildless). First run found one stale quote: `rcl_qos_profile_rosout_default`, upstream rcl's profile that `logging.md` cites — EXEMPT with its reason, like `rcl_node_init` |
| `executor-stack-floor` | FAST lane (exemption removed; 1 s, buildless, green) |
| `nextest-test-filters` | stays exempt — the "no caller found" reason was STALE: `test-all` (justfile) runs it first, and `just ci tier1 run` / `just ci matrix` (run-matrix.yml) reach `test-all`. Reason rewritten |
| `workspace-rmw-agreement` | **RETIRED.** Its subject — an AUTHORED `nano_ros_workspace(BACKEND … SYSTEM …)` call — cannot exist in the tree: workspace roots are generated from `spec.rmw` (RFC-0098 D9) and `check-workspace-root-build-files` forbids a tracked one, so its population is 0 by construction and it printed `OK — 0 call sites`. One source of the fact leaves nothing to disagree with. Script, recipe, exemption and baseline row removed |
| `weak-symbols-image` | a step of `just ci matrix` (`run-matrix.yml`), AFTER that lane's fixture build — the lane that builds what it nm's. A row it cannot find stays a ledger skip |
| `dist-runtime-deps` | **the gate was half wrong**: its `ldd` reader took the first word of any line with `=>`/`not found`, so loader diagnostics (`…/librt-2.27.so: version GLIBC_PRIVATE not found`) became `:`-suffixed pseudo-sonames and the dist's own relocated loader (an absolute `PT_INTERP`) an undeclared host lib. Fixed (`ldd_external`, selftest row on the exact lines). **The rest is real**: both Zephyr SDK dists need host libraries their `system = [..]` omits → issue 1744 (needs a ruling: declare the closure, or scope upstream dists). Stays exempt until then, reason rewritten |
| `launch-resolve-fresh` | `gate.yml` step right after `just setup-launch-resolve` (pull_request / merge_group / schedule / dispatch) — the resolver is built in that job, and `setup-launch-resolve` rebuilds through the same `nros_launch_resolve_stale` the gate asks |

### The meta-gate

`check-default-gates-run-somewhere` R1 now puts every gate a `check::<name>`
step of `ci gate` reaches IN SCOPE (`ci_gate_check_gates`), not "covered through
the lane". Normal-path selftest row: `check::fast` + `check::lane-exempt-x`
expands to the fast members plus the exempt one. It found
`launch-resolve-fresh` live, which the new `gate.yml` step answers.

### The rest

- `.config/gate-lane-exempt.txt`: `archive-lang-items`' reason names its
  caller (`build-test-fixtures`).
- `scripts/build/check-store-corrosion.sh`: `nros_gate_require_store_corrosion_self_test`
  drives a fake `nros setup --check` through all three answers against a
  scratch ledger; both probe gates run it on their normal path.
- `check-sizes-header-mirrors.sh`: zero pairs compared is `nros_check_unverified`
  (a ledger SKIP; FAIL under `NROS_CHECK_SKIP_STRICT=1`), never `OK — 0 pair(s)`.

| mutation | old rc | new rc |
| --- | --- | --- |
| add `check::nextest-test-filters` (exempt, no workflow) to `ci gate`'s `steps=(…)` | 0 | 1 |
| `check-store-corrosion.sh` MISSING branch `return 0` | 0 (`check fast` green) | 1 (selftest) |
| `sizes-header-mirrors` with no build tree | 0, "OK — 0 pair(s)" | 0 + ledger SKIP; 1 under `NROS_CHECK_SKIP_STRICT=1` |
| `dist-runtime-deps` on this host | 1, with path-shaped pseudo-sonames | 1, genuine sonames only (issue 1744) |

The `check-issue-index` content freeze this issue's audit called fixed (a
digest SWAP still passed) landed separately, as item 0 of this follow-up.
