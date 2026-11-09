---
id: 2306
title: "An interrupted submodule checkout reads as provisioned, and the lane fails three gates later"
status: resolved
type: bug
area: [build, ci, testing]
severity: medium
found: 2026-11-06
resolved: 2026-11-09
related: [1513, phase-543]
---

**Home: [phase-543](../../roadmap/phase-543-nextest-gates.md).**

## Measured

On 2026-11-06, a `just setup-worktree` in a fresh worktree (`nros-wt543`) was
stopped mid-run: its background task was killed so the disk could be freed.
Re-running provisioning then reported every submodule ready, and `just ci gate`
passed its provisioning preflight. The lane then failed three times, each for a
reason that did not name the cause:

1. `check::probe-workspace-caps`: `failed to run custom build command for
   zpico-sys`.
2. After that was fixed, six `check::build` gates (`rmw-xrce`, `test-targets`,
   `workspace-all`, `linux-check`, `workspace-features`,
   `required-features-tests`): `nros-rmw-xrce-cffi: vendored micro-cdr source
   root … is missing or has no .c files`.

The submodules were in two broken states, both left by the interrupted checkout:

| Submodule | `git submodule status` | Working tree |
| --- | --- | --- |
| `zpico-sys/zenoh-pico`, `third-party/dds/cyclonedds`, `xrce-sys/micro-xrce-dds-client` | `+` (HEAD off the pin: upstream `main`/`master`) | 683 / 1541 / 317 entries staged `D` |
| `xrce-sys/micro-cdr` | ` ` (on the pin) | only `.git`; 65 entries staged `D` |

The second row is the one no check can see today. `setup-worktree` and the
preflight (`_require-worktree-provisioning`, issue 1513) ask
`git submodule status --cached`, which compares the gitlink to the submodule's
HEAD. An on-pin HEAD over an empty working tree answers "initialised, on the
pin". The first row was visible to `git submodule status` (the `+`), but the
preflight does not treat `+` as unprovisioned either.

Each was fixed by `git submodule update --init --force <path>`, after checking
that the git dirs were this worktree's own (`.git/worktrees/<wt>/modules/…`) and
held nothing but the staged deletions.

## What would close it

The provisioning check reads the submodule's WORKING TREE, not only its HEAD.
A submodule the lane needs counts as provisioned when its HEAD equals the pin
AND `git -C <path> status --porcelain` is empty. Anything else is a red that
names the path and the repair (`git submodule update --init --force <path>`),
before any gate runs. A submodule off its pin is reported as such, rather than
surfacing as a build error three steps later.

The same check belongs wherever "is this submodule here" is asked:
`setup-worktree`, `_require-worktree-provisioning`, and
`check-tier-preconditions` (CLAUDE.md: a `doctor` that says OK over something
`build` then refuses is the same lie).

## Resolution (2026-11-09)

One answer, `scripts/lib/submodule-provision.sh`
(`nros_submodule_provision_state`), now reads the submodule's working tree as
well as its HEAD, and every place that asked "is it provisioned" uses it:
`_require-worktree-provisioning` (the lane preflight), `just setup-worktree`,
and `check-submodule-drift.sh` (run by `check-tier-preconditions`).

| State | Meaning | Red? |
| --- | --- | --- |
| `missing` | not initialised | yes |
| `incomplete:<n>` | `<n>` tracked files absent from the tree or staged as deleted: the interrupted checkout, both rows of the table above | yes |
| `diverged` | HEAD shares no line with the pin (e.g. upstream `main`) | yes |
| `behind:<n>` / `ahead:<n>` | an un-updated checkout, or a fork fix in progress | no |
| `ok` | on the pin, tree complete | no |

A red names the path and the repair. `setup-worktree` does not repair an
incomplete or diverged checkout itself, because `--force` would discard what
the tree holds, but it now fails instead of reporting "done".

Two departures from "What would close it", both deliberate:

- **Cleanliness is not required.** `git status --porcelain` being empty would
  make every modified file in a fork submodule refuse the lane. What is
  checked is completeness: tracked files present, nothing staged as deleted.
- **Behind and ahead are not red here.** `check-submodule-drift.sh` already
  fails a checkout that is behind, listing the commits, in
  `check-tier-preconditions`, and ahead is the vendored-fork workflow's normal
  middle state. A checkout on an unrelated line (`diverged`) is red.

The drift check also gained a completeness pass over every initialised
submodule, because its existing loop only visits `+`/`U` rows and the
on-pin, emptied checkout (micro-cdr's row) is a ` ` row.

Test: `just check submodule-provision` (`tests/submodule-provision-tests.sh`)
builds a superproject and drives every state, including both rows that used to
read as provisioned.
