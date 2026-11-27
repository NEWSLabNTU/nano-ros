---
id: 2997
title: "Three boards read no verdict every night: the collector let one job's lane skip erase another's pass, QemuBaremetal's nightly runs no nros-tests cell, and NuttxRiscv's one cell is out of its own lane"
status: open
type: bug
severity: high
area: [ci, testing, boards]
related: [phase-585, issue-1226]
found: 2026-11-27
---

## What happened

Phase-585 W5's release recorder, run read-only against the repository on
2026-11-27, recorded each board's last six results. Three boards read "–"
(no verdict) in every one of them:

| board | last 6 results |
| --- | --- |
| `nros-board-mps2-an385@QemuBaremetal` | – – – – – – |
| `nros-board-nuttx-qemu@NuttxArm` | – – – – – – |
| `nros-board-nuttx-qemu@NuttxRiscv` | – – – – – – |

In nightly 45868131204 each of the three jobs PASSED. Each board's
`board-results` row read "the job passed, and its tests resolved no fixture of
this board". So a board looked covered to CI (a green job) and unverified to
its status, which is issue 1226's shape. RFC-0114 D13 derives the board's
word from these results, so all three would read "Not run since" or "Builds".

## Three causes, one per board

1. **NuttxArm: the collector (fixed here).** `board-results.py`'s
   `read_junit` merged every job's junit into one map keyed by
   `(binary, test)`. The NuttxRiscv job lane-skips the same `rtos_e2e` Nuttx
   cases that NuttxArm runs, and its artifact sorts later. So its
   `[SKIPPED:lane]` overwrote NuttxArm's 10 passes, and the collector dropped
   each ledger line as skipped. The artifacts held the evidence all along: 10
   passes and 23 ledger lines on `nuttx`.
2. **QemuBaremetal: the nightly runs no nros-tests cell.** Its test step,
   `just test qemu`, runs `qemu test-basic` and `qemu test-lan9118`, which
   are shell harnesses (`tests/run-test.sh`), not nextest. It writes no junit
   and no ledger, and its upload found no file. The board's runtime cells
   (`consumer_of` over `matrix::CELLS`: `baremetal_run_plan_runtime`,
   `emulator`, `logging_smoke`) never run in that job.
3. **NuttxRiscv: its one runtime cell is out of its own lane.**
   `c_riscv_nuttx_talker_delivers_cross_process` reads
   `[SKIPPED:lane] out of lane: workspace-rust-native-listener is at
   coordinate linux,rust,zenoh, which this run's lane does not select`. The
   cell needs a Linux peer fixture, and the NuttxRiscv job's scope coordinates
   hold only its own platform's. `realtime_sched_tasks` reports "22 out of
   lane". So nothing for the board runs there.

## The class

Every per-board nightly recipe HAND-LISTS the test binaries it runs
(`just/<plat>.just`, `_nextest-platform '<binaries>' …`). `consumer_of` is
the one assignment of a cell to the binary that answers it, and on
2026-11-27 each list was a subset of its board's runtime consumers:

| board | consumers (`matrix::CELLS`, Runtime) | nightly runs |
| --- | --- | --- |
| QemuBaremetal | baremetal_run_plan_runtime emulator logging_smoke | none (shell harnesses) |
| NuttxArm | entry_e2e logging_smoke param_store_carve realtime_sched_tasks_e2e rtos_e2e typed_carrier_e2e | rtos_e2e realtime_sched_tasks_e2e c_riscv_nuttx_e2e |
| ThreadxLinux | entry_e2e linux_api logging_smoke realtime_sched_tasks_e2e rtos_e2e | rtos_e2e |
| ThreadxRiscv64 | entry_e2e logging_smoke param_store_carve rtos_e2e threadx_riscv64_qemu typed_carrier_e2e | threadx_riscv64_qemu rtos_e2e |
| FreertosMps2 | bridge_network_e2e entry_e2e freertos_qemu lending_freertos_e2e logging_smoke port_templates_e2e realtime_sched_tasks_e2e rtos_e2e | rtos_e2e realtime_sched_tasks_e2e freertos_posix freertos_stack_guard |

Tier 1 and tier 2 (`run-matrix.yml`) run the matrix's cells, but that workflow
has no `board-results` job and sets no evidence ledger. So the cells that
run there are not evidence for any board's status.

## Fix

- **(1) Here:** the collector keys junit verdicts by ARTIFACT, the job that
  produced them, and reads a ledger line's verdict from its own artifact's
  junit. Over nightly 45868131204's real artifacts, NuttxArm now reads
  "passed 10/10". The self-test gains the cross-job collision; it fails
  without the fix (mutation-checked).
- **The evidence the matrix lanes already produce:** `run-matrix.yml`'s tier
  jobs set the ledger and upload it beside their junit, and a `board-results`
  job collects them. That covers QemuBaremetal's emulator cells and the other
  boards' unlisted consumers with no new test execution.
- **Report the class:** a board that has a registry row and a scheduled lane
  but had NO verdict in its recent runs across the scheduled lanes is named,
  in the daily lane-health report (`scripts/ci/lane-health.py`, phase-591
  W8), never left to read "Builds".
- **(3):** NuttxRiscv's scope coordinates include the coordinates of the
  peer fixtures its cells resolve, or the cell moves to a lane that has them.
  To be decided with the lane-scope owner.

## Acceptance

- The collector's cross-job case, mutation-checked (done here).
- A nightly, and a run-matrix run, whose `board-results` gives
  QemuBaremetal, NuttxArm and NuttxRiscv a verdict, or names the board in
  the lane-health report when it still has none.
