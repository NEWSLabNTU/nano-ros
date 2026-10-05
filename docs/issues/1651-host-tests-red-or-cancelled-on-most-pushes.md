---
id: 1651
title: "`host-tests.yml` has not gone green since 2026-06-17 — most runs are cancelled by the next merge, and the rest fail at `just ci tier1`"
status: open
type: bug
area: ci, testing
severity: high
found: 2026-10-03
related: [1521, 1644, 1226, 1158]
---

## What this is

`.github/workflows/host-tests.yml` is the only workflow that runs the
fixture-backed `nros-tests` integration suite on the host. Measured 2026-10-03
over its last 30 runs:

| outcome | count |
| --- | --- |
| `cancelled` | 24 |
| `failure` | 4 |
| in progress | 2 |
| `success` | **0** |

The most recent successful run is **2026-06-17** (a `schedule` event). Three and
a half months of a lane that reports nothing, while being the one place this
suite runs.

## Two mechanisms, both measured; neither root-caused here

1. **Cancellation.** The workflow fires on `push` (i.e. after every merge to
   `main`), `schedule` and `workflow_dispatch`. Its `workspace unit tests` job
   has a concurrency group with `cancel-in-progress: ${{ github.event_name ==
   'push' }}`, so each merge cancels the previous run's unit job — and the RUN
   then reads `cancelled`. The merge queue lands merges faster than the
   integration job finishes, so almost every run is superseded. The integration
   job's own group has `cancel-in-progress: false`, so its verdict may still
   exist inside a run the summary calls cancelled — but nobody reads per-job
   results of a cancelled run, which is the same signal loss.
2. **Failure.** When a run completes, `nros-tests integration (host)` fails at
   step `just ci tier1` (latest: run 36968648766); `workspace unit tests`
   passes. Which tests fail was not investigated here.

This is CLAUDE.md's "a red CI lane answers one of two questions and they look
identical" in its strongest form: a lane that never reports a pass cannot
report a regression either. Issue 1521 (tests no merge-gating lane ran) and
the three fixture-free tests PR #1581 found already red on `main` both surfaced
because nothing here was being read.

## What to do

1. **Get a verdict.** Run the integration job to completion on current `main`
   (`workflow_dispatch`) and record which tests fail. Retest any QEMU red solo
   before believing it (CLAUDE.md: full-sweep QEMU lanes flake under load), and
   check fixture mtimes against the code blamed before filing from it (issues
   0859-0862).
2. **Stop the cancellation from hiding the verdict.** Options, not decided
   here: scope `cancel-in-progress` so it never cancels the integration verdict
   on `main`; or drop `push` and run on `schedule` only, so each run completes;
   or make the run's summary reflect the integration job's own outcome. The
   decision belongs with whoever owns the CI budget.
3. **Then fix the reds**, one issue each if they are unrelated.

## Acceptance

A `host-tests.yml` run on `main` completes and reports `success`, and the lane's
configuration makes a later regression produce a `failure` someone sees rather
than a `cancelled`.

## Verdict, 2026-10-05 (still OPEN)

**Cancellation, re-measured.** `push` left the triggers on 2026-10-03 (commit
a287e940074f5653082b8795a733da405a3a77aa), so supersession is gone. The
cancellations since are the integration job's own 150-minute TIMEOUT, which
GitHub reports as `cancelled`: runs 37115523816 and 37176625915 (the
2026-10-04 schedule) reached `just ci tier1` ~82 min in, spent ~62 min in
`just check` (check-fast 788 s + check-build 2942 s at -P4) and were killed
before `test-all`. The two `failure`s of 2026-10-03 (37104028050,
37108493890) were `nros_rmw_cyclonedds_config_compose` running as root, fixed
by commit 462c2b75d95d77b4602b2127c864d8945397c621 and green in the 2026-10-04
run. `test-all` itself had not executed on any run since 2026-06-17.

**First `test-all` verdict** (dispatch 37252649866 on branch
`fix/1651-host-tests-verdict`, which runs `just check` in a sibling job):
the integration job COMPLETED (96 min) as a `failure` — 214 real failures and
81 undeclared capability skips. Grouped by root cause, each retested SOLO
(`-j1`) on fixtures built from the same tree by
`just build-test-fixtures lane=tier1`:

| cause | tests | issue |
| --- | --- | --- |
| job builds a fixture subset, runs the whole tier-1 lane | 199 + ~8 | 1684 |
| runner lacks Zephyr / riscv QEMU / PX4 / ROS peer | 81 skips | 1685 |
| XRCE C/C++ action + service round-trip | 4 | 1686 |
| declared / overridden QoS not advertised | 4 | 1687 |
| Rust workspace remap not on the wire | 1 | 1688 |
| C++ contract-monitor test asserted a spelling the line never had | 1 | 1689 (fixed) |
| `rmw_coordinate_truth` entry loop outlives 60 s | 1 | 1690 |
| `ros2_action_e2e` over zenoh: stock server not discovered (local only) | 1 | 1691 |
| `multihost_bake` tests a retired `--lang rust` verb | 1 | 1692 |
| probe self-test read the ambient `NROS_SKIP_FIXTURE_CHECK` | 1 | fixed here |

**Cancellation fix: not zero-cost, so not merged.** Splitting `just check`
into its own job (branch `fix/1651-host-tests-verdict`) lets the integration
job finish in ~96 min instead of timing out, but the new job re-provisions
~26–29 min (`just generate-bindings` alone ~17–20 min, the workspace syncs) —
about +25 runner-minutes a night over the 160 the timed-out pair used. Left
for the CI-budget owner, with the alternatives: raise the integration
`timeout-minutes` (~+30 min/night, same signal); or drop `just check` from
host-tests since gate.yml's nightly runs the same gates (saves ~62 min, but
gate.yml's `check build` is itself red/dying on disk, so this lane is its
only green).
