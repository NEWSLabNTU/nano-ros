---
id: 863
title: "`check-submodule-pinned-locks` fails intermittently in CI on a commit that
  also passed — a flaky check inside the about-to-be-required set"
status: resolved
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

## Cause — found, reproduced, fixed

Not a network flake. The gate resolves `--offline` ON PURPOSE, so it reads THIS
HOST's cargo registry cache. In CI that cache is restored by `actions/cache`,
and when it does not contain a crate the leaf's lock names, offline resolution
fails during SELECTION — no download is attempted at all:

```
error: no matching package named `clap` found
location searched: crates.io index
required by package `nros-launch-resolve v0.5.0 (…)`
note: offline mode (via `--offline`) can sometimes cause surprising resolution failures
```

Reproduced locally with an empty `CARGO_HOME`, identical line-for-line to the CI
red. That is the whole flake: cache warm, pass; cache cold, red.

Issue 0600 already built the classifier for exactly this, and it was
INCOMPLETE. `_is_offline_cache_miss` matched only the two DOWNLOAD wordings
(`HTTP request`, `failed to download`), and this third shape has neither. So a
cold cache was classified as a **mismatch** and the operator was told to
`lock-update` a byte-correct lock — the precise churn 0600 exists to prevent,
in the imperative, to a reader with no reason to doubt it.

The classifier now keys on cargo's own hedge, `offline mode (via `--offline`)`,
which is cargo itself saying the verdict may be an offline artifact. A genuine
`--locked` mismatch is disjoint: it says the lock file needs updating and never
mentions offline mode.

Two further defects fixed alongside:

- **The reporter discarded its own evidence.** Both branches printed
  `err[-4:]`, and `no matching package named X` is the FIRST line. That is why
  the CI log could not be classified after the fact — the gate threw away the
  line naming the crate. Now prints the head.
- **A cold cache was a hard FAIL.** It says nothing about the lock, so failing
  on it makes the gate flaky; it is also not a pass. It now reports
  `0/1 leaf/leaves verified — 1 could NOT be checked` and exits 0, loudly
  enough that it cannot read as coverage.

Proved in both directions: an empty `CARGO_HOME` gives rc 0 with the unverified
banner; a deliberately corrupted lock (`play_launch_parser` bumped to a version
the manifest cannot satisfy) still gives rc 1 and the mismatch text. The lock
was restored and `git diff` is clean.

## The rate measurement, and why it did not happen

Five `workflow_dispatch` runs were fired to count the failure rate. **Three were
cancelled by the workflow's own concurrency group** — `pr-checks-${{ github.ref
}}-${{ github.event_name }}` with `cancel-in-progress: true` — because every
dispatch on the same ref shares one group. Worth recording for anyone who tries
to measure a CI flake this way: N dispatches on one ref do not give N samples,
they give one. Distinct refs, or serialised runs, are needed.

It became moot once the mechanism was identified: the rate of a diagnosed and
fixed defect is not worth 50 minutes of runner time. Had the cause stayed
unknown, the measurement would still be the right next step.

## What is NOT established

- **The rate.** Never measured, for the reason above. Unknown, and now
  uninteresting.
- **Why the CI cache was cold on that run specifically.** The fix makes a cold
  cache harmless either way, so this was not chased. If the gate starts
  reporting `could NOT be checked` on most CI runs, that becomes worth knowing —
  it would mean the gate verifies nothing in CI, which is a coverage hole rather
  than a flake.
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
