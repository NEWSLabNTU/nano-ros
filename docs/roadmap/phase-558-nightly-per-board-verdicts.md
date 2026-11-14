# Phase 558 -- Nightly verdicts per board, not per platform family

**Status (2026-11-14). OPEN: W1 to W6 done; W7 (Zephyr's second board)
open.**

User ruling, 2026-11-13, verbatim: "Have results per board instead of per
platform family".

Implements the ruling as an amendment to RFC-0122 D8 (the `daily` row) and
RFC-0114 D10's `nightly_token` wording.

## Why

`nightly.yml`'s 07:00 `platform` job was one job per `just` module. Three
registry rows share `nightly_token = "freertos"` and two share `nuttx`
(`packages/boards/board-support.toml`), so one red board withdrew every
sibling board's daily verdict. `freertos` and `nuttx` were red on every 07:00
run measured (runs 42773101191, 42977216883, 43172962836), and neither red
said which board it was about.

## Design

- **The job is the board.** The matrix is the daily registry rows whose module
  the sweep can run, one entry per row, `{board: <matrix_platform>, module:
  <nightly_token>}`, derived by `scripts/ci/nightly-boards.py` in the
  `changes` job. The job's display name is the board, so every reader of a
  verdict (`tier-health.py`, `record-lane-runs.py`, `nros board`,
  `nightly-triage.py`) reads one board's result.
- **The board narrows its module.** The job still runs its module's verbs
  (`just setup|build-all|test <module>`), with `NROS_SCOPE_BOARD=<board>`.
  `nros_scope_coords_file` then writes that board's slice
  (`lane-coords --scope <module> --board <PlatformId>`), which the job exports
  as `NROS_FIXTURE_COORDS` for every build and which `just test <module>` uses
  as `NROS_TEST_COORDS`. One file, so a board's build and run cannot select
  different sets (issue 0482's rule).
- **`nightly_token` names the module, the board names the verdict.** No second
  hand list: the module set is still the lane-derived `runnable` set, and the
  board set is the registry.

## Work items

### W1 -- A board slice of a module scope

- **Status:** DONE (2026-11-13, branch `ci/nightly-per-board`)

`lane-coords --scope <module> --board <PlatformId>` prints the rows of that
board's fixture tokens and refuses a board the module does not own.
`nros_scope_coords_file` passes `--board "$NROS_SCOPE_BOARD"` when set, writes
its own file name, and is FATAL on a refusal (an unscoped fallback would hand
one board's verdict every other board's tests).

**Acceptance:** `NROS_SCOPE_BOARD=NuttxRiscv nros_scope_coords_file nuttx`
writes only `nuttx-riscv` rows; `NROS_SCOPE_BOARD=Bogus` fails naming
`NuttxArm, NuttxRiscv`. Measured.

### W2 -- Module stages that belong to one board skip on the other

- **Status:** DONE (2026-11-13, branch `ci/nightly-per-board`)

The coordinate-filtered builders already narrow. The stages that do not go
through them now ask `nros_lane_wants_platform`, as nuttx's arm/riscv fixture
halves already did:

- `just freertos`: `_examples-leaves` and `build-fixture-extras` (MPS2-AN385)
  on `freertos`; `build-fixtures-posix` on `freertos-posix`.
- `just nuttx`: `build` (the arm kernel) and `build-examples` on `nuttx`.

**Acceptance:** with a NuttxRiscv slice, `just nuttx build` prints
`nuttx skip: the arm kernel: no nuttx (arm) coordinate in this run`. Measured.

### W3 -- `nightly.yml` runs one job per board

- **Status:** DONE (2026-11-13, branch `ci/nightly-per-board`)

`changes` emits `boards`; `platform` is `matrix: include:` over it, named
`${{ matrix.board }}`, with a `Board scope` step that exports
`NROS_FIXTURE_COORDS`. The C-port smokes moved to their boards (`ThreadxLinux`,
`FreertosPosix`). Artifacts are `nightly-<board>-junit` / `-fixture-logs`.

On the live registry the matrix is 7 jobs where it was 5: `FreertosMps2`,
`FreertosPosix`, `NuttxArm`, `NuttxRiscv`, `QemuBaremetal`, `ThreadxLinux`,
`ThreadxRiscv64`.

### W4 -- Every verdict reader keys on the board

- **Status:** DONE (2026-11-13, branch `ci/nightly-per-board`)

- `check-cadence-jobs`: a daily job is served only when it is named
  `${{ matrix.board }}`, its matrix is `include:` over `boards`, the `changes`
  job derives `boards` with `nightly-boards.py`, and it runs
  `just test ${{ matrix.module }}`. A daily row with no `matrix_platform` is
  unserved. Three new selftest cases (a per-family job, a hand matrix, a
  boardless row); the baseline is unchanged (ZephyrQemuCortexM, issue 2462).
- `tier-health.py`: a daily row's lane is the job named after its board. The
  zephyr-* jobs build `native_sim` only, so ZephyrQemuCortexM no longer borrows
  their verdict and reports UNCHECKED (issue 2462).
- `gen-board-support.py` emits `nightly_job`, and `nros board` matches a row's
  last run by it (falling back to `nightly_token` for a record cut before the
  split).
- `record-lane-runs.py`: wording.

### W5 -- RFC wording

- **Status:** DONE (2026-11-13, branch `ci/nightly-per-board`)

RFC-0122 D8's `daily` row and an amendment note; RFC-0114's `nightly_token`
predicate and its open question.

### W6 -- The hosted-minute delta, measured

- **Status:** DONE (2026-11-14, runs 43383470982 and 43559931051)

Estimated from the three runs above: each platform job pays a fixed ~6-10
min before it builds anything (container init ~80 s, CLI build 190-375 s,
system provision ~25 s, `just setup` 85-140 s). Two more jobs is about
+16 hosted min a night in setup. Shared stages that both boards of a module
still run (the module's `just setup`, the CLI) are counted in that; the
fixture builds split rather than repeat.

**Acceptance:** the first scheduled 07:00 run after merge, per-job and total
minutes against run 43172962836's 5-job total, recorded here.

**Measured.** The per-board matrix first ran on the 07:00 cron of 2026-11-13
(run 43383470982, `509f86c693`, which contains #3715). Phase-559 W4b then
folded the 07:00 cron into the 05:00 one, so the second sample is run
43559931051 (2026-11-14 05:06). Minutes are job wall time from the API
(`completedAt - startedAt`). "Fixed" is every step before `Reclaim disk` /
`Build (<board>)`: container init, checkout, the CLI build and its cache, the
system provision, `just setup <module>` and the board scope.

| run | jobs | total min | fixed min | fixed per job |
| --- | ---: | ---: | ---: | ---: |
| 42773101191 (per module) | 5 | 320.3 | 39.0 | 7.8 |
| 42977216883 (per module) | 5 | 454.2 | 36.3 | 7.3 |
| 43172962836 (per module, the baseline) | 5 | 313.6 | 45.1 | 9.0 |
| 43383470982 (per board) | 7 | 596.5 | 75.9 | 10.8 |
| 43559931051 (per board) | 7 | 662.8 | 42.9 | 6.1 |

Per job, per board (total / build step / test step, min):

| board | 43383470982 | 43559931051 |
| --- | --- | --- |
| FreertosMps2 | 121.2 / 85.7 / 23.6 | 155.9 / 101.0 / 47.7 |
| FreertosPosix | 36.8 / 19.7 / 7.9 | 29.9 / 17.6 / 6.3 |
| NuttxArm | 89.1 / 59.3 / 18.6 | 117.6 / 79.0 / 32.4 |
| NuttxRiscv | 26.5 / 10.7 / 3.7 | 13.0 / 5.9 / 2.2 |
| QemuBaremetal | 50.6 / 29.1 / 9.9 | 55.5 / 35.6 / 13.8 |
| ThreadxLinux | 171.4 / 107.5 / 52.1 | 152.1 / 95.4 / 49.7 |
| ThreadxRiscv64 | 101.0 / 73.9 / 17.4 | 138.9 / 91.2 / 40.8 |

What this says:

- **The fixed cost is what the estimate said.** The per-module runs averaged
  40.1 fixed min a night, the per-board runs 59.4: +19 min, against the
  estimated +16. Per job it stays 6-11 min.
- **The total doubled, but the totals are not like for like**, so they do not
  measure the split. Every job of the baseline failed in its build step, and
  a failing build stops early: 43172962836's `qemu` ended at 18.3 min where a
  passing `qemu` took 40.9-59.8. In each per-board run, three boards
  pass their builds and run their whole test step. `ThreadxLinux` is one
  board in both shapes and took 152-171 min where `threadx_linux` took
  57-147 min, so more of the night is spent on what each board now reaches,
  not on duplicated setup.
- **The split did split the fixture builds.** The two boards of a module do
  not both build the module's fixtures: `FreertosPosix` builds in 18-20 min
  and `NuttxRiscv` in 6-11 min, beside 86-101 and 59-79 for their siblings.

A like-for-like total needs two runs where the same boards pass. That is a
property of the nightly being green, not of this phase, so it is not an item
here.

### W7 -- Zephyr's second board

- **Status:** OPEN

The zephyr-* jobs (05:00 cron) build `native_sim/native/64` only, so they are
ZephyrNativeSim's. ZephyrQemuCortexM has no daily job at all; that is issue
2462 (phase-535 W8.4) and stays baselined in `cadence-job-baseline.toml`.
This phase only stops the borrowing (W4).
