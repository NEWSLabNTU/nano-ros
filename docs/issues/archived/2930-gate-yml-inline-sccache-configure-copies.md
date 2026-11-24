---
id: 2930
title: "gate.yml starts sccache with ten inline copies of one configure block, in six variants, instead of the helper"
status: resolved
type: tech-debt
area: [ci]
severity: low
found: 2026-11-24
resolved: 2026-11-24
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

phase-591 W3 wrote `scripts/ci/sccache-configure.sh` as one spelling of that
step (`--dir`, `--size`, `--wrapper`, `--stats`), for live-peer's fixture
loop. That use was measured and dropped (#4750, closed: no gain), so the
helper lands with this issue's fix instead, as the only spelling.

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

## Resolution (2026-11-24)

Eleven copies, not ten: `host-tests.yml`'s `check-build` job carried one too.
Every one now calls `scripts/ci/sccache-configure.sh`. Diffed, the six
variants differed in three ways, each now a flag:

- the self-hosted skip, intended, so `--skip-self-hosted` (four gate.yml
  jobs whose `runs-on` is self-hosted on merge_group and dispatch);
- the warm job's `RUSTC_WRAPPER` and its unconditional cache-dir create,
  intended, so `--wrapper` (the helper always creates the dir);
- the warning's wording ("the compile tier", "the link smoke", "this job
  warms NOTHING"), cosmetic, so `--what`.

The values every copy shared are passed explicitly (`--dir /tmp/sccache --size
2G --log /tmp/sccache-server.log`), since later steps read that log path.

The helper differs from the copies in two places:
- It stops a server an earlier step started before starting its own, except
  on a self-hosted runner, whose server is shared. Live-peer run 45313891417
  showed why: a server the justfile started first made the helper's start
  fail with `Address in use`, and the step cached nothing.
- It starts the server with no jobserver in its environment, as the shim
  does (issue 2284).

`check-rustc-wrapper-staticlib` gains rule 3, which refuses an inline
`sccache --start-server` in any workflow or composite action. Against
main's gate.yml it reports all ten copies.
