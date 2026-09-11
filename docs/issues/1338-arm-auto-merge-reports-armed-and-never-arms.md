---
id: 1338
title: "`arm-auto-merge` prints `armed #N` on every run and has never armed
  anything: no `--repo` with no checkout, and an `if` that tests `tee`"
status: open
type: bug
area: ci
severity: high
related: [issue-1040, issue-1226, issue-1249, issue-0952]
---

## Symptom

Pull request #947, 2026-09-11: every required check green, `mergeStateStatus`
`CLEAN`, the `arm auto-merge` check **SUCCESS** — and `autoMergeRequest` is
`null`. The PR is not armed, so it never enters the merge queue and never
lands, while the check that exists to say so is green.

## The two faults, in one line

`.github/workflows/arm-auto-merge.yml:176`:

```bash
if gh pr merge "$NUMBER" --auto --rebase 2>&1 | tee /tmp/arm.log; then
  echo "armed #$NUMBER"
else
  echo "::warning::could not arm auto-merge on #$NUMBER"
```

1. **No `--repo`, and the workflow checks out nothing.** That is deliberate and
   correct — it is `pull_request_target`, and its own header says *"Do not add
   `actions/checkout` here"*, because checking out PR code under that trigger
   hands a writable token to untrusted code. But `gh pr merge` resolves the
   repository from the local git remote, and there is no local git:

       failed to run git: fatal: not a git repository (or any of the parent
       directories): .git

   The command fails on **every** invocation. It has never armed a pull request.

2. **The `if` tests the wrong exit status, so the failure branch is
   unreachable.** A pipeline's status is its LAST command's, i.e. `tee`'s, which
   is 0 whether `gh` succeeded or died. So the `else` can never run, the
   `::warning::` has never been emitted, and the run prints `armed #947`
   immediately after the git error — both lines are in the log of run
   34653777221, three lines apart.

The two compound in the worst direction: fault 1 makes it always fail, fault 2
makes it always report success.

## Why it stayed invisible

The workflow was written to fix a measured throughput problem — *"of 23 open
pull requests, 16 were not armed"* — and its header reasons carefully about the
re-arming loop (*"a force-push CANCELS auto-merge... `synchronize` is what
closes it"*). That reasoning is right. The loop simply never ran.

Nothing contradicted it, because the only evidence anyone sees is a green check
named `arm auto-merge` and a log line saying `armed #N`. A human arming a PR by
hand (as happened on #947 at 19:58Z) looks exactly like the workflow working.

This is issue 1226's shape — *a gate that WORKS is not a gate that RUNS* — with
the sign flipped: a job that runs, cannot work, and reports that it did. It is
also issue 0952's withdrawal class and issue 1249's exit-status class in one
line.

## Fix

```bash
rc=0
gh pr merge "$NUMBER" --repo "$GITHUB_REPOSITORY" --auto --rebase \
  > /tmp/arm.log 2>&1 || rc=$?
if [ "$rc" -eq 0 ]; then
```

`--repo` because there is no checkout and there must not be one; `rc=0; cmd ||
rc=$?` because that is this repo's one spelling for a status you mean to
INSPECT under `set -e` (issue 1249), and because a pipe destroys it.

## Acceptance

- A pull request whose checks are green and whose `mergeStateStatus` is `CLEAN`
  reports non-null `autoMergeRequest` after the workflow runs.
- Negative control: with a deliberately bad `--repo`, the run emits the
  `::warning::` and the captured `gh` output, rather than `armed #N`.
- The `armed #N` line is only printed when the API call actually succeeded.
