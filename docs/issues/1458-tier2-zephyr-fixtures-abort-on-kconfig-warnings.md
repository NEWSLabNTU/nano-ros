---
id: 1458
title: "tier 2 (1-wise) stops in its zephyr fixture build on `Aborting due to
  Kconfig warnings` — three west fixtures, and the warning that caused the abort
  is in a sub-log the job never prints"
status: open
type: bug
area: [ci, zephyr, testing]
severity: high
related: [1457, 1389, 1360, 1158]
found: 2026-09-22
---

## What happens

`run-matrix` run **35694989528** (schedule, 2026-09-22T06:28), job
**106639786094** (`tier 2 (1-wise matrix)`), step `just build tier2`. The lane's
own stage reporting says `tier 2 — NO VERDICT: stopped in the build`, and the
zephyr module is the one that stops:

```
== zephyr == FAILED (rc=1)
first error line(s) in …/tmp/build-test-fixtures-20260922-064731-404837/zephyr.log:
  137:error: Aborting due to Kconfig warnings
  139:CMake Error at …/zephyr/3.7/zephyr/cmake/modules/kconfig.cmake:389 (message):
  149:FATAL ERROR: command exited with status 1: cmake … -B…/west_bringup_zephyr \
        -DBOARD=native_sim/native/64 '-DCONF_FILE=prj.conf;prj-zenoh.conf' \
        -S…/packages/testing/nros-tests/fixtures/multi_pkg_workspace_zephyr/zephyr_app
  186:error: Aborting due to Kconfig warnings
  …
  251:error: Aborting due to Kconfig warnings
```

Three west fixtures off the same source tree
(`fixtures/multi_pkg_workspace_zephyr/zephyr_app`) — `west_bringup_zephyr`
(`prj-zenoh.conf`), `west_bringup_zephyr_cyclone_user_config`
(`prj-cyclonedds.conf`), and a third at line 251.

## What this is NOT

- **Not issue 1457.** That is the *nightly* lane (`tier2-nightly`, pairwise)
  stopping on `msg2idl.py failed … Duration.msg (exit 1)`. This is the 1-wise
  lane, a different error, at a different stage of the same module. Two tier-2
  variants, two stops — they should not be collapsed.
- **Not 1389 / 1360 / 1158 / 1387.** No schema-version refusal, no codegen
  `#error`, the lane does reach its build, and no foreign-checkout complaint.
- **Not a missing workspace.** The Zephyr 3.7 workspace resolves — the abort
  comes from `kconfig.cmake` inside it.

## The thing that blocks diagnosis

**The warning that caused the abort is not in the job log.** Zephyr aborts on
*any* Kconfig warning and prints it BEFORE the `error:` line; the fixture
runner only quotes the "first error line(s)" from `zephyr.log`, so the job
carries the abort and not its cause. An undefined symbol, a `select` on a
non-existent option and a type mismatch all render identically here.

The module log at
`tmp/build-test-fixtures-20260922-064731-404837/zephyr.log` has it around
line 130; that directory is on the runner and is not uploaded.

## What would close it

The three west fixtures configuring again. Before a fix, one measurement:
the Kconfig warning text itself — either by re-running
`just build-test-fixtures lane=tier2` locally and reading `zephyr.log`, or by
having the fixture runner quote the lines ABOVE `Aborting due to Kconfig
warnings` rather than the abort alone. The second is worth doing regardless:
a runner that reports "there was a warning" without the warning turns every
instance of this class into a manual reproduction.
