---
id: 1621
title: "A fast-line probe gate in a linked worktree with zenoh-pico uninitialised
  provisions the submodule, fails the fetch, and leaves a `.git` gitlink pointing
  at a module dir that does not exist — after which `git status` fails for the
  whole worktree"
status: open
type: bug
area: cli, build, ci
severity: medium
related: [1280, 1336, 1596]
---

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
