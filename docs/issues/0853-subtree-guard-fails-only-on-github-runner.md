---
id: 853
title: "The subtree guard's SIGTERM path fails only on the GitHub runner, and the
  survivors are genuine — three hypotheses ruled out, cause still unknown"
status: open
type: bug
area: testing
related: [issue-0762, phase-395]
---

## Problem

`check-subtree-guard` fails on every GitHub push and passes everywhere I can
reproduce. It blocks `pr-checks`, which means **no push gets a green PR run** —
the condition phase-395 W0.5 exists to remove.

```
FAIL: subtree survived SIGTERM to its launcher — 2 of ITS OWN process(es)
      still in pgid NNNN. This is the orphan bug.
```

## The survivors are real

The message above says *"of ITS OWN"* because the check was rewritten during
W0.5 to record the member PIDs while the group is at depth and then require
same-pid-AND-same-pgid to count a survivor. Before that it counted anything
whose numeric pgid matched.

That rewrite was made on the hypothesis that PGID **recycling** explained the
failure. **It did not** — the stricter check still fails, which proves the
survivors are the original processes rather than reused ids. The hypothesis was
wrong; the diagnostic is better for it, and that is how the wrongness became
visible.

## What has been ruled out

| hypothesis | test | result |
| --- | --- | --- |
| PGID recycling under churn | identity-based check (same pid + same pgid) | **ruled out** — still fails, survivors genuine |
| Containerisation changes reaping | `docker run ubuntu:22.04` + procps | passes, all 3 paths |
| CPU starvation on a 4-vCPU runner | `docker run --cpus=0.3` | passes, all 3 paths |
| A slow drain | the assertion already polls 20 s; cleanup does TERM, waits 10 s, then KILL | not a timing shortfall |

Locally it passes on a 32-thread host, in a stock container, and under a 0.3-CPU
quota.

## What is left

Untested because the CI image `ghcr.io/newslabntu/nano-ros-ci:humble` needs
registry auth:

- something in that image — bash version, `procps` build, or running as root;
- the runner's container invocation (whether PID 1 reaps, `--init`, etc.);
- interaction with `check-fast`'s 32-way parallelism, since the guard runs
  alongside ~31 other gates rather than alone.

The cleanest next step is to pull that image and run
`bash packages/testing/nros-tests/tests/subtree_guard.sh` inside it. That is one
command for someone who can authenticate to ghcr.

## Why it matters beyond the gate

If the failure is genuine — and the identity check says the processes are real —
then **the subtree guard does not work in that environment**, and the guard
exists to stop a killed build from orphaning its descendants (issue 0762). The
71 orphaned `add_two_ints_server` processes found on this host, oldest 10 days,
are what that failure looks like when nobody is watching.

So this is not "a flaky gate to quarantine". It is either a real defect in the
guard under the CI environment, or a defect in the test's assumptions there.
Both need the image to tell apart.

## Not to do

Do not silence it to make `pr-checks` green. A gate reporting a reproducible
failure is doing its job; the value of W0.5 was finding that the reds were real
and unattended, not making them disappear.
