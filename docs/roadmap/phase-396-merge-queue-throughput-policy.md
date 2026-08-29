# Phase 396 — the merge queue never blocked anyone; the required check is red for every input

**Status (2026-08-29). W1 landed — the merge group runs `ci-l1`. W2/W3 need a
decision from the maintainer (auto-merge convention, ruleset numbers); W4 is
blocked on PR #19; W5 is open.** Opened from "if one PR fails the merge check, it blocks everyone —
find a better policy". The premise turns out not to describe what happened here,
and the real defect is one layer down — but the policy question is still worth
answering, because the settings that would have contained a real blocking
failure are not the ones we have.

## What the queue actually did

Every merge group this repository has ever run contained **exactly one pull
request**:

```
08-29T03:20  group=[pr-7]   failure   failed job: check
08-28T17:53  group=[pr-19]  failure   failed job: check
08-28T16:02  group=[pr-6]   failure   failed job: check
08-28T13:41  group=[pr-6]   failure   failed job: check
08-28T12:28  group=[pr-6]   failure   failed job: check
08-28T10:18  group=[pr-6]   failure   failed job: check
```

So no pull request has ever waited behind another one's failure. GitHub's merge
queue already does the thing the premise asks for, and its documentation is
explicit about it: *"When the GitHub API receives a failing status for
`main/pr-1`, the merge queue automatically removes pull request #1 from the
merge queue"*, then rebuilds the temporary branches for the remaining entries
without it. Batching plus speculative prefixes plus eject-the-culprit is the
design.

Two facts explain why it looked like blocking:

1. **Auto-merge is on for 1 of 15 open pull requests.** The queue is being used
   as a serial door that each author opens by hand, so there is never a second
   entry to batch with, and `max_entries_to_build: 4` has never once engaged.
2. **The required check fails for every possible input.** `check-build` — which
   the merge group runs and the pull-request event does not — ends with
   `native::check`, which hard-requires generated message bindings, and includes
   `check-source-gates`, which asserts prebuilt `.compile-ok` fixtures. **No CI
   job produces either.** So every pull request is "the culprit", ejecting it
   changes nothing, and the next one fails identically.

That second point is the whole freeze, and **no queue policy can fix it.** A
queue routes around a change-specific failure. It cannot route around a check
that is red before it sees your change. Which is the general lesson worth
keeping: *throughput settings protect you from bad changes; nothing protects you
from a bad check except not requiring it.*

## Where the bad check came from

