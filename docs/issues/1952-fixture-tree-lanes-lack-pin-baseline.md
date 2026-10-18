---
id: 1952
title: "The fixture-tree lanes never receive the submodule-pin baseline, so check fast fails closed there since issue 1912's fix"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-18
related: [1912, 1941]
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
