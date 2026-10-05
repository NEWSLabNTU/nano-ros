---
id: 1684
title: "`host-tests` builds a hand-picked fixture SUBSET, then runs `test-all` over
  the whole tier-1 lane with the preflight bypassed — 199 of its 214 real failures
  are fixtures the job never builds"
status: open
type: bug
area: [ci, testing]
severity: high
found: 2026-10-05
related: [1651, 1685, 0584, 0482, 0029]
---

## What was measured

Issue 1651's first `test-all` verdict since 2026-06-17: workflow_dispatch run
37252649866, integration job 111583270143, at the tree of commit
dc5691a584cfb9f2a53b4b92372370d81ee8cd38 (`main` plus CI-file changes only).

    2657 tests run: 2310 passed, 346 failed, 1 timed out
    Real failures: 214 / 214      (the junit rewrite moved 132 skips out)

**199 of the 214** are one message:

    Test fixture binary MISSING for an in-lane coordinate: .../build/cargo-fixtures/linux-3263301353/nros-relwithdebinfo/action-server

and four more are its siblings (`libnros_rmw_zenoh_staticlib.a fixture not
built`, `Phase N.E fixture not built`, `POSIX zenoh staticlib fixture not
built`, plus the per-cell `MISSING` lines inside `entry_matrix`,
`realtime_tiers`, `roundtrip_xprocess`, `sched_dims`, `multihost`).

## Why

The job builds `just native build-fixture-rust-core` +
`just native build-workspace-fixtures` — the comment in `host-tests.yml` calls
it "a DELIBERATE SUBSET", with the ~52 GB extras left out for disk (issue #29)
— then runs `just ci tier1 run` with `NROS_SKIP_FIXTURE_CHECK=1`. But
`test-all` under `NROS_TEST_COORDS=<tier1>` treats every tier-1 row as in-lane,
and a missing in-lane fixture has been a hard failure since issue 0584. So the
subset decides what is built and the lane decides what is asserted, and they are
different sets. `build-fixture-rust-core` builds "default-config rows only" into
`build/cargo-fixtures/linux/`, while the tests resolve the non-default rows
(`no_default_features`/`features` variants, e.g. `examples/fixtures.toml` line
~1875) into `linux-<hash>/`, and no C/C++ cmake fixture is built at all.

That the bypass only "skips the PRE-FLIGHT" (the workflow comment) is not what
it does: `require_prebuilt_binary_fresh` also skips the per-resolution
staleness probe under it. That leak made one self-test red in this lane; it is
fixed in the PR that filed this (`probe_prebuilt_binary_fresh`).

## Measured: building the lane instead

Locally (32 cores, same tree), `just build-test-fixtures lane=tier1`: **42 min,
`build/` 12 G -> 51 G (+39 G)**. Over those fixtures the same `test-all` gave
**30 real failures instead of 214**; the residue is issues 1685–1692. On the
CI runner (4 vCPU, the job already at 130 G of 146 G before the tier, issue
1353) that build is several times longer and may not fit the disk, so it is a
budget decision, not a bug fix — not taken here.

The lane's own preflight also refuses to call a Zephyr-less build complete
("the recorded fixture build did not cover every module: zephyr"), so building
the lane does not by itself make the preconditions pass — see issue 1685.

## Options (not decided)

1. Build `just build-test-fixtures lane=tier1` in the job and drop the bypass.
   Honest; costs the build above plus disk the runner may not have.
2. Narrow the RUN to what the job builds — a lane (or coordinate file) whose
   rows are exactly `build-fixture-rust-core` + `build-workspace-fixtures`.
   Cheap; the job stops claiming tier 1, which `check-tier-has-ci-owner`
   would then report as unowned unless another lane takes it.
3. Move the tier to a self-hosted runner with the disk for it.

## Acceptance

The integration job's fixture producer and its `test-all` scope are the same
set, with no `NROS_SKIP_FIXTURE_CHECK`, and the `MISSING for an in-lane
coordinate` class reads zero.

## 2026-10-06 — option 3 taken: tier 1's run moves to the self-hosted runner

Measured first, and it decided the option: building what the lane asserts
grew a fresh worktree **+37 G** for the linux coordinates ALONE (unfinished,
26 min on 32 cores), against ~16 G free on the hosted runner. A split of tier
1 by provisioning (a host half / an SDK half) saves nothing there, because the
linux half IS the cost. So:

- `host-tests.yml`'s `integration` job is DELETED (its subset build and its
  `NROS_SKIP_FIXTURE_CHECK=1` with it). host-tests keeps `unit` and tier 1's
  `gates` half.
