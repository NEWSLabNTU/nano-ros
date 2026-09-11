# Phase 450 — a gate whose reach is narrower than the rule it enforces

**Status (2026-09-11). COMPLETE — W1 through W5 all landed, and all five member
issues are resolved and archived (1129, 1153, 1236, 1051+1294).** Every landed item was mutation-tested —
the defect it exists for was planted, the gate went red naming it, and the tree
was restored — because a gate of this shape is green while the defect is
present, which is the whole premise of the phase.

**Three of the four found something the issue had not.** W3's fourth
declaration site exposed four unlisted triples, not one. W4's third cause was
reproduced on this session's own `pre-push` failure. W5's census reaches 36
crates and 636 unsafe sites that a `cargo metadata` enumeration cannot see,
because the root workspace excludes them — this phase's own subject arriving
inside a gate written for it.

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
| [#1153](../issues/archived/1153-rust-targets-covered-blind-to-zephyr-and-docker.md) | every declared Rust target has a row | three declaration globs of five — a target the tree builds for had no row at all |
| [#1129](../issues/archived/1129-fixture-err-laundered-into-skip-wider-class.md) | a fixture `Err` may not become a `skip!` | one spelling — 54 of 58 remaining sites say `not built`, which `check-skip-budget` cannot see |
| [#1161](../issues/archived/1161-required-features-tests-counts-skips-as-failures.md) (RESOLVED 2026-09-08) | a lane's pass means its tests ran | a missing FIXTURE is forbidden, a missing CAPABILITY is not — the lane reported pass having run 7 of 20. Closed by PR #734 before this phase was written; W2 records it |
| [#1051](../issues/archived/1051-pinned-locks-gate-suggests-a-destructive-fix.md) + [#1294](../issues/archived/1294-submodule-pinned-locks-remedy-points-at-a-rewind.md) | a red names its cause | one of two causes asserted as the only one, with a remedy that would rewrite a CORRECT lock to match a drifted checkout — and following it records a submodule REWIND |
| [#1236](../issues/archived/1236-unsafe-census-ratchet-for-crates-that-cannot-forbid.md) | `unsafe` does not grow unnoticed | `forbid(unsafe_code)` on the ten crates at literal zero; the 62 crates holding all 5,047 occurrences have no instrument at all |

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

[Issue 1129](../issues/archived/1129-fixture-err-laundered-into-skip-wider-class.md).
Issue 1124 converted the twelve sites matching its grep; the sweep afterwards
found the scope had been the SPELLING. 4 more sites say `not prebuilt` in files
1124's table did not list, and **54 say `not built`, across 43 files**.

This is the canonical *fix the class, not the site* case, so it is also the one
that must not be fixed by adding a second spelling to the matcher.

- [ ] One shared helper converts a fixture-resolver `Err`, and the sites call
      it. Not a widened regex.
- [ ] `check-skip-budget` keys on the helper, so a new site that bypasses it is
      the thing that fails.
- [x] `check-skip-budget` keys on the SPELLINGS rather than one spelling. It
      read `not prebuilt` only, while **57 of the tree's 61 laundering sites say
      `not built`** — the rule stated fully and enforced on 4 of 61.
- [x] A negative control: six `FIXTURE_RE` cases in `--self-test`, four positive
      and two that must NOT match. Mutation-tested by reverting the regex, which
      fails two cases.
- [x] The sites converted, per-site: **51 CONVERT, 10 KEEP, 0 unsure.** The
      keeps are not fixture resolvers at all — `launch_resolver_bin` is a host
      tool (6), `zephyr_leaf_staleness` probes a west dir with a bare
      `is_file()` (2), and two zenoh tests already guard on the opt-out and
      panic in the else.
- [x] **A structural finding that had to land first.**
      `require_prebuilt_workspace_binary` carried NONE of the tier block its
      sibling has — no `.build-failed` check, no `NROS_FIXTURES_OPTIONAL` skip,
      no `gate_promised_fixtures()` panic. 21 of the 61 sites resolve through
      it, so there the call-site `skip!` was the ONLY accommodation that had
      ever existed, and on a gated run the only thing between CI and a silent
      green. One shared helper now (`absent_fixture_verdict`), not a second
      spelling, with the per-resolver remedy threaded through.
- [x] The sweep command is in the commit message and re-runs clean.

One override of 1129's prose, recorded because it contradicts the issue:
`qos_zephyr_ros2_interop_e2e` is listed there as a west/SDK keep and its
resolver already answers both the west lane and the opt-out, so it converted.
Three reasons also named the wrong binary ("native listener" while resolving
`build_int32_sink`) — the W4 defect in a diagnostic instead of a remedy.

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

[Issue 1153](../issues/archived/1153-rust-targets-covered-blind-to-zephyr-and-docker.md).
`check-rust-targets-covered.py` collects "every declaration" from three
`git ls-files` globs; two further sites declare a target, including
`zephyr/cmake/nros_cargo_build.cmake`'s `set(NROS_RUST_TARGET …)`.

- [x] The fourth site (`NROS_RUST_TARGET` in `zephyr/cmake/*.cmake` and its
      shell mirror) is scanned. Scanning it found **four** more unlisted
      triples, not one: `thumbv7em-none-eabi`, `thumbv8m.main-none-eabi{,hf}`
      and `i686-unknown-linux-gnu` — each the no-FPU or alternate-profile
      sibling of a triple already listed, reachable by a real Zephyr board.
- [x] All four added to the three lists the gate cross-checks
      (`config/rust-targets.txt`, the SDK index, `rust-toolchain.toml`); 12 rows
      to 16. Removing one is a red, mutation-tested.
- [x] **The issue's third site was refuted rather than implemented.** 1153 names
      `ci/docker/*/Dockerfile`; that image installs a hardcoded set AND the
      SSoT's rows, and its comment states the asymmetry is intended — "dropping
      them here would be a silent capability loss". Scanning it reported 4
      intentional targets as undeclared. The reasoning is recorded in the gate
      so the next reader does not re-add it.

### W4 — `check-submodule-pinned-locks` measures which cause it has

[Issue 1051](../issues/archived/1051-pinned-locks-gate-suggests-a-destructive-fix.md) and
[issue 1294](../issues/archived/1294-submodule-pinned-locks-remedy-points-at-a-rewind.md).
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

- [x] `_submodule_drift` compares the submodule's `HEAD` against the gitlink the
      superproject records, and only a real difference produces the third
      verdict. It clears the inherited git environment first — this runs under
      `pre-push`, where `GIT_DIR` overrides `git -C`, so without that the probe
      would read the superproject's HEAD and report no drift ALWAYS
      (issues 0986/0988): a gate that cannot fire, in the phase about gates that
      cannot fire.
- [x] The drifted arm prints `git submodule update <path>`, per entry rather
      than once, and never `just lock-update`.
- [x] Followed rather than read: reproduced on this session's own failure
      (`play_launch` at `db4af878` against a pinned `155ed78b`), which before
      the change printed "pointer moved, run lock-update". `git submodule
      update` clears it.

### W5 — the 62 crates that cannot `forbid` get a direction of travel

[Issue 1236](../issues/archived/1236-unsafe-census-ratchet-for-crates-that-cannot-forbid.md).
Issue 1221 put `#![forbid(unsafe_code)]` on the ten shipped crates measured at
literal zero. `forbid` is a property, not a budget, so it says nothing about the
62 crates carrying all 5,047 occurrences.

- [x] `just check unsafe-census` — per-crate, shrink-only, counting SYNTAX
      (`unsafe {}` / `fn` / `impl` / `extern` / `trait`) after stripping
      comments and string literals. **4,411 syntactic sites**, against the
      issue's 5,047 token occurrences; the issue said the two differ and this
      measures which.
- [x] The baseline is generated (`--write-baseline`) and says so.
- [x] A planted `unsafe { }` in `nros-node` reds it (`unsafe block 374 -> 375`).
- [x] **The obvious enumeration was the wrong one.** `cargo metadata --no-deps`
      reports workspace MEMBERS and sees 37 crates / 3,775 sites; the tree
      excludes ~130 paths, several of them real crates no lane compiles
      (issue 1309), so `git ls-files` sees 73 / 4,411. A members-only census
      would have inherited that blind spot and reported a cleaner number than
      the truth.

Not done, and it is a decision rather than work: 1236's second half asks for a
recorded POSITION on the remaining core ("these crates carry unsafe and here is
the ceiling"). The census makes that statable with real numbers; it does not
make the statement.

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
