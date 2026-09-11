# Phase 450 — a gate whose reach is narrower than the rule it enforces

**Status (2026-09-11). Opened to give five homeless issues one owner. W2 was
already LANDED when the phase was written — issue 1161 closed via PR #734 on
2026-09-08, which the opening survey read while the PR was still open. W1, W3,
W4 and W5 are open. Every remaining member issue was measured against the tree
rather than read off the gate's own description.**

## Why this phase exists

CLAUDE.md already names this class — *"If a gate exists for the class, check the
gate actually covers the new site (issue-0196 rule) — audit 2026-07-28 found
four gates whose coverage was narrower than the rule they enforce"* — and the
rule has since recurred at least four more times, each time filed as a separate
issue with no phase.

The reason it recurs is that a gate of this shape is **green while the defect it
exists for is present**. Nothing about the output distinguishes "checked and
clean" from "did not look". So the finding never arrives from the gate; it
arrives when someone re-derives the rule by hand, and that only happens by
accident.

Five issues are that shape. Four are open; #1161 closed on 2026-09-08 and
stays in the table as the worked example:

| issue | the rule | what the instrument actually reaches |
| --- | --- | --- |
| [#1153](../issues/1153-rust-targets-covered-blind-to-zephyr-and-docker.md) | every declared Rust target has a row | three declaration globs of five — a target the tree builds for had no row at all |
| [#1129](../issues/1129-fixture-err-laundered-into-skip-wider-class.md) | a fixture `Err` may not become a `skip!` | one spelling — 54 of 58 remaining sites say `not built`, which `check-skip-budget` cannot see |
| [#1161](../issues/archived/1161-required-features-tests-counts-skips-as-failures.md) (RESOLVED 2026-09-08) | a lane's pass means its tests ran | a missing FIXTURE is forbidden, a missing CAPABILITY is not — the lane reported pass having run 7 of 20. Closed by PR #734 before this phase was written; W2 records it |
| [#1051](../issues/1051-pinned-locks-gate-suggests-a-destructive-fix.md) + [#1294](../issues/1294-submodule-pinned-locks-remedy-points-at-a-rewind.md) | a red names its cause | one of two causes asserted as the only one, with a remedy that would rewrite a CORRECT lock to match a drifted checkout — and following it records a submodule REWIND |
| [#1236](../issues/1236-unsafe-census-ratchet-for-crates-that-cannot-forbid.md) | `unsafe` does not grow unnoticed | `forbid(unsafe_code)` on the ten crates at literal zero; the 62 crates holding all 5,047 occurrences have no instrument at all |

#1051 is in this set deliberately. It is not a coverage gap — the gate fires
correctly — but it is the same failure of fidelity one step later: the verdict
is true and the DIAGNOSIS is a guess, and acting on the printed remedy damages
a correct file. A gate that reports the wrong cause is worse than one that
reports nothing, because it is acted on.

## The shape, stated once

**A gate must be able to fail on the tree it is pointed at, and must not assert
a cause it did not measure.** Two consequences this phase applies throughout:

* *Coverage is a property to be derived, not declared.* #1153's three globs and
  #1129's one spelling are both hand-authored inventories of where a rule
  applies. That inventory drifts in the safe-looking direction, exactly as
  CLAUDE.md records for the api-parity map and the lane-stage map.
* *A negative control is part of the gate.* `check-reconfigure-stale` already
  carries one, for precisely the reason this phase exists: `"N build dir(s)
  load"` is also what a probe that can never fail would print.

## Work items

Ordered by blast radius: the two that let real defects through first, then the
one that reports a false cause, then the census that does not exist yet.

### W1 — the fixture-skip rule reaches the defect, not one spelling

[Issue 1129](../issues/1129-fixture-err-laundered-into-skip-wider-class.md).
Issue 1124 converted the twelve sites matching its grep; the sweep afterwards
found the scope had been the SPELLING. 4 more sites say `not prebuilt` in files
1124's table did not list, and **54 say `not built`, across 43 files**.

This is the canonical *fix the class, not the site* case, so it is also the one
that must not be fixed by adding a second spelling to the matcher.

- [ ] One shared helper converts a fixture-resolver `Err`, and the sites call
      it. Not a widened regex.
- [ ] `check-skip-budget` keys on the helper, so a new site that bypasses it is
      the thing that fails.
- [ ] The sweep command is in the commit message and re-runs clean.

### W2 — a capability skip is budgeted like a fixture skip — LANDED

[Issue 1161](../issues/archived/1161-required-features-tests-counts-skips-as-failures.md)
(RESOLVED 2026-09-08, archived). Read its correction header: the filed premise
was wrong and the issue marks it so. What survived was that
`check-required-features-tests` reported pass having run **7 of its 20** tests,
because issue 0584's rule names fixtures and not capabilities.

- [x] A lane declares the capabilities it provides; a skip for an undeclared
      capability fails the lane.
- [x] The lane that reported 7 of 20 either runs 20 or fails.

Landed via PR #734 before this phase was written — the phase was drafted on
2026-09-11 against a survey taken while that PR was still open with two failing
checks, and it had merged by the time the branch rebased. Left here rather than
deleted because the phase's five members are its evidence: this is the one that
was already being worked, and it is the shape the other four should follow.

### W3 — every Rust-target declaration site is reached

[Issue 1153](../issues/1153-rust-targets-covered-blind-to-zephyr-and-docker.md).
`check-rust-targets-covered.py` collects "every declaration" from three
`git ls-files` globs; two further sites declare a target, including
`zephyr/cmake/nros_cargo_build.cmake`'s `set(NROS_RUST_TARGET …)`.

- [ ] The declaration set is derived from one place, or the gate fails when it
      finds a target declared somewhere it does not scan.
- [ ] The target that had no row has one, and removing that row is a red.

### W4 — `check-submodule-pinned-locks` measures which cause it has

[Issue 1051](../issues/1051-pinned-locks-gate-suggests-a-destructive-fix.md) and
[issue 1294](../issues/1294-submodule-pinned-locks-remedy-points-at-a-rewind.md).
Two causes produce one message: the pointer moved and the lock did not follow,
or the pointer did not move and the WORKTREE drifted from it. The printed
remedy is correct for the first and destructive for the second.

**The two ids are one defect, filed five days apart by sessions that each met
it cold, and #1294 measures the worse half: following the printed remedy on the
second cause records a submodule REWIND** — the thing `check-submodule-pins`
and the `pre-push` hook exist to refuse. Close one into the other when either
is worked.

It also reproduced during this phase's own homing commit: `just ci gate` went
red here on 2026-09-11 naming the lock, and the cause was a drifted
`play_launch` checkout that `git submodule update` cleared. A gate this phase
is about, caught while checking the commit that files it.

- [ ] The gate distinguishes the two by measuring the checkout against the
      recorded pointer before it prints anything.
- [ ] Each cause prints its own remedy; the drifted-checkout arm says
      `git submodule update <path>` and never `just lock-update`.
- [ ] The remedy the drifted-checkout arm prints cannot produce a rewind —
      checked by following it, not by reading it.

### W5 — the 62 crates that cannot `forbid` get a direction of travel

[Issue 1236](../issues/1236-unsafe-census-ratchet-for-crates-that-cannot-forbid.md).
Issue 1221 put `#![forbid(unsafe_code)]` on the ten shipped crates measured at
literal zero. `forbid` is a property, not a budget, so it says nothing about the
62 crates carrying all 5,047 occurrences.

- [ ] A per-crate census with a ratchet that may only SHRINK, in the shape the
      other ratchets in this tree already use.
- [ ] The baseline is generated, not hand-authored — a hand-authored inventory
      is W1's and W3's defect.
- [ ] A new `unsafe` in a crate at its budget is a red.

## Acceptance for the phase

* Each of the five gates fails on a mutation of the defect it exists for,
  introduced deliberately and reverted — the method
  `check-c-array-guard-probe` already applies to guards, applied here to gates.
* No member gate's coverage set is a hand-authored list of sites.

## Non-goals

* Auditing every gate in the tree. That is the periodic audit
  ([docs/development/codebase-audit-checklist.md](../development/codebase-audit-checklist.md)),
  and this phase is the five issues already filed.
* Making a gate merge-gating. Whether a lane runs at all is
  [phase-413](phase-413-ci-workflow-user-parity.md)'s question; this phase is
  about what a lane can see once it runs. The two meet in
  [phase-449](phase-449-state-keyed-by-name-not-by-tree.md) W7, whose
  verification printed `[OK]` on a configuration it should have refused.
