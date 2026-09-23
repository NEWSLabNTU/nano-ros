---
id: 1458
title: "tier 2 (1-wise) stops in its zephyr fixture build on `Aborting due to
  Kconfig warnings` — three west fixtures, and the warning that caused the abort
  is in a sub-log the job never prints"
status: resolved
type: bug
area: [ci, zephyr, testing]
severity: high
related: [1457, 1389, 1360, 1158, 1379, 1258]
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

## RESOLVED — 2026-09-23, phase-466 W3

**The warning is `prj.conf:1: warning: attempt to assign the value 'y' to the
undefined symbol NROS`, and the cause is a missing
`-DZEPHYR_EXTRA_MODULES`.** It was in the job log all along, in the module's
`log tail` block rather than its quoted "first error line(s)" — the tail
carries the LAST fixture's full output, and that one happened to be
`zephyr_self_pkg_sibling`. So the diagnosis blocker this issue describes was
real but narrower than stated: the quoting drops the cause, the tail sometimes
keeps it, and which of the five leaves you learn about is luck.

`CONFIG_NROS` is undefined because the `nros` module is not in west's project
list. phase-449 W1 (issue 1258) unbound the manifest project of a provisioned
workspace on purpose, so one shared store workspace does not bind every
checkout on the host to whichever one provisioned it. The other half of that
bargain is that every configuring build names its own module.
`scripts/build/west-fixtures.sh` — the builder of all five west fixtures, and
therefore of tier 2's entire west cover — never did.

**Why nobody saw it locally.** A developer's workspace is usually still BOUND:
`zephyr-workspace/nano-ros` is a symlink to the checkout, so the module
resolves from the manifest and the missing flag changes nothing. The omission
is invisible on a bound workspace and total on an unbound one.

**Reproduced**, in a scratch topdir built with `cp -al` from a real workspace
(a symlinked `zephyr` resolves the ORIGINAL topdir's manifest and cannot
reproduce this — CLAUDE.md, phase-447 F1), with the manifest project replaced
by a plain directory holding `west.yml` and no `zephyr/module.yml`:

```
west build --cmake-only -b native_sim/native/64 \
    packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling/caller
  .../caller/prj.conf:1: warning: attempt to assign the value 'y' to the undefined symbol NROS
  error: Aborting due to Kconfig warnings
```

byte-for-byte the CI failure. Adding
`-DZEPHYR_EXTRA_MODULES=<checkout>` to the same command: Kconfig parses, the
module loads, and configure runs on into nano-ros's own cmake.

Running the real lane against that unbound workspace after the fix: **5 of 5
fixtures save a Kconfig header** where 4 previously aborted, and
`west_board_import` configures green end to end. The other four then fail on
gaps in the agent worktree used for the measurement — an uninitialised
`third-party/dds/cyclonedds`, and `nros sync` producing no SystemModel without
`nros-launch-resolve` — not on anything this lane does.

**Fixed in `fix(#1458, phase-466)`:** the flag, through the one spelling
(`scripts/lib/zephyr-module.sh`), plus the gate that exists for this class and
could not see the call site. `check-zephyr-module-binding` matched only a
literal `build` after `west`, and this lane spells it
`env "${tc_env[@]}" west "${args[@]}"`; its command-position test also had no
boundary for an `env` wrapper, whose prefix ends in a quote. Two independent
blind spots, either sufficient — issue 0196's reach gap, for the third time in
the one gate whose docstring warns about it.

**Still open, deliberately not fixed here:** the fixture runner quotes the
first `error:` lines and not the context above them, which is what made this a
manual reproduction. Recorded in issue 1158 as the reporting half.
