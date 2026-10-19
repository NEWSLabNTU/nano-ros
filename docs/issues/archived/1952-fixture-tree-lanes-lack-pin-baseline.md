---
id: 1952
title: "The fixture-tree lanes never receive the submodule-pin baseline, so check fast fails closed there since issue 1912's fix"
status: resolved  # closed by issue 1959's steps; composition now tested
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-18
related: [1912, 1941, 1959]
resolved_in: 2026-10-19
---

## Measured

The first nightly after issue 1912's fix (`50d8d52206`, #2362) failed its tier-2 job in
`check fast`, before any fixture was built:

- Run: nightly 39099637271 (schedule, 05:10 UTC, head `6b58a7fb5`).
- Job: 117593074369, `tier 2 nightly (pairwise cover)`, on `nano-ros-runner-newslab-server-241`.
- Decisive lines:

```
submodule-pins: FAILED — baseline '26bf4e1d6c6e1a7d94d64251bcbcb3ea6e8db4f2' does not resolve, in CI.
check-fast (parallel): 1 of 456 gate(s) FAILED
```

On a `schedule` event the rule correctly resolves the baseline to the tested
commit's first parent, `26bf4e1d6c`. The problem is where `check fast` runs.
The run-matrix `matrix` and `tier1` jobs and the nightly `matrix-nightly` job
all run it inside the persistent `nros-fixture-tree`, which is their
`defaults.run.working-directory`. That tree is a SEPARATE repository, and
`scripts/ci/fixture-tree.sh sync` fetches only the job's own commit into it,
from the job checkout. None of the three jobs runs `pin-baseline.sh provide`.
Running it in the checkout would not help either: the fixture tree has no
`origin` to fetch the baseline from. So the baseline commit is absent and the
gate fails closed, which is what it is designed to do in CI.

This is not the earlier two shapes (REWIND, NOT VERIFIED). It does not affect
the gate.yml or host-tests.yml jobs, which run `check fast` in the checkout
after `provide`.

**What would close it.** Run the gate against a baseline that exists in the
fixture tree. There are at least two defensible ways, so this is not patched
blind:

1. `fixture-tree.sh sync` fetches the resolved baseline commit alongside the
   tested one. It can call `pin-baseline.sh resolve` in the checkout, then
   fetch both shas over the same local transport after `provide` has fetched
   the baseline into the checkout. It also exports
   `NROS_SUBMODULE_PIN_BASELINE` for the tree.
2. The three jobs run `provide` with `working-directory: ${{ github.workspace }}`
   before `sync`, and `sync` carries the fetched commit across.

Acceptance: a scheduled run-matrix and a nightly tier-2 job whose `check fast`
reports `submodule-pins: OK`. The run-matrix schedule (06:24 UTC) is the next
lane to hit this.

## Resolution

**Closed by issue 1959** (`9a1ff7855f`), filed the same day for the same runs.
Issue 1959 took a variant of option 2: the three jobs (and `live-peer`'s `board` and
`board-ci`) run `pin-baseline.sh provide`, `init-moved` and `deepen` before
`just build <lane>`, with the fixture tree as their working directory.
`check-workflow-pin-baseline` holds that rule for every job that reaches the gate.

**One premise above was wrong.** The fixture tree does have an `origin`. `sync`
has mirrored the job checkout's `origin` URL and `refs/remotes/origin/*` into
the tree since phase-489 W6 (`b522535dba`). So `provide`, run inside the tree,
fetches the baseline from GitHub just as it does in a checkout. The repository is
public, so the fetch needs none of the credentials `actions/checkout` keeps in the
checkout's own config.

**Why option 1 was not added.** It would be a second carrier for the baseline
commit: `sync` would carry it over from the checkout, and the in-tree `provide`
would still have to run to export `NROS_SUBMODULE_PIN_BASELINE` and fetch
`origin/main` for the other fast gates. Each job would also need one more step in
the checkout before `sync`. It would help only if the tree could not reach
`origin`, and the self-test below shows that it can.

**What was missing was proof that the steps work in the tree.**
`check-workflow-pin-baseline` checks that the `provide` step exists and runs
before the build. It does not check that the step can succeed in the tree's
working directory. If it could not, every fixture lane would go red with exactly
this issue's message. `scripts/ci/fixture-tree.sh --self-test` now runs the
real scripts in the lane's order. The tree is synced from a depth-1 clone and
holds one commit. The event is CI `schedule`.

- With no `provide`, `submodule-pins-check.sh` fails closed: this issue's shape.
- `pin-baseline.sh provide` with the tree as cwd exports the first parent and
  fetches it through the mirrored `origin`.
- After that, the gate reports `submodule-pins: OK`.

The self-test runs on the fast line through `check-fixture-tree-persistence`.
Mutation-tested: if `sync` stops mirroring `origin`, three cases go red. If
`provide` skips its fetch, two go red.

Sweep: `git grep -n 'fixture-tree\|working-directory' -- .github/workflows`
lists the three fixture-tree jobs (run-matrix `matrix` and `tier1`, nightly
`matrix-nightly`). All three provide the baseline before they build, and
`python3 scripts/check-workflow-pin-baseline.py` passes.

Acceptance is still the one stated above. It needs a scheduled or dispatched run
on the self-hosted runner, and none has run since `9a1ff7855f`.

