---
id: 3022
title: "The auto-arm automation arms and enqueues every ready pull request during a release freeze"
status: open
type: bug
severity: medium
area: [ci, merge-queue, release]
found: 2026-11-28
---

## What happens

The `arm` job of `.github/workflows/automation.yml` (`pull_request_target`)
arms auto-merge on every non-draft pull request that has no `no-auto-merge`
label and an author it trusts. It does this with the queue App's token
(`nano-ros-merge-queue`), so the entry it creates also gets merge-group CI and
lands like any other. It does not know about a release freeze. Nothing in the
repository records a freeze: it is announced between sessions, and the rule
"no arming or queue entries" is followed by hand.

Observed 2026-11-28, during the nros12 B51 freeze: #5059 (the B51 revert) was
opened READY for the release owner to arm after their user's ruling. The
`arm` job armed it at 06:45:58Z, `enabledBy: app/nano-ros-merge-queue`, before
anyone ruled. It happened to match the ruling this time. A different ready PR
would have entered the queue mid-freeze the same way, and so would a
dequeued PR on its next push: the job runs on `synchronize` too.

The workaround is to open every PR as a DRAFT during a freeze, since `draft ==
false` is part of the job's `if:`. A workaround that depends on every author
knowing a freeze is on is the gap itself.

And drafting is not dequeueing. A PR the App had already enqueued stays in
the merge queue after it is converted to a draft. #5056, drafted but still
App-enqueued, made #5059 (the freeze's own revert) UNMERGEABLE behind it at
07:24 the same day. So stopping new arms is half of a freeze; the other half
is emptying the queue of everything the freeze does not admit.

The thaw has the mirror gap: a PR readied during the freeze sees its
`ready_for_review` event skipped, and nothing arms it later. It sits unarmed
until its author notices (the 2026-09-06 shape).

## Fix direction (after nros12 publishes)

Designed by 118 (CI owner), who lands it.

- **A freeze flag** the release owner sets and clears: a repository variable
  (`vars.RELEASE_FREEZE`, readable in an `if:` with no API call).
- **One decision script** with `--self-test`, called from `arm`, from the
  `enqueue` backstop and from the sweep below. It answers "may this PR be armed
  or enqueued now?" and logs why, the same way the trust decision is a logged
  step rather than a silent `if:`. While frozen, the answer is no unless the
  exception applies.
- **A `release-fix` exception:** the label admits a PR during a freeze only if
  the LAST `labeled` event for it was by a login on a release-owner list.
  Anyone can add a label; the list decides whose label counts.
- **Dequeue on freeze:** setting the flag (or the freeze step that sets it)
  also DEQUEUES every merge-queue entry that is not an admitted `release-fix`
  PR. Without it, an entry enqueued before the freeze, or one drafted after,
  stays in the queue and blocks the release's own fixes.
- **Arm at thaw:** a dispatchable `arm-sweep` job, run when the flag is
  cleared, arms ready, clean, trusted PRs oldest first, at most 5 per run, so
  PRs readied during the freeze are not left unarmed.
- **Self-test** (the decision script's, plus a check over the workflow): it
  fails if any job in `automation.yml` can arm or enqueue without asking the
  script, if the freeze step can leave a non-`release-fix` entry queued, and if
  a `release-fix` label from a login off the list is honoured.
- The release runbook names the flag, the dequeue and the sweep in its freeze
  and thaw steps.

Owner: 118 (CI), after nros12 publishes.
