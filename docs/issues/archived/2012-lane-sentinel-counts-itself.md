---
id: 2012
title: "The lane sentinel counts its own probe as a group member, so after every stop it lingers ~11 s and SIGKILLs itself, and `check-lane-stop-kills-fanout` flakes under load"
status: resolved
type: bug
area: [ci, build]
severity: medium
found: 2026-10-23
resolved: 2026-10-23
related: [1350, 0853, 2008]
---

## What happens

`check-lane-stop-kills-fanout` failed in `just ci gate` three times out of
three on 2026-10-23, always on one case, and passed three times out of three
run alone:

```
check-lane-stop-kills-fanout: FAILED (issue 1350)
  - owner-kill: 1 process(es) survived a SIGKILLed owner — the sentinel did not fire: [1888465]
```

An earlier red of the same gate in the same session was unexplained, because
`check pristine-checkout` dropped the failure body (issue 2008). With that
fixed, the body above is what it printed.

## Cause

Measured, not inferred.

1. **The survivor is the sentinel itself.** A probe that prints each
   survivor's parent and group at kill time found one process alive 11.8 s
   after the owner's SIGKILL. Its parent and process group were both the
   guard's holder subshell, so it was the sentinel `nros_guard_lane` starts in
   the guarded group, not a stray gate.
2. **It always lingers, idle too.** With no load the last process was gone
   11.3, 11.3 and 11.6 s after the kill, against the case's 12 s window. Under
   load: up to 12.0 s. So the case passed by 0.4 s idle and lost that margin
   under the parallel fast line. It was never a fast path that load slowed
   down.
3. **Why.** After the group TERM, the sentinel ignores TERM and drains:
   "only this sentinel left (or nothing): done", spelled
   `[ "$(_nros_guard_group_members <own pgid> | wc -l)" -le 1 ]`. Asking
   forks: the `$(…)` subshells, `ps`, `awk` and `wc` are members of the
   sentinel's own group when `ps` takes its snapshot. A shell alone in its
   group counted **6**. So the condition could never hold, the loop always ran
   its full 10 s, and the sentinel then `kill -KILL 0`-ed its own group.

Every other caller of `_nros_guard_group_members` (the build guard's reap and
cleanup, the lane owner's stop) asks about a group it is OUTSIDE of, so its
probes land in its own group and are not counted. Only the sentinel asks about
the group it is in.

Consequence beyond the flake: every lane stop left the sentinel polling for
~11 s, and the gate meant to see a lingering fan-out could not tell an 11 s
sentinel from a 1 s one.

## Fix

`_nros_guard_group_others <pgid> <self-pid>` (`scripts/build/subtree-guard.sh`)
lists a group's live members minus the caller and its descendants. That is
exact, because the sentinel starts nothing but its own probes. The sentinel's
drain uses it.

The owner-kill case now waits 6 s instead of 12. The window width is the
assertion: a working sentinel is gone in about one poll, a self-counting one at
about 11 s.

Measured:

| | owner-kill: last process gone after the kill | gate, 3 runs |
| --- | --- | --- |
| main | 11.3 - 12.0 s | passes by the 12 s window, flakes under load |
| fix | 0.5 - 0.7 s | 3/3 idle, 3/3 under 32 busy loops |
| fix, sentinel line reverted (mutant), 6 s window | - | **red 3/3** |

Positive control for the helper: in the real topology (holder, then sentinel
plus a sibling), the sentinel's query reports the sibling and the holder, and
excludes only itself and its probes.
