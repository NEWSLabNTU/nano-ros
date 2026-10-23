---
id: 2008
title: "`check pristine-checkout` printed a failing gate's name and dropped its reason"
status: resolved
type: bug
area: [ci, tooling]
severity: low
found: 2026-10-23
related: [1373, 1345, phase-466]
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**

## What happened

On 2026-10-23, `just ci gate` step 4 (`check::pristine-checkout`) went red in
one run on `lane-stop-kills-fanout`. The log had this:

```
===== FAIL (lane-stop-kills-fanout, rc=1, …ms) =====

pristine-checkout: FAILED (rc=1) -- the fast tier is red in a checkout
```

It did not say why. The gate passed 3/3 on its own and passed on the next
gate run, so the failure is a load flake. But that is an inference, because
the one record of the failure was thrown away.

## Cause

`run-gates-parallel.sh` writes each failure as a record: a `===== FAIL (<gate>,
rc=…, …ms) =====` header followed by that gate's full output.
`pristine-checkout-fast.sh` reported a red with `grep -E '^===== FAIL'`, which
keeps the headers and drops every body line. The clone that holds the full log
is deleted on exit unless `--keep` was passed.

## Fix

Each FAIL record is now printed whole: from its header to the next header or to
the `check-fast (parallel):` summary. Each record is capped at
`NROS_PRISTINE_FAIL_LINES` lines (60 by default), and a cut record says so.
`tail -40` is still the fallback when the log has no records at all.

I checked the awk by hand on a two-record log: both bodies printed and the
summary line was excluded. The script's own self-test already asserts the FAIL
header (`pristine-selftest-probe`). Its planted gate prints nothing, so the
self-test cannot also check that a body is printed.
