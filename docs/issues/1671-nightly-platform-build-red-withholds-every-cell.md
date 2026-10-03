---
id: 1671
title: "One row's link failure in a nightly platform job withholds every cell
  verdict on that platform — the build stops at the first red row and the Test
  step never runs"
status: open
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
