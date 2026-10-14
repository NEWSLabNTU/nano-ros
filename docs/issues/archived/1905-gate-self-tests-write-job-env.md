---
id: 1905
title: "A gate's self-test wrote the GitHub job's `$GITHUB_ENV`, so tier 1 ran every step after its build against a deleted temp tree"
status: resolved
type: bug
severity: high
area: [ci, testing]
related: [1832, 1226, phase-466, phase-489]
found: 2026-10-14
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — a scheduled lane that cannot reach its cells.

## Measured

`run-matrix` run 38615001083 (workflow_dispatch on `main` at `36ec94ba04`,
2026-10-14, after the runners were restarted on the new image). Job
`tier 1 (cells)`:

- `just build tier1` succeeded. Its step environment named the persistent
  tree: `NROS_FIXTURE_TREE: /home/runner/_work/nano-ros/nros-fixture-tree`.
- The next step, `Build-tree gate: artifact-identity-budget (issue 1832)`,
  ran with `NROS_FIXTURE_TREE: /tmp/tmp.lwHgabOJDL/_work/repo/nros-fixture-tree`
  and failed with `no build tree at
  examples/workspaces/mixed/build-workspace-fixtures`.
- The junit upload looked under the same temp path and found nothing. The
  lane-stage step reported "tier 1 — NO VERDICT: stopped in the build".

So the lane built everything and then never reached a cell. Between the two
steps, something had appended a sandbox path to the job's `$GITHUB_ENV`.

## Cause

`scripts/ci/fixture-tree.sh`'s `sync_tree` appends
`NROS_FIXTURE_TREE=$tree` to `$GITHUB_ENV`, as it should when a workflow
calls it. Its `--self-test` calls `sync_tree` seven times on trees under a
`mktemp -d`, and it inherited `$GITHUB_ENV` from the caller. The self-test
runs inside the `fixture-tree-persistence` gate, which `check fast` runs, and
`just build tier1` runs `check fast`. So inside a CI step, the gate wrote
seven sandbox paths into the JOB's environment file, and the last one won for
every later step.

Reproduced off CI: running the `main` version of the script with
`GITHUB_ENV` pointing at an empty file leaves seven lines in it, of the form
`NROS_FIXTURE_TREE=/tmp/tmp.XXXX/_work/repo/nros-fixture-tree`. That is the
shape of the path tier 1 saw.

A sibling with milder effect: `check-fixture-row-keep-going.sh` case D runs
`fixture-rows-keep-going.sh`, which appends failed rows to
`$GITHUB_STEP_SUMMARY`. The case's fake `x rc=1` row went onto the real job's
summary page.

## Fix

- **The gate runner** (`scripts/build/run-gates-parallel.sh`, used by both
  the serial and the parallel lanes) unsets `GITHUB_ENV`, `GITHUB_OUTPUT`,
  `GITHUB_PATH`, `GITHUB_STATE` and `GITHUB_STEP_SUMMARY` before any gate
  runs. A gate answers a question; it never writes the job's state. No gate
  reads these except as text in a workflow it inspects.
- **`fixture-tree.sh --self-test`** points `GITHUB_ENV` at a file in its own
  sandbox. It now also checks that `sync_tree` writes the line there, so the
  writer is tested rather than merely contained. With `GITHUB_ENV` set to a
  probe file, the probe stays empty.
- **`check-fixture-row-keep-going.sh`** points `GITHUB_STEP_SUMMARY` at a
  file in its own sandbox. With a probe file, the probe stays empty.

Sweep for the class (every script that writes a job file, then each caller
that reaches it from a test):

```
git grep -ln 'GITHUB_ENV\|GITHUB_OUTPUT\|GITHUB_STEP_SUMMARY\|GITHUB_PATH\|GITHUB_STATE' -- scripts
```

The other writers are `disk-report.sh`, `disk-transcript.sh`, `reclaim-disk.sh`,
`report-interlock-coverage.sh` and `lane-stage.py`. None of them is reached by
a self-test that inherits the job's files: `lane-stage.py --selftest` never
calls `report()`, and the rest have no self-test.

Acceptance is a scheduled `run-matrix` run whose tier-1 build-tree gates read
the persistent tree. That evidence comes from the next run, not from here.
