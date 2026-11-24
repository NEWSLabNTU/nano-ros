---
id: 2930
title: "gate.yml starts sccache with ten inline copies of one configure block, in six variants, instead of the helper"
status: open
type: tech-debt
area: [ci]
severity: low
found: 2026-11-24
related: [874, 1646]
---

## What

Every gate.yml job that compiles starts sccache with its own inline
`Configure sccache` step: ten copies, at lines 484, 850, 991, 1236, 1618,
1856, 2093, 2371, 2683 and 3195 on main at the time of filing. Hashed with
their comments removed, the bodies fall into six different variants. All ten
carry the same contract, issue 0874's: prove the server starts, publish to
`$GITHUB_ENV` only if it did, and warn rather than fail when sccache is absent
or broken. The warm job also publishes `RUSTC_WRAPPER` as the shim (issue 1646).

phase-591 W3 added `scripts/ci/sccache-configure.sh` as the one spelling of
that step (`--dir`, `--size`, `--wrapper`, `--stats`). ros2-interop and
host-suite use it. gate.yml was left alone so the change that added the
helper did not also rewrite the merge-gating workflow.

## Why it matters

This is the shape CLAUDE.md's "fix the class" rule is about. A fix to the
contract (a new variable, a different log path, a size change) lands in
whichever copies the editor finds. The six variants show it has already
happened: the copies do not all say the same thing any more, and nothing
checks which differences are intended.

## Fix

Replace each inline step's body with `scripts/ci/sccache-configure.sh` and
the flags that reproduce it: `--dir /tmp/sccache --size 2G` for the
compiling-event jobs, with `--wrapper` added for the warm job. Keep each
step's `if:` condition and the `actions/cache` restore/save steps around it
unchanged.

First diff the six variants. Any difference that is intended becomes a flag;
any that is not is drift, and the issue's fix commit says which was which. A
gate that refuses an inline `sccache --start-server` in a workflow keeps the
class closed.

## Closes when

No workflow starts sccache except through the helper, and a gate holds that.
