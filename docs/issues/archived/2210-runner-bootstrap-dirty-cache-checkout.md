---
id: 2210
title: "`runner-bootstrap.sh` refused to refresh a `src` checkout holding a build-rewritten tracked file, leaving the runner offline"
status: resolved
type: bug
area: [ci]
severity: medium
found: 2026-11-01
resolved: 2026-11-02
related: [1863, 1841, 1585, 2191]
---

## Measured

On runner 118, 2026-11-01, the first `runner-bootstrap.sh` after PR #2973
merged failed, and the runner stayed offline for about 5 minutes. Inside the
`nros-runner-src` volume's checkout,
`packages/cli/third-party/play_launch/src/ros-launch-resolve/parser/Cargo.lock`
had been rewritten (52 lines deleted). The submodule update then stopped:

    Your local changes to the following files would be overwritten by checkout
    Unable to checkout '442faaa34f…' in submodule path 'packages/cli/third-party/play_launch'

The 118 session discarded that one file with `git checkout --` from a
throwaway container as uid 1001 and re-ran the bootstrap, which passed.

## Cause

Step 2 of the bootstrap (clone-or-fetch into `src`, then every submodule to its
pin — issue 1585) used a plain `git checkout FETCH_HEAD` and a plain
`git submodule update`. Both refuse to overwrite a modified tracked file. That
refusal protects work, but this checkout holds none: it is a cache the
bootstrap owns, and nobody edits it. A modified file there is residue from
some build, and refusing over it turns a stray write into a runner outage.

**Who wrote the lock is NOT established.** The 52-line deletion has the shape
issue 1863 measured (an unlocked build of the parser's Python half drops lock
content), and 1863 made that build `--locked` on 2026-10-13. Its gate also
covers every cargo call in the tree. So the likeliest writer is a pre-1863
setup whose residue sat in the persistent volume until a pin move first made
the submodule check out. That is unverified, because the file was discarded
before anyone diffed it.

## Fix

- The superproject checkout is `--force`, and both submodule updates are
  `--force`. `--force` also re-checks-out a submodule that is already at its
  pin, so a dirty file there is restored rather than left behind.
- Before moving, the bootstrap prints every discarded tracked change: the
  superproject's `status --porcelain`, then each submodule's, prefixed with its
  path. The discard is never silent, so the next occurrence names its writer
  in the bootstrap log.
- Step 2 is its own script, `SRC_SYNC_SH`, and `runner-bootstrap.sh
  --sync-src <dir>` runs it alone on the host. That is a manual repair for a
  cache checkout, and it is what the test drives.

Gate: `just check runner-bootstrap-src-sync`
(`tests/runner-bootstrap-src-sync-tests.sh`, 6 checks, a local `file://`
origin). The superproject and the `play_launch` submodule both move over a
dirtied file, and the test asserts: the refresh succeeds, both land on the new
pins, nothing is left modified, and each discarded file is named. A dirty
submodule already at its pin is restored too. Mutation-checked: without the
three `--force`s, 4 of the 6 checks fail.
