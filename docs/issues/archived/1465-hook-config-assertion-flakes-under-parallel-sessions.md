---
id: 1465
title: "`check-hook-repo-side-effects` asserted on a config every worktree SHARES, so a
  concurrent agent session made it report issue 0986 — a red about the observer,
  not about the hook"
status: resolved
type: bug
area: [ci, tooling]
related: [0986, 0988, 1336, 1391, 1466]
found: 2026-09-23
resolved_in: "2026-09-24 — the end-to-end hook run's working directory is now a checkout the gate built"
---

## What it was

The gate ended with a sha1 of `git rev-parse --path-format=absolute --git-path
config` taken before and after an end-to-end `pre-push` run. In a linked
worktree that resolves to the MAIN checkout's `.git/config` — one file shared by
every agent session in this repository (issue 1336) — and the premise written
beside it, "nothing else writes it", is false here: `just setup-hooks` writes
three git builtins into exactly that file. So any concurrent session made the
gate print 0986's text, *it is live right now*, about a second terminal, on the
fast line.

## Measured

Reproduced verbatim on `origin/main`, running the gate while a background
process wrote a scratch key into the shared config once a second:

```
config under observation: /home/aeon/repos/nano-ros/.git/config
  ok    .githooks/pre-push [worktree] left the environment's repository untouched
  FAIL  this repo's own config CHANGED across a hook run — that is
        0986's exact symptom (core.bare=true), and it is live right now.
check-hook-repo-side-effects: FAILED
gate exit status: 1
```

The flake is intermittent for a reason worth writing down: a set/unset PAIR that
lands entirely inside the ~4 s window restores the bytes and the gate never
notices. Only a write that MOVES the file across the two samples shows up, which
is why the first attempt at this control passed.

## Why the fix is not "compare fewer keys"

The gate could only ever distinguish "the hook wrote this config" from "somebody
wrote this config" when nothing else ran. Enumerating the keys other recipes
legitimately set would have been the authored-list drift this repo refuses, and
a lock would have serialised sessions rather than fixed the attribution.

The real defect was the SUBJECT. A hook clears the inherited git environment as
its first act, so from its second line on it resolves its repository from the
WORKING DIRECTORY — and the gate was handing it the developer's own worktree as
that cwd. Once the cwd is a repository the gate built, a difference is
attributable to the hook by construction.

## What landed

`run_probe` takes the cwd token `@checkout`, which builds a `git clone --local
--shared` of this checkout INSIDE the victim directory and runs the command
there. One snapshot then covers both repositories the hook can reach, so the
assertion got STRONGER rather than weaker: config, index, object store and every
file's mtime, compared byte for byte, where it used to be one file's sha1. The
working tree's uncommitted tracked edits are overlaid, so the gate probes the
scripts being edited rather than HEAD's copies.

Measured cost: 0.94 s to build the clone, 0.46 s per snapshot, twice — about
3.7 s on a gate that costs ~45 s, against a hook run that is 20 s (18.5 s of
which is `issue-ids-check.sh`, unchanged either way). The old header's "~3.7 s"
for the hook run was stale by 5x and is corrected.

## Controls

Three, all run on the real tree:

1. The reproduction above, re-run after the fix: **gate OK, exit 0**, with the
   shared config still churning once a second.
2. Mutation — point the new cwd-offender probe back at `"$REPO"`. Both new
   selftest assertions fire, including the decisive one: *the probe wrote into
   THIS checkout's config … it is now doing the damage, not reporting it.* The
   selftest cleans the key up, and the shared config was verified empty after.
3. Mutation — make `.githooks/pre-push` itself write `git config --local
   nros.mutant 1`. The gate FAILS, naming `checkout/.git/config` in its diff,
   and the developer's own config is untouched. So the gate still catches a
   genuine hook side effect; it simply no longer reads anybody else's repo.

The second control is now a permanent part of the selftest: a script that clears
the environment and then writes to its cwd's repository must be caught, and the
write must land in the gate's clone.

## Still out of reach, deliberately

The ~50 per-script probes keep the real worktree as their cwd, so a script that
ignored its environment and wrote to its cwd's repository is not caught there.
That was equally true before — the old config sample was taken once, after those
probes had all finished — and giving each its own clone would cost ~75 s. The
cwd arm is covered where 0986 actually happened: the hook. Said in the gate's
header rather than left for the next reader to discover.
