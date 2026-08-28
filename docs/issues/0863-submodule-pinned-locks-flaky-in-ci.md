---
id: 863
title: "`check-submodule-pinned-locks` fails intermittently in CI on a commit that
  also passed — a flaky check inside the about-to-be-required set"
status: open
type: bug
area: ci
related: [phase-395, issue-0854]
---

## Problem

`check-submodule-pinned-locks` failed a `pr-checks` run on `6ae0249aa`:

```
[FAIL] 1 lock(s) pinned by a submodule manifest no longer resolve:
      note: offline mode (via `--offline`) can sometimes cause surprising resolution failures
error: recipe `check-submodule-pinned-locks` failed on line 1413 with exit code 1
```

**The same commit passed an earlier run** (33126514770 success, 33143052057
failure, both `6ae0249aa`). So this is not a regression in that commit — the
check is non-deterministic in CI. It passes locally:

```
submodule-pinned locks: OK (1 leaf/leaves resolve under --locked)
```

## Why this one matters more than an ordinary flake

It sits inside the `check` job, which is the job phase-395 W7 intends to make a
**required status check**. A flaky required check is a third way to break
merging, alongside the two already recorded on this campaign:

| failure mode | symptom |
| --- | --- |
| always red | nothing can merge (issue 0853) |
| always pending | nothing can merge, and it looks like GitHub being slow |
| **flaky** | merges land or not by luck, and a merge queue AMPLIFIES it |

The amplification is the real cost. In a batch of four, one flaky red ejects and
re-tests every innocent PR in the batch — which is why phase-395 puts flake
quarantine before the queue.

## What is NOT established

- **The mechanism.** The `--offline` hint in the output is the check's own
  generic note, not a diagnosis. Whether this is registry resolution, a network
  timeout, a cache-state difference, or something about the CI image is unknown.
- **The rate.** One observed failure in four runs of that job is not a
  measurement. It could be far rarer or far worse.
- **Whether it predates the campaign.** Nobody was reading these reds before
  phase-395 W0.5, so "it started recently" would be an artifact of when we
  started looking.

## Not to do

Do not add it to `.config/flake-quarantine.toml`. That registry is for nextest
TESTS, and its entry contract is evidence of a solo pass; this is a `just` gate
and the mechanism is unknown. Quarantining an unexplained failure in the
submodule-pin family is especially bad, because that family exists to catch a
class (`0359`/`0378` lockfile drift, submodule rewinds) whose whole danger is
being invisible.

The cheap first step is a rate measurement: re-run the job N times on one
unchanged commit and count.
