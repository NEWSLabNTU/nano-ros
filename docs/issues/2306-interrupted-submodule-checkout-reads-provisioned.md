---
id: 2306
title: "An interrupted submodule checkout reads as provisioned, and the lane fails three gates later"
status: open
type: bug
area: [build, ci, testing]
severity: medium
found: 2026-11-06
related: [1513, phase-543]
---

**Home: [phase-543](../roadmap/phase-543-nextest-gates.md).**

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
