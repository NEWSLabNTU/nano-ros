---
id: 986
title: "The pre-push hook writes into the repository it is guarding"
status: open
area: tooling
severity: high
related: [0840, 0966]
---

# A selftest that runs against the caller's repo

## What happens

Git sets `GIT_DIR` in a hook's environment. Three scripts reachable from
`.githooks/pre-push` build throwaway repositories, and an inherited `GIT_DIR`
overrides BOTH a path argument and `git -C`:

* `scripts/ci/submodule-commits-reachable.sh` (`git init -q "$tmp/work"`,
  `git init -q "$tmp/super"`, then `git -C "$tmp/super" update-index --add`)
* `scripts/check-source-manifest.sh` (`git init -q .` in two places)
* `scripts/reserve-claim.sh` (four `git init` calls in its selftest)

So the selftests do not run against a temp repo. They run against the caller's.

## Measured, on this checkout

Two distinct kinds of damage, both observed:

1. **Config.** `git init -q "$tmp/work"` with `GIT_DIR` set writes
   `core.bare = true` into `GIT_DIR`'s config. Every later
   `git rev-parse --show-toplevel` then fails with "fatal: this operation must
   be run in a work tree" -- including the one on line 25 of the hook itself.
   The push aborts, the config stays broken, and the next push repeats it.
   Reproduced against a scratch repo shaped like a submodule (separate gitdir,
   `core.worktree` set, no `bare` key):

       before: bare=unset
       git init -q "$tmp/work"   # with GIT_DIR set
       after:  bare=true

2. **Index.** `git -C "$tmp/super" update-index --add --cacheinfo "160000,$1,dep"`
   staged a gitlink named `dep` into the CALLER's index, carrying the selftest's
   deliberately invalid sha:

       AD dep
       Submodule dep 000000000...012345678 (new submodule)

   That is a bogus submodule pin staged into a working repo by the hook whose
   stated job is to refuse bad submodule pins. Had it been committed by an
   unrelated `git commit -a`, the hook would have been the source of exactly the
   failure it exists to prevent.

## Why it went unseen

The selftests redirect to `/dev/null` and return a status, so the damage is
silent. And the hook is only exercised with `GIT_DIR` set when git itself runs
it -- running `bash .githooks/pre-push` by hand does NOT reproduce it, because
an interactive shell has no `GIT_DIR`. A hand-run hook passes and the real one
corrupts.

Worse, it is self-masking: run 1 sets `core.bare=true`, and from run 2 onward
the hook dies at `rev-parse --show-toplevel` BEFORE reaching the selftests, so
the symptom presents as a broken repository rather than as a bad script.

## Fix

`unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY` at the top of
each of the three scripts. Every git invocation in them already names its target
explicitly, so nothing else changes.

## Not covered

No gate runs a hook under a hook's environment. A gate that exports `GIT_DIR`,
runs the hook, and asserts the repo's config and index are untouched afterwards
would have caught this on the day it landed. Related to 0840: hooks that are
installed but never exercised as git exercises them.
