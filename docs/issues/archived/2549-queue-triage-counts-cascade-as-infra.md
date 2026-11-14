---
id: 2549
title: "queue-notify told #3805 its own defect was \"almost certainly not your pull request\""
status: resolved
type: bug
severity: medium
area: [ci, tooling]
related: [issue-1843]
found: 2026-11-14
---

**Home: `scripts/ci/queue-triage.sh`, `classify()`, which `automation.yml`'s
`notify` job runs on every merge-queue ejection**

## Symptom

On 2026-11-14 #3805 was ejected for
`check / test-lane-contracts`. The notifier said the check "failed in the merge
group for 3 DIFFERENT pull requests" and that the ejection was almost
certainly not #3805's. The failure was #3805's own. A false "not yours" is
worse than no verdict: it tells the author to stop looking and not to
re-queue.

## Cause (two, both measured on the live run history)

1. **A merge group carries every entry ahead of it.** Its ref is
   `gh-readonly-queue/main/pr-<N>-<base>`, and `<base>` is the head commit of
   the group in front. #3805's group failed on main's tip (`856211d5a3`, head
   `96d6f99fba`). #3787's group was built on `96d6f99fba` and failed. #3807's
   was built on #3787's head `412b4d49d8` and failed. One defect, three PR
   numbers, and the classifier counted PR numbers.
2. **The window spans a day.** The 75-run window also held #3766's failure of
   the same check from 2026-11-13 18:32Z. That check had passed in every group
   since, so it was no evidence the check was red for everyone.

## Fix

- A failure whose group was built on the head of a group that failed the
  SAME check is dependent, and is not counted (the row now carries the base
  and head shas).
- A failure older than the newest successful run of the workflow still makes
  the verdict MINE, but does not count toward INFRA.

The self-test gains four cases: the #3805 cascade (MINE), failures older than
a green run (MINE), a green run older than the failures (still INFRA), and a
group built on a failure of a different check (independent). Removing the
dependency rule fails the cascade case. Run live on #3805 after the fix:
"LIKELY YOUR PULL REQUEST".
