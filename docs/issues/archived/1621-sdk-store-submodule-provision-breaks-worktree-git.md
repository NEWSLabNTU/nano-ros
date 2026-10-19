---
id: 1621
title: "A fast-line probe gate in a linked worktree with zenoh-pico uninitialised
  provisions the submodule, fails the fetch, and leaves a `.git` gitlink pointing
  at a module dir that does not exist — after which `git status` fails for the
  whole worktree"
status: resolved
type: bug
area: cli, build, ci
severity: medium
resolved_in: "branch fix/phase-505-w2-provision"
related: [1280, 1336, 1596, 1745, 1775, phase-505]
---

**Home: [phase-505](../../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What happens

Measured 2026-10-01 in an agent worktree
(`.claude/worktrees/agent-ab4dee612f5d76094`, a linked worktree nested in the
main checkout) whose `packages/rmw/zenoh/zpico-sys/zenoh-pico` submodule was
never initialised, after `just setup-cli` had built the release CLI.

`just check fast` ran `probe-workspace-caps` and `probe-shared-types`. Each
configure reached `cmake/bootstrap.cmake`, which ran the CLI's source
provisioning (`orchestration/sdk_store.rs:1596`):

```text
fatal: Fetched in submodule path 'packages/rmw/zenoh/zpico-sys/zenoh-pico', but it did not contain e28ff6033a2ec31d4fe3b190c75a2718e2a1467d. Direct fetching of that commit failed.
fatal: 'origin' does not appear to be a git repository
Error: provision source zenoh-pico
   0: git fetch --depth 1 e28ff6033a… (packages/rmw/zenoh/zpico-sys/zenoh-pico, source zenoh-pico)
```

Both gates went red (expected — the source is absent). What is NOT expected is
what it left behind: `packages/rmw/zenoh/zpico-sys/zenoh-pico/.git`, a file
reading

```text
gitdir: ../../../../../../../../.git/worktrees/agent-ab4dee612f5d76094/modules/packages/rmw/zenoh/zpico-sys/zenoh-pico
```

That directory does not exist, so from then on:

* `git status` in the worktree fails: `fatal: not a git repository: …/zenoh-pico/../../…/modules/…`;
* `git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico` fails:
  `could not get a repository handle for submodule`.

Deleting that one `.git` file restores both. Before the first run the gate
skipped (no release CLI), so the damage appeared only once the worktree had a
CLI — i.e. it reads as "my setup broke git", not as a gate side effect.

## Why it matters

Parallel agent sessions run `just check fast` in exactly this shape (CLAUDE.md:
"a push from a linked worktree"), and a worktree whose `git status` fails cannot
commit. A gate must not change the repository it checks (issue 0986's rule,
`check-hook-repo-side-effects`), and a provisioning step that fails must not
leave a half-made gitlink.

## Not measured

Whether the gitdir path is wrong (eight `..` from a five-deep path lands outside
the checkout) or merely names a module dir that the failed fetch never created;
whether the main checkout is affected the same way. Fix direction: on a failed
provision, remove what this step created; and resolve the module dir with
`git rev-parse --git-path modules/<path>` (issue 1336) rather than assembling it.

## Resolution (2026-10-19)

### What it was

The "not measured" question has an answer: **the gitdir path was right.** Eight
`..` from `<main>/.claude/worktrees/<id>/packages/rmw/zenoh/zpico-sys/zenoh-pico`
land on `<main>`, and `.git/worktrees/<id>/modules/…` under it is where git
keeps a linked worktree's submodule gitdirs. A fresh reproduction in a scratch
linked worktree writes a gitlink of the same shape (nine `..` for a path nine
directories deep). Only the module dir was missing.

The likely cause is two processes. This is INFERRED from the error sequence;
the race itself was not replayed. `probe-workspace-caps` and `probe-shared-types` ran in
parallel, which was [issue 1745](1745-probe-gates-race-mbedtls-submodule-fetch.md)'s
race in a worktree:

1. One provisioner's `submodule update` wrote the gitlink.
2. The other's `git clone --separate-git-dir` failed, and git removed the
   shared gitdir as junk.
3. The first provisioner's fallback then ran `git -C <path> fetch origin`.
   `'origin' does not appear to be a git repository` is what git prints when
   it walks up from a path that holds no repository.

Three things have changed since:

1. **Serialised.** [Issue 1775](1775-provision-source-races-parallel-gates.md)
   (2026-10-09) allows one provisioner per source per worktree. Two `nros`
   processes can no longer race to remove the gitdir.
2. **Off this path.** phase-484 W3c moved zenoh-pico and mbedtls to
   `location = "store"`, which never writes the checkout's submodule. The
   workspace arm still serves `nuttx-*`, `px4-*`, `cyclonedds-src` and
   `tonbandgeraet`. The fix below covers those.
3. **This fix: a failed provision removes what it created.** Before it runs,
   the workspace submodule arm of `provision_source` records a
   `SubmoduleBefore`: whether `<path>/.git` existed, whether the dir had
   content, and the module dir. The module dir is ASKED for with `git rev-parse
   --path-format=absolute --git-path modules/<name>`, never assembled (issue
   1336). The name comes from `.gitmodules` through the new `submodule_name`,
   which `ensure_submodule_branch_refspec` now calls too. On any error, the arm
   removes each module dir, gitlink or checked-out entry that did not exist
   before, and the error says what it removed. Two guards come with it:
   - `heal_dangling_gitlink`: before provisioning, a `<path>/.git` FILE that
     `git rev-parse --absolute-git-dir` cannot follow is removed. Such a file
     makes every git command in the worktree fatal, so keeping it helps
     nobody. This is also how a worktree damaged by an older CLI repairs itself
     on the next `nros setup`. A `.git` that resolves, or a `.git` directory,
     is never touched.
   - `is_own_repository`: the by-SHA fallback runs `fetch` and `checkout` with
     `-C <path>`. If there is no repository at `<path>`, git walks up and finds
     the SUPERPROJECT. That walk produced the measured `'origin'` message, and
     it would let `checkout <sha>` run against the checkout being provisioned.
     The fallback now refuses unless `<path>` is its own top level.

### Measured

In a scratch linked worktree of this repo, created for the run and removed
afterwards, with `tonbandgeraet` (a workspace-located submodule source):

| step | `git status` |
| --- | --- |
| before | OK |
| submodule initialised, its module dir deleted, only `<path>/.git` left (the 1621 state) | **BROKEN** (`fatal: not a git repository`) |
| `nros setup --source tonbandgeraet` (this branch) prints `removed …/.git, a gitlink naming no repository (issue 1621)`, re-clones, rc=0 | OK, submodule at its pin |
| deinit and remove the module dir, then 3 concurrent `nros setup --source tonbandgeraet` | 3/3 rc=0; OK, submodule at its pin |

### Guard

Two tests in `sdk_store::tests`, each mutation-checked: with `undo()` and the
heal stubbed out, both FAIL; with them in place, both pass.

- `a_failed_submodule_provision_removes_the_gitlink_it_wrote`: the gitlink
  records a commit that the module dir (deinit'd, kept) does not have. The
  provision fails. The `<path>/.git` it wrote is gone, the module dir that
  existed before survives, and `git status` works.
- `a_gitlink_naming_no_repository_is_removed_in_a_linked_worktree`: builds the
  measured state in a real `git worktree add` worktree and asserts that
  `git status` fails (the reproduction). It then provisions, and asserts that
  `git status` works in the worktree and in the main checkout.
