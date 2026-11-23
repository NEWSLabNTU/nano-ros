---
id: 2868
title: "`nros run`'s SIGTERM-to-SIGKILL grace equalled a threadx-linux image's own termination grace, so the verb could kill an image before it reported and ended"
status: open
type: bug
severity: low
area: [cli, threadx, testing]
found: 2026-11-23
related: [1741, 2800]
---

## What was reported, and what it turned out to be

Measuring #4629, host 175 saw no `nros: stack peak` lines (issue 2800's
threadx-linux exit report) from `nros run … --timeout`, and got them by running
the binary under `timeout -s INT`. Its own follow-up diagnostic retracted that.
The trees it ran `nros run` on (#4629's base 3575bf6c05, and 0f01749cab)
predate 546cd5fcec, which added the reporter, so nothing could print. On a tree
that has it, `examples/threadx-linux/rust/talker` under `nros run --timeout 10`
prints both peak lines, with and without an `--expect` that never matches.

## The defect that is real

`nros run` stops its target's process group with SIGTERM, then sends SIGKILL
after `TEARDOWN_GRACE` (`packages/cli/nros-cli-core/src/target_session.rs`),
which was 2 s.

A threadx-linux image's termination guard (issue 1741,
`board_threadx_linux.c`) behaves in one of two ways:
- With no application handler, it reports and ends at once. That is the Rust
  talker above, which is why it printed.
- With an application SIGTERM handler (the C/C++ examples install one), it
  calls the handler, then gives the application
  `NROS_THREADX_LINUX_TERM_GRACE_MS` = 2000 ms to finish. Then it reports the
  stack peaks and ends by the default action.

So for an application whose shutdown takes the whole grace (a wedged
scheduler, issue 1741's measured case), the verb's SIGKILL and the image's own
ending were due at the same instant. If the SIGKILL won, the report was lost.

The board already states the rule this breaks: the image's grace is "below the
harness's own kill-after (`nros_tests::process::KILL_GRACE`), so the image ends
by its own hand before any escalation has to fire". `check-process-deadline`
enforced that rule against the test harness, but not against `nros run`, which
is the other escalator.

Not measured: the race firing. The C entry under `nros run` was not re-run on a
tree with the reporter, and the race needs an application that ignores its own
cancel for two seconds.

## Fix

`TEARDOWN_GRACE` is now 3 s, the harness's kill-after, with a comment stating
the relation. `check-process-deadline` checks the image's grace against it as
well as against the harness.

Mutation check: put back at 2 s, the gate fails, naming `target_session.rs`
and this issue.
