# Phase 558 -- Nightly verdicts per board, not per platform family

**Status (2026-11-13). OPEN: W1 to W5 done on the first PR; W6 (the measured
hosted-minute delta from the first scheduled run) and W7 (Zephyr's second
board) open.**

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

### W1 -- A board slice of a module scope -- DONE

`lane-coords --scope <module> --board <PlatformId>` prints the rows of that
board's fixture tokens and refuses a board the module does not own.
`nros_scope_coords_file` passes `--board "$NROS_SCOPE_BOARD"` when set, writes
its own file name, and is FATAL on a refusal (an unscoped fallback would hand
one board's verdict every other board's tests).

**Acceptance:** `NROS_SCOPE_BOARD=NuttxRiscv nros_scope_coords_file nuttx`
writes only `nuttx-riscv` rows; `NROS_SCOPE_BOARD=Bogus` fails naming
`NuttxArm, NuttxRiscv`. Measured.

### W2 -- Module stages that belong to one board skip on the other -- DONE

The coordinate-filtered builders already narrow. The stages that do not go
through them now ask `nros_lane_wants_platform`, as nuttx's arm/riscv fixture
halves already did:

- `just freertos`: `_examples-leaves` and `build-fixture-extras` (MPS2-AN385)
  on `freertos`; `build-fixtures-posix` on `freertos-posix`.
- `just nuttx`: `build` (the arm kernel) and `build-examples` on `nuttx`.

**Acceptance:** with a NuttxRiscv slice, `just nuttx build` prints
`nuttx skip: the arm kernel: no nuttx (arm) coordinate in this run`. Measured.

### W3 -- `nightly.yml` runs one job per board -- DONE

`changes` emits `boards`; `platform` is `matrix: include:` over it, named
`${{ matrix.board }}`, with a `Board scope` step that exports
`NROS_FIXTURE_COORDS`. The C-port smokes moved to their boards (`ThreadxLinux`,
`FreertosPosix`). Artifacts are `nightly-<board>-junit` / `-fixture-logs`.

On the live registry the matrix is 7 jobs where it was 5: `FreertosMps2`,
`FreertosPosix`, `NuttxArm`, `NuttxRiscv`, `QemuBaremetal`, `ThreadxLinux`,
`ThreadxRiscv64`.

### W4 -- Every verdict reader keys on the board -- DONE

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

### W5 -- RFC wording -- DONE

RFC-0122 D8's `daily` row and an amendment note; RFC-0114's `nightly_token`
predicate and its open question.

### W6 -- The hosted-minute delta, measured -- OPEN

Estimated from the three runs above: each platform job pays a fixed ~6-10
min before it builds anything (container init ~80 s, CLI build 190-375 s,
system provision ~25 s, `just setup` 85-140 s). Two more jobs is about
+16 hosted min a night in setup. Shared stages that both boards of a module
still run (the module's `just setup`, the CLI) are counted in that; the
fixture builds split rather than repeat.

**Acceptance:** the first scheduled 07:00 run after merge, per-job and total
minutes against run 43172962836's 5-job total, recorded here.

### W7 -- Zephyr's second board -- OPEN

The zephyr-* jobs (05:00 cron) build `native_sim/native/64` only, so they are
ZephyrNativeSim's. ZephyrQemuCortexM has no daily job at all; that is issue
2462 (phase-535 W8.4) and stays baselined in `cadence-job-baseline.toml`.
This phase only stops the borrowing (W4).
