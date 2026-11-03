---
id: 2226
title: "`build-test-fixtures` orders every family after zephyr, so one Zephyr leaf failure silently builds no other family"
status: resolved
type: bug
severity: medium
area: [build, testing]
related: [2224, 2221]
found: 2026-11-03
---

## What

The generated `build-test-fixtures.mk` declares every non-Zephyr family with an
order-only prerequisite on zephyr:

```
all: zephyr linux qemu freertos nuttx threadx_linux threadx_riscv64 px4
zephyr: ...
linux: | zephyr
qemu: | zephyr
...
```

Zephyr runs first and alone on purpose (it takes the whole job budget). But make
does not run a target whose prerequisite failed, order-only or not. So when one
Zephyr leaf fails, `linux`, `qemu`, `freertos`, `nuttx`, `threadx_linux`,
`threadx_riscv64` and `px4` are never started. The recipe then exits 2.

## Measured

`just build-test-fixtures lane=all` on main (`75d49a3541`), twice: the zephyr
family failed on one leaf (issue 2224), and the log holds only `== zephyr ==`.
No other family line appears. A test run over the issue 2221 lanes then reported
116 of 177 red, almost all `[UNMET PRECONDITION] fixture not built`, for
families that had a working build. Building each family by hand
(`just <family> build-fixtures`) succeeded for all six.

The summary reads like a Zephyr failure, which it is, and says nothing about
the six families it cost. A reader who skims for "which family failed" gets
one name where seven are missing.

## Resolution

Order is kept; failure no longer propagates through it. Each generated stage
rule records its rc in the joblog (it already did) and exits 0 after printing
`FAILED`, so make starts every family whatever happened before. Once make
returns, `report_stages` reads the joblog, names every stage whose rc is
neither 0 nor 78 (`build-test-fixtures: FAILED stage(s): zephyr(rc=1)`), and
fails the recipe. The stamp is still written only after a successful build.

The class was wider than zephyr. Without `-k`, make starts no further target
after ANY failure, so a failing qemu stage also stopped whichever families had
not started yet. And the serial `NROS_JOBSERVER=1` launcher had the same shape
under `set -e`: its loop ended at the first failing stage. Both launchers now
run every stage and share `report_stages`.

Measured with a `just` shim on PATH that fails `zephyr build-fixtures` (rc=1)
and passes the other families' `build-fixtures`:
- make launcher: all eight stages ran (`== linux == OK` … `== px4 == OK`), the
  summary named `zephyr(rc=1)`, the recipe exited 1.
- `NROS_JOBSERVER=1` launcher: same eight stages, same summary, exit 1.
- shim with no failure: `All test fixtures built.`, exit 0.

Before the fix, two real `lane=all` builds ran zephyr only.
