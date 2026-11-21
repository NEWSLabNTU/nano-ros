# Phase 585 — A board's status reports what its CI lane said

**Status (2026-11-22). DRAFT: design only, awaiting the user's decisions on
[RFC-0114 D13](../design/0114-board-support-registry-primitives.md#amendment-2026-11-22-draft-d13--a-status-reports-what-the-lane-said-not-only-that-it-exists).
No item is started. Owner: nros 243.** Implements RFC-0114 D13:
- every automated lane records an outcome per registry row per run;
- the status word degrades when the row's lane has not passed recently;
- the book and the CLI show the same, without committing CI output.

## Why

- The status word says a lane EXISTS, never that it PASSES. On 2026-11-22
  the nightly's `ThreadxLinux` job was red while the book said
  ThreadX-on-Linux is "Verified on host (daily)"; 2026-11-18 and 2026-11-19
  had other reds.
- `check-board-registry` and `check-cadence-jobs` hold that a lane exists,
  so both were green.

## Decisions it waits on

The four in RFC-0114 D13's "Decisions for the user": qualifier or degraded
word (and the window), the deploy-time render, row or cell outcomes, and the
"Not run since" reading. The items below assume the recommendations; a
different ruling changes W3 and W4 only.

## Work items

Each item is one PR, claimed with `just claim phase-585-wN` before it starts,
in the order below.

### W1 — The collector and its results artifact

- **Status:** OPEN
- **What:** `scripts/ci/board-results.py`: read one workflow run's jobs and
  steps (`gh run view --json jobs`), map each job to its registry row by the
  derivation that named it, classify it with `nightly-triage.py`'s step
  classifier (moved into a shared `scripts/lib/` module, not copied), and
  write `board-results.json` (schema 1, keyed by `row_name`). A final
  `board-results` job in `nightly.yml`, `weekly.yml` and `merge-runtime.yml`
  (`needs:` every board job, `if: always()`) runs it and uploads the
  artifact, retained 90 days. An unmapped job fails the collector, naming
  it.
- **Acceptance:** the self-test over recorded run JSON (a pass, a verdict
  fail, a set-up fail, a cancelled job, a job that never started, an
  unmapped job) gives the expected file; one real nightly produces it.

### W2 — The derivation, in one place

- **Status:** OPEN
- **What:** `board_registry.outcome_status(row, history, today)`: the D13 Q3
  rule (window, "Failing since", "Not run since"). The generator writes its
  thresholds (`outcome_window`) into `board-support-status.json`. A shared
  fixture of histories and expected words is read by the Python self-test and
  by a Rust test of the CLI's reader.
- **Acceptance:** both readers pass the same fixture; a mutation of the
  window in one fails the other's test.

### W3 — The book: deploy-time render

- **Status:** OPEN
- **What:** the generator writes outcome marker regions on
  `supported-boards.md` and `support-status.md` only (D13 Q4's table). An
  mdBook preprocessor fills them from the results, through W2, with the
  agreed wording; with no results it writes "Last run: not recorded in this
  build of the book." `publish.yml`'s `deploy` gains a `workflow_run`
  trigger on the nightly and the weekly, downloads their newest results
  (enough runs to find a streak's start, at most 30), and publishes the
  merged `board-status.json` at the site root. `board-support-levels.md`
  defines the outcome lines and degraded words.
- **Acceptance:** `just book` builds with and without results; a recorded
  red history renders "Failing since <date>" on `supported-boards.md` and
  leaves the starters and the levels page unchanged.

### W4 — The CLI, offline

- **Status:** OPEN
- **What:** `record-lane-runs.py` (issue 2325) writes the same per-row
  outcome history into `board-support-runs.json` at release; `nros board
  list|show` and `nros doctor --report` apply W2's rule to it with today's
  date. A checkout says "outcome not recorded in this checkout". `--online`
  reads the published `board-status.json`; nothing fetches implicitly.
- **Acceptance:** an installed release with a red history shows "Failing
  since <date>" offline; `--online` and offline agree on one history.

### W5 — The consistency gate

- **Status:** OPEN
- **What:** extend `check-board-registry` / `check-cadence-jobs` with D13
  Q6's checks: every automated row has a job the collector maps; every board
  job maps to one row; every fixture and `matrix::CELLS` platform has a row;
  the outcome markers are on exactly the pages Q4 names.
- **Acceptance:** each check has a negative control (a row with no job, a
  job with no row, a fixture platform with no row, a starter with a marker),
  and the tree passes.

## Not in this phase

- Per-cell outcomes from the junit (D13 Q1's later refinement).
- Any outcome typed into the registry.
- A hardware runner: `runs = "hardware"` rows get outcomes the same way once
  a lane exists.
