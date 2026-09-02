---
id: 994
title: "The detector for silently-ineligible pull requests exists and nothing
  calls it — `pr-verdicts` runs only when a human thinks to ask"
status: resolved
type: tech-debt
area: ci
related: [issue-0975, issue-0196]
resolved_in: "issue 0994 (this filing)"
---

## Symptom

On 2026-09-02, PR #207 had **zero check suites for its head sha**. Not failing,
not pending — no verdict at all, so it was ineligible to merge and would have
stayed that way indefinitely. It was found by eye, from a `gh pr checks` line
reading `no checks reported`.

`scripts/ci/pr-verdict-check.sh` detects exactly this, and has since PR #71 sat
thirteen hours in the same state with auto-merge armed against a check that was
never requested. Run against the tree that day it answered correctly in seconds.

**The detector was not missing. Its caller was.** It is `just pr-verdicts`, in
the `setup` group, read-only, invoked only when someone already suspects the
problem — which is the one situation where a detector adds least. The failure it
looks for is SILENCE, and silence prompts nobody to run anything.

## Why this is not a gate, and never can be

The script needs the network and an authenticated `gh`. Gates here are
buildless, offline and deterministic, and `check-lane-contracts` enforces that
for merge-gating lanes. It also asks a question about OTHER pull requests, which
is not a property of the diff under review.

So the honest options were a schedule or nothing.

## Third instance of the class

| when | what | how it was found |
| --- | --- | --- |
| PR #71 | stacked PR; retarget emitted `pull_request.edited`, not a default dispatch type | by hand, 13 h later |
| issue 0975 | a `merge_group`-only check on the required set — no PR could satisfy it, so no PR entered the queue, so the event that would run it never fired | by hand, 7 PRs stalled |
| PR #207 | zero check suites for the head sha | by eye, 2026-09-02 |

Each time the trigger was fixed. The DETECTION has depended on a person
noticing an absence — and an absence is what people are worst at noticing.

## Fix

`.github/workflows/pr-verdicts.yml` — `schedule` (09:00 and 21:00 UTC) plus
`workflow_dispatch`, running `scripts/ci/pr-verdict-check.sh --min-age 15`. The
script already exits 1 when it finds a stuck PR, so it needed no change.

Its own workflow rather than a job in `nightly.yml`: nightly is the L3/L4
cross-build matrix that provisions SDKs and runs QEMU, and a 20-second `gh`
sweep must run whether or not that matrix is healthy. Folding it in would mean a
red cross-build takes down the check that notices when nothing is reporting.

`--min-age 15` because a PR opened seconds ago legitimately has no checks yet.
On a schedule there is no reason to be impatient, and a false "stuck" would
teach people to ignore the alert — which is the failure mode this whole class
already has.

NOT a required check, and it runs on `schedule` only, never on `pull_request`.
Adding a scheduled job to the required set is precisely issue 0975: a context
that produces no verdict for a PR blocks it forever.

## Verified

* `bash scripts/ci/pr-verdict-check.sh --min-age 15` — the exact invocation the
  workflow makes — returns `OK — 7 pull request(s) reporting, 0 too fresh to
  judge, none stuck`, rc 0.
* Before #207 was rebased the same script reported it stuck; after, it reports.
  So the detector distinguishes the two states on real data, not just in
  principle.
* `check-workflow-repo-env` green over 12 workflows (was 11), 175 steps.

## What this does not fix

The sweep reports; it does not remedy. A stuck PR still needs a human to rebase
(if conflicting — GitHub cannot build a merge ref, so nothing dispatches) or to
re-fire the event (if not). The script already names WHICH of those two applies,
because prescribing the wrong one is worse than prescribing nothing.

Twice-daily is a latency of up to twelve hours. That is a deliberate trade
against one `gh` call per open PR per run; if the class recurs faster than that,
the cron is the thing to change.
