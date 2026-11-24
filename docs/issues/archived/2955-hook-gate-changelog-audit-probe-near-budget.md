---
id: 2955
title: "check-hook-repo-side-effects fails under host load: its changelog-audit.py probe takes 78-82 s alone against a 120 s budget"
status: resolved
type: bug
area: [ci]
severity: medium
found: 2026-11-24
resolved: 2026-11-25
related: [2669, 1964]
---

## What happens

`check-hook-repo-side-effects` is on the `check fast` line. On a loaded
host it fails, and the same gate passes when rerun alone at the same head.
Today (2026-11-24) that happened at least three times:

| Where | Load at the time | Failing probe | Solo rerun |
| --- | --- | --- | --- |
| host 142, `just check fast` on the 2947 branch at f1e2dd5346 | 1-min load ~116; the token pool measured host runnable p50 85, p90 291, max 332 | `scripts/changelog-audit.py`, both `[explicit]` and `[worktree]` shapes: "did not finish: 120.3s (build 0.3s, run 120.0s)" | passed |
| host 142, `check fast` on the lane-rename branch (#4791) | heavy (a full gate ran beside it) | not recorded | passed 3/3 |
| #4741's lane (reported by the 241 manager) | under load | not recorded here | passed |

A killed probe is a failure, not a skip (issue 2669's rule), which is
right. The trouble is how little headroom this probe has.

## The measurement

The same gate run alone on host 142 at the same head, a few minutes later,
printed:

```
ok    scripts/changelog-audit.py [worktree] 82.4s (build 0.0s, run 82.4s)
ok    scripts/changelog-audit.py [explicit] 78.1s (build 0.0s, run 78.0s)
ok    scripts/changelog-audit.py [tmpdir]   25.3s (build 0.1s, run 25.1s)
```

So the probe uses about two thirds of its 120 s budget (`PROBE_TIMEOUT`)
with the host to itself. Any load that slows it by about 1.5x kills it. Every
other probe in the same run finished in under 20 s.

## Why it matters

`check fast` runs on every event, including `merge_group`. A
`merge_group` run on a busy self-hosted runner that hits this ejects the
entry and every entry behind it, and the ejection names a hook-safety
gate. That reads like a real git-environment leak (issues 0986/1465), so
each one gets re-diagnosed from scratch.

## Where to look (as filed)

`changelog-audit.py` runs its NORMAL path in the probe (`probe_args`
returns `""` for it), which audits the real tree's history. The
`git init` sites the probe exists to exercise are in its self-test. The
same shape was fixed twice before by giving the probe the self-test:
`check-workspace-probes.sh` (issue 2669) and `check-book-probe-tracks.py`
(#4228, slow-disk hosts hitting 120 s in all three shapes). Before
switching it, check that `--self-test` reaches every repository-building
path in `changelog-audit.py`, since that coverage is what the probe asserts.

## Resolution

The probe now runs `changelog-audit.py --self-test` (`probe_args` in
`scripts/check-hook-repo-side-effects.sh`), the shape issue 2669 and #4228
gave two other probes.

Coverage checked first: the script builds repositories only inside
`self_test()` (its `git init -b main` scratch repo and the shallow
`git clone --depth 1` used to test the HistoryUnknown outcome). The normal
path runs that same self-test and then audits the real history, which is
`git log`/`git describe` reads after `nros_clear_inherited_git_env()` and a
`gh` GraphQL call per 100 commits. So `--self-test` reaches every
repository-building site, and the audit adds only reads and network time.
The separate "clears the inherited git environment" assertion on this
script is unchanged.

Measured on host 142 (aarch64): the self-test alone is 1.4 s against 77 s
for the normal path. With the change the gate's three shapes take 1.6 s,
1.5 s and 1.5 s, and `check-hook-repo-side-effects` is OK.
