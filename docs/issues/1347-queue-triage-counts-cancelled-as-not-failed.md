---
id: 1347
title: "`just queue-triage` said MINE for a check broken across 11 pull
  requests: an ALLGREEN batch CANCELS the entries behind a failure, and the
  classifier counts only `failure`"
status: open
type: bug
area: ci
severity: high
related: [issue-1158, issue-0952, issue-1040, phase-450]
---

## Symptom

PR #947 was ejected from the merge queue. `just queue-triage 947` answered:

    == LIKELY YOUR PULL REQUEST ==
      'queue' failed in the merge group, and for only one pull request.

It was not. The same `queue` check was failing for **11 different pull
requests**, including **#994 — a two-file change to
`.github/workflows/arm-auto-merge.yml` and a markdown file, with no Rust in
it** — which failed identically:

    /third-party/nuttx/nuttx/include/stdbool.h:30:10:
        fatal error: nuttx/config.h: No such file or directory
    error: recipe `rust-rtos-link-check` failed with exit code 101
    error: recipe `l3` failed with exit code 101

A change touching one YAML file cannot break a NuttX cross-link. The tool that
exists to answer *"is this mine, or is it red for everyone?"* gave the wrong
answer on the first case it was asked.

## Cause — two mechanisms, both about cancellations

The merge queue uses `grouping_strategy: ALLGREEN` over up to five entries, so
when one entry fails, **every entry behind it is `cancelled`, not `failed`**.

1. **The classifier reads only `failure`.** `scripts/ci/queue-triage.sh`'s
   `classify()` matches `$3 == "failure"` and ignores every other conclusion. A
   check that is broken for everyone therefore arrives as ONE `failure` beside N
   `cancelled` — which is exactly the shape it reports as MINE.
2. **Cancellations consume the window.** `LOOKBACK=15` counts RUNS, not
   verdicts, and the history is mostly cancellations: of the 12 most recent
   merge-group runs on 2026-09-12, **nine were `cancelled`** and two were
   `failure`. So even a classifier that read them correctly would rarely see two
   distinct failing PRs inside the window.

The self-test never covered it — its four cases use `success` and `failure`
only, so the one conclusion that dominates real batches was the one never
tested. That is the phase-450 shape in a triage tool: the reach of the test is
narrower than the rule the tool states.

## Why it matters more than a wrong label

The two verdicts prescribe OPPOSITE actions, and the tool says so itself:

* MINE — *"reproduce that exact state… fix, push, re-queue."*
* INFRA — *"DO NOT re-queue. Rebasing will not fix it, and re-queuing burns a
  batch slot against a check that cannot go green for anyone."*

So a wrong MINE sends every author of those 11 pull requests to hunt a defect in
their own diff and then re-queue, which ejects the batch again. It converts one
broken check into N wasted investigations plus N wasted batch slots — the exact
cost the tool was written to prevent (issue 1158's lane-triage argument, one
lane over).

## Fix (applied)

* `classify()` drops `cancelled` / `skipped` / `running` before counting, and
  reports how many rows carried no verdict.
* The fetch is `LOOKBACK * 5` runs so the window holds that many VERDICTS rather
  than that many rows.
* Three self-test cases added: cancelled-is-not-a-pass, an all-cancelled batch
  reads CLEAN rather than MINE, and `running` is not a verdict.

With the fix, the same query answers:

    == NOT YOUR PULL REQUEST ==
      'queue' failed in the merge group for 11 DIFFERENT pull requests.

## Still open: the `queue` check itself

This issue is about the TRIAGE. The underlying breakage — `nuttx/config.h`
missing in the L3 cross-build, so `rust-rtos-link-check` fails for every pull
request in the queue — is real, is not caused by any of those PRs, and is what
should be fixed or dropped from the required set until it is. Nothing merges
through the queue while it stands.

## Acceptance

- `queue-triage --selftest` covers `cancelled`, `skipped` and `running`.
- A batch of one `failure` plus four `cancelled` for the same check across two
  or more PRs classifies INFRA.
- The printed run list distinguishes verdicts from cancellations, so a reader
  can see how much of the history carried no signal.
