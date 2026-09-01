---
id: 981
title: "`codegen_golden` has been red on `main` since 5f3c08545 — the C++ pack's
  REAL bound moved `RX_MAX_SERIALIZED_SIZE` by 3 and the golden was not
  regenerated"
status: open
type: bug
area: codegen, ci
severity: medium
found: 2026-09-01
related: [phase-408, issue-0896, issue-0952]
---

## Symptom

`just ci gate` fails at step 3 of 6 (`check::build`, via `check-cli-tests`):

```
CHANGED configured/Capped.hpp:
  line 66:
    golden:     static constexpr size_t RX_MAX_SERIALIZED_SIZE = 157;
    now   :     static constexpr size_t RX_MAX_SERIALIZED_SIZE = 160;

CHANGED inline/Bounded.hpp:
  line 70:
    golden:     static constexpr size_t RX_MAX_SERIALIZED_SIZE = 133;
    now   :     static constexpr size_t RX_MAX_SERIALIZED_SIZE = 136;

failures:
    generated_output_matches_the_committed_golden
```

Three files, every one of them `RX_MAX_SERIALIZED_SIZE`, every one +3.

## Measured, on `main`, in a pristine worktree

Not a local-state artefact. Reproduced in a `git worktree` freshly checked out
at `6662869d2` (`origin/main`) with nothing else in it:

```
6662869d2  test result: FAILED. 1 passed; 1 failed
```

Bisected by running that one test at each revision, again in the clean
worktree:

```
0744e8ba1  ok       (ec63d4ed9^)
ec63d4ed9  ok       fix: a receive buffer is sized for what the transport DELIVERS
07748a644  ok       feat(phase-408 W5b): the info and validated C subscriptions size their arena
5f3c08545  FAILED   feat(phase-408, #0896): the C++ pack emits the REAL bound, and spends it
6ef2c2281  FAILED   fix(phase-408): ledger the three C++ items the parity gate had no row for
```

`07748a644` is `5f3c08545`'s parent, so **`5f3c08545` is the first bad
commit** — adjacent revisions, no interval left to search.

Its own subject says the +3 is the intended behaviour ("the C++ pack emits the
REAL bound, and spends it on the receive buffer"). It touched
`packages/cli/rosidl-codegen/tests/` in the same commit, so the golden was not
forgotten wholesale — the three `RX_MAX_SERIALIZED_SIZE` lines were just not
regenerated with it.

**Not yet established:** whether the generator's new number is right in all
three cases, or whether one of them is a real sizing bug wearing the same +3.
That is why this is filed rather than fixed with `NROS_UPDATE_GOLDEN=1` — the
test's own remedy line says to read the resulting diff before committing, and
nobody has.

## Why nobody noticed for a day

`codegen_golden` runs in `check-cli-tests`, which is on `check::build`. Per
CLAUDE.md, `check-build` is `schedule` / `workflow_dispatch` only:

> `check-build` is now `schedule`/`workflow_dispatch` only — it was on the merge
> group and could never pass there

So no pull request and no merge group runs this test. The required `CI` context
is `check-fast` + `test-unit`, and both are green on the commit that broke it.
A red that no merge-gating lane can see is a red that lands.

This is the second half of issue 0952's point about a lane that stops at its
first failure: `check::build` is the only step where the C/C++ backends compile,
and it has been withdrawing every step after it — `check::api-parity`,
`test-unit`, `test-lane-contracts` — for anyone running the local tier since
2026-08-31 19:12Z.

## Direction

1. Regenerate (`NROS_UPDATE_GOLDEN=1 cargo test -p rosidl-codegen --test
   codegen_golden`) and READ the diff: confirm each +3 is the encapsulation
   allowance `5f3c08545` intended and not an off-by-one that happens to be the
   same size in all three.
2. Separately: `check-cli-tests` gates nothing on a PR. Either it belongs on a
   lane that merge-gates, or the fact that it does not should be said where
   someone reads it — a golden test nobody runs before merging is a golden test
   that records history rather than enforcing it.

## Acceptance

* `just ci gate` reaches step 6 on a pristine `main`.
* The three regenerated numbers each have a stated reason, not just a new value.