- `run-matrix.yml` gains a `tier1` job on the self-hosted labels:
  `just setup tier1` -> `just build tier1` (codegen + `build-test-fixtures
  lane=tier1`) -> `just ci tier1 run`, NO bypass, after the tier-2 job
  (`needs: [matrix]` + `always()`; the runner is single-occupancy), with the
  tier-2 stage report (`coverage-tier1`, registered in `lane-stage.py`).
- `tier-health.py` / nightly's `elsewhere` map name the new home.

Measured locally on the full lane once issue 1700 was fixed (#1707):
`just build-test-fixtures lane=tier1` exits 0, and over it `just ci tier1 run`
reports 24 real failures. The `MISSING for an in-lane coordinate` class is
**7, not 0**: five `build/cargo-fixtures/baremetal/…` rows that no row
attributes (the 0828 fail-closed shape — tier 1 does not build them, so the
run cannot skip them) and the `subscription_with_info` compile-check stamp.
Those were closed the same day by #1728 — next section.

STILL OPEN until the first `run-matrix` `tier1` run reports — that run, on
the runner that will own the lane, is the acceptance.

## 2026-10-06 — the `MISSING for an in-lane coordinate` class reads ZERO locally

A complete local `lane=tier1` build (`build-test-fixtures` exits 0, after
issue 1700's fix), then `just ci tier1 run`: 2688 tests, **0 MISSING-in-lane**
(was 7, and 199 on the hosted subset), **0 capability skips** (was 64 on
this host, 81 on the hosted runner), 127 lane deselections, 17 real failures.
What closed the last seven:

- **Five baremetal images** (`build/cargo-fixtures/baremetal/…`): a shared
  group dir attributes to no single ROW, so the row-level lane skip could
  never fire, and `require_shared_fixture_binary` reported "MISSING" in a
  lane that selects no baremetal coordinate. The resolver knows the group's
  PLATFORM and now asks the platform-level lane question first
  (`fixtures::lane::platforms_selected`, the one spelling #1699 introduced):
  out of lane is `[SKIPPED:lane]`, in lane but absent still fails hard.
  `attribute_path`'s None-never-skips contract is unchanged.
- **`lane_scope::admits`** keyed on `NROS_TEST_SCOPE=native`, which no recipe
  has set since phase-395 W19, so under tier 1 it admitted everything and
  `baremetal_board_run_executes_run_plan` booted QEMU for 11 s before failing.
  It now asks the same coordinate question (`fixtures::lane::platform_admitted`);
  its five consumers and `every_cell_iterating_test_is_classified` are unchanged.
- **`subscription_with_info`** was NOT a lane-build omission (the tier-1 build
  runs every compile-check row): the snippet stopped COMPILING when phase-456
  W7 made every C++ subscription state its receive bound, and its stand-in
  message had no `SERIALIZED_SIZE_MAX`. Fixed in the snippet; the stamp is
  produced again.
- **`test_esp32_workspace_entry_e2e`** (not in the MISSING class, but the same
  ordering): its probes pass on a host with an ESP32 toolchain, so it
  `.expect()`ed an ELF the lane never built. Deselects first now.

Remaining reds are the known residue (1686, 1687, 1688, 1690, 1691, 1692),
1703 (#1720, queued), and `sched_dims_applied`'s nested skip marker.

STILL OPEN: the acceptance names the CI job, and the first `run-matrix` tier-1
run (#1708, draft) is that proof.

## 2026-10-07 — the hosted-runner build (68c2ff9d0) is superseded

`68c2ff9d0` took option 1 the same day: build `lane=tier1` inside the hosted
`integration` job, with a disk reclaim and `timeout-minutes` 300. Its first
run (workflow_dispatch 37547660332) failed in `just setup tier1` after four
minutes: the hosted image has no `west`/`pyelftools`/`pykwalify`, so Zephyr
provisioning refused, and the disk fit was never reached. Option 3 stands by
the maintainer's decision; the job that commit rewrote is deleted here, and
the self-hosted runner, which already provisions Zephyr, owns the run.
