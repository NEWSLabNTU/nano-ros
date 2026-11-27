---
id: 3009
title: "A submodule whose checkout never happened reads as provisioned: an unborn HEAD and no index answer every probe with 0"
status: resolved
type: bug
area: [build, ci, testing]
severity: medium
found: 2026-11-27
resolved: 2026-11-27
related: [2306, 2505, 2701]
---

## Measured

In a linked worktree (`nano-ros-box2/.claude/worktrees/w7r3`), on 2026-11-27,
`just setup-worktree` printed

```
  [OK]      packages/rmw/zenoh/zpico-sys/zenoh-pico already checked out.
```

over an empty `zenoh-pico` directory. Its gitdir
(`.git/worktrees/w7r3/modules/.../zenoh-pico`) held the objects, including the
pinned commit, but `HEAD` was `ref: refs/heads/main` on a branch that did not
exist, and there was no `index`. The directory held only its `.git` file.
`git submodule update <path>` then refused with `Unable to find current
revision in submodule path`.

The lane ran anyway: `just check fast` went 6 red and `just ci gate` 63 red,
every one a `zpico-sys` build script or a gate that reads the zenoh-pico tree.
None of them named the cause; a full gate run (about 90 minutes) was spent
finding it.

A related sighting the same day: after a zenoh-pico pin move, another session
found its checkouts behind the pin, and the pre-push hook refused until a manual
`git submodule update`.

## Cause

`nros_submodule_provision_state` (`scripts/lib/submodule-provision.sh`, issue
2306) is the one answer to "is this submodule provisioned". Every probe it ran
fails quietly on a checkout that never happened:

- `git submodule status --cached` marked the path ` `, as if it were on its pin.
- `git -C <p> ls-files --deleted` with no index lists nothing to be missing.
- `git -C <p> diff --cached HEAD` on an unborn HEAD errors, and the error read
  as an empty list.

So both counts were 0, the mark was ` `, and the state was `ok`. The same
holds for a missing index or an empty one under a valid HEAD. Separately,
`just post-rebase` did not ask the helper at all: it read the `+` mark alone
and printed "every initialised submodule matches its pin".

## Resolution (2026-11-27)

- The helper now requires the checkout to prove itself before any count is
  read. It must have its own `.git` (and `git -C` must not resolve to another
  repository), a HEAD that names a commit, an index, and an index that lists
  something its HEAD has. Otherwise the state is `incomplete:no-checkout`,
  `incomplete:no-head`, `incomplete:no-index` or `incomplete:empty-index`, all
  red.
- `ok` is now a measured equality, HEAD equal to the recorded gitlink, never
  the ` ` mark alone.
- An unborn HEAD gets its own remedy, `git -C <p> checkout --detach -f <pin>`,
  because `git submodule update` cannot repair it.
- `nros_submodule_incomplete_reason` gives each state one phrase. The
  `setup-worktree` and drift-check messages use it, instead of printing a
  word like `no-head` as a file count.
- `setup-worktree` prints `[OK]` only for the `ok` state. An unrecognised
  state is a failure, not a pass.
- `just post-rebase` classifies every initialised submodule through the
  helper, and names each one that is not `ok` with its repair.

`tests/submodule-provision-tests.sh` (`check submodule-provision`) builds each
shape: the measured one (unborn HEAD, no index, empty tree), an unborn HEAD over
a complete tree, no index, an empty index, and a missing `.git`. It also runs
each state's own remedy and expects `ok` after it. With the old helper, the
first four read `ok`.
