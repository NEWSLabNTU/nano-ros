---
id: 1320
title: "Every rebase re-stales the in-tree CLI, and the gate that NAMES that cause
  runs only after merge — so it arrives at `pre-push` as a cmake CONFIGURE error
  four frames from the remedy"
status: open
type: bug
area: [ci, build, tooling]
severity: medium
found: 2026-09-11
related: [0466, 0627, 1018, 0363, phase-450]
---

## What

Any rebase rewrites tracked files, which re-arms the in-tree CLI's source stamp
(issue 0466 — the same mtime treadmill that re-stales every fixture). That is
known and documented. What is not is **who tells you**.

`check-cli-fresh` is the gate that names this cause exactly:

```
Error: source-stamp: STALE — built from 97dc7c5ce03a3ac9, sources are now cec157677e473239.
Rebuild: ./scripts/bootstrap.sh   (contributors: just setup-cli)
```

Measured 2026-09-11 — where it runs:

| surface | runs `cli-fresh`? |
| --- | --- |
| `just check fast` (what `pre-push` runs) | **no** |
| `just check build` / `build-serial` | yes |
| `just ci gate` step list | yes |
| `.github/workflows/post-submit.yml` | yes — `on: push: branches: [main]` |
| any `pull_request` or `merge_group` workflow | **no** |

So the gate whose message names the remedy runs in the BUILD tier and, in CI,
**only after the merge**. `pre-push` runs `check fast`, which does not include
it.

## What happens instead

`check-declared-qos-header` is a FAST gate that needs a built `nros` to drive a
real configure. Its own comment says it "SKIPS rather than passing when any is
missing, because a gate that examined nothing must not read as a pass" — and a
STALE CLI is not missing, so it does not skip. It fails, reporting the staleness
as somebody else's error:

```
CMake Error at cmake/NanoRosNodeRegister.cmake:1542 (message):
    Error: in-tree nros CLI is STALE — its sources changed since it was built
  Override for a deliberate experiment: NROS_SKIP_STALE_CHECK=1
-- Configuring incomplete, errors occurred!
```

The text is accurate, and the frame is wrong: it names a cmake module in the
RMW gate's transitive configure, so the reader's first hypothesis is the QoS
header machinery, not their own CLI. The remedy (`just setup-cli`) appears
nowhere in it — only an override that suppresses the check.

## Measured, in one session

Three incidents on one branch on 2026-09-11, all from ordinary rebases (458
commits, then 29, then 10):

* **once** in `just check build`, where `cli-fresh` named it correctly — and
  `check-build` runs on `schedule` / `workflow_dispatch` only, so no PR would
  have seen it;
* **twice** at `pre-push`, both as
  `NanoRosNodeRegister.cmake:1542`. Both times the fix was `just setup-cli` +
  `just setup-launch-resolve`, after which `declared-qos-header` passed its 20
  assertions.

The second and third incidents cost a push cycle each, and neither message said
so.

## Why this is the phase-450 shape and not just an annoyance

A gate whose reach is narrower than its rule is green while its defect is
present. This is the adjacent failure: **the gate is CORRECT and is not where
the defect surfaces.** `cli-fresh` cannot fire before a push because it is not
in the pre-push lane; the lane that does fire has no vocabulary for the cause,
so the diagnosis is re-derived by hand every time.

That is also why it recurs rather than being fixed once: the person who hits it
is mid-push, gets a cmake error about an unrelated module, and the cheapest path
is to rebuild and move on without filing anything.

## Suggested shape

Not decided — the cheap option and the correct one differ, and the choice is
worth making explicitly.

1. **Cheapest:** put `cli-fresh` in `check-fast`. It is a stamp comparison, so
   it costs milliseconds, and it would turn both pre-push incidents into a
   message naming `just setup-cli`. The objection is that `check-fast` is meant
   to be buildless and this asserts something about a BUILT artifact — though it
   only READS a stamp, and the lane already contains gates that skip when a
   toolchain is absent.
2. **More correct:** make the failing gate name the cause. `declared-qos-header`
   already distinguishes "missing" (skip) from "present"; a STALE CLI is a third
   state it could report as its own precondition, with the remedy, instead of
   letting a cmake `message(FATAL_ERROR)` four frames down speak for it.
3. Or decide that a rebase should re-run `setup-cli`, and say where — the
   `pre-push` hook already knows a rebase happened in the sense that matters.

(1) and (2) are not exclusive and the second is the one that survives the next
gate growing the same dependency.

## Not to be confused with

Issue 1018 is the CONFIGURE-time emitter problem — a tool with no `DEPENDS`
edge, so a stale CLI's OUTPUT survives. This is one layer up: the CLI itself is
stale and the tree does say so, in a lane that runs too late and through a gate
that describes it as something else.
