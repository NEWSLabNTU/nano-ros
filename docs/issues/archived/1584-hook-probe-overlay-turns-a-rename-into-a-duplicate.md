---
id: 1584
title: "`check-hook-repo-side-effects` overlays uncommitted edits with
  `diff --name-only`, which hides a rename's source — so an uncommitted
  `git mv` into `archived/` makes the gate report a duplicate issue id that
  exists nowhere but inside its own probe"
status: resolved
type: bug
area: ci, tooling
severity: medium
found: 2026-09-29
resolved: 2026-09-29
related: [0986, 0988, 1465, 1053]
---

## What happened

Archiving issue 1553 — a plain `git mv docs/issues/1553-….md
docs/issues/archived/` — turned `just check fast` red:

```
===== FAIL (hook-repo-side-effects, rc=1, 24120ms) =====
```

with the gate reporting:

```
FAIL  .githooks/pre-push exited before its final stage (rc=1)
      [FAIL] duplicate issue ids:
        id 1553:
          docs/issues/1553-ci-image-has-no-corrosion-so-probe-gates-race-a-shared-fetch.md
          docs/issues/archived/1553-ci-image-has-no-corrosion-so-probe-gates-race-a-shared-fetch.md
        Pick the next free id (highest existing + 1) for the LATER filing,
        rename the file, update its `id:` frontmatter, and fix references.
```

There was no duplicate. The working tree held the file at exactly one path;
`git status` showed a clean `R` rename. Committing the rename and re-running
the gate gives `check-hook-repo-side-effects: OK` with nothing else changed.

## Why

`build_checkout` in `scripts/check-hook-repo-side-effects.sh` clones the
checkout and then overlays the working tree's uncommitted tracked edits, so the
probe runs against the code you are editing rather than HEAD's copies (issue
1465). The overlay copies each path that still exists and deletes each path
that does not — the `rm -f` arm is there precisely for deletions. It read the
list from:

```sh
git -C "$REPO" diff --name-only HEAD
```

**Rename detection is on by default, and `--name-only` prints a rename's
DESTINATION only.** Measured in a worktree with one `git mv` staged:

```
$ git diff --name-only HEAD | grep 1569
docs/issues/archived/1569-….md

$ git diff --no-renames --name-only HEAD | grep 1569
docs/issues/1569-….md
docs/issues/archived/1569-….md
```

So the source path is never listed, the `rm -f` arm never fires, and the clone
ends up holding the file at both paths. `scripts/ci/issue-ids-check.sh` scans
`find docs/issues -maxdepth 2`, which covers `archived/`, so it sees two files
with id 1553 and correctly reports a duplicate — of a state that exists only
inside the probe.

## What makes it worth a fix rather than a note

The message is confidently wrong in the expensive direction. It does not say
"the probe may be misreading your rename"; it says the id namespace would be
corrupted and instructs the developer to renumber the later filing and fix its
references. Following that advice renumbers a perfectly good issue. Archiving a
resolved issue is a routine operation, so this is on the path of anyone closing
one.

## What this is NOT

* **Not issue 1465.** That was about the probe asserting on a repository it did
  not build; the fix was to build one. This is the overlay that fix introduced,
  reading its own input with rename detection left on.
* **Not a defect in `issue-ids-check.sh`.** Scanning `archived/` is correct —
  an issue should exist at exactly one path — and it reported faithfully on the
  tree it was given.

## Resolution

`--no-renames` on that diff, so both paths are listed and the `rm -f` arm gets
the source it was written for. Verified both directions in a worktree with a
`git mv` staged: with the flag the gate is `OK`, and removing the flag with the
same rename still staged reproduces the duplicate report exactly.

The same one-word exposure was swept rather than fixed only at the reported
site. `scripts/check-unsafe-census.py` uses `git diff --name-only` to decide
which crates a branch touched, so a file moved from crate A to crate B marks B
touched and leaves **A unenforced**; it takes `--no-renames` too, which widens
the touched set — the fail-closed direction that function's own docstring says
it prefers. Those are the only two `--name-only` consumers in `scripts/`,
`just/` and `.githooks/`.
