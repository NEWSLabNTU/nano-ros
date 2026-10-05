---
id: 1671
title: "One row's link failure in a nightly platform job withholds every cell
  verdict on that platform — the build stops at the first red row and the Test
  step never runs"
status: resolved
type: tech-debt
area: [ci, testing, build]
severity: medium
found: 2026-10-03
related: [1658, 1657, 1158, 1226]
---

## What

The nightly `platform` job runs `just <plat> build-all` and then, only if that
step succeeded, `just <plat> test`. The build is `set -e` row by row, so the
FIRST row that fails to build ends the job before any cell runs — including
cells whose fixtures built fine or would have.

Measured on run `36977810939` (2026-10-02, `freertos`): the one red row was
`examples/workspaces/realtime-cpp` (`freertos_entry section '.bss' will not
fit in region 'RAM' … overflowed by 151016 bytes`, job log line 7868). That
night's tree also carried issue 1657, which `rtos_e2e`'s six FreeRTOS C/C++
cells catch (6 of 6 red with the fix reverted, measured for issue 1658) — and
none of them ran, so the second regression hid behind the first. Issue 1158 is
the same shape for tier 2 ("how far did it get").

## Why it matters

A red build step is a real verdict about ONE row, but the job turns it into
no verdict about every other row on the platform. The next regression on any
of them lands unseen for as long as the first stays red, which is exactly how
1657 sat on main.

## Direction

Either keep building past a failed row (collect, report every failed row, exit
non-zero at the end) and run the Test step `if: always()` after a build
FAILURE (not after a cancellation), so cells whose fixtures exist still report
— a missing fixture for the broken row then fails in its own cell, which names
it; or split the build so a failure in one road (workspace C/C++, single-node,
Rust) does not stop the others. The job stays red either way; what changes is
how many cells say why.

## Resolution

Fixed 2026-10-05 on `fix/1671-one-red-row-withholds-no-cells`, the first of
the issue's two directions: rows build independently, the Test step runs after
a failed build, and the job stays red.

* `scripts/build/fixture-row-ledger.sh` — `nros_fixture_row <label> <cmd…>`.
  With `NROS_FIXTURE_FAILED_ROWS` unset it IS the wrapped command (status and
  `set -e` unchanged, so every developer invocation is what it was). With it
  set, a failed row is appended to that ledger and the builder moves to the
  next row. The row runs as `( cmd ) & wait $!`: the obvious
  `cmd || rc=$?` suspends `set -e` for the whole row body, so a row failing
  midway would run on and "pass" on its last command.
* Both shared row builders route EVERY row through it:
  `scripts/build/fixtures-build.sh` (serial loop and the per-row make
  targets) and `scripts/build/workspace-fixtures-build.sh` (serial loop, the
  records-file loop each pooled make group re-enters). These build every row
  of the six nightly platform modules (qemu, freertos, nuttx, threadx_linux,
  threadx_riscv64, esp32). A failure that is NOT a row — a missing SDK, a sync
  error, a jobserver stall — still stops the step where it happens.
* `scripts/ci/fixture-rows-keep-going.sh <ledger> -- <cmd…>` owns the verdict:
  truncates the ledger, runs the step with it set, lists every failed row (and
  in `$GITHUB_STEP_SUMMARY`), exits 1 if any failed, and keeps the command's
  own status when the command itself failed.
* `nightly.yml`'s platform job: Build and Test both run through it, and Test
  is `!cancelled() && steps.build.outcome != 'skipped' && <schedule/e2e>` —
  it runs after a FAILED build, never after a cancellation or when an earlier
  step stopped the build from starting. (`test` depends on `build-fixtures`,
  which revisits the failed row; the wrapper keeps that from ending the step
  before nextest.) Step names unchanged, so `nightly-triage` still reads a red
  Build as a verdict.
* Gate `check-fixture-row-keep-going` (fast line): unset propagates and stops;
  set records and continues; `-e` holds inside a kept-going row (negative
  control: the `cmd || rc=$?` spelling fails it, measured); the owner fails on
  a recorded row, passes on none, keeps a command's rc, ignores a stale ledger;
  neither builder calls a row function except through the wrapper.

**Measured** on `threadx_linux` (host-built, so the whole lane runs here), with
`#error` appended to `examples/threadx-linux/c/action-client/src/main.c`
(reverted after):

| | BEFORE (`just threadx_linux build-all`) | AFTER (through the wrapper) |
| --- | --- | --- |
| build | rc 2 at `fixture-0005` (c/action-client, zenoh); the cyclonedds C rows and every C++ row never visited | rc 1; both rows sharing that source listed (`c zenoh` and `c cyclonedds: examples/threadx-linux/c/action-client rc=1`), every other row built through `[555/555] Linking CXX executable cpp_action_server` |
| cells | none (the nightly Test step is skipped after a red Build) | `9 tests run: 8 passed, 1 failed` — the one failure is `test_rtos_action_e2e … ThreadxLinux::Lang__C`, `not prebuilt: …/examples/threadx-linux/c/action-client/build-zenoh`, i.e. the broken row's own cell naming it; the step exits 1 |

Not measured: the workflow itself (it runs only on the nightly cron); a
platform whose failing row is in a cargo (Rust) row rather than a cmake one —
the same wrapper covers both lanes of `fixtures-build.sh`, read not run. The
issue's second direction (splitting the build per road) was not needed.

Sweep: `grep -nE '"\$fn" "\$line"|build_workspace "\$record"' scripts/build/*.sh`
(both now only inside `nros_fixture_row` / `nros_ws_row`).
