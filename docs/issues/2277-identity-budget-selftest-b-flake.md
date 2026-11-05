---
id: 2277
title: "`check-artifact-identity-budget --self-test` case (b) failed once inside `pristine-checkout`, then passed"
status: open
type: bug
area: gates, ci
found: 2026-11-05
---

## Measured

This failure happened once and has not been reproduced. It is filed so the
next occurrence has somewhere to go, not because it is understood.

During the phase-535 W3 results PR, `just ci gate` failed at step 4,
`check::pristine-checkout`, which runs `just check fast` in a submodule-free
clone under `/tmp/nros-pristine.*`. The output was:

```
FAIL (b): a filtered reading lost its 'do not delete' advice
check-artifact-identity-budget --self-test: 1 case(s) FAILED
```

- **Same tree, no edits.** `bash scripts/check-artifact-identity-budget.sh
  --self-test` passed 10 of 10 in the worktree, then `just check
  pristine-checkout` passed, then a full `just ci gate` exited 0.
- **Host load.** Another agent's builds shared the host and reached a load
  average of about 200 earlier that day. The value at the moment of the
  failure was not recorded.

## Where to look

Case (b) (`scripts/check-artifact-identity-budget.sh`, the `--self-test`
block):

- It back-dates five `libnros_core-*.rlib` files with `touch -d '2020-01-01'`.
- It writes `started_at=2010-01-01T00:00:00Z` to a stamp.
- It expects the era verdict "accumulation is ruled out".

Case (a), just before it, leaves `libother-*.rlib` dated 2035. Case (b)
therefore depends on how the era window treats a file newer than "now". It
also depends on the script's own run under the stamp, inside `check-fast`'s
`-P20` fan-out.

Two hypotheses, neither tested:

- a clock or filesystem timestamp-granularity edge in the window comparison;
- an interaction with another gate's temp tree under parallel load.

Per CLAUDE.md, the fix is the race, not a retry. The run's evidence is in
the phase-535 measuring checkout, `tmp/p535/w3r-gate-run1.log`, which is not
committed.
