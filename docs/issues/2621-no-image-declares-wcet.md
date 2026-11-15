---
id: 2621
title: "No in-tree image declares a WCET, so no sched task derives a budget and the sporadic-server path never runs"
status: open
type: tech-debt
severity: low
area: [orchestration, executor, examples]
found: 2026-11-15
related: [2616]
---

**Owner:** unassigned. **Home:** RFC-0079 "Open questions" (bootstrapping), and
RFC-0078, which defines the declaration. No active phase owns it.

## What happens

RFC-0079 deleted `budget_us`: a sched task's budget now comes from its RFC-0078
WCET, and a task with none runs fixed-priority. That was the intended
consequence. Absent must mean fixed-priority, not a guessed allowance.

No tracked `system.toml` or board descriptor outside tests sets `exec_ms` or a
`[wcet]` declaration (`git grep -l exec_ms -- 'examples/**/*.toml'
'packages/**/*.toml'` is empty). So every in-tree sched task runs
fixed-priority. The budgeted, sporadic-server path is reached only from unit
tests, and no image or fixture exercises it end to end.

## What closes it

One example image with a measured WCET declaration (RFC-0078's
`measured_at_commit`, `counter_valid`, profile `clock_hz`), and a fixture that
runs it and checks the derived budget is applied. Issue 2616's clock check
would make that declaration's time conversion trustworthy, but the fixture
does not need it to run.
