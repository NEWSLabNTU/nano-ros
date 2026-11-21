# Phase 585 — A board's status reports what its CI lane said

**Status (2026-11-22). OPEN: design ruled by the user (all four
[RFC-0114 D13](../design/0114-board-support-registry-primitives.md#amendment-2026-11-22-d13--a-boards-status-is-what-ran-collected-automatically)
decisions as recommended); no item started. Owner: nros 243.** Implements RFC-0114 D13, whose goal
is the user's: no manual maintenance of board status.
- a test's board is collected from the coordinate it resolves, with no
  annotation, into one results file per run;
- `runs`, `cadence`, `last_run` and breadth's cells are observed, not typed;
- the job summary, the book and `nros board list --status` report the same
  records.

## Why

- The status word says a lane EXISTS, never that it PASSES. On 2026-11-22
  the nightly's `ThreadxLinux` job was red while the book said
  ThreadX-on-Linux is "Verified on host (daily)"; 2026-11-18 and 2026-11-19
  had other reds.
- `check-board-registry` and `check-cadence-jobs` hold that a lane exists,
  so both were green.

## Decisions (ruled 2026-11-22, user)

All four of RFC-0114 D13's, as recommended: the typed `runs`, `cadence` and
`last_run` are retired for observed ones; the word degrades after three
cadence periods; manual reports go on the `board-evidence` orphan branch;
the book is rendered at deploy with a published `board-status.json`, N = 6.

## Zephyr runtime coverage (ruled 2026-11-22, user)

W2b measured where Zephyr's scheduled runtime evidence comes from. The
nightly's Zephyr jobs only build; `merge-runtime.yml` is dispatch-only; tier 2
(`run-matrix.yml`) would run Zephyr cells but stopped in its build on each of
the five scheduled runs to 2026-11-20; the one scheduled Zephyr runtime is
`live-peer.yml`'s board-zephyr runner (two interop rows). So the registry's
"Verified on host (daily)" for ZephyrNativeSim was not backed by its matrix
cells running.

**Ruling: fix tier 2 first, with no dedicated Zephyr nightly job.** Tier 2's
Zephyr cells become the evidence once its build is green; its build red is
nros 175's parameter-store / `MAX_CBS` work. Until then, the Zephyr rows read
"Not run since <date>" under D13.5, and that is the CORRECT reading, not a
defect of this phase: the derivation is reporting what ran.

## Work items

Each item is one PR, claimed with `just claim phase-585-wN` before it starts,
in the order below.

### W1 — The evidence ledger, from the resolver

- **Status:** OPEN
- **What:** every `fixtures::lane` attribution entry point (`attribute_path`,
  `attribute_workspace_id`, and the interop `assert_test_bound`) appends one
  JSONL line (test name, row, coordinate) to `$NROS_TEST_EVIDENCE_DIR` when
  it is set. No test changes.
- **Acceptance:** a self-test runs each entry point under a temp ledger and
  finds exactly one line each; a real `cargo nextest` of one emulator test
  writes its coordinate.

### W2 — The collector, the results file and the job summary

- **Status:** OPEN
- **What:** `scripts/ci/board-results.py` joins the junit with the ledgers,
  maps each coordinate to its registry row (refusing one with none), adds the
  job-level no-verdict signal (nightly-triage's step classifier, moved to
  `scripts/lib/`), writes `board-results.json` (D13.3) and a per-board table
  into `$GITHUB_STEP_SUMMARY`. A final `board-results` job in `nightly.yml`,
  `weekly.yml`, `merge-runtime.yml` and `host-tests.yml` runs it and uploads
  the file (90 days). The board jobs set `NROS_TEST_EVIDENCE_DIR` and upload
  the ledger with the junit.
- **Acceptance:** the self-test over recorded inputs (a pass, a failed cell,
  a set-up failure, a cancelled job, an unmapped coordinate, `Linux`
  evidenced by another job); one real nightly's summary shows every board.

### W3 — The derivation, and the typed fields retired

- **Status:** OPEN
- **What:** `board_registry.status_from_evidence(row, records, today)`: D13.5's
  words and D13.4's observed `runs`, `cadence`, `last_run` and breadth cells,
  with `outcome_window` as a registry value. A shared fixture (histories and
  expected words) run by the Python self-test and a Rust test of the CLI's
  reader. Then `runs`, `cadence` and `last_run` are deleted from every row,
  and `check-board-registry` refuses them.
- **Acceptance:** both readers pass the fixture; the registry has no typed
  evidence field; with no records every view says "not recorded".
- **Progress (2026-11-22).** The derivation and its shared fixture are in
  (`board_registry.status_from_evidence`,
  `scripts/lib/testdata/board-status-cases.json`, 14 cases, run by the
  registry's self-test in `check-board-tiers.py`; a window mutation fails the
  weekly case). **Retiring `runs`, `cadence` and `last_run` moves to land WITH
  W4**: the committed pages are rendered from the typed fields today, and with
  them gone and no deploy-time render yet, every committed status would read
  "not recorded". The Rust reader of the fixture lands with W5.

### W4 — The book, rendered at deploy

- **Status:** OPEN
- **What:** marker regions on `support-status.md` (last N runs as marks,
  last red date and why, the labelled claim, the cells that ran) and
  `supported-boards.md` (the word and a "Last run:" line), filled by an mdBook
  preprocessor through W3; the agreed wording; no region on the levels page,
  starters, Tutorial or release notes. `publish.yml`'s `deploy` gains a
  `workflow_run` trigger on the nightly, the weekly and `host-tests`, collects
  the newest results and the `board-evidence` branch, and publishes
  `board-status.json` at the site root.
- **Acceptance:** `just book` builds with and without results; a recorded
  history renders "Last 6 runs, newest first: ✓ ✓ ✗ ✓ ✓ ✓", each mark with
  its `aria-label`, the summary sentence, and "Failing since <date>" for that
  board only; a no-verdict run renders "–", never ✗.

### W5 — The CLI: `nros board list --status`

- **Status:** OPEN
- **What:** `record-lane-runs.py` writes the results history into
  `board-support-runs.json` at release; `nros board list --status`, `show` and
  `nros doctor --report` apply W3's derivation offline; `--online` reads the
  published `board-status.json`.
- **Acceptance:** an installed release shows the same word and marks as the
  book for one recorded history; a checkout says "not recorded in this
  checkout".

### W6 — Manual runs: `nros board report`

- **Status:** OPEN
- **What:** `nros board report <board>` runs the board's declared cells
  locally through the same resolver and writes a `report`-source results
  file; `--submit` adds it to the `board-evidence` branch. W3 reads it for
  `last_run` and `runs = hardware`.
- **Acceptance:** a report on a host board produces a record identical in
  shape to CI's; the PX4 SITL row's "Boots, manual (date)" comes from a
  report, with no typed date.

### W7 — The consistency gate

- **Status:** OPEN
- **What:** D13.8's static checks on the fast line: every fixture and cell
  platform has a row; every non-infra row has a declared cell; the ledger
  self-test; the collector self-test; the markers on exactly D13.7's pages.
- **Acceptance:** each check has a negative control (a fixture platform with
  no row, a row with no cell, an attribution path that writes no line, a
  marker on a starter), and the tree passes.

### W8 — `host-tests` keeps its junit and ledger

- **Status:** OPEN (approved 2026-11-22, after W2)
- **What:** `host-tests.yml` uploads no junit today, so the Linux cells it
  runs on its schedule carry no evidence. Upload its junit with the evidence
  ledger (from `$GITHUB_WORKSPACE`, as W2 does, since its jobs run in
  containers) and add its `board-results` job.
- **Acceptance:** a scheduled `host-tests` run's summary lists the Linux row
  with the cells it ran.

## Not in this phase

- Any outcome, cadence or date typed into the registry.
- Proving a `report` record was made on real hardware: it is trusted as its
  submitter is, as the typed `last_run` was.
- A hardware runner: `runs = "hardware"` rows get outcomes the same way once
  a lane exists.
