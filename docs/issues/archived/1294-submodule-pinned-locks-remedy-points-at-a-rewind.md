---
id: 1294
title: "`check-submodule-pinned-locks` names `just lock-update` as the remedy even
  when the CHECKOUT is behind its pin — and following it records a rewind"
status: resolved
type: bug
area: build, ci
severity: medium
found: 2026-09-11
related: [issue-0560, issue-0409, rfc-0099, issue-1051, phase-450]
---

## What happens

When a lock pinned by a submodule manifest stops resolving, the gate prints one
remedy:

```
The submodule pointer moved and the lock did not follow (issue 0560).
Update it the sanctioned way — never a bare `cargo generate-lockfile`:
    just lock-update "" "" <leaf-dir>
```

That diagnosis assumes the LOCK is behind the pointer. The same symptom arises
in the opposite state — the pointer and the lock agree, and the submodule
CHECKOUT is the thing that lags — and there the remedy is actively wrong:
`just lock-update` re-resolves the lock against the stale checkout, which moves
the lock BACKWARD. A rewind, produced by following the gate's own instruction.

## Measured

phase-447 F1 rebased a worktree 59 commits onto `main`. `main` had moved
`packages/cli/third-party/play_launch` 0647131 -> 155ed78; the worktree's
checkout stayed at 0647131:

```
$ git submodule status packages/cli/third-party/play_launch
+0647131a7cc157aeed2ef7d4de49062001c965df packages/cli/third-party/play_launch
```

The gate failed with `failed to load source for dependency
ros-launch-manifest-check` — a crate that exists only in the NEWER play_launch —
and named `just lock-update`. The actual fix:

```
git submodule update --init --depth 1 packages/cli/third-party/play_launch
```

left `Cargo.lock` untouched and turned the gate green. No lock change was the
proof the diagnosis was right.

## Not the first time

Earlier in the same campaign `just lock-update` proposed moving rlm
`v0.1.33 -> v0.1.23` for the same reason, caught only because someone read the
`+` prefix. CLAUDE.md already documents the rule ("when a submodule is BEHIND
the recorded pointer … the fix is `git submodule update <path>` regardless of
kind"), which means the knowledge exists and the gate's message does not use it.

## Fix

Before printing a remedy, ask `git submodule status -- <owning-submodule>`.
A `+` or `-` prefix means the checkout differs from the recorded pin: name
`git submodule update --init --depth 1 <path>` and say plainly that the lock is
not the problem. Only a clean (space) prefix earns `just lock-update`.

One second-order cost worth stating in the same message: `play_launch` lives
under `packages/cli/`, so advancing its checkout re-arms the in-tree CLI's
source stamp, and `just setup-cli` is needed before the next push. That cost F1
one more refused push after the right fix had already landed.

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
