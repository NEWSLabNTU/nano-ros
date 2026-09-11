---
id: 1051
title: "`check-submodule-pinned-locks` blames the lock for a drifted CHECKOUT, and
  the remedy it prints would rewrite a correct lock to match the wrong tree"
status: resolved
type: bug
area: tooling, ci
related: [phase-450, issue-1294]
---

## Problem

The gate resolves each submodule-pinned leaf under `--locked` and, on failure,
prints:

```
  The submodule pointer moved and the lock did not follow (issue 0560).
  Update it the sanctioned way — never a bare `cargo generate-lockfile`:
      just lock-update "" "" <leaf-dir>
```

That is one of **two** causes, and the message asserts it as the only one.

The other: the pointer did NOT move, the **worktree checkout** drifted from it.
A rebase does this routinely — the superproject's gitlink advances and the
submodule working tree stays where it was, which is the state CLAUDE.md
describes as "the superproject's pin disagreeing with your checkout".

Both produce the identical symptom, because `--locked` resolution reads the
checked-out source either way.

## Why the wrong remedy is destructive

Following the printed advice on the second cause regenerates the lock **against
the drifted checkout** — so a correct lock is rewritten to match a tree that is
not what the pin names, and the result is committed looking like deliberate
dependency work. The gate then passes, which is the worst part: the tree is
consistent with itself and inconsistent with the pin.

Encountered 2026-09-04 on `packages/cli/nros-launch-resolve` after a rebase.
The lock and the gitlink were **byte-identical to `origin/main`** — nothing had
moved — while the checkout sat at `8fda8d89` against a pin of `4c214a63`. The
fix was `git submodule update packages/cli/third-party/play_launch`, which the
message does not mention.

## Fix

Distinguish the two before advising, which is cheap — the gate already knows
the leaf, so it can compare the recorded gitlink against the submodule's
`HEAD`:

* `HEAD != gitlink` → the CHECKOUT drifted. Print
  `git submodule update <path>`, and do NOT mention `lock-update`.
* `HEAD == gitlink` and the lock still fails → the pin genuinely moved ahead of
  the lock. Print today's message.

Worth a rule beyond this gate: **a remedy line is part of the diagnostic, and a
wrong one is worse than none.** A gate that names a cause it has not
distinguished sends the next person to make the tree worse in a way that then
passes. Related in shape to issue 0445, where an absorbing STALE verdict
replaced the runtime result with a confident wrong explanation.

## Resolved (phase-450 W4, 2026-09-11)

`check-submodule-pinned-locks` split two causes (issue 0600) and there was a
third: the pointer did not move, the CHECKOUT drifted from it. cargo's failure
is identical either way, so the gate printed `just lock-update` — correct for a
moved pointer and destructive here, since the lock matches the RECORDED pin.
Issue 1294 measured the worse half: following that remedy records a submodule
REWIND, which `check-submodule-pins` and the `pre-push` hook exist to refuse.

`_submodule_drift` now compares the submodule's `HEAD` against the gitlink the
superproject records, and only a measured difference produces the third verdict,
whose remedy is `git submodule update <path>` — printed per entry, because two
leaves can drift in different submodules.

**The probe clears the inherited git environment first, and without that it
would be a gate that can never fire.** This runs under `pre-push`, where
`GIT_DIR` is set, and `GIT_DIR` overrides `git -C` — so the probe would read the
SUPERPROJECT's HEAD, find no difference, and report "no drift" always
(issues 0986/0988).

Reproduced on the failure that fired twice during the session that fixed it:
`play_launch` checked out at `db4af878` against a pinned `155ed78b`. Before, it
said "pointer moved, run lock-update"; after, it names the drift and the
`git submodule update` that actually cleared it both times by hand.

1051 and 1294 are one defect filed five days apart by two sessions that each met
it cold, and they close together.
