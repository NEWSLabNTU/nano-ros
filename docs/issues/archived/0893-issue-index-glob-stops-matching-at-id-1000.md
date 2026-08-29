---
id: 893
title: "the issue index enumerates `docs/issues/0*.md`, so at id 1000 it stops
  seeing new issues — and the gate goes GREEN because both halves miss them"
status: resolved
type: bug
area: tooling
related: [issue-0884, issue-0883, issue-0196]
resolved_in: ISSUE_PATHSPEC + `gen-issue-index.py --self-test`
---

## Problem

`scripts/gen-issue-index.py` and `scripts/check-issue-index.sh` both enumerated
the issue files with the git pathspec `docs/issues/0*.md`. That assumes every id
begins with a zero, which is true only below 1000. Ids are currently at 0893.

At 1000 the glob stops matching. The generator drops the issue from the open
list; the checker cannot see the file either. **The two then agree about a set
that is missing it, and the gate passes.** Measured, with a synthetic
a throwaway issue file numbered 1000 present in `docs/issues/`:

| | rows naming `#1000` | `check-issue-index` |
| --- | --- | --- |
| old pathspec | **0** | **rc=0** — silently clean |
| fixed | 1 | rc=0 |

A ceiling that ERRORS is a nuisance. This one reports success, so nothing points
at the file that is wrong — the same shape as issue 0196 (a probe narrower than
the rule it enforces), and the same shape as the vacuous tests that passed on the
host they were meant to warn about.

The blast radius is every consumer of the list: the index is how issues are
found, and `check-issue-index` is what stops a resolved issue from sitting in
`docs/issues/`. Both would have quietly covered ids 0001–0999 only.

## Not affected

`scripts/reserve-issue-id.sh` enumerates `docs/issues/*.md` — unbounded — and
also claims `refs/issue-ids/NNNN` on origin. So ids would NOT have been silently
reused; the reservation half was already correct. That matters, because a
duplicate-id bug is far worse than an invisible-row bug, and it is worth
recording that the two halves failed independently.

## Fix

One spelling, `ISSUE_PATHSPEC = "docs/issues/[0-9]*.md"`, named and explained in
`gen-issue-index.py`; `check-issue-index.sh` uses the same pattern at both of its
enumeration sites. The id is now read as the whole leading run of digits
(`re.match(r"\d+", …)`) rather than a fixed four-character slice, which returned
`1000` for `10000-e.md` — a different issue.

Gated by `gen-issue-index.py --self-test`, on the `check-issue-index` recipe. It
builds a throwaway git repo containing `0001`, `0999`, `1000`, `9999`, `10000`,
`README.md` and `open.md`, and asserts `git ls-files` returns exactly the five
numeric ones. It tests GIT PATHSPEC matching rather than a regex, because
pathspec matching is what broke — a regex assertion would have passed while the
real enumeration still dropped files. It also asserts the shell checker carries
the same pathspec, so the two cannot drift apart again.

Verified to fail on the old spelling, naming what it missed:

```
gen-issue-index --self-test FAILED
  pathspec 'docs/issues/0*.md'
  missed:  ['1000-c.md', '10000-e.md', '9999-d.md']
```

## Sweep

```sh
git grep -n "issues/0\*" -- scripts just justfile .github
git grep -nE '\[:4\]|\[0-9\]\{4\}' -- scripts
```

Four sites: two globs and one id-extraction in `check-issue-index.sh`, and the
glob plus `[:4]` slice in `gen-issue-index.py`.
