---
id: 2226
title: "`build-test-fixtures` orders every family after zephyr, so one Zephyr leaf failure silently builds no other family"
status: open
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

## Fix direction

Keep the serialisation without the failure coupling. Either run the other
families after zephyr regardless of its status (a wrapper that records the
status and exits 0, with the overall rc computed from the joblog), or invoke
make with `-k`. Then report every family that did not build, by name.