Phase-395 (`197eef4fa`, "the required check does different work per event — PR
cheap, batch thorough") deliberately moved the heavy tier into `merge_group`.
The intent was sound — pay for the expensive verification once per batch instead
of once per push — and it is exactly the amortisation the merge-queue literature
recommends.

What it missed is that the heavy tier was never runnable in that job.
`check-build` was written for a developer tree where `just build-test-fixtures`
has run; the CI job builds the CLI, provisions sources, and stops. The tier was
affordable and unsatisfiable at the same time, and because a required check that
cannot pass looks exactly like a required check that is failing, it read as
fourteen broken pull requests.

This is the same defect class `check-lane-contracts` was built for in phase-395
W2 — *a gate may resolve a compile-stage stamp only if its lane builds it* — one
tier up, where that gate does not look.

## The policy, from practice

The consistent recommendation across merge-queue writing is a three-bucket split
by *reliability and cost*, not by *importance*:

| bucket | contents | blocks a merge? |
| --- | --- | --- |
| **required** | lint, type check, fast unit tests — cheap and deterministic | yes |
| **informational** | e2e, integration, benchmarks, anything needing fixtures or hardware | no; reported on the PR |
| **post-merge** | the full matrix, on `main`, with a revert path | no; alerts |

Graphite's guidance is the sharpest statement of the principle: *"Every
additional required check can flake and block the queue, so ask if each check
actually catches bugs that would break production."* Tenki's is the most
concrete: required = unit/type/lint/security; e2e and performance are
informational; *"post-merge validation on main can capture these without
pipeline delays."*

We already have all three buckets. `post-submit.yml` runs tier-2 fixtures and
`just ci-matrix` on every push to `main`. The heavy work in the merge group is
**duplicating a lane that already exists**, and paying for it with the merge
button.

The tier ladder RFC-0061 defines already names the right required set, and
CLAUDE.md already tells every agent to run it before pushing:

> `just ci-l1` — compile + unit, ~6 min, NO FIXTURES. Run this before every push.

A required check should be the thing the contributor was told to run. Ours is
not, and that gap is where the freeze lives.

## Waves

### W1 — the merge group runs `ci-l1`, not the build tier

Drop `just check-build` + `just check-no-std` from the `merge_group` arm of
`pr-checks.yml`. The required `CI` context becomes exactly `check-fast` +
`test-unit` — `ci-l1`, the tier already gated by `check-lane-contracts` as
fixture-free and therefore actually runnable in that job.

What this gives up, stated plainly: the merge group stops catching a
compile-tier break in the merged state. `post-submit` catches it minutes later
on `main` instead, and phase-395's `queue-notify` shape gives us somewhere to
report it. That is a real reduction in pre-merge coverage and it is the correct
trade — a check that has never once passed provides no coverage at all, and has
cost fourteen pull requests a day each.

### W2 — auto-merge is the default, and the docs say so

Batching cannot help while one pull request enters at a time. AGENTS.md already
documents `gh pr merge --auto --rebase` as the flow; make it unambiguous that
enabling auto-merge is part of *opening* a PR, not a thing to do once it looks
ready. Then `max_entries_to_build` starts meaning something.

### W3 — queue settings, once batching engages

Current: `ALLGREEN`, build 4, merge 4, min 1, wait 5 min, timeout 60 min.

- **Keep `ALLGREEN`.** `HEADGREEN` admits pull requests with failing required
  checks as long as the last entry passes. For a repo whose failures are mostly
  environmental rather than semantic, that lands untested changes.
- **`min_entries_to_merge_wait_minutes: 5 -> 0`.** With one or two entries a
  day, this is five minutes of pure latency per merge and buys no grouping.
  Revisit above ~20 merges/day.
- **`max_entries_to_build: 4 -> 5`**, matching the common starting
  recommendation, once W2 makes groups larger than one possible.
- **`check_response_timeout_minutes: 60`** is right for a 30-ish minute lane;
  the usual rule is ~2× observed duration.

### W4 — a circuit breaker for the case a queue cannot handle

**Blocked on PR #19**, which is where `queue-notify.yml` lives; it is not on
`main` yet, so this wave cannot be written until that lands.

`scripts/ci/queue-triage.sh` already answers "is this red mine, or is it red for
everyone?" by looking for one check failing across several *different* pull
requests. Nothing runs it automatically, so today a human has to suspect the
problem before the tool that detects it gets used.

Wire that verdict into `queue-notify.yml`: on an ejection, if the same check has
failed for N ≥ 2 distinct pull requests in the window, say so on the PR and stop
telling the author to rebase — the advice is actively wrong in that case, and
re-queuing burns batch slots against a check that cannot go green for anyone.

Same shape as phase-395's merge-queue INFRA/MINE split and issue 0878's
verdict/no-verdict split for nightly. Third instance of one idea: **a red that
is not about your change must not be reported as though it were.**

### W5 — the gap that let this happen

`check-lane-contracts` proves `ci-l1` is affordable. Nothing proves the same of
the tier the merge group actually runs. Extend it to assert the contract for
*every* tier a CI job invokes: a gate may resolve a build-stage artifact only if
its lane produces it.

That would have failed on `native::check` and `check-source-gates` the moment
the merge group started running `check-build`, in the fast line, before any of
this reached the queue.

## Not doing

- **Removing the required check.** An unprotected `main` is a bigger problem
  than a slow one; the ruleset stays.
- **`HEADGREEN`.** See W3.
- **A merge-queue service** (Mergify, Aviator, Graphite). They add batch
  bisection and richer policy, and none of it addresses a check that fails
  independent of its input. Revisit when volume, not correctness, is the limit.

## Sources

- [Managing a merge queue — GitHub Docs](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)
- [Merge queue best practices — Graphite](https://graphite.com/guides/merge-queue-best-practices)
- [GitHub Merge Queue in 2026 — Tenki](https://tenki.cloud/blog/github-merge-queue-setup)
- [Pre and post-merge tests using a merge queue — Aviator](https://www.aviator.co/blog/pre-and-post-merge-tests-using-a-merge-queue/)
