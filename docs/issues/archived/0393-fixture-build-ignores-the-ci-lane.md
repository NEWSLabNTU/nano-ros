---
id: 393
title: "The fixture BUILD ignores the CI lane that the gate and the test run already agree on, and `.fixtures-built` records no scope"
status: resolved
type: tech-debt
area: testing
related: [rfc-0051, rfc-0061, issue-0196, issue-0351, phase-318, phase-324]
resolved_in: "issue-0393-lane-scoped-fixture-build"
---

# 0393 — fixture build ignores the CI lane; stamp records no scope

**Status:** Resolved (2026-08-01)
**Filed:** 2026-08-01

## What was wrong

`ci_lane.rs:225` claimed "a lane's build, its staleness gate and its test run
cannot disagree about what the lane covers". Only the gate and the run read the
selection: `build-test-fixtures` took no lane, fanned out over all nine platform
families unconditionally (`justfile:1167`), and `fixtures-manifest.py`'s existing
`--coords-from` filter was reachable only from `check-fixtures-stale.sh`. Tier 1
therefore had to build all 337 manifest rows to run the 180 native ones.

The stamp had the matching hole: three separate copies of
`date -u > target/nextest/.fixtures-built` wrote a bare timestamp, so
`_require-fixtures` could ask "did a build finish?" but never "does what was
built cover what I am about to run?" — the scope half of issue 0351.

## What landed

**One helper, `scripts/build/fixture-lane.sh`**, is now the single place a lane
becomes coordinates, modules, or a stamp. The three private copies of the stamp
writer (`justfile` `build-all`, `justfile` `build-test-fixtures`, `build-all.mk`)
call it instead of re-spelling it.

**`just build-test-fixtures lane=<all|native|tier1|tier2|tier2-nightly>`**
narrows in two layers that derive from the same `lane-coords` computation the
gate uses:

- *modules* — which `just <mod> build-fixtures` runs at all (measured: `native`
  → 1 module, `tier2` → 8, `all` → 10)
- *coords* — which manifest ROWS each surviving module builds, via
  `NROS_FIXTURE_COORDS` → `fixtures-build.sh` / `workspace-fixtures-build.sh` →
  `fixtures-manifest.py --coords-from` (measured: native rust 62 → 9 rows,
  native workspaces 65 → 16)

**The stamp records the set, not the moment** — `lane=` plus one `coord=` line
per coordinate. `_require-fixtures` checks COVERAGE against `NROS_FIXTURE_LANE`,
so a `lane=native` build satisfies `just ci` and is *rejected* by an unscoped
`test-all`. A pre-0393 timestamp-only stamp reads as `lane=all`, which is what it
meant, so existing trees do not start failing.

**`just ci` (tier 1) sets `NROS_FIXTURE_LANE=native`.**

## Two corrections to the issue as filed

**The tier-1 build lane is `native`, not `tier1`.** Tier 1 scopes its run with
`NROS_TEST_SCOPE=native`, which selects every native test BINARY — a broader set
than `coords(Tier1)` (verified: 10 of 47 coordinates, all on `native`). Building
only the tier-1 coordinates would leave the other native binaries absent and the
run would mass-fail "Binary not found". **The build set has to cover the RUN set,
not the gate set** — the issue's direction section conflated the two.

**Tier 2 deliberately still builds `all`.** `ci-matrix` does not scope its run,
so every test binary executes and every fixture must exist. Its saving stays in
the staleness gate. Narrowing the tier-2 build needs the tier-2 run narrowed
first; that is left open rather than shipped half-done.

## Gate against the drift coming back

`ci_lane::tests::build_fanout_names_every_module_the_matrix_can_select` asserts
the justfile's canonical platform list is a superset of every
`PlatformId::just_module()`. The fan-out narrows by FILTERING that ordered list
(order is a scheduling property — zephyr first and solo), so a module the matrix
can select and the list does not name would be silently skipped. Verified to have
teeth: dropping `px4` from the list fails the test with that name.

## Verified

- stamp coverage matrix, 15 cases (no stamp / all / native / tier lanes / legacy
  timestamp / bad lane) — all as specified
- `lane-coords tier1` = 10 coords, all `native,*`; modules per lane as above
- both fan-out paths (jobserver-serial and make-graph) select the right modules
- coords filtering on both `list` and `list-workspaces`
- the new gate fails when a module is removed; `cargo test -p nros-tests --lib`
  70/70; clippy clean; rustfmt clean; shellcheck clean on the new file

**Not run:** a full fixture build or `just ci` end to end — those need a multi-hour
native fixture build. The `lane=native` path's only behavior change versus before
is the module filter, which is verified directly above.
