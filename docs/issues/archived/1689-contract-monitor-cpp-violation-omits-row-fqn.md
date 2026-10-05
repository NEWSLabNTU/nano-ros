---
id: 1689
title: "`contract_monitor_cpp_twin_reports_rate_violation` asserted `fqn=<fqn>` on a
  violation line that has never spelled it that way"
status: resolved
type: bug
area: [testing]
severity: low
found: 2026-10-05
resolved: 2026-10-05
related: [1651, 0514, phase-462]
---

## What it was

The C++ twin's case (phase-462 W1, 2026-09-21) asserted the violation line
contains `fqn=/cm/pub/cm_header declared=10000` or `fqn=/cm/pub/cm_header
measured=`. `log_violation` (`nros-node/src/executor/monitor.rs`, issue #514,
unchanged since 2026-08-11) prints

    contract violation: rate-hierarchy-runtime /cm/pub/cm_header measured=1994 declared=10000

— the fqn bare, after the rule. Only the twin's ROW line uses `fqn=`. The
image was right and the assertion could never pass; nothing ran it, because
`host-tests` had not reached `test-all` since 2026-06-17 (issue 1651). Red in
CI run 37252649866 and locally solo.

## Fix

The assertion now composes the expected text from
`nros_tests::output::RULE_RATE_HIERARCHY_RUNTIME` and the fqn in the line's
real shape. `contract_monitor_parity`: 6/6 pass locally on fixtures built by
`just build-test-fixtures lane=tier1`.
